use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const DEFAULT_TEAM_NAME: &str = "default_team";
pub(crate) const DEFAULT_TEAM_DISPLAY_NAME: &str = "默认团队";

#[derive(Debug, Clone)]
pub(crate) struct AppPaths {
    config_dir: PathBuf,
    store_dir: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct TeamConfig {
    pub(crate) team_name: String,
    pub(crate) display_name: String,
    pub(crate) salt: String,
    pub(crate) password_verifier: String,
    pub(crate) cipher_key: Option<String>,
    pub(crate) git_remote: Option<String>,
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
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        records.push(read_json(&path)?);
    }
    Ok(records)
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
}
