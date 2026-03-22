use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::cli::{
    Commands, KeyCommands, ParsedCli, TeamCommands, TeamCreateArgs, TeamScopedCommands,
};
use crate::crypto::{decrypt_text, derive_key, password_verifier, read_existing_password};
use crate::git_sync::sync_team_repo;
use crate::storage::{
    AppPaths, SecretRecord, TeamConfig, list_team_configs, load_secret_record, load_team_config,
};
use crate::tui_ops;

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
            Commands::Key { command } => dispatch_key_command(&paths, command),
        },
        ParsedCli::TeamScoped(cli) => {
            let team = resolve_target_team(&paths, cli.team.as_deref())?;
            match cli.command {
                TeamScopedCommands::Get(args) => get_secret(&paths, &team, &args.key, None),
            }
        }
    }
}

fn dispatch_team_command(paths: &AppPaths, command: TeamCommands) -> Result<()> {
    match command {
        TeamCommands::List => list_teams(paths),
        TeamCommands::Create(args) => create_team(paths, args),
        TeamCommands::Delete(args) => delete_team(paths, &args.team, args.password.as_deref()),
        TeamCommands::SetRemote(args) => set_team_remote(paths, &args.team, &args.url, args.password.as_deref()),
        TeamCommands::ClearRemote(args) => clear_team_remote(paths, &args.team, args.password.as_deref()),
        TeamCommands::Sync(args) => sync_team(paths, &args.team, args.password.as_deref()),
    }
}

fn dispatch_key_command(paths: &AppPaths, command: KeyCommands) -> Result<()> {
    match command {
        KeyCommands::List(args) => list_keys(paths, args.team.as_deref(), args.password.as_deref()),
        KeyCommands::Get(args) => {
            let team = resolve_target_team(paths, args.team.as_deref())?;
            get_secret(paths, &team, &args.key, args.password.as_deref())
        }
        KeyCommands::Set(args) => {
            let team = resolve_target_team(paths, args.team.as_deref())?;
            set_secret(paths, &team, &args.key, &args.value, args.password.as_deref())
        }
        KeyCommands::Delete(args) => {
            let team = resolve_target_team(paths, args.team.as_deref())?;
            delete_secret(paths, &team, &args.key, args.password.as_deref())
        }
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
        if team.git_remote.is_none() {
            eprintln!("skipped team without remote: {}", team.team_name);
            skipped.push(team.team_name);
            continue;
        }
        let access = authenticate_team(paths, &team.team_name)?;
        sync_team_repo(&paths.team_store_dir(&team.team_name), &access.config)?;
        println!("synced team: {}", team.team_name);
        synced += 1;
    }

    if synced == 0 {
        bail!(
            "no team has a remote configured: {}",
            skipped.join(", ")
        );
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
            "{}\t{}",
            team.team_name,
            team.git_remote.as_deref().unwrap_or("-")
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

fn delete_team(paths: &AppPaths, team: &str, password: Option<&str>) -> Result<()> {
    let password = resolve_existing_password(team, password)?;
    tui_ops::delete_team(paths, team, &password)?;
    println!("deleted team: {team}");
    Ok(())
}

fn set_team_remote(paths: &AppPaths, team: &str, url: &str, password: Option<&str>) -> Result<()> {
    let access = tui_ops::open_team(paths, team, password)?;
    tui_ops::set_remote(paths, &access, url)?;
    println!("updated remote: {team}\t{url}");
    Ok(())
}

fn clear_team_remote(paths: &AppPaths, team: &str, password: Option<&str>) -> Result<()> {
    let access = tui_ops::open_team(paths, team, password)?;
    tui_ops::set_remote(paths, &access, "")?;
    println!("cleared remote: {team}");
    Ok(())
}

fn sync_team(paths: &AppPaths, team: &str, password: Option<&str>) -> Result<()> {
    let access = tui_ops::open_team(paths, team, password)?;
    if access.config.git_remote.is_none() {
        bail!("team has no remote configured: {team}");
    }
    tui_ops::sync_team(paths, &access)?;
    println!("synced team: {team}");
    Ok(())
}

fn list_keys(paths: &AppPaths, explicit_team: Option<&str>, password: Option<&str>) -> Result<()> {
    let team = resolve_target_team(paths, explicit_team)?;
    let access = tui_ops::open_team(paths, &team.name, password)?;
    let keys = tui_ops::list_keys(paths, &access)?;
    for key in keys {
        println!("{key}");
    }
    Ok(())
}

fn get_secret(paths: &AppPaths, team: &ResolvedTeam, key: &str, password: Option<&str>) -> Result<()> {
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

fn resolve_create_passwords(args: &TeamCreateArgs) -> Result<(String, String)> {
    match (&args.password, &args.password_confirm) {
        (Some(password), Some(confirm)) => Ok((password.clone(), confirm.clone())),
        (Some(password), None) => Ok((password.clone(), password.clone())),
        (None, Some(_)) => bail!("--password-confirm requires --password"),
        (None, None) => {
            let password =
                rpassword::prompt_password(format!("password for {}: ", args.team))
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
