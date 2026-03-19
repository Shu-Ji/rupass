use std::fs;

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::cli::{Cli, Commands, TeamCommands};
use crate::crypto::{
    decrypt_text, derive_key, encrypt_text, password_verifier, random_bytes,
    read_existing_password, read_new_password,
};
use crate::git_sync::{ensure_git_repo, sync_team_repo};
use crate::storage::{
    AppPaths, SecretRecord, TeamConfig, delete_secret_record, list_secret_records,
    list_team_configs, load_secret_record, load_team_config, save_secret_record, save_team_config,
    validate_team_name,
};

pub(crate) fn dispatch(cli: Cli) -> Result<()> {
    let paths = AppPaths::resolve()?;
    paths.ensure_base_dirs()?;

    match cli.command {
        Commands::Init(args) => init_team(&paths, &args.team, &args.display_name),
        Commands::Set(args) => set_secret(&paths, &args.team, &args.key, &args.value),
        Commands::Get(args) => get_secret(&paths, &args.team, &args.key),
        Commands::Delete(args) => delete_secret(&paths, &args.team, &args.key),
        Commands::List(args) => list_secrets(&paths, &args.team),
        Commands::Team { command } => match command {
            TeamCommands::Create(args) => create_team(
                &paths,
                &args.name,
                args.display_name.as_deref().unwrap_or(&args.name),
                args.remote.as_deref(),
            ),
            TeamCommands::List => list_teams(&paths),
            TeamCommands::SetRemote(args) => set_team_remote(&paths, &args.team, &args.url),
            TeamCommands::Sync(args) => sync_team_only(&paths, &args.team),
        },
    }
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
) -> Result<()> {
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
        git_remote: remote.map(ToOwned::to_owned),
    };

    save_team_config(paths, &config)?;
    ensure_git_repo(&paths.team_store_dir(team))?;

    if config.git_remote.is_some() {
        sync_team_repo(&paths.team_store_dir(team), &config)?;
    }

    Ok(())
}

fn list_teams(paths: &AppPaths) -> Result<()> {
    for config in list_team_configs(paths)? {
        let remote = config.git_remote.unwrap_or_else(|| "-".to_string());
        println!("{}\t{}\t{}", config.team_name, config.display_name, remote);
    }
    Ok(())
}

fn set_team_remote(paths: &AppPaths, team: &str, url: &str) -> Result<()> {
    let mut config = load_team_config(paths, team)?;
    config.git_remote = Some(url.to_string());
    save_team_config(paths, &config)?;
    sync_team_repo(&paths.team_store_dir(team), &config)?;
    println!("updated remote for {team}");
    Ok(())
}

fn sync_team_only(paths: &AppPaths, team: &str) -> Result<()> {
    let config = load_team_config(paths, team)?;
    sync_team_repo(&paths.team_store_dir(team), &config)?;
    println!("synced team: {team}");
    Ok(())
}

fn set_secret(paths: &AppPaths, team: &str, key: &str, value: &str) -> Result<()> {
    let (config, cipher_key) = unlock_team(paths, team)?;
    let (encrypted_key, key_nonce) = encrypt_text(&cipher_key, key)?;
    let (encrypted_value, value_nonce) = encrypt_text(&cipher_key, value)?;
    let record = SecretRecord {
        encrypted_key,
        encrypted_value,
        key_nonce,
        value_nonce,
    };

    save_secret_record(paths, team, key, &record)?;
    sync_team_repo(&paths.team_store_dir(team), &config)?;
    println!("saved key in {team}: {key}");
    Ok(())
}

fn get_secret(paths: &AppPaths, team: &str, key: &str) -> Result<()> {
    let (_, cipher_key) = unlock_team(paths, team)?;
    let record = load_secret_record(paths, team, key)?;
    verify_secret_key(&cipher_key, key, &record)?;
    println!(
        "{}",
        decrypt_text(&cipher_key, &record.encrypted_value, &record.value_nonce)?
    );
    Ok(())
}

fn delete_secret(paths: &AppPaths, team: &str, key: &str) -> Result<()> {
    let (config, cipher_key) = unlock_team(paths, team)?;
    let record = load_secret_record(paths, team, key)?;
    verify_secret_key(&cipher_key, key, &record)?;
    delete_secret_record(paths, team, key)?;
    sync_team_repo(&paths.team_store_dir(team), &config)?;
    println!("deleted key in {team}: {key}");
    Ok(())
}

fn list_secrets(paths: &AppPaths, team: &str) -> Result<()> {
    let (_, cipher_key) = unlock_team(paths, team)?;
    let mut keys = list_secret_records(paths, team)?
        .into_iter()
        .map(|record| decrypt_text(&cipher_key, &record.encrypted_key, &record.key_nonce))
        .collect::<Result<Vec<_>>>()?;

    keys.sort();
    for key in keys {
        println!("{key}");
    }
    Ok(())
}

fn unlock_team(paths: &AppPaths, team: &str) -> Result<(TeamConfig, [u8; 32])> {
    let config = load_team_config(paths, team)?;
    let password = read_existing_password(team)?;
    let salt = STANDARD
        .decode(&config.salt)
        .with_context(|| format!("invalid salt for {team}"))?;
    let expected = STANDARD
        .decode(&config.password_verifier)
        .with_context(|| format!("invalid password verifier for {team}"))?;
    let derived_key = derive_key(&password, &salt)?;

    if expected != password_verifier(&derived_key) {
        bail!("invalid password for team: {team}");
    }

    Ok((config, derived_key))
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
