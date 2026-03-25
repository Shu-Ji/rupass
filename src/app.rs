use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::cli::{
    Commands, ParsedCli, TeamClearS3Args, TeamCommands, TeamCreateArgs, TeamImportArgs,
    TeamScopedCommands, TeamSetRemoteArgs, TeamSetS3Args,
};
use crate::crypto::{decrypt_text, derive_key, password_verifier, read_existing_password};
use crate::git_sync::{bootstrap_team_repo, load_team_metadata};
use crate::storage::{
    AppPaths, SecretRecord, SyncBackend, TeamConfig, TeamMetadata, TeamS3Config, list_team_configs,
    load_secret_record, load_team_config, save_team_config, validate_team_name,
};
use crate::team_sync::{has_remote, sync_team_backends};
use crate::tui_ops;

static IMPORT_TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct TeamAccess {
    config: TeamConfig,
    cipher_key: [u8; 32],
}

#[derive(Debug)]
struct ResolvedTeam {
    name: String,
}

pub(crate) fn dispatch(cli: ParsedCli) -> Result<()> {
    let paths = AppPaths::resolve()?;
    paths.ensure_base_dirs()?;

    match cli {
        ParsedCli::Standard(cli) => match cli.command {
            Commands::Tui => crate::tui::run(paths),
            Commands::SyncAll => sync_all_teams(&paths),
            Commands::Team { command } => dispatch_team_command(&paths, command),
        },
        ParsedCli::TeamScoped(cli) => {
            let team = resolve_target_team(&paths, cli.team.as_deref())?;
            match cli.command {
                TeamScopedCommands::List(args) => {
                    list_keys(&paths, &team, args.password.as_deref())
                }
                TeamScopedCommands::Get(args) => {
                    get_secret(&paths, &team, &args.key, args.password.as_deref())
                }
                TeamScopedCommands::Set(args) => set_secret(
                    &paths,
                    &team,
                    &args.key,
                    &args.value,
                    args.password.as_deref(),
                ),
                TeamScopedCommands::Del(args) => {
                    delete_secret(&paths, &team, &args.key, args.password.as_deref())
                }
            }
        }
    }
}

fn dispatch_team_command(paths: &AppPaths, command: TeamCommands) -> Result<()> {
    match command {
        TeamCommands::List => list_teams(paths),
        TeamCommands::Create(args) => create_team(paths, args),
        TeamCommands::Import(args) => {
            let team = import_team(paths, args)?;
            println!("imported team: {team}");
            Ok(())
        }
        TeamCommands::Del(args) => delete_team(paths, &args.team, args.password.as_deref()),
        TeamCommands::SetRemote(args) => set_team_remote(paths, args),
        TeamCommands::ClearRemote(args) => {
            clear_team_remote(paths, &args.team, args.password.as_deref())
        }
        TeamCommands::SetS3(args) => set_team_s3(paths, args),
        TeamCommands::ClearS3(args) => clear_team_s3(paths, args),
        TeamCommands::Sync(args) => sync_team(paths, &args.team, args.password.as_deref()),
    }
}

fn resolve_target_team(paths: &AppPaths, explicit_team: Option<&str>) -> Result<ResolvedTeam> {
    resolve_target_team_with(paths, explicit_team, || {
        bail!("no team initialized. run `rupass tui` first")
    })
}

