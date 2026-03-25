use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, UtcOffset};

use crate::storage::{TeamConfig, TeamMetadata};

const TEAM_METADATA_FILE: &str = "rupass-team.json";
const LEGACY_TEAM_METADATA_FILE: &str = ".rupass-team.json";
const LOCAL_ONLY_FILES: &[&str] = &[
    "rupass-s3-state.json",
    ".rupass-s3-state.json",
    "rupass-manifest.json",
    ".rupass-manifest.json",
];
const CHINA_OFFSET_HOURS: i8 = 8;
const CHINA_DATETIME_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

pub(crate) fn ensure_git_repo(repo_dir: &Path) -> Result<()> {
    if repo_dir.join(".git").exists() {
        ensure_git_local_excludes(repo_dir)?;
        return Ok(());
    }

    fs::create_dir_all(repo_dir)
        .with_context(|| format!("failed to create repo dir {}", repo_dir.display()))?;
    run_git(repo_dir, &["init", "-b", "main"])?;
    ensure_git_local_excludes(repo_dir)?;
    Ok(())
}

pub(crate) fn bootstrap_team_repo(repo_dir: &Path, remote: &str) -> Result<()> {
    ensure_git_repo(repo_dir)?;
    ensure_git_remote(repo_dir, remote)?;
    bootstrap_from_remote_if_needed(repo_dir)?;
    Ok(())
}

pub(crate) fn load_team_metadata(repo_dir: &Path) -> Result<TeamMetadata> {
    let path = migrate_legacy_team_metadata(repo_dir)?;
    if !path.exists() {
        bail!(
            "missing remote team metadata: {}\n请在旧机器上升级到最新 rupass 后执行一次同步，再重试导入",
            team_metadata_path(repo_dir).display()
        );
    }

    read_json(&path)
}

pub(crate) fn sync_team_repo(repo_dir: &Path, config: &TeamConfig) -> Result<()> {
    let _lock = acquire_sync_lock(repo_dir)?;
    ensure_git_repo(repo_dir)?;
    cleanup_local_only_files(repo_dir)?;

    if let Some(remote) = &config.git_remote {
        bootstrap_team_repo(repo_dir, remote)?;
        sync_team_metadata(repo_dir, config)?;
    }

    let has_changes = repo_has_changes(repo_dir)?;
    if has_changes {
        run_git(repo_dir, &["add", "."])?;
        run_git(
            repo_dir,
            &[
                "commit",
                "-m",
                &format!("rupass sync {}", china_standard_time_now()?),
            ],
        )?;
    }

    if config.git_remote.is_some() {
        let remote_has_main = remote_has_main_branch(repo_dir)?;
        if remote_has_main
            && has_local_commits(repo_dir)?
            && let Err(err) = run_git(repo_dir, &["pull", "--rebase", "origin", "main"])
        {
            bail!("{}", format_sync_error(repo_dir, &err.to_string()));
        }

        if has_local_commits(repo_dir)?
            && let Err(err) = run_git(repo_dir, &["push", "-u", "origin", "main"])
        {
            bail!("{}", format_sync_error(repo_dir, &err.to_string()));
        }
    }

    Ok(())
}

fn sync_team_metadata(repo_dir: &Path, config: &TeamConfig) -> Result<()> {
    let path = team_metadata_path(repo_dir);
    let expected = TeamMetadata::from(config);
    let legacy_path = legacy_team_metadata_path(repo_dir);

    if path.exists() {
        let actual: TeamMetadata = read_json(&path)?;
        if actual != expected {
            bail!(
                "remote team metadata does not match local config\nrepo: {}\nmetadata: {}",
                repo_dir.display(),
                path.display()
            );
        }
        return Ok(());
    }

    if legacy_path.exists() {
        let actual: TeamMetadata = read_json(&legacy_path)?;
        if actual != expected {
            bail!(
                "remote team metadata does not match local config\nrepo: {}\nmetadata: {}",
                repo_dir.display(),
                legacy_path.display()
            );
        }
        fs::rename(&legacy_path, &path).with_context(|| {
            format!(
                "failed to migrate team metadata from {} to {}",
                legacy_path.display(),
                path.display()
            )
        })?;
        return Ok(());
    }

    write_json(&path, &expected)
}

#[derive(Debug)]
struct SyncLock {
    path: std::path::PathBuf,
}

impl Drop for SyncLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn ensure_git_remote(repo_dir: &Path, remote: &str) -> Result<()> {
    let remotes = run_git(repo_dir, &["remote"]).unwrap_or_default();
    let has_origin = remotes.lines().any(|line| line.trim() == "origin");

    if has_origin {
        run_git(repo_dir, &["remote", "set-url", "origin", remote])?;
    } else {
        run_git(repo_dir, &["remote", "add", "origin", remote])?;
    }

    Ok(())
}

