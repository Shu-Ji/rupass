use std::fs;

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::crypto::{
    decrypt_text, derive_key, encrypt_text, password_verifier, random_bytes, read_existing_password,
};
use crate::storage::{
    AppPaths, EncryptedTeamSecrets, TeamConfig, TeamSecrets, delete_team_secrets_file,
    has_legacy_team_store, list_legacy_secret_records, list_team_configs, load_team_config,
    load_team_secrets_file, remove_legacy_team_store, save_team_config, save_team_secrets_file,
    validate_team_name,
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
    if paths.config_path(team).exists() {
        bail!("team already exists: {team}");
    }

    let salt = random_bytes::<16>();
    let key = derive_key(password, &salt)?;
    let config = TeamConfig {
        team_name: team.to_string(),
        salt: STANDARD.encode(salt),
        password_verifier: STANDARD.encode(password_verifier(&key)),
        cipher_key: Some(STANDARD.encode(key)),
    };
    save_team_config(paths, &config)?;
    let access = TeamAccess {
        config,
        cipher_key: key,
    };
    save_team_secrets(paths, &access, &TeamSecrets::default())?;
    Ok(access)
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
    let store = load_team_secrets(paths, access)?;
    Ok(store.secrets.keys().cloned().collect())
}

pub(crate) fn get_secret(paths: &AppPaths, team: &str, key: &str) -> Result<String> {
    let (_, cipher_key) = load_team_for_get(paths, team)?;
    let store = load_team_secrets_with_key(paths, team, &cipher_key)?;
    store
        .secrets
        .get(key)
        .cloned()
        .ok_or_else(|| anyhow!("secret not found: {key}"))
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

    let config_path = paths.config_path(team);
    if config_path.exists() {
        fs::remove_file(&config_path)
            .with_context(|| format!("failed to delete {}", config_path.display()))?;
    }
    delete_team_secrets_file(paths, team)?;
    remove_legacy_team_store(paths, team)?;
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

fn load_team_secrets(paths: &AppPaths, access: &TeamAccess) -> Result<TeamSecrets> {
    load_team_secrets_with_key(paths, &access.config.team_name, &access.cipher_key).or_else(|err| {
        if has_legacy_team_store(paths, &access.config.team_name) {
            migrate_legacy_team_store(paths, access)
        } else {
            Err(err)
        }
    })
}

fn load_team_secrets_with_key(
    paths: &AppPaths,
    team: &str,
    cipher_key: &[u8; 32],
) -> Result<TeamSecrets> {
    let Some(store) = load_team_secrets_file(paths, team)? else {
        if has_legacy_team_store(paths, team) {
            bail!("legacy store pending migration for {team}");
        }
        return Ok(TeamSecrets::default());
    };
    decrypt_team_secrets(cipher_key, &store)
}

fn save_team_secrets(paths: &AppPaths, access: &TeamAccess, secrets: &TeamSecrets) -> Result<()> {
    let payload = serde_json::to_string(secrets).context("failed to serialize team secrets")?;
    let (encrypted_payload, nonce) = encrypt_text(&access.cipher_key, &payload)?;
    save_team_secrets_file(
        paths,
        &access.config.team_name,
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

fn migrate_legacy_team_store(paths: &AppPaths, access: &TeamAccess) -> Result<TeamSecrets> {
    let mut secrets = TeamSecrets::default();
    for record in list_legacy_secret_records(paths, &access.config.team_name)? {
        let key = decrypt_text(&access.cipher_key, &record.encrypted_key, &record.key_nonce)?;
        let value = decrypt_text(
            &access.cipher_key,
            &record.encrypted_value,
            &record.value_nonce,
        )?;
        secrets.secrets.insert(key, value);
    }
    save_team_secrets(paths, access, &secrets)?;
    remove_legacy_team_store(paths, &access.config.team_name)?;
    Ok(secrets)
}

fn decode_cipher_key(team: &str, cipher_key: &str) -> Result<[u8; 32]> {
    let raw = STANDARD
        .decode(cipher_key)
        .with_context(|| format!("invalid stored cipher key for {team}"))?;
    raw.try_into()
        .map_err(|_| anyhow!("invalid stored cipher key length for {team}"))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::storage::{SecretRecord, save_team_config};

    fn test_paths() -> AppPaths {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "rupass-tui-ops-test-{}-{suffix}",
            std::process::id()
        ));
        AppPaths::from_dirs(base.join("config"), base.join("store"))
    }

    #[test]
    fn migrates_legacy_team_store_to_single_file() {
        let paths = test_paths();
        paths.ensure_base_dirs().unwrap();

        let salt = [3_u8; 16];
        let key = derive_key("secret", &salt).unwrap();
        save_team_config(
            &paths,
            &TeamConfig {
                team_name: "dev_team".to_string(),
                salt: STANDARD.encode(salt),
                password_verifier: STANDARD.encode(password_verifier(&key)),
                cipher_key: Some(STANDARD.encode(key)),
            },
        )
        .unwrap();

        let legacy_dir = paths.legacy_team_store_dir("dev_team");
        fs::create_dir_all(&legacy_dir).unwrap();
        fs::write(legacy_dir.join("config.json"), b"{}").unwrap();
        fs::write(legacy_dir.join("rupass-team.json"), b"{}").unwrap();

        let (encrypted_key_a, key_nonce_a) = encrypt_text(&key, "db_password").unwrap();
        let (encrypted_value_a, value_nonce_a) = encrypt_text(&key, "hello123").unwrap();
        let (encrypted_key_b, key_nonce_b) = encrypt_text(&key, "api_key").unwrap();
        let (encrypted_value_b, value_nonce_b) = encrypt_text(&key, "secret456").unwrap();

        fs::write(
            legacy_dir
                .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json"),
            serde_json::to_vec_pretty(&SecretRecord {
                encrypted_key: encrypted_key_a,
                encrypted_value: encrypted_value_a,
                key_nonce: key_nonce_a,
                value_nonce: value_nonce_a,
            })
            .unwrap(),
        )
        .unwrap();
        fs::write(
            legacy_dir
                .join("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb.json"),
            serde_json::to_vec_pretty(&SecretRecord {
                encrypted_key: encrypted_key_b,
                encrypted_value: encrypted_value_b,
                key_nonce: key_nonce_b,
                value_nonce: value_nonce_b,
            })
            .unwrap(),
        )
        .unwrap();

        let access = open_team(&paths, "dev_team", None).unwrap();
        let keys = list_keys(&paths, &access).unwrap();

        assert_eq!(keys, vec!["api_key".to_string(), "db_password".to_string()]);
        assert_eq!(
            get_secret_with_access(&paths, &access, "db_password").unwrap(),
            "hello123"
        );
        assert!(paths.team_store_path("dev_team").exists());
        assert!(!paths.legacy_team_store_dir("dev_team").exists());
    }
}