fn resolve_target_team_with<F>(
    paths: &AppPaths,
    explicit_team: Option<&str>,
    on_missing_team: F,
) -> Result<ResolvedTeam>
where
    F: FnOnce() -> Result<ResolvedTeam>,
{
    if let Some(team) = explicit_team {
        return Ok(ResolvedTeam {
            name: team.to_string(),
        });
    }

    let configs = list_team_configs(paths)?;
    match configs.as_slice() {
        [] => on_missing_team(),
        [config] => Ok(ResolvedTeam {
            name: config.team_name.clone(),
        }),
        _ => {
            let names = configs
                .iter()
                .map(|config| config.team_name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            bail!("multiple teams found: {names}. please specify a team explicitly")
        }
    }
}

fn sync_all_teams(paths: &AppPaths) -> Result<()> {
    let teams = list_team_configs(paths)?;
    if teams.is_empty() {
        bail!("no team initialized. run `rupass tui` first");
    }

    let mut synced = 0_usize;
    let mut skipped = Vec::new();
    for team in teams {
        if !has_remote(&team) {
            eprintln!("skipped team without remote: {}", team.team_name);
            skipped.push(team.team_name);
            continue;
        }
        let access = authenticate_team(paths, &team.team_name)?;
        sync_team_backends(&paths.team_store_dir(&team.team_name), &access.config)?;
        println!("synced team: {}", team.team_name);
        synced += 1;
    }

    if synced == 0 {
        bail!("no team has a remote configured: {}", skipped.join(", "));
    }
    Ok(())
}

fn list_teams(paths: &AppPaths) -> Result<()> {
    let teams = list_team_configs(paths)?;
    if teams.is_empty() {
        println!("no teams");
        return Ok(());
    }

    for team in teams {
        println!(
            "{}\tsync={}\tgit={}\ts3={}",
            team.team_name,
            team.sync_backend_label(),
            team.git_remote.as_deref().unwrap_or("-"),
            team.s3.as_ref().map(|s3| s3.bucket.as_str()).unwrap_or("-")
        );
    }
    Ok(())
}

fn create_team(paths: &AppPaths, args: TeamCreateArgs) -> Result<()> {
    let (password, password_confirm) = resolve_create_passwords(&args)?;
    tui_ops::create_team(paths, &args.team, &password, &password_confirm)?;
    println!("created team: {}", args.team);
    Ok(())
}

pub(crate) fn import_team(paths: &AppPaths, args: TeamImportArgs) -> Result<String> {
    let (requested_team, url) = resolve_import_target(&args)?;
    if let Some(team) = requested_team.as_deref() {
        validate_team_name(team)?;
        if paths.config_path(team).exists() {
            bail!("team already exists: {team}");
        }
    }

    let temp_repo = TemporaryImportRepo::create()?;
    bootstrap_team_repo(temp_repo.path(), &url)?;
    let metadata = load_team_metadata(temp_repo.path())
        .with_context(|| format!("failed to load team metadata from {url}"))?;
    let team = requested_team.unwrap_or_else(|| metadata.team_name.clone());
    validate_team_name(&team)?;
    if paths.config_path(&team).exists() {
        bail!("team already exists: {team}");
    }

    let password = resolve_existing_password(&team, args.password.as_deref())?;
    let config = build_imported_team_config(&team, &url, &password, &metadata)?;
    temp_repo.persist_to(&paths.team_store_dir(&team))?;
    save_team_config(paths, &config)?;
    Ok(team)
}

fn delete_team(paths: &AppPaths, team: &str, password: Option<&str>) -> Result<()> {
    let password = resolve_existing_password(team, password)?;
    tui_ops::delete_team(paths, team, &password)?;
    println!("deleted team: {team}");
    Ok(())
}

fn set_team_remote(paths: &AppPaths, args: TeamSetRemoteArgs) -> Result<()> {
    let access = tui_ops::open_team(paths, &args.team, args.password.as_deref())?;
    tui_ops::set_remote(paths, &access, &args.url)?;
    println!("updated git remote: {}\t{}", args.team, args.url);
    Ok(())
}

fn clear_team_remote(paths: &AppPaths, team: &str, password: Option<&str>) -> Result<()> {
    let access = tui_ops::open_team(paths, team, password)?;
    tui_ops::set_remote(paths, &access, "")?;
    println!("cleared git remote: {team}");
    Ok(())
}

fn set_team_s3(paths: &AppPaths, args: TeamSetS3Args) -> Result<()> {
    let access = tui_ops::open_team(paths, &args.team, args.password.as_deref())?;
    let root = args.root.unwrap_or_default().trim_matches('/').to_string();
    let s3 = TeamS3Config {
        endpoint: args.endpoint,
        region: args.region,
        bucket: args.bucket,
        access_key_id: args.access_key_id,
        secret_access_key: args.secret_access_key,
        root,
        force_path_style: args.force_path_style,
    };
    tui_ops::set_s3(paths, &access, Some(s3))?;
    println!("updated S3 remote: {}", args.team);
    Ok(())
}

fn clear_team_s3(paths: &AppPaths, args: TeamClearS3Args) -> Result<()> {
    let access = tui_ops::open_team(paths, &args.team, args.password.as_deref())?;
    tui_ops::set_s3(paths, &access, None)?;
    println!("cleared S3 remote: {}", args.team);
    Ok(())
}

fn sync_team(paths: &AppPaths, team: &str, password: Option<&str>) -> Result<()> {
    let access = tui_ops::open_team(paths, team, password)?;
    if !has_remote(&access.config) {
        bail!("team has no remote configured: {team}");
    }
    tui_ops::sync_team(paths, &access)?;
    println!("synced team: {team}");
    Ok(())
}

fn list_keys(paths: &AppPaths, team: &ResolvedTeam, password: Option<&str>) -> Result<()> {
    let access = tui_ops::open_team(paths, &team.name, password)?;
    let keys = tui_ops::list_keys(paths, &access)?;
    for key in keys {
        println!("{key}");
    }
    Ok(())
}

fn get_secret(
    paths: &AppPaths,
    team: &ResolvedTeam,
    key: &str,
    password: Option<&str>,
) -> Result<()> {
    let cipher_key = if let Some(password) = password {
        tui_ops::open_team(paths, &team.name, Some(password))?.cipher_key
    } else {
        load_team_for_get(paths, &team.name)?.1
    };
    let record = load_secret_record(paths, &team.name, key)?;
    verify_secret_key(&cipher_key, key, &record)?;
    println!(
        "{}",
        decrypt_text(&cipher_key, &record.encrypted_value, &record.value_nonce)?
    );
    Ok(())
}

fn set_secret(
    paths: &AppPaths,
    team: &ResolvedTeam,
    key: &str,
    value: &str,
    password: Option<&str>,
) -> Result<()> {
    let access = tui_ops::open_team(paths, &team.name, password)?;
    tui_ops::set_secret(paths, &access, key, value)?;
    println!("saved key: {key}");
    Ok(())
}

fn delete_secret(
    paths: &AppPaths,
    team: &ResolvedTeam,
    key: &str,
    password: Option<&str>,
) -> Result<()> {
    let access = tui_ops::open_team(paths, &team.name, password)?;
    tui_ops::delete_secret(paths, &access, key)?;
    println!("deleted key: {key}");
    Ok(())
}

fn load_team_for_get(paths: &AppPaths, team: &str) -> Result<(TeamConfig, [u8; 32])> {
    let config = load_team_config(paths, team)?;
    if let Some(cipher_key) = config.cipher_key.clone() {
        return Ok((config, decode_cipher_key(team, &cipher_key)?));
    }

    migrate_team_cipher_key(paths, config, team)
}

fn authenticate_team(paths: &AppPaths, team: &str) -> Result<TeamAccess> {
    let config = load_team_config(paths, team)?;
    let password = read_existing_password(team)?;
    authenticate_team_with_password(paths, config, team, &password)
}

fn authenticate_team_with_password(
    paths: &AppPaths,
    mut config: TeamConfig,
    team: &str,
    password: &str,
) -> Result<TeamAccess> {
    let salt = STANDARD
        .decode(&config.salt)
        .with_context(|| format!("invalid salt for {team}"))?;
    let expected = STANDARD
        .decode(&config.password_verifier)
        .with_context(|| format!("invalid password verifier for {team}"))?;
    let derived_key = derive_key(password, &salt)?;

    if expected != password_verifier(&derived_key) {
        bail!("invalid password for team: {team}");
    }

    if config.cipher_key.is_none() {
        config.cipher_key = Some(STANDARD.encode(derived_key));
        crate::storage::save_team_config(paths, &config)?;
    }

    Ok(TeamAccess {
        config,
        cipher_key: derived_key,
    })
}

fn migrate_team_cipher_key(
    paths: &AppPaths,
    config: TeamConfig,
    team: &str,
) -> Result<(TeamConfig, [u8; 32])> {
    let password = read_existing_password(team)?;
    let access = authenticate_team_with_password(paths, config, team, &password)?;
    Ok((access.config, access.cipher_key))
}

fn resolve_import_target(args: &TeamImportArgs) -> Result<(Option<String>, String)> {
    if let Some(team) = args.team.as_deref() {
        validate_team_name(team)?;
        let [url] = args.args.as_slice() else {
            bail!("`rupass team import --team <team>` requires exactly one remote url");
        };
        return Ok((Some(team.to_string()), url.clone()));
    }

    match args.args.as_slice() {
        [url] => Ok((None, url.clone())),
        [team, url] if validate_team_name(team).is_ok() => Ok((Some(team.clone()), url.clone())),
        [_, _] => bail!(
            "invalid import arguments. use `rupass team import <remote>` or `rupass team import <team> <remote>`"
        ),
        _ => bail!("missing remote url"),
    }
}

fn build_imported_team_config(
    team: &str,
    remote: &str,
    password: &str,
    metadata: &TeamMetadata,
) -> Result<TeamConfig> {
    if metadata.team_name != team {
        bail!(
            "remote team mismatch: expected {}, got {}",
            team,
            metadata.team_name
        );
    }

    let salt = STANDARD
        .decode(&metadata.salt)
        .with_context(|| format!("invalid salt for {team}"))?;
    let expected = STANDARD
        .decode(&metadata.password_verifier)
        .with_context(|| format!("invalid password verifier for {team}"))?;
    let derived_key = derive_key(password, &salt)?;

    if expected != password_verifier(&derived_key) {
        bail!("invalid password for team: {team}");
    }

    Ok(TeamConfig {
        team_name: team.to_string(),
        salt: metadata.salt.clone(),
        password_verifier: metadata.password_verifier.clone(),
        cipher_key: Some(STANDARD.encode(derived_key)),
        git_remote: Some(remote.to_string()),
        s3: None,
        sync_backend: Some(SyncBackend::Git),
    })
}

struct TemporaryImportRepo {
    path: std::path::PathBuf,
    keep: bool,
}

impl TemporaryImportRepo {
    fn create() -> Result<Self> {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock before unix epoch")?
            .as_nanos();
        let counter = IMPORT_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rupass-import-{}-{suffix}-{counter}",
            std::process::id()
        ));
        fs::create_dir_all(&path)
            .with_context(|| format!("failed to create temporary import dir {}", path.display()))?;
        Ok(Self { path, keep: false })
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn persist_to(mut self, target: &std::path::Path) -> Result<()> {
        if target.exists() {
            let is_empty = fs::read_dir(target)
                .with_context(|| format!("failed to read {}", target.display()))?
                .next()
                .is_none();
            if !is_empty {
                bail!("team store already exists: {}", target.display());
            }
            fs::remove_dir(target)
                .with_context(|| format!("failed to remove {}", target.display()))?;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }
        fs::rename(&self.path, target).with_context(|| {
            format!(
                "failed to move imported repo from {} to {}",
                self.path.display(),
                target.display()
            )
        })?;
        self.keep = true;
        Ok(())
    }
}

