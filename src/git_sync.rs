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
            run_git(repo_dir, &["pull", "--rebase", "origin", "main"])?;
        }

        if has_local_commits(repo_dir)? {
            run_git(repo_dir, &["push", "-u", "origin", "main"])?;
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
