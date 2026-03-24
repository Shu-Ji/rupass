use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::crypto::{
    decrypt_text, derive_key, encrypt_text, password_verifier, random_bytes, read_existing_password,
};
use crate::git_sync::ensure_git_repo;
use crate::storage::{
    AppPaths, SecretRecord, SyncBackend, TeamConfig, TeamS3Config, delete_secret_record,
    list_secret_records, list_team_configs, load_secret_record, load_team_config,
    save_secret_record, save_team_config, validate_team_name,
};
use crate::team_sync::sync_team_backends;

#[derive(Clone, Debug)]
pub(crate) struct TeamAccess {
    pub(crate) config: TeamConfig,
    pub(crate) cipher_key: [u8; 32],
}

#[derive(Clone, Debug)]
pub(crate) struct TeamSummary {
    pub(crate) team_name: String,
    pub(crate) sync_backend: Option<SyncBackend>,
    pub(crate) git_remote: Option<String>,
    pub(crate) s3_bucket: Option<String>,
}

pub(crate) fn list_teams(paths: &AppPaths) -> Result<Vec<TeamSummary>> {
    Ok(list_team_configs(paths)?
        .into_iter()
        .map(|team| {
            let sync_backend = team.effective_sync_backend();
            let s3_bucket = team.s3.as_ref().map(|s3| s3.bucket.clone());
            TeamSummary {
                team_name: team.team_name,
                sync_backend,
                git_remote: team.git_remote,
                s3_bucket,
            }
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
    if paths.config_path(team).exists() {
        bail!("team already exists: {team}");
    }

    fs::create_dir_all(paths.team_store_dir(team))
        .with_context(|| format!("failed to create store for {team}"))?;

    let salt = random_bytes::<16>();
    let key = derive_key(password, &salt)?;
    let config = TeamConfig {
        team_name: team.to_string(),
        salt: STANDARD.encode(salt),
        password_verifier: STANDARD.encode(password_verifier(&key)),
        cipher_key: Some(STANDARD.encode(key)),
        git_remote: None,
        s3: None,
        sync_backend: None,
    };
    save_team_config(paths, &config)?;
    ensure_git_repo(&paths.team_store_dir(team))?;
    Ok(TeamAccess {
        config,
        cipher_key: key,
    })
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

    if let Some(cipher_key) = config.cipher_key.clone() {
        return Ok(TeamAccess {
            config,
            cipher_key: decode_cipher_key(team, &cipher_key)?,
        });
    }

    let password = read_existing_password(team)?;
    authenticate_team(paths, config, team, &password)
}

pub(crate) fn list_keys(paths: &AppPaths, access: &TeamAccess) -> Result<Vec<String>> {
    let mut keys = list_secret_records(paths, &access.config.team_name)?
        .into_iter()
        .map(|record| decrypt_text(&access.cipher_key, &record.encrypted_key, &record.key_nonce))
        .collect::<Result<Vec<_>>>()?;
    keys.sort();
    Ok(keys)
}

pub(crate) fn get_secret(paths: &AppPaths, team: &str, key: &str) -> Result<String> {
    let (_, cipher_key) = load_team_for_get(paths, team)?;
    let record = load_secret_record(paths, team, key)?;
    verify_secret_key(&cipher_key, key, &record)?;
    decrypt_text(&cipher_key, &record.encrypted_value, &record.value_nonce)
}

pub(crate) fn set_secret(
    paths: &AppPaths,
    access: &TeamAccess,
    key: &str,
    value: &str,
) -> Result<()> {
    let (encrypted_key, key_nonce) = encrypt_text(&access.cipher_key, key)?;
    let (encrypted_value, value_nonce) = encrypt_text(&access.cipher_key, value)?;
    save_secret_record(
        paths,
        &access.config.team_name,
        key,
        &SecretRecord {
            encrypted_key,
            encrypted_value,
            key_nonce,
            value_nonce,
        },
    )?;
    sync_team_backends(
        &paths.team_store_dir(&access.config.team_name),
        &access.config,
    )?;
    Ok(())
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

    if original_key != new_key
        && paths
            .secret_path(&access.config.team_name, new_key)
            .exists()
    {
        bail!("target key already exists: {new_key}");
    }

    let (encrypted_key, key_nonce) = encrypt_text(&access.cipher_key, new_key)?;
    let (encrypted_value, value_nonce) = encrypt_text(&access.cipher_key, value)?;
    save_secret_record(
        paths,
        &access.config.team_name,
        new_key,
        &SecretRecord {
            encrypted_key,
            encrypted_value,
            key_nonce,
            value_nonce,
        },
    )?;

    if original_key != new_key {
        let record = load_secret_record(paths, &access.config.team_name, original_key)?;
        verify_secret_key(&access.cipher_key, original_key, &record)?;
        delete_secret_record(paths, &access.config.team_name, original_key)?;
    }

    sync_team_backends(
        &paths.team_store_dir(&access.config.team_name),
        &access.config,
    )?;
    Ok(())
}

pub(crate) fn delete_secret(paths: &AppPaths, access: &TeamAccess, key: &str) -> Result<()> {
    let record = load_secret_record(paths, &access.config.team_name, key)?;
    verify_secret_key(&access.cipher_key, key, &record)?;
    delete_secret_record(paths, &access.config.team_name, key)?;
    sync_team_backends(
        &paths.team_store_dir(&access.config.team_name),
        &access.config,
    )?;
    Ok(())
}

pub(crate) fn set_remote(paths: &AppPaths, access: &TeamAccess, url: &str) -> Result<TeamAccess> {
    let mut config = access.config.clone();
    config.git_remote = if url.trim().is_empty() {
        None
    } else {
        Some(url.trim().to_string())
    };
    config.sync_backend = match (&config.git_remote, &config.s3) {
        (Some(_), _) => Some(SyncBackend::Git),
        (None, Some(_)) => Some(SyncBackend::S3),
        (None, None) => None,
    };
    save_team_config(paths, &config)?;
    Ok(TeamAccess {
        config,
        cipher_key: access.cipher_key,
    })
}

pub(crate) fn set_s3(
    paths: &AppPaths,
    access: &TeamAccess,
    s3: Option<TeamS3Config>,
) -> Result<TeamAccess> {
    let mut config = access.config.clone();
    config.s3 = s3;
    config.sync_backend = match (&config.git_remote, &config.s3) {
        (_, Some(_)) => Some(SyncBackend::S3),
        (Some(_), None) => Some(SyncBackend::Git),
        (None, None) => None,
    };
    save_team_config(paths, &config)?;
    Ok(TeamAccess {
        config,
        cipher_key: access.cipher_key,
    })
}

pub(crate) fn set_sync_backend(
    paths: &AppPaths,
    access: &TeamAccess,
    backend: SyncBackend,
) -> Result<TeamAccess> {
    let mut config = access.config.clone();
    match backend {
        SyncBackend::Git if config.git_remote.is_none() => {
            bail!("Git 远程尚未配置");
        }
        SyncBackend::S3 if config.s3.is_none() => {
            bail!("S3 远程尚未配置");
        }
        _ => {}
    }
    config.sync_backend = Some(backend);
    save_team_config(paths, &config)?;
    Ok(TeamAccess {
        config,
        cipher_key: access.cipher_key,
    })
}

pub(crate) fn sync_team(paths: &AppPaths, access: &TeamAccess) -> Result<()> {
    sync_team_backends(
        &paths.team_store_dir(&access.config.team_name),
        &access.config,
    )
}

pub(crate) fn delete_team(paths: &AppPaths, team: &str, password: &str) -> Result<()> {
    let config = load_team_config(paths, team)?;
    authenticate_team(paths, config, team, password)?;

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
    bail!("team `{team}` needs one password-protected action first")
}

fn authenticate_team(
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
        save_team_config(paths, &config)?;
    }
    Ok(TeamAccess {
        config,
        cipher_key: derived_key,
    })
}

fn decode_cipher_key(team: &str, cipher_key: &str) -> Result<[u8; 32]> {
    let raw = STANDARD
        .decode(cipher_key)
        .with_context(|| format!("invalid stored cipher key for {team}"))?;
    raw.try_into()
        .map_err(|_| anyhow!("invalid stored cipher key length for {team}"))
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
