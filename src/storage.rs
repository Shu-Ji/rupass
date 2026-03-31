use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub(crate) struct AppPaths {
    config_dir: PathBuf,
    store_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct TeamConfig {
    pub(crate) team_name: String,
    pub(crate) salt: String,
    pub(crate) password_verifier: String,
    pub(crate) cipher_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub(crate) struct TeamSecrets {
    pub(crate) secrets: BTreeMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct EncryptedTeamSecrets {
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

impl AppPaths {
    pub(crate) fn resolve() -> Result<Self> {
        let home = dirs::home_dir().context("failed to locate home directory")?;
        let base_dir = home.join(".rupass");
        Ok(Self {
            config_dir: base_dir.join("config"),
            store_dir: base_dir.join("store"),
        })
    }

    pub(crate) fn ensure_base_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.config_dir)
            .with_context(|| format!("failed to create {}", self.config_dir.display()))?;
        fs::create_dir_all(&self.store_dir)
            .with_context(|| format!("failed to create {}", self.store_dir.display()))?;
        self.cleanup_legacy_state_dir()?;
        Ok(())
    }

    pub(crate) fn config_path(&self, team: &str) -> PathBuf {
        self.config_dir.join(format!("{team}.sec"))
    }

    pub(crate) fn team_store_path(&self, team: &str) -> PathBuf {
        self.store_dir.join(format!("{team}.json"))
    }

    pub(crate) fn legacy_team_store_dir(&self, team: &str) -> PathBuf {
        self.store_dir.join(team)
    }

    fn state_dir(&self) -> PathBuf {
        self.config_dir
            .parent()
            .unwrap_or(self.config_dir.as_path())
            .join("state")
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
}

#[cfg(test)]
impl AppPaths {
    pub(crate) fn from_dirs(config_dir: PathBuf, store_dir: PathBuf) -> Self {
        Self {
            config_dir,
            store_dir,
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
    let path = paths.config_path(team);
    if !path.exists() {
        bail!("team not initialized: {team}. run `rupass init` or `rupass team create` first");
    }
    let config: TeamConfig = read_json(&path)?;
    normalize_team_config_file(paths, &config)?;
    Ok(config)
}

pub(crate) fn save_team_config(paths: &AppPaths, config: &TeamConfig) -> Result<()> {
    write_json(&paths.config_path(&config.team_name), config)
}

pub(crate) fn list_team_configs(paths: &AppPaths) -> Result<Vec<TeamConfig>> {
    let mut configs: Vec<TeamConfig> = Vec::new();
    for entry in fs::read_dir(&paths.config_dir)
        .with_context(|| format!("failed to read {}", paths.config_dir.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("sec") {
            continue;
        }
        let config: TeamConfig = read_json(&path)?;
        normalize_team_config_file(paths, &config)?;
        configs.push(config);
    }

    configs.sort_by(|left, right| left.team_name.cmp(&right.team_name));
    Ok(configs)
}

pub(crate) fn load_team_secrets_file(
    paths: &AppPaths,
    team: &str,
) -> Result<Option<EncryptedTeamSecrets>> {
    let path = paths.team_store_path(team);
    if !path.exists() {
        return Ok(None);
    }
    read_json(&path).map(Some)
}

pub(crate) fn save_team_secrets_file(
    paths: &AppPaths,
    team: &str,
    secrets: &EncryptedTeamSecrets,
) -> Result<()> {
    write_json(&paths.team_store_path(team), secrets)
}

pub(crate) fn delete_team_secrets_file(paths: &AppPaths, team: &str) -> Result<()> {
    let path = paths.team_store_path(team);
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
    let content = canonical_json_bytes(value)?;
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let content = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&content).with_context(|| format!("failed to parse {}", path.display()))
}

fn normalize_team_config_file(paths: &AppPaths, config: &TeamConfig) -> Result<()> {
    let path = paths.config_path(&config.team_name);
    let current = fs::read(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let canonical = canonical_json_bytes(config)?;
    if current != canonical {
        fs::write(&path, canonical)
            .with_context(|| format!("failed to write {}", path.display()))?;
    }
    Ok(())
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value).context("failed to serialize json")
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
        let paths = AppPaths::from_dirs(base.join("config"), base.join("store"));
        fs::create_dir_all(base.join("state").join("s3")).unwrap();
        fs::write(base.join("state").join("s3").join("dev_team.json"), b"{}").unwrap();

        paths.ensure_base_dirs().unwrap();

        assert!(!base.join("state").join("s3").exists());
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
        let paths = AppPaths::from_dirs(base.join("config"), base.join("store"));
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
    fn normalizes_legacy_team_config_file() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "rupass-storage-test-{}-{suffix}",
            std::process::id()
        ));
        let paths = AppPaths::from_dirs(base.join("config"), base.join("store"));
        paths.ensure_base_dirs().unwrap();
        fs::write(
            paths.config_path("dev_team"),
            br#"{
  "team_name": "dev_team",
  "salt": "salt",
  "password_verifier": "verifier",
  "cipher_key": "cipher",
  "git_remote": "git@example.com:repo.git",
  "s3": {"bucket":"demo"},
  "sync_backend": "s3"
}"#,
        )
        .unwrap();

        let config = load_team_config(&paths, "dev_team").unwrap();
        let normalized = fs::read_to_string(paths.config_path("dev_team")).unwrap();

        assert_eq!(config.team_name, "dev_team");
        assert!(!normalized.contains("git_remote"));
        assert!(!normalized.contains("sync_backend"));
        assert!(!normalized.contains("\"s3\""));
    }
}