impl Drop for TemporaryImportRepo {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn resolve_create_passwords(args: &TeamCreateArgs) -> Result<(String, String)> {
    match (&args.password, &args.password_confirm) {
        (Some(password), Some(confirm)) => Ok((password.clone(), confirm.clone())),
        (Some(password), None) => Ok((password.clone(), password.clone())),
        (None, Some(_)) => bail!("--password-confirm requires --password"),
        (None, None) => {
            let password = rpassword::prompt_password(format!("password for {}: ", args.team))
                .context("failed to read password")?;
            if password.is_empty() {
                bail!("password cannot be empty");
            }
            let confirm =
                rpassword::prompt_password(format!("confirm password for {}: ", args.team))
                    .context("failed to read password confirmation")?;
            Ok((password, confirm))
        }
    }
}

fn resolve_existing_password(team: &str, password: Option<&str>) -> Result<String> {
    match password {
        Some(password) => Ok(password.to_string()),
        None => read_existing_password(team),
    }
}

fn decode_cipher_key(team: &str, cipher_key: &str) -> Result<[u8; 32]> {
    let raw = STANDARD
        .decode(cipher_key)
        .with_context(|| format!("invalid stored cipher key for {team}"))?;
    raw.try_into()
        .map_err(|_| anyhow::anyhow!("invalid stored cipher key length for {team}"))
}

fn verify_secret_key(
    cipher_key: &[u8; 32],
    expected_key: &str,
    record: &SecretRecord,
) -> Result<()> {
    let stored_key = decrypt_text(cipher_key, &record.encrypted_key, &record.key_nonce)?;
    if stored_key != expected_key {
        bail!("secret key mismatch for {expected_key}");
    }
    Ok(())
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
