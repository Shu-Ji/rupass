use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::*;
use crate::crypto::{derive_key, password_verifier};
use crate::storage::{TeamConfig, save_team_config};

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
fn load_team_for_get_uses_stored_cipher_key_without_password() {
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
        },
    )
    .unwrap();

    let unlocked = tui_ops::open_team(&paths, "dev_team", None).unwrap();

    assert_eq!(unlocked.cipher_key, key);
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
        },
    )
    .unwrap();

    let err = tui_ops::unlock_team(&paths, "dev_team", "wrong").unwrap_err();

    assert!(
        err.to_string()
            .contains("invalid password for team: dev_team")
    );
}
