use std::time::{SystemTime, UNIX_EPOCH};

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::*;
use crate::crypto::{derive_key, password_verifier};
use crate::storage::{
    EncryptedTeamSecrets, TeamConfig, TeamFile, TeamKeyCache, save_key_cache, save_team_file,
};

fn test_paths() -> AppPaths {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base =
        std::env::temp_dir().join(format!("rupass-app-test-{}-{suffix}", std::process::id()));
    AppPaths::from_dirs(base.join("privite"), base.join("public"))
}

fn test_config(team_name: &str) -> TeamConfig {
    TeamConfig {
        team_name: team_name.to_string(),
        salt: "salt".to_string(),
        password_verifier: "verifier".to_string(),
    }
}

fn save_test_team(paths: &AppPaths, config: &TeamConfig) {
    save_team_file(
        paths,
        config,
        &EncryptedTeamSecrets {
            encrypted_payload: "payload".to_string(),
            nonce: "nonce".to_string(),
        },
    )
    .unwrap();
}

#[test]
fn infers_team_when_only_one_exists() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    save_test_team(&paths, &test_config("dev_team"));

    let team = resolve_target_team(&paths, None).unwrap();

    assert_eq!(team.name, "dev_team");
}

#[test]
fn requires_team_when_multiple_exist() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    save_test_team(&paths, &test_config("dev_team"));
    save_test_team(&paths, &test_config("ops_team"));

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
    let config = TeamConfig {
        team_name: "dev_team".to_string(),
        salt: "salt".to_string(),
        password_verifier: "verifier".to_string(),
    };
    save_test_team(&paths, &config);
    save_key_cache(
        &paths,
        "dev_team",
        &TeamKeyCache {
            cipher_key: STANDARD.encode(key),
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
    save_test_team(
        &paths,
        &TeamConfig {
            team_name: "dev_team".to_string(),
            salt: STANDARD.encode(salt),
            password_verifier: STANDARD.encode(password_verifier(&key)),
        },
    );

    let err = tui_ops::unlock_team(&paths, "dev_team", "wrong").unwrap_err();

    assert!(
        err.to_string()
            .contains("invalid password for team: dev_team")
    );
}

#[test]
fn imports_team_file_and_caches_cipher_key() {
    let paths = test_paths();
    paths.ensure_base_dirs().unwrap();
    let salt = [5_u8; 16];
    let key = derive_key("secret", &salt).unwrap();
    let source = std::env::temp_dir().join(format!(
        "rupass-import-test-{}-{}.json",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(
        &source,
        serde_json::to_vec_pretty(&TeamFile {
            team_name: "finn_team".to_string(),
            salt: STANDARD.encode(salt),
            password_verifier: STANDARD.encode(password_verifier(&key)),
            encrypted_payload: "payload".to_string(),
            nonce: "nonce".to_string(),
        })
        .unwrap(),
    )
    .unwrap();

    let team = tui_ops::import_team_file(&paths, source.to_str().unwrap(), "secret").unwrap();

    assert_eq!(team, "finn_team");
    assert!(paths.team_file_path("finn_team").exists());
    assert!(paths.key_cache_path("finn_team").exists());
}
