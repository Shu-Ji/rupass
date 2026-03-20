use std::fs;

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::cli::{Commands, ParsedCli, TeamCommands, TeamScopedCommands};
use crate::crypto::{
    decrypt_text, derive_key, encrypt_text, password_verifier, random_bytes,
    read_existing_password, read_new_password,
};
use crate::git_sync::{ensure_git_repo, sync_team_repo};
use crate::storage::{
    AppPaths, DEFAULT_TEAM_DISPLAY_NAME, DEFAULT_TEAM_NAME, SecretRecord, TeamConfig,
    delete_secret_record, list_secret_records, list_team_configs, load_secret_record,
    load_team_config, save_secret_record, save_team_config, validate_team_name,
};

#[derive(Debug)]
struct TeamAccess {
    config: TeamConfig,
    cipher_key: [u8; 32],
}

#[derive(Debug)]
struct ResolvedTeam {
    name: String,
    access: Option<TeamAccess>,
}

pub(crate) fn dispatch(cli: ParsedCli) -> Result<()> {
    let paths = AppPaths::resolve()?;
    paths.ensure_base_dirs()?;

    match cli {
        ParsedCli::Standard(cli) => match cli.command {
            Commands::Init(args) => init_team(
                &paths,
                &args.team,
                args.display_name.as_deref().unwrap_or(&args.team),
            ),
            Commands::Tui => crate::tui::run(paths),
            Commands::SyncAll => sync_all_teams(&paths),
            Commands::Team { command } => match command {
                TeamCommands::List => list_teams(&paths),
                TeamCommands::Delete(args) => delete_team(&paths, &args.team),
            },
        },
        ParsedCli::TeamScoped(cli) => {
            let team = resolve_target_team(&paths, cli.team.as_deref())?;
            match cli.command {
                TeamScopedCommands::List => list_secrets(&paths, &team),
                TeamScopedCommands::Set(args) => set_secret(&paths, &team, &args.key, &args.value),
                TeamScopedCommands::Get(args) => get_secret(&paths, &team, &args.key),
                TeamScopedCommands::Delete(args) => delete_secret(&paths, &team, &args.key),
                TeamScopedCommands::SetRemote(args) => set_team_remote(&paths, &team, &args.url),
                TeamScopedCommands::Sync => sync_team_only(&paths, &team),
            }
        }
    }
}

fn resolve_target_team(paths: &AppPaths, explicit_team: Option<&str>) -> Result<ResolvedTeam> {
    resolve_target_team_with(paths, explicit_team, create_default_team)
}

