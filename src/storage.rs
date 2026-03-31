use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};

use crate::crypto::encrypt_text;

#[derive(Debug, Clone)]
pub(crate) struct AppPaths {
    private_dir: PathBuf,
    public_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct TeamConfig {
    pub(crate) team_name: String,
    pub(crate) salt: String,
    pub(crate) password_verifier: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct TeamKeyCache {
    pub(crate) cipher_key: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct TeamSecrets {
    pub(crate) secrets: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct EncryptedTeamSecrets {
    pub(crate) encrypted_payload: String,
    pub(crate) nonce: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct TeamFile {
    pub(crate) team_name: String,
    pub(crate) salt: String,
    pub(crate) password_verifier: String,
    pub(crate) encrypted_payload: String,
    pub(crate) nonce: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct SecretRecord {
    pub(crate) encrypted_key: String,
    pub(crate) encrypted_value: String,
    pub(crate) key_nonce: String,
    pub(crate) value_nonce: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LegacyTeamConfig {
    team_name: String,
    salt: String,
    password_verifier: String,
    cipher_key: Option<String>,
}

impl AppPaths {
    pub(crate) fn resolve() -> Result<Self> {
        let home = dirs::home_dir().context("failed to locate home directory")?;
        let base_dir = home.join(".rupass");
        Ok(Self {
            private_dir: base_dir.join("privite"),
            public_dir: base_dir.join("public"),
        })
    }

    pub(crate) fn ensure_base_dirs(&self) -> Result<()> {
        self.migrate_legacy_base_dirs()?;
        fs::create_dir_all(&self.private_dir)
            .with_context(|| format!("failed to create {}", self.private_dir.display()))?;
        fs::create_dir_all(&self.public_dir)
            .with_context(|| format!("failed to create {}", self.public_dir.display()))?;
        self.cleanup_legacy_state_dir()?;
        self.migrate_legacy_team_files()?;
        Ok(())
    }

    pub(crate) fn team_file_path(&self, team: &str) -> PathBuf {
        self.public_dir.join(format!("{team}.json"))
    }

    pub(crate) fn key_cache_path(&self, team: &str) -> PathBuf {
        self.private_dir.join(format!("{team}.key"))
    }

    #[cfg(test)]
    pub(crate) fn legacy_config_path(&self, team: &str) -> PathBuf {
        self.private_dir.join(format!("{team}.sec"))
    }

    pub(crate) fn legacy_team_store_dir(&self, team: &str) -> PathBuf {
        self.public_dir.join(team)
    }

    fn base_dir(&self) -> &Path {
        self.private_dir
            .parent()
            .unwrap_or(self.private_dir.as_path())
    }

    fn state_dir(&self) -> PathBuf {
        self.base_dir().join("state")
    }

    fn legacy_private_dir(&self) -> PathBuf {
        self.base_dir().join("config")
    }

    fn legacy_public_dir(&self) -> PathBuf {
        self.base_dir().join("store")
    }

    fn cleanup_legacy_state_dir(&self) -> Result<()> {
        let legacy_s3_dir = self.state_dir().join("s3");
        if legacy_s3_dir.exists() {
            fs::remove_dir_all(&legacy_s3_dir)
                .with_context(|| format!("failed to delete {}", legacy_s3_dir.display()))?;
        }
        let state_dir = self.state_dir();
        if state_dir.exists()
            && fs::read_dir(&state_dir)
                .with_context(|| format!("failed to read {}", state_dir.display()))?
                .next()
                .is_none()
        {
            fs::remove_dir(&state_dir)
                .with_context(|| format!("failed to delete {}", state_dir.display()))?;
        }
        Ok(())
    }

    fn migrate_legacy_base_dirs(&self) -> Result<()> {
        migrate_dir_if_needed(&self.legacy_private_dir(), &self.private_dir)?;
        migrate_dir_if_needed(&self.legacy_public_dir(), &self.public_dir)?;
        Ok(())
    }

    fn migrate_legacy_team_files(&self) -> Result<()> {
        if !self.private_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&self.private_dir)
            .with_context(|| format!("failed to read {}", self.private_dir.display()))?
        {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("sec") {
                continue;
            }

            let legacy: LegacyTeamConfig = read_json(&path)?;
            validate_team_name(&legacy.team_name)?;

            let needs_public_migration = if self.team_file_path(&legacy.team_name).exists() {
                read_json::<TeamFile>(&self.team_file_path(&legacy.team_name)).is_err()
            } else {
                true
            };

            if needs_public_migration {
                let secrets = self
                    .load_legacy_public_file(&legacy.team_name)?
                    .map(Ok)
                    .unwrap_or_else(|| {
                        empty_encrypted_team_secrets(legacy.cipher_key.as_deref())
                    })?;
                save_team_file(
                    self,
                    &TeamConfig {
                        team_name: legacy.team_name.clone(),
                        salt: legacy.salt.clone(),
                        password_verifier: legacy.password_verifier.clone(),
                    },
                    &secrets,
                )?;
            }

            if let Some(cipher_key) = legacy.cipher_key {
                save_key_cache(self, &legacy.team_name, &TeamKeyCache { cipher_key })?;
            }

            fs::remove_file(&path)
                .with_context(|| format!("failed to delete {}", path.display()))?;
        }

        Ok(())
    }

    fn load_legacy_public_file(&self, team: &str) -> Result<Option<EncryptedTeamSecrets>> {
        let path = self.team_file_path(team);
        if !path.exists() {
            return Ok(None);
        }

        if read_json::<TeamFile>(&path).is_ok() {
            return Ok(None);
        }

        let legacy: EncryptedTeamSecrets = read_json(&path)?;
        Ok(Some(legacy))
    }
}

#[cfg(test)]
impl AppPaths {
    pub(crate) fn from_dirs(private_dir: PathBuf, public_dir: PathBuf) -> Self {
        Self {
            private_dir,
            public_dir,
        }
    }
}

pub(crate) fn validate_team_name(team: &str) -> Result<()> {
    if team.is_empty() {
        bail!("team name cannot be empty");
    }
    if !team.ends_with("_team") {
        bail!("team name must end with `_team`");
    }
    if !team
        .chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
    {
        bail!("team name must use lowercase letters, digits, or `_`");
    }
    Ok(())
}

pub(crate) fn load_team_config(paths: &AppPaths, team: &str) -> Result<TeamConfig> {
    validate_team_name(team)?;
    let path = paths.team_file_path(team);
    if !path.exists() {
        bail!("team not initialized: {team}. run `rupass tui` or `rupass team import-file` first");
    }
    let team_file: TeamFile = read_json(&path)?;
    Ok(TeamConfig {
        team_name: team_file.team_name,
        salt: team_file.salt,
        password_verifier: team_file.password_verifier,
    })
}

pub(crate) fn list_team_configs(paths: &AppPaths) -> Result<Vec<TeamConfig>> {
    if !paths.public_dir.exists() {
        return Ok(Vec::new());
    }

    let mut configs = Vec::new();
    for entry in fs::read_dir(&paths.public_dir)
        .with_context(|| format!("failed to read {}", paths.public_dir.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(team_file) = read_json::<TeamFile>(&path) else {
            continue;
        };
        configs.push(TeamConfig {
            team_name: team_file.team_name,
            salt: team_file.salt,
            password_verifier: team_file.password_verifier,
        });
    }

    configs.sort_by(|left, right| left.team_name.cmp(&right.team_name));
    Ok(configs)
}

pub(crate) fn load_team_secrets_file(
    paths: &AppPaths,
    team: &str,
) -> Result<Option<EncryptedTeamSecrets>> {
    let path = paths.team_file_path(team);
    if !path.exists() {
        return Ok(None);
    }
    let team_file: TeamFile = read_json(&path)?;
    Ok(Some(EncryptedTeamSecrets {
        encrypted_payload: team_file.encrypted_payload,
        nonce: team_file.nonce,
    }))
}

pub(crate) fn save_team_file(
    paths: &AppPaths,
    config: &TeamConfig,
    secrets: &EncryptedTeamSecrets,
) -> Result<()> {
    write_json(
        &paths.team_file_path(&config.team_name),
        &TeamFile {
            team_name: config.team_name.clone(),
            salt: config.salt.clone(),
            password_verifier: config.password_verifier.clone(),
            encrypted_payload: secrets.encrypted_payload.clone(),
            nonce: secrets.nonce.clone(),
        },
    )
}

pub(crate) fn load_team_file_from_path(path: &Path) -> Result<TeamFile> {
    let team_file: TeamFile = read_json(path)?;
    validate_team_name(&team_file.team_name)?;
    Ok(team_file)
}

pub(crate) fn copy_team_file_into_public(paths: &AppPaths, team_file: &TeamFile) -> Result<()> {
    write_json(&paths.team_file_path(&team_file.team_name), team_file)
}

pub(crate) fn delete_team_file(paths: &AppPaths, team: &str) -> Result<()> {
    let path = paths.team_file_path(team);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("failed to delete {}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn load_key_cache(paths: &AppPaths, team: &str) -> Result<Option<TeamKeyCache>> {
    let path = paths.key_cache_path(team);
    if !path.exists() {
        return Ok(None);
    }
    read_json(&path).map(Some)
}

pub(crate) fn save_key_cache(paths: &AppPaths, team: &str, cache: &TeamKeyCache) -> Result<()> {
    write_json(&paths.key_cache_path(team), cache)
}

pub(crate) fn delete_key_cache(paths: &AppPaths, team: &str) -> Result<()> {
    let path = paths.key_cache_path(team);
    if path.exists() {
        fs::remove_file(&path).with_context(|| format!("failed to delete {}", path.display()))?;
    }
    Ok(())
}

pub(crate) fn has_legacy_team_store(paths: &AppPaths, team: &str) -> bool {
    paths.legacy_team_store_dir(team).exists()
}

pub(crate) fn list_legacy_secret_records(
    paths: &AppPaths,
    team: &str,
) -> Result<Vec<SecretRecord>> {
    let team_dir = paths.legacy_team_store_dir(team);
    if !team_dir.exists() {
        return Ok(Vec::new());
    }

    let mut records = Vec::new();
    for entry in
        fs::read_dir(&team_dir).with_context(|| format!("failed to read {}", team_dir.display()))?
    {
        let path = entry?.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.starts_with('.') || !is_legacy_secret_record_file_name(file_name) {
            continue;
        }
        records.push(read_json(&path)?);
    }
    Ok(records)
}

pub(crate) fn remove_legacy_team_store(paths: &AppPaths, team: &str) -> Result<()> {
    let path = paths.legacy_team_store_dir(team);
    if path.exists() {
        fs::remove_dir_all(&path)
            .with_context(|| format!("failed to delete {}", path.display()))?;
    }
    Ok(())
}

fn empty_encrypted_team_secrets(cipher_key: Option<&str>) -> Result<EncryptedTeamSecrets> {
    let Some(cipher_key) = cipher_key else {
        bail!("legacy team is missing cipher_key cache");
    };
    let raw = STANDARD
        .decode(cipher_key)
        .context("invalid stored cipher key for legacy team")?;
    let key: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow!("invalid stored cipher key length for legacy team"))?;
    let payload = serde_json::to_string(&TeamSecrets::default())
        .context("failed to serialize empty team secrets")?;
    let (encrypted_payload, nonce) = encrypt_text(&key, &payload)?;
    Ok(EncryptedTeamSecrets {
        encrypted_payload,
        nonce,
    })
}

fn is_legacy_secret_record_file_name(file_name: &str) -> bool {
    let Some(stem) = file_name.strip_suffix(".json") else {
        return false;
    };
    stem.len() == 64 && stem.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let content = serde_json::to_vec_pretty(value).context("failed to serialize json")?;
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let content = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&content).with_context(|| format!("failed to parse {}", path.display()))
}

fn migrate_dir_if_needed(from: &Path, to: &Path) -> Result<()> {
    if !from.exists() {
        return Ok(());
    }
    if !to.exists() {
        fs::rename(from, to)
            .with_context(|| format!("failed to move {} to {}", from.display(), to.display()))?;
        return Ok(());
    }

    for entry in fs::read_dir(from).with_context(|| format!("failed to read {}", from.display()))? {
        let entry = entry?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        if target.exists() {
            continue;
        }
        fs::rename(&source, &target).with_context(|| {
            format!(
                "failed to move {} to {}",
                source.display(),
                target.display()
            )
        })?;
    }

    if fs::read_dir(from)
        .with_context(|| format!("failed to read {}", from.display()))?
        .next()
        .is_none()
    {
        fs::remove_dir(from).with_context(|| format!("failed to delete {}", from.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn team_name_must_end_with_team() {
        assert!(validate_team_name("dev_team").is_ok());
        assert!(validate_team_name("default").is_err());
        assert!(validate_team_name("Default_team").is_err());
    }

    #[test]
    fn removes_legacy_s3_state_dir() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "rupass-storage-test-{}-{suffix}",
            std::process::id()
        ));
        let paths = AppPaths::from_dirs(base.join("privite"), base.join("public"));
        fs::create_dir_all(base.join("state").join("s3")).unwrap();
        fs::write(base.join("state").join("s3").join("dev_team.json"), b"{}").unwrap();

        paths.ensure_base_dirs().unwrap();

        assert!(!base.join("state").join("s3").exists());
    }

    #[test]
    fn migrates_legacy_config_and_store_dirs() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "rupass-storage-test-{}-{suffix}",
            std::process::id()
        ));
        let paths = AppPaths::from_dirs(base.join("privite"), base.join("public"));
        fs::create_dir_all(base.join("config")).unwrap();
        fs::create_dir_all(base.join("store")).unwrap();
        fs::write(
            base.join("config").join("dev_team.sec"),
            br#"{"team_name":"dev_team","salt":"s","password_verifier":"p","cipher_key":"c2VjcmV0"}"#,
        )
        .unwrap();
        fs::write(
            base.join("store").join("dev_team.json"),
            br#"{"encrypted_payload":"x","nonce":"y"}"#,
        )
        .unwrap();

        let _ = paths.ensure_base_dirs();

        assert!(base.join("privite").exists());
        assert!(base.join("public").exists());
        assert!(!base.join("config").exists());
        assert!(!base.join("store").exists());
    }

    #[test]
    fn lists_only_legacy_secret_record_files() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "rupass-storage-test-{}-{suffix}",
            std::process::id()
        ));
        let paths = AppPaths::from_dirs(base.join("privite"), base.join("public"));
        paths.ensure_base_dirs().unwrap();
        let team_dir = paths.legacy_team_store_dir("dev_team");
        fs::create_dir_all(&team_dir).unwrap();
        fs::write(
            team_dir.join("rupass-team.json"),
            br#"{"team_name":"dev_team"}"#,
        )
        .unwrap();
        fs::write(team_dir.join("config.json"), br#"{"theme":"dark"}"#).unwrap();
        fs::write(
            team_dir.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json"),
            br#"{
  "encrypted_key":"k",
  "encrypted_value":"v",
  "key_nonce":"n1",
  "value_nonce":"n2"
}"#,
        )
        .unwrap();

        let records = list_legacy_secret_records(&paths, "dev_team").unwrap();

        assert_eq!(records.len(), 1);
    }

