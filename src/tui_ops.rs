use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::crypto::{
    decrypt_text, derive_key, encrypt_text, password_verifier, random_bytes, read_existing_password,
};
use crate::storage::{
    AppPaths, EncryptedTeamSecrets, TeamConfig, TeamFile, TeamKeyCache, TeamSecrets,
    copy_team_file_into_public, delete_key_cache, delete_team_file, list_team_configs,
    load_key_cache, load_team_config, load_team_file_from_path, load_team_secrets_file,
    save_key_cache, save_team_file, validate_team_name,
};

#[derive(Clone, Debug)]
pub(crate) struct TeamAccess {
    pub(crate) config: TeamConfig,
    pub(crate) cipher_key: [u8; 32],
}

#[derive(Clone, Debug)]
pub(crate) struct TeamSummary {
    pub(crate) team_name: String,
}

pub(crate) fn list_teams(paths: &AppPaths) -> Result<Vec<TeamSummary>> {
    Ok(list_team_configs(paths)?
        .into_iter()
        .map(|team| TeamSummary {
            team_name: team.team_name,
        })
        .collect())
}

pub(crate) fn create_team(
    paths: &AppPaths,
    team: &str,
    password: &str,
    password_confirm: &str,
) -> Result<TeamAccess> {
    validate_team_name(team)?;
    if password.is_empty() {
        bail!("password cannot be empty");
    }
    if password != password_confirm {
        bail!("passwords do not match");
    }
    if paths.team_file_path(team).exists() {
        bail!("team already exists: {team}");
    }

    let salt = random_bytes::<16>();
    let key = derive_key(password, &salt)?;
    let config = TeamConfig {
        team_name: team.to_string(),
        salt: STANDARD.encode(salt),
        password_verifier: STANDARD.encode(password_verifier(&key)),
    };
    let access = TeamAccess {
        config,
        cipher_key: key,
    };
    save_team_secrets(paths, &access, &TeamSecrets::default())?;
    save_key_cache(
        paths,
        team,
        &TeamKeyCache {
            cipher_key: STANDARD.encode(key),
        },
    )?;
    Ok(access)
}

pub(crate) fn import_team_file(
    paths: &AppPaths,
    file_path: &str,
    password: &str,
) -> Result<String> {
    let (team_name, team_file) = load_team_file_from_path(std::path::Path::new(file_path))?;
    if paths.team_file_path(&team_name).exists() {
        bail!("team already exists: {}", team_name);
    }
    let key = verify_team_password(&team_name, &team_file, password)?;
    copy_team_file_into_public(paths, &team_name, &team_file)?;
    save_key_cache(
        paths,
        &team_name,
        &TeamKeyCache {
            cipher_key: STANDARD.encode(key),
        },
    )?;
    Ok(team_name)
}

pub(crate) fn unlock_team(paths: &AppPaths, team: &str, password: &str) -> Result<TeamAccess> {
    let config = load_team_config(paths, team)?;
    authenticate_team(paths, config, team, password)
}

pub(crate) fn open_team(
    paths: &AppPaths,
    team: &str,
    password: Option<&str>,
) -> Result<TeamAccess> {
    let config = load_team_config(paths, team)?;

    if let Some(password) = password {
        return authenticate_team(paths, config, team, password);
    }

    if let Some(cache) = load_key_cache(paths, team)? {
        return Ok(TeamAccess {
            config,
            cipher_key: decode_cipher_key(team, &cache.cipher_key)?,
        });
    }

    let password = read_existing_password(team)?;
    authenticate_team(paths, config, team, &password)
}

pub(crate) fn list_keys(paths: &AppPaths, access: &TeamAccess) -> Result<Vec<String>> {
    let store = load_team_secrets(paths, access)?;
    Ok(store.secrets.keys().cloned().collect())
}

pub(crate) fn get_secret(paths: &AppPaths, team: &str, key: &str) -> Result<String> {
    let access = load_team_for_get(paths, team)?;
    get_secret_with_access(paths, &access, key)
}

pub(crate) fn get_secret_with_access(
    paths: &AppPaths,
    access: &TeamAccess,
    key: &str,
) -> Result<String> {
    let store = load_team_secrets(paths, access)?;
    store
        .secrets
        .get(key)
        .cloned()
        .ok_or_else(|| anyhow!("secret not found: {key}"))
}

pub(crate) fn set_secret(
    paths: &AppPaths,
    access: &TeamAccess,
    key: &str,
    value: &str,
) -> Result<()> {
    if key.is_empty() {
        bail!("key cannot be empty");
    }
    let mut store = load_team_secrets(paths, access)?;
    store.secrets.insert(key.to_string(), value.to_string());
    save_team_secrets(paths, access, &store)
}

