use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

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
    assert!(team.access.is_none());
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
fn falls_back_to_default_team_when_none_exists() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();

    let team = resolve_target_team_with(&paths, None, |_| {
        Ok(ResolvedTeam {
            name: DEFAULT_TEAM_NAME.to_string(),
            access: None,
        })
    })
    .unwrap();

    assert_eq!(team.name, DEFAULT_TEAM_NAME);
}

#[test]
fn default_team_creation_reuses_current_auth() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();

    let team = resolve_target_team_with(&paths, None, |_| {
        Ok(ResolvedTeam {
            name: DEFAULT_TEAM_NAME.to_string(),
            access: Some(TeamAccess {
                config: test_config(DEFAULT_TEAM_NAME),
                cipher_key: [5_u8; 32],
            }),
        })
    })
    .unwrap();

    let (_, cipher_key) = require_team_access(&paths, &team).unwrap();

    assert_eq!(cipher_key, [5_u8; 32]);
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

#[test]
fn delete_team_removes_config_and_store() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    let salt = [3_u8; 16];
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
    fs::create_dir_all(paths.team_store_dir("dev_team")).unwrap();

    let config_path = paths.config_path("dev_team");
    let store_dir = paths.team_store_dir("dev_team");
    delete_team_with_password(&paths, "dev_team", "secret").unwrap();

    assert!(!config_path.exists());
    assert!(!store_dir.exists());
}
