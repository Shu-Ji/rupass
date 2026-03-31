use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

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
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFile {
    pub(crate) salt: String,
    pub(crate) password_verifier: String,
    pub(crate) encrypted_payload: String,
    pub(crate) nonce: String,
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
        fs::create_dir_all(&self.private_dir)
            .with_context(|| format!("failed to create {}", self.private_dir.display()))?;
        fs::create_dir_all(&self.public_dir)
            .with_context(|| format!("failed to create {}", self.public_dir.display()))?;
        Ok(())
    }

    pub(crate) fn team_file_path(&self, team: &str) -> PathBuf {
        self.public_dir.join(format!("{team}.json"))
    }

    pub(crate) fn key_cache_path(&self, team: &str) -> PathBuf {
        self.private_dir.join(format!("{team}.key"))
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
    let (team_name, team_file) = load_team_file_from_path(&path)?;
    Ok(TeamConfig {
        team_name,
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
        let Ok((team_name, team_file)) = load_team_file_from_path(&path) else {
            continue;
        };
        configs.push(TeamConfig {
            team_name,
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
    let (_, team_file) = load_team_file_from_path(&path)?;
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
            salt: config.salt.clone(),
            password_verifier: config.password_verifier.clone(),
            encrypted_payload: secrets.encrypted_payload.clone(),
            nonce: secrets.nonce.clone(),
        },
    )
}

pub(crate) fn load_team_file_from_path(path: &Path) -> Result<(String, TeamFile)> {
    let team_name = team_name_from_path(path)?;
    let team_file = read_json(path)?;
    Ok((team_name, team_file))
}

pub(crate) fn copy_team_file_into_public(
    paths: &AppPaths,
    team_name: &str,
    team_file: &TeamFile,
) -> Result<()> {
    write_json(&paths.team_file_path(team_name), team_file)
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

fn team_name_from_path(path: &Path) -> Result<String> {
    let team_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("invalid team file name: {}", path.display()))?;
    validate_team_name(team_name)?;
    Ok(team_name.to_string())
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
    fn ensure_base_dirs_creates_privite_and_public() {
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

        assert!(base.join("privite").exists());
        assert!(base.join("public").exists());
    }

    #[test]
    fn load_team_file_uses_file_name_as_team_name() {
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
        fs::write(
            paths.team_file_path("dev_team"),
            br#"{"salt":"s","password_verifier":"p","encrypted_payload":"x","nonce":"y"}"#,
        )
        .unwrap();

        let config = load_team_config(&paths, "dev_team").unwrap();
        let (team_name, team_file) =
            load_team_file_from_path(&paths.team_file_path("dev_team")).unwrap();

        assert_eq!(config.team_name, "dev_team");
        assert_eq!(team_name, "dev_team");
        assert_eq!(team_file.encrypted_payload, "x");
    }
}