fn bootstrap_from_remote_if_needed(repo_dir: &Path) -> Result<()> {
    if has_local_commits(repo_dir)? || !remote_has_main_branch(repo_dir)? {
        return Ok(());
    }

    run_git(repo_dir, &["fetch", "origin"])?;
    run_git(repo_dir, &["checkout", "-B", "main"])?;
    run_git(repo_dir, &["pull", "--rebase", "origin", "main"])?;
    Ok(())
}

fn repo_has_changes(repo_dir: &Path) -> Result<bool> {
    Ok(!run_git(repo_dir, &["status", "--porcelain"])?
        .trim()
        .is_empty())
}

fn has_local_commits(repo_dir: &Path) -> Result<bool> {
    Ok(run_git(repo_dir, &["rev-parse", "--verify", "HEAD"]).is_ok())
}

fn team_metadata_path(repo_dir: &Path) -> PathBuf {
    repo_dir.join(TEAM_METADATA_FILE)
}

fn legacy_team_metadata_path(repo_dir: &Path) -> PathBuf {
    repo_dir.join(LEGACY_TEAM_METADATA_FILE)
}

fn migrate_legacy_team_metadata(repo_dir: &Path) -> Result<PathBuf> {
    let path = team_metadata_path(repo_dir);
    if path.exists() {
        return Ok(path);
    }

    let legacy_path = legacy_team_metadata_path(repo_dir);
    if legacy_path.exists() {
        fs::rename(&legacy_path, &path).with_context(|| {
            format!(
                "failed to migrate team metadata from {} to {}",
                legacy_path.display(),
                path.display()
            )
        })?;
    }
    Ok(path)
}

fn remote_has_main_branch(repo_dir: &Path) -> Result<bool> {
    Ok(
        !run_git(repo_dir, &["ls-remote", "--heads", "origin", "main"])?
            .trim()
            .is_empty(),
    )
}

fn run_git(repo_dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stderr.is_empty() { stdout } else { stderr };
        bail!("git {} failed: {}", args.join(" "), message);
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn china_standard_time_now() -> Result<String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_secs() as i64;
    format_china_standard_time(timestamp)
}

fn format_china_standard_time(unix_timestamp: i64) -> Result<String> {
    let offset = UtcOffset::from_hms(CHINA_OFFSET_HOURS, 0, 0)
        .context("failed to build china standard time offset")?;
    OffsetDateTime::from_unix_timestamp(unix_timestamp)
        .context("invalid unix timestamp")?
        .to_offset(offset)
        .format(CHINA_DATETIME_FORMAT)
        .context("failed to format china standard time")
}

fn acquire_sync_lock(repo_dir: &Path) -> Result<SyncLock> {
    let lock_path = sync_lock_path(repo_dir)?;
    if let Some(parent) = lock_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create sync lock dir {}", parent.display()))?;
    }

    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(mut file) => {
            let _ = writeln!(file, "pid={}", std::process::id());
            Ok(SyncLock { path: lock_path })
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => bail!(
            "another sync is already running\nrepo: {}\nlock: {}\n请等待当前同步完成后再试",
            repo_dir.display(),
            lock_path.display()
        ),
        Err(err) => {
            Err(err).with_context(|| format!("failed to create sync lock {}", lock_path.display()))
        }
    }
}

fn cleanup_local_only_files(repo_dir: &Path) -> Result<()> {
    for file_name in LOCAL_ONLY_FILES {
        let path = repo_dir.join(file_name);
        if matches!(
            path.file_name().and_then(|name| name.to_str()),
            Some("rupass-manifest.json" | ".rupass-manifest.json")
        ) && path.exists()
        {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove local-only file {}", path.display()))?;
        }
    }

    for file_name in LOCAL_ONLY_FILES {
        let _ = run_git(
            repo_dir,
            &["rm", "--cached", "--ignore-unmatch", "--", file_name],
        );
    }

    Ok(())
}

pub(crate) fn ensure_git_local_excludes(repo_dir: &Path) -> Result<()> {
    let git_dir = repo_dir.join(".git");
    if !git_dir.exists() {
        return Ok(());
    }
    let exclude_path = repo_dir.join(".git").join("info").join("exclude");
    let existing = if exclude_path.exists() {
        fs::read_to_string(&exclude_path)
            .with_context(|| format!("failed to read {}", exclude_path.display()))?
    } else {
        String::new()
    };

    let mut content = existing;
    let mut changed = false;
    for file_name in LOCAL_ONLY_FILES {
        if !content.lines().any(|line| line.trim() == *file_name) {
            if !content.ends_with('\n') && !content.is_empty() {
                content.push('\n');
            }
            content.push_str(file_name);
            content.push('\n');
            changed = true;
        }
    }

    if changed {
        fs::write(&exclude_path, content)
            .with_context(|| format!("failed to write {}", exclude_path.display()))?;
    }
    Ok(())
}

fn sync_lock_path(repo_dir: &Path) -> Result<std::path::PathBuf> {
    let Some(store_dir) = repo_dir.parent() else {
        bail!("invalid repo dir: {}", repo_dir.display());
    };
    let Some(base_dir) = store_dir.parent() else {
        bail!("invalid repo dir: {}", repo_dir.display());
    };
    Ok(base_dir.join("sync.lock"))
}

