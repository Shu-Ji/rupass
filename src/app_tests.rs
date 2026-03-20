use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
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
        display_name: team_name.to_string(),
        salt: "salt".to_string(),
        password_verifier: "verifier".to_string(),
        cipher_key: None,
        git_remote: None,
    }
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
fn unlock_uses_stored_cipher_key_without_password() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    let key = [7_u8; 32];
    save_team_config(
        &paths,
        &TeamConfig {
            team_name: "dev_team".to_string(),
            display_name: "dev_team".to_string(),
            salt: "salt".to_string(),
            password_verifier: "verifier".to_string(),
            cipher_key: Some(STANDARD.encode(key)),
            git_remote: None,
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
            display_name: "dev_team".to_string(),
            salt: STANDARD.encode(salt),
            password_verifier: STANDARD.encode(password_verifier(&key)),
            cipher_key: Some(STANDARD.encode(key)),
            git_remote: None,
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