pub(crate) fn update_secret(
    paths: &AppPaths,
    access: &TeamAccess,
    original_key: &str,
    new_key: &str,
    value: &str,
) -> Result<()> {
    if original_key.is_empty() || new_key.is_empty() {
        bail!("key cannot be empty");
    }

    let mut store = load_team_secrets(paths, access)?;
    if original_key != new_key && store.secrets.contains_key(new_key) {
        bail!("target key already exists: {new_key}");
    }
    if store.secrets.remove(original_key).is_none() {
        bail!("secret not found: {original_key}");
    }
    store.secrets.insert(new_key.to_string(), value.to_string());
    save_team_secrets(paths, access, &store)
}

pub(crate) fn delete_secret(paths: &AppPaths, access: &TeamAccess, key: &str) -> Result<()> {
    let mut store = load_team_secrets(paths, access)?;
    if store.secrets.remove(key).is_none() {
        bail!("secret not found: {key}");
    }
    save_team_secrets(paths, access, &store)
}

pub(crate) fn delete_team(paths: &AppPaths, team: &str, password: &str) -> Result<()> {
    let config = load_team_config(paths, team)?;
    authenticate_team(paths, config, team, password)?;
    delete_team_file(paths, team)?;
    delete_key_cache(paths, team)?;
    Ok(())
}

fn load_team_for_get(paths: &AppPaths, team: &str) -> Result<TeamAccess> {
    let config = load_team_config(paths, team)?;
    let Some(cache) = load_key_cache(paths, team)? else {
        bail!("team `{team}` needs one password-protected action first");
    };
    Ok(TeamAccess {
        config,
        cipher_key: decode_cipher_key(team, &cache.cipher_key)?,
    })
}

fn authenticate_team(
    paths: &AppPaths,
    config: TeamConfig,
    team: &str,
    password: &str,
) -> Result<TeamAccess> {
    let key = verify_team_config_password(&config, password)?;
    save_key_cache(
        paths,
        team,
        &TeamKeyCache {
            cipher_key: STANDARD.encode(key),
        },
    )?;
    Ok(TeamAccess {
        config,
        cipher_key: key,
    })
}

fn verify_team_password(team_name: &str, team_file: &TeamFile, password: &str) -> Result<[u8; 32]> {
    let config = TeamConfig {
        team_name: team_name.to_string(),
        salt: team_file.salt.clone(),
        password_verifier: team_file.password_verifier.clone(),
    };
    verify_team_config_password(&config, password)
}

fn verify_team_config_password(config: &TeamConfig, password: &str) -> Result<[u8; 32]> {
    let salt = STANDARD
        .decode(&config.salt)
        .with_context(|| format!("invalid salt for {}", config.team_name))?;
    let expected = STANDARD
        .decode(&config.password_verifier)
        .with_context(|| format!("invalid password verifier for {}", config.team_name))?;
    let derived_key = derive_key(password, &salt)?;
    if expected != password_verifier(&derived_key) {
        bail!("invalid password for team: {}", config.team_name);
    }
    Ok(derived_key)
}

fn load_team_secrets(paths: &AppPaths, access: &TeamAccess) -> Result<TeamSecrets> {
    load_team_secrets_with_key(paths, &access.config.team_name, &access.cipher_key)
}

fn load_team_secrets_with_key(
    paths: &AppPaths,
    team: &str,
    cipher_key: &[u8; 32],
) -> Result<TeamSecrets> {
    let Some(store) = load_team_secrets_file(paths, team)? else {
        return Ok(TeamSecrets::default());
    };
    decrypt_team_secrets(cipher_key, &store)
}

fn save_team_secrets(paths: &AppPaths, access: &TeamAccess, secrets: &TeamSecrets) -> Result<()> {
    let payload = serde_json::to_string(secrets).context("failed to serialize team secrets")?;
    let (encrypted_payload, nonce) = encrypt_text(&access.cipher_key, &payload)?;
    save_team_file(
        paths,
        &access.config,
        &EncryptedTeamSecrets {
            encrypted_payload,
            nonce,
        },
    )
}

fn decrypt_team_secrets(
    cipher_key: &[u8; 32],
    store: &EncryptedTeamSecrets,
) -> Result<TeamSecrets> {
    let payload = decrypt_text(cipher_key, &store.encrypted_payload, &store.nonce)?;
    serde_json::from_str(&payload).context("failed to parse team secrets")
}

fn decode_cipher_key(team: &str, cipher_key: &str) -> Result<[u8; 32]> {
    let raw = STANDARD
        .decode(cipher_key)
        .with_context(|| format!("invalid stored cipher key for {team}"))?;
    raw.try_into()
        .map_err(|_| anyhow!("invalid stored cipher key length for {team}"))
}
