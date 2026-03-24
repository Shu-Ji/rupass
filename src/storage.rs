use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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
    pub(crate) git_remote: Option<String>,
    pub(crate) s3: Option<TeamS3Config>,
    pub(crate) sync_backend: Option<SyncBackend>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SyncBackend {
    Git,
    S3,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct TeamS3Config {
    pub(crate) endpoint: String,
    pub(crate) region: String,
    pub(crate) bucket: String,
    pub(crate) access_key_id: String,
    pub(crate) secret_access_key: String,
    pub(crate) root: String,
    pub(crate) force_path_style: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct TeamMetadata {
    pub(crate) team_name: String,
    pub(crate) salt: String,
    pub(crate) password_verifier: String,
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
        Ok(())
    }

    pub(crate) fn config_path(&self, team: &str) -> PathBuf {
        self.config_dir.join(format!("{team}.sec"))
    }

    pub(crate) fn team_store_dir(&self, team: &str) -> PathBuf {
        self.store_dir.join(team)
    }

    pub(crate) fn secret_path(&self, team: &str, key: &str) -> PathBuf {
        let digest = hex::encode(Sha256::digest(key.as_bytes()));
        self.team_store_dir(team).join(format!("{digest}.json"))
    }
}

impl TeamConfig {
    pub(crate) fn has_remote(&self) -> bool {
        self.effective_sync_backend().is_some()
    }

    pub(crate) fn effective_sync_backend(&self) -> Option<SyncBackend> {
        match self.sync_backend {
            Some(SyncBackend::Git) if self.git_remote.is_some() => Some(SyncBackend::Git),
            Some(SyncBackend::S3) if self.s3.is_some() => Some(SyncBackend::S3),
            Some(_) => None,
            None => {
                if self.git_remote.is_some() {
                    Some(SyncBackend::Git)
                } else if self.s3.is_some() {
                    Some(SyncBackend::S3)
                } else {
                    None
                }
            }
        }
    }

    pub(crate) fn sync_backend_label(&self) -> &'static str {
        match self.effective_sync_backend() {
            Some(SyncBackend::Git) => "Git",
            Some(SyncBackend::S3) => "S3",
            None => "未选择",
        }
    }
}

impl From<&TeamConfig> for TeamMetadata {
    fn from(config: &TeamConfig) -> Self {
        Self {
            team_name: config.team_name.clone(),
            salt: config.salt.clone(),
            password_verifier: config.password_verifier.clone(),
        }
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
    read_json(&path)
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
        configs.push(read_json(&path)?);
    }

    configs.sort_by(|left, right| left.team_name.cmp(&right.team_name));
    Ok(configs)
}

pub(crate) fn save_secret_record(
    paths: &AppPaths,
    team: &str,
    key: &str,
    record: &SecretRecord,
) -> Result<()> {
    write_json(&paths.secret_path(team, key), record)
}

pub(crate) fn load_secret_record(paths: &AppPaths, team: &str, key: &str) -> Result<SecretRecord> {
    let path = paths.secret_path(team, key);
    if !path.exists() {
        bail!("secret not found: {key}");
    }
    read_json(&path)
}

pub(crate) fn delete_secret_record(paths: &AppPaths, team: &str, key: &str) -> Result<()> {
    let path = paths.secret_path(team, key);
    if !path.exists() {
        bail!("secret not found: {key}");
    }
    fs::remove_file(&path).with_context(|| format!("failed to delete {}", path.display()))?;
    Ok(())
}

pub(crate) fn list_secret_records(paths: &AppPaths, team: &str) -> Result<Vec<SecretRecord>> {
    let team_dir = paths.team_store_dir(team);
    if !team_dir.exists() {
        bail!("team store missing: {team}");
    }

    let mut records = Vec::new();
    for entry in
        fs::read_dir(&team_dir).with_context(|| format!("failed to read {}", team_dir.display()))?
    {
        let path = entry?.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.starts_with('.') {
            continue;
        }
        if !is_secret_record_file_name(file_name) {
            continue;
        }
        records.push(read_json(&path)?);
    }
    Ok(records)
}

fn is_secret_record_file_name(file_name: &str) -> bool {
    let Some(stem) = file_name.strip_suffix(".json") else {
        return false;
    };
    stem.len() == 64 && stem.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let content = serde_json::to_vec_pretty(value).context("failed to serialize json")?;
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let content = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&content).with_context(|| format!("failed to parse {}", path.display()))
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
    fn list_secret_records_ignores_hidden_json_files() {
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
        let team_dir = paths.team_store_dir("dev_team");
        fs::create_dir_all(&team_dir).unwrap();
        fs::write(
            team_dir.join("rupass-team.json"),
            br#"{"team_name":"dev_team"}"#,
        )
        .unwrap();
        fs::write(
            team_dir.join("config.json"),
            br#"{"theme":"dark"}"#,
        )
        .unwrap();
        fs::write(
            team_dir.join(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json",
            ),
            br#"{
  "encrypted_key":"k",
  "encrypted_value":"v",
  "key_nonce":"n1",
  "value_nonce":"n2"
}"#,
        )
        .unwrap();

        let records = list_secret_records(&paths, "dev_team").unwrap();

        assert_eq!(records.len(), 1);
    }
}