    #[test]
    fn migrates_legacy_sec_into_team_file_and_key_cache() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "rupass-storage-test-{}-{suffix}",
            std::process::id()
        ));
        let paths = AppPaths::from_dirs(base.join("privite"), base.join("public"));
        fs::create_dir_all(&paths.private_dir).unwrap();
        fs::create_dir_all(&paths.public_dir).unwrap();
        fs::write(
            paths.legacy_config_path("dev_team"),
            br#"{
  "team_name": "dev_team",
  "salt": "salt",
  "password_verifier": "verifier",
  "cipher_key": "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE=",
  "sync_backend": "s3"
}"#,
        )
        .unwrap();
        fs::write(
            paths.team_file_path("dev_team"),
            br#"{"encrypted_payload":"payload","nonce":"nonce"}"#,
        )
        .unwrap();

        paths.ensure_base_dirs().unwrap();

        let config = load_team_config(&paths, "dev_team").unwrap();
        let cache = load_key_cache(&paths, "dev_team").unwrap().unwrap();
        let team_file = load_team_file_from_path(&paths.team_file_path("dev_team")).unwrap();

        assert_eq!(config.team_name, "dev_team");
        assert_eq!(
            cache.cipher_key,
            "YWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWFhYWE="
        );
        assert_eq!(team_file.encrypted_payload, "payload");
        assert!(!paths.legacy_config_path("dev_team").exists());
    }
}
