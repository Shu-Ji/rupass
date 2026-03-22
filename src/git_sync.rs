use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::storage::TeamConfig;

pub(crate) fn ensure_git_repo(repo_dir: &Path) -> Result<()> {
    if repo_dir.join(".git").exists() {
        return Ok(());
    }

    fs::create_dir_all(repo_dir)
        .with_context(|| format!("failed to create repo dir {}", repo_dir.display()))?;
    run_git(repo_dir, &["init", "-b", "main"])?;
    Ok(())
}

pub(crate) fn sync_team_repo(repo_dir: &Path, config: &TeamConfig) -> Result<()> {
    ensure_git_repo(repo_dir)?;

    if let Some(remote) = &config.git_remote {
        ensure_git_remote(repo_dir, remote)?;
        bootstrap_from_remote_if_needed(repo_dir)?;
    }

    let has_changes = repo_has_changes(repo_dir)?;
    if has_changes {
        run_git(repo_dir, &["add", "."])?;
        run_git(
            repo_dir,
            &[
                "commit",
                "-m",
                &format!("rupass sync {}", unix_timestamp()?),
            ],
        )?;
    }

    if config.git_remote.is_some() {
        let remote_has_main = remote_has_main_branch(repo_dir)?;
        if remote_has_main && has_local_commits(repo_dir)? {
            if let Err(err) = run_git(repo_dir, &["pull", "--rebase", "origin", "main"]) {
                bail!("{}", format_sync_error(repo_dir, &err.to_string()));
            }
        }

        if has_local_commits(repo_dir)? && let Err(err) = run_git(repo_dir, &["push", "-u", "origin", "main"]) {
            bail!("{}", format_sync_error(repo_dir, &err.to_string()));
        }
    }

    Ok(())
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

fn unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_secs())
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
    Ok(parse_conflict_paths(&run_git(repo_dir, &["status", "--porcelain"])?))
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(is_rebase_conflict("git pull failed: CONFLICT (content): Merge conflict in a"));
        assert!(is_rebase_conflict("could not apply 1234567"));
        assert!(!is_rebase_conflict("authentication failed"));
    }
}
