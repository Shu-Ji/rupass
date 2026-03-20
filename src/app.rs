use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::cli::{Commands, ParsedCli, TeamScopedCommands};
use crate::crypto::{decrypt_text, derive_key, password_verifier, read_existing_password};
use crate::git_sync::sync_team_repo;
use crate::storage::{
    AppPaths, SecretRecord, TeamConfig, list_team_configs, load_secret_record, load_team_config,
};

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
        },
        ParsedCli::TeamScoped(cli) => {
            let team = resolve_target_team(&paths, cli.team.as_deref())?;
            match cli.command {
                TeamScopedCommands::Get(args) => get_secret(&paths, &team, &args.key),
            }
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

    for team in teams {
        let access = authenticate_team(paths, &team.team_name)?;
        sync_team_repo(&paths.team_store_dir(&team.team_name), &access.config)?;
        println!("synced team: {}", team.team_name);
    }
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