fn resolve_target_team_with<F>(
    paths: &AppPaths,
    explicit_team: Option<&str>,
    create_missing_team: F,
) -> Result<ResolvedTeam>
where
    F: FnOnce(&AppPaths) -> Result<ResolvedTeam>,
{
    if let Some(team) = explicit_team {
        return Ok(ResolvedTeam {
            name: team.to_string(),
            access: None,
        });
    }

    let configs = list_team_configs(paths)?;
    match configs.as_slice() {
        [] => create_missing_team(paths),
        [config] => Ok(ResolvedTeam {
            name: config.team_name.clone(),
            access: None,
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

fn create_default_team(paths: &AppPaths) -> Result<ResolvedTeam> {
    let access = create_team(paths, DEFAULT_TEAM_NAME, DEFAULT_TEAM_DISPLAY_NAME, None)?;
    println!("initialized team: {DEFAULT_TEAM_NAME}");
    Ok(ResolvedTeam {
        name: DEFAULT_TEAM_NAME.to_string(),
        access: Some(access),
    })
}

fn init_team(paths: &AppPaths, team: &str, display_name: &str) -> Result<()> {
    if paths.config_path(team).exists() {
        println!("team already initialized: {team}");
        return Ok(());
    }

    create_team(paths, team, display_name, None)?;
    println!("initialized team: {team}");
    Ok(())
}

fn create_team(
    paths: &AppPaths,
    team: &str,
    display_name: &str,
    remote: Option<&str>,
) -> Result<TeamAccess> {
    validate_team_name(team)?;

    let config_path = paths.config_path(team);
    if config_path.exists() {
        bail!("team already exists: {team}");
    }

    fs::create_dir_all(paths.team_store_dir(team))
        .with_context(|| format!("failed to create store for {team}"))?;

    let password = read_new_password(team)?;
    let salt = random_bytes::<16>();
    let key = derive_key(&password, &salt)?;
    let config = TeamConfig {
        team_name: team.to_string(),
        display_name: display_name.to_string(),
        salt: STANDARD.encode(salt),
        password_verifier: STANDARD.encode(password_verifier(&key)),
        cipher_key: Some(STANDARD.encode(key)),
        git_remote: remote.map(ToOwned::to_owned),
    };

    save_team_config(paths, &config)?;
    ensure_git_repo(&paths.team_store_dir(team))?;

    if config.git_remote.is_some() {
        sync_team_repo(&paths.team_store_dir(team), &config)?;
    }

    Ok(TeamAccess {
        config,
        cipher_key: key,
    })
}

fn list_teams(paths: &AppPaths) -> Result<()> {
    for config in list_team_configs(paths)? {
        let remote = config.git_remote.unwrap_or_else(|| "-".to_string());
        println!("{}\t{}\t{}", config.team_name, config.display_name, remote);
    }
    Ok(())
}

fn set_team_remote(paths: &AppPaths, team: &ResolvedTeam, url: &str) -> Result<()> {
    let (mut config, _) = require_team_access(paths, team)?;
    config.git_remote = Some(url.to_string());
    save_team_config(paths, &config)?;
    sync_team_repo(&paths.team_store_dir(&team.name), &config)?;
    println!("updated remote for {}", team.name);
    Ok(())
}

fn sync_team_only(paths: &AppPaths, team: &ResolvedTeam) -> Result<()> {
    let (config, _) = require_team_access(paths, team)?;
    sync_team_repo(&paths.team_store_dir(&team.name), &config)?;
    println!("synced team: {}", team.name);
    Ok(())
}

fn sync_all_teams(paths: &AppPaths) -> Result<()> {
    let teams = list_team_configs(paths)?;
    if teams.is_empty() {
        bail!("no team initialized. run `rupass init <team>` first");
    }

    for team in teams {
        let resolved = ResolvedTeam {
            name: team.team_name,
            access: None,
        };
        sync_team_only(paths, &resolved)?;
    }
    Ok(())
}

fn set_secret(paths: &AppPaths, team: &ResolvedTeam, key: &str, value: &str) -> Result<()> {
    let (config, cipher_key) = require_team_access(paths, team)?;
    let (encrypted_key, key_nonce) = encrypt_text(&cipher_key, key)?;
    let (encrypted_value, value_nonce) = encrypt_text(&cipher_key, value)?;
    let record = SecretRecord {
        encrypted_key,
        encrypted_value,
        key_nonce,
        value_nonce,
    };

    save_secret_record(paths, &team.name, key, &record)?;
    sync_team_repo(&paths.team_store_dir(&team.name), &config)?;
    println!("saved key in {}: {key}", team.name);
    Ok(())
}

fn get_secret(paths: &AppPaths, team: &ResolvedTeam, key: &str) -> Result<()> {
    let (_, cipher_key) = load_team_for_get(paths, &team.name)?;
    let record = load_secret_record(paths, &team.name, key)?;
    verify_secret_key(&cipher_key, key, &record)?;
    println!(
        "{}",
        decrypt_text(&cipher_key, &record.encrypted_value, &record.value_nonce)?
    );
    Ok(())
}

fn delete_secret(paths: &AppPaths, team: &ResolvedTeam, key: &str) -> Result<()> {
    let (config, cipher_key) = require_team_access(paths, team)?;
    let record = load_secret_record(paths, &team.name, key)?;
    verify_secret_key(&cipher_key, key, &record)?;
    delete_secret_record(paths, &team.name, key)?;
    sync_team_repo(&paths.team_store_dir(&team.name), &config)?;
    println!("deleted key in {}: {key}", team.name);
    Ok(())
}

fn list_secrets(paths: &AppPaths, team: &ResolvedTeam) -> Result<()> {
    let (_, cipher_key) = require_team_access(paths, team)?;
    let mut keys = list_secret_records(paths, &team.name)?
        .into_iter()
        .map(|record| decrypt_text(&cipher_key, &record.encrypted_key, &record.key_nonce))
        .collect::<Result<Vec<_>>>()?;

    keys.sort();
    for key in keys {
        println!("{key}");
    }
    Ok(())
}

fn require_team_access(paths: &AppPaths, team: &ResolvedTeam) -> Result<(TeamConfig, [u8; 32])> {
    if let Some(access) = &team.access {
        return Ok((access.config.clone(), access.cipher_key));
    }
    authenticate_team(paths, &team.name)
}

fn delete_team(paths: &AppPaths, team: &str) -> Result<()> {
    let password = read_existing_password(team)?;
    delete_team_with_password(paths, team, &password)?;
    println!("deleted team: {team}");
    Ok(())
}

fn delete_team_with_password(paths: &AppPaths, team: &str, password: &str) -> Result<()> {
    let config = load_team_config(paths, team)?;
    authenticate_team_with_password(paths, config, team, password)?;

    let config_path = paths.config_path(team);
    if config_path.exists() {
        fs::remove_file(&config_path)
            .with_context(|| format!("failed to delete {}", config_path.display()))?;
    }

    let store_dir = paths.team_store_dir(team);
    if store_dir.exists() {
        fs::remove_dir_all(&store_dir)
            .with_context(|| format!("failed to delete {}", store_dir.display()))?;
    }

    Ok(())
}

fn load_team_for_get(paths: &AppPaths, team: &str) -> Result<(TeamConfig, [u8; 32])> {
    let config = load_team_config(paths, team)?;
    if let Some(cipher_key) = config.cipher_key.clone() {
        return Ok((config, decode_cipher_key(team, &cipher_key)?));
    }

    migrate_team_cipher_key(paths, config, team)
}

fn authenticate_team(paths: &AppPaths, team: &str) -> Result<(TeamConfig, [u8; 32])> {
    let config = load_team_config(paths, team)?;
    let password = read_existing_password(team)?;
    authenticate_team_with_password(paths, config, team, &password)
}

fn authenticate_team_with_password(
    paths: &AppPaths,
    mut config: TeamConfig,
    team: &str,
    password: &str,
) -> Result<(TeamConfig, [u8; 32])> {
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
        save_team_config(paths, &config)?;
    }

    Ok((config, derived_key))
}

fn migrate_team_cipher_key(
    paths: &AppPaths,
    config: TeamConfig,
    team: &str,
) -> Result<(TeamConfig, [u8; 32])> {
    let password = read_existing_password(team)?;
    authenticate_team_with_password(paths, config, team, &password)
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
