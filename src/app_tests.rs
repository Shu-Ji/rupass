use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, process::Command};

use super::*;
use crate::cli::TeamImportArgs;
use crate::storage::save_team_config;

fn test_paths() -> AppPaths {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("rupass-app-test-{}-{suffix}", std::process::id()));
    AppPaths::from_dirs(base.join("config"), base.join("store"))
}

fn test_config(team_name: &str) -> TeamConfig {
    TeamConfig {
        team_name: team_name.to_string(),
        salt: "salt".to_string(),
        password_verifier: "verifier".to_string(),
        cipher_key: None,
        git_remote: None,
        s3: None,
        sync_backend: None,
    }
}

fn run_git(repo_dir: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "git command failed: git {}",
        args.join(" ")
    );
}

fn init_remote_repo(team_name: &str, password: &str) -> (std::path::PathBuf, [u8; 32]) {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let repo_dir = std::env::temp_dir().join(format!("rupass-remote-repo-{suffix}"));
    fs::create_dir_all(&repo_dir).unwrap();
    run_git(&repo_dir, &["init", "-b", "main"]);
    run_git(&repo_dir, &["config", "user.email", "test@example.com"]);
    run_git(&repo_dir, &["config", "user.name", "rupass-test"]);

    let salt = [4_u8; 16];
    let key = derive_key(password, &salt).unwrap();
    let metadata = serde_json::json!({
        "team_name": team_name,
        "salt": STANDARD.encode(salt),
        "password_verifier": STANDARD.encode(password_verifier(&key)),
    });
    fs::write(
        repo_dir.join("rupass-team.json"),
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();
    run_git(&repo_dir, &["add", "."]);
    run_git(&repo_dir, &["commit", "-m", "init"]);

    (repo_dir, key)
}

#[test]
fn infers_team_when_only_one_exists() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    save_team_config(&paths, &test_config("dev_team")).unwrap();

    let team = resolve_target_team(&paths, None).unwrap();

    assert_eq!(team.name, "dev_team");
}

#[test]
fn requires_team_when_multiple_exist() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    save_team_config(&paths, &test_config("dev_team")).unwrap();
    save_team_config(&paths, &test_config("ops_team")).unwrap();

    let err = resolve_target_team(&paths, None).unwrap_err();

    assert!(
        err.to_string()
            .contains("multiple teams found: dev_team, ops_team")
    );
}

#[test]
fn errors_when_no_team_exists() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();

    let err = resolve_target_team(&paths, None).unwrap_err();

    assert!(err.to_string().contains("no team initialized"));
}

#[test]
fn sync_all_errors_when_no_remote_configured() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    save_team_config(&paths, &test_config("dev_team")).unwrap();

    let err = sync_all_teams(&paths).unwrap_err();

    assert!(err.to_string().contains("no team has a remote configured"));
    assert!(err.to_string().contains("dev_team"));
}

#[test]
fn unlock_uses_stored_cipher_key_without_password() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    let key = [7_u8; 32];
    save_team_config(
        &paths,
        &TeamConfig {
            team_name: "dev_team".to_string(),
            salt: "salt".to_string(),
            password_verifier: "verifier".to_string(),
            cipher_key: Some(STANDARD.encode(key)),
            git_remote: None,
            s3: None,
            sync_backend: None,
        },
    )
    .unwrap();

    let (_, unlocked) = load_team_for_get(&paths, "dev_team").unwrap();

    assert_eq!(unlocked, key);
}

#[test]
fn authenticate_requires_valid_password() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    let salt = [9_u8; 16];
    let key = derive_key("secret", &salt).unwrap();
    save_team_config(
        &paths,
        &TeamConfig {
            team_name: "dev_team".to_string(),
            salt: STANDARD.encode(salt),
            password_verifier: STANDARD.encode(password_verifier(&key)),
            cipher_key: Some(STANDARD.encode(key)),
            git_remote: None,
            s3: None,
            sync_backend: None,
        },
    )
    .unwrap();

    let config = load_team_config(&paths, "dev_team").unwrap();
    let err = authenticate_team_with_password(&paths, config, "dev_team", "wrong").unwrap_err();

    assert!(
        err.to_string()
            .contains("invalid password for team: dev_team")
    );
}

#[test]
fn imports_team_from_remote_metadata() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    let (remote_repo, key) = init_remote_repo("dev_team", "secret");

    let team = import_team(
        &paths,
        TeamImportArgs {
            args: vec![remote_repo.display().to_string()],
            team: None,
            password: Some("secret".to_string()),
        },
    )
    .unwrap();

    assert_eq!(team, "dev_team");
    let config = load_team_config(&paths, "dev_team").unwrap();
    let encoded_key = STANDARD.encode(key);
    assert_eq!(config.team_name, "dev_team");
    assert_eq!(
        config.git_remote.as_deref(),
        Some(remote_repo.to_str().unwrap())
    );
    assert_eq!(config.cipher_key.as_deref(), Some(encoded_key.as_str()));
    assert!(
        paths
            .team_store_dir("dev_team")
            .join("rupass-team.json")
            .exists()
    );
}

#[test]
fn import_rejects_wrong_remote_password() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    let (remote_repo, _) = init_remote_repo("dev_team", "secret");

    let err = import_team(
        &paths,
        TeamImportArgs {
            args: vec![remote_repo.display().to_string()],
            team: None,
            password: Some("wrong".to_string()),
        },
    )
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("invalid password for team: dev_team")
    );
}