fn format_sync_error(repo_dir: &Path, message: &str) -> String {
    if is_rebase_conflict(message) {
        let conflicts = conflict_paths(repo_dir).unwrap_or_default();
        let conflict_hint = if conflicts.is_empty() {
            "冲突文件请运行 `git status` 查看。".to_string()
        } else {
            format!("冲突文件: {}", conflicts.join(", "))
        };
        return format!(
            "sync failed due to git conflict: {message}\n\
             repo: {}\n\
             {conflict_hint}\n\
             处理建议:\n\
             1. 进入该目录后运行 `git status`\n\
             2. 解决冲突并 `git add <files>`\n\
             3. 继续执行 `git rebase --continue`\n\
             4. 如果想放弃这次同步，执行 `git rebase --abort`",
            repo_dir.display()
        );
    }

    format!("sync failed: {message}\nrepo: {}", repo_dir.display())
}

fn is_rebase_conflict(message: &str) -> bool {
    message.contains("could not apply")
        || message.contains("has conflicts")
        || message.contains("Merge conflict")
        || message.contains("CONFLICT")
}

fn conflict_paths(repo_dir: &Path) -> Result<Vec<String>> {
    Ok(parse_conflict_paths(&run_git(
        repo_dir,
        &["status", "--porcelain"],
    )?))
}

fn parse_conflict_paths(status: &str) -> Vec<String> {
    status
        .lines()
        .filter_map(|line| {
            if line.len() < 4 {
                return None;
            }
            let code = &line[..2];
            let is_conflict = matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU");
            if !is_conflict {
                return None;
            }
            Some(line[3..].trim().to_string())
        })
        .collect()
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let content = serde_json::to_vec_pretty(value).context("failed to serialize json")?;
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn read_json<T: for<'de> serde::Deserialize<'de>>(path: &Path) -> Result<T> {
    let content = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_slice(&content).with_context(|| format!("failed to parse {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_conflict_paths_from_git_status() {
        let status = "\
UU store/a.json\n\
M  store/b.json\n\
AA store/c.json\n";
        assert_eq!(
            parse_conflict_paths(status),
            vec!["store/a.json".to_string(), "store/c.json".to_string()]
        );
    }

    #[test]
    fn detects_rebase_conflict_messages() {
        assert!(is_rebase_conflict(
            "git pull failed: CONFLICT (content): Merge conflict in a"
        ));
        assert!(is_rebase_conflict("could not apply 1234567"));
        assert!(!is_rebase_conflict("authentication failed"));
    }

    #[test]
    fn formats_sync_commit_time_in_china_standard_time() {
        assert_eq!(
            format_china_standard_time(0).unwrap(),
            "1970-01-01 08:00:00"
        );
    }

    #[test]
    fn prevents_parallel_sync_with_lock_file() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("rupass-sync-lock-test-{suffix}"));
        let repo_dir = base.join("store").join("dev_team");
        fs::create_dir_all(&repo_dir).unwrap();

        let _guard = acquire_sync_lock(&repo_dir).unwrap();
        let err = acquire_sync_lock(&repo_dir).unwrap_err();

        assert!(err.to_string().contains("another sync is already running"));
    }

    #[test]
    fn writes_team_metadata_when_missing() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!("rupass-team-metadata-test-{suffix}"));
        fs::create_dir_all(&repo_dir).unwrap();
        let config = TeamConfig {
            team_name: "dev_team".to_string(),
            salt: "salt".to_string(),
            password_verifier: "verifier".to_string(),
            cipher_key: None,
            git_remote: Some("git@example.com:org/dev_team.git".to_string()),
            s3: None,
            sync_backend: None,
        };

        sync_team_metadata(&repo_dir, &config).unwrap();
        let metadata = load_team_metadata(&repo_dir).unwrap();

        assert_eq!(metadata, TeamMetadata::from(&config));
    }

    #[test]
    fn rejects_mismatched_team_metadata() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!("rupass-team-metadata-mismatch-{suffix}"));
        fs::create_dir_all(&repo_dir).unwrap();
        write_json(
            &team_metadata_path(&repo_dir),
            &TeamMetadata {
                team_name: "ops_team".to_string(),
                salt: "salt".to_string(),
                password_verifier: "verifier".to_string(),
            },
        )
        .unwrap();
        let config = TeamConfig {
            team_name: "dev_team".to_string(),
            salt: "salt".to_string(),
            password_verifier: "verifier".to_string(),
            cipher_key: None,
            git_remote: Some("git@example.com:org/dev_team.git".to_string()),
            s3: None,
            sync_backend: None,
        };

        let err = sync_team_metadata(&repo_dir, &config).unwrap_err();

        assert!(
            err.to_string()
                .contains("remote team metadata does not match local config")
        );
    }
}
