use anyhow::Result;

use crate::app;
use crate::cli::TeamImportArgs;
use crate::tui_app::{App, Dialog, FormDialog, FormKind, InputField, Page};
use crate::tui_ops;

impl App {
    pub(crate) fn open_create_team(&mut self) {
        self.dialog = Dialog::Form(FormDialog {
            title: "创建团队",
            submit_label: "Enter 创建",
            kind: FormKind::CreateTeam,
            fields: vec![
                InputField {
                    label: "团队名",
                    value: String::new(),
                    secret: false,
                },
                InputField {
                    label: "密码",
                    value: String::new(),
                    secret: true,
                },
                InputField {
                    label: "确认密码",
                    value: String::new(),
                    secret: true,
                },
            ],
            index: 0,
            error: None,
        });
    }

    pub(crate) fn open_import_team(&mut self) {
        self.dialog = Dialog::Form(FormDialog {
            title: "导入团队",
            submit_label: "Enter 导入",
            kind: FormKind::ImportTeam,
            fields: vec![
                InputField {
                    label: "Remote URL",
                    value: String::new(),
                    secret: false,
                },
                InputField {
                    label: "密码",
                    value: String::new(),
                    secret: true,
                },
            ],
            index: 0,
            error: None,
        });
    }

    pub(crate) fn open_unlock_team(&mut self) {
        let Some(team) = self.selected_team() else {
            self.status = "没有可解锁的团队".to_string();
            return;
        };
        self.dialog = Dialog::Form(FormDialog {
            title: "解锁团队",
            submit_label: "Enter 解锁",
            kind: FormKind::UnlockTeam(team.team_name.clone()),
            fields: vec![InputField {
                label: "密码",
                value: String::new(),
                secret: true,
            }],
            index: 0,
            error: None,
        });
    }

    pub(crate) fn open_add_secret(&mut self) {
        let Some(team) = self.selected_team() else {
            self.status = "请先创建团队".to_string();
            return;
        };
        if self.selected_access().is_none() {
            self.status = format!("请先解锁团队: {}", team.team_name);
            return;
        }
        self.dialog = Dialog::Form(FormDialog {
            title: "新增密钥",
            submit_label: "Enter 新增",
            kind: FormKind::AddSecret(team.team_name.clone()),
            fields: vec![
                InputField {
                    label: "Key",
                    value: String::new(),
                    secret: false,
                },
                InputField {
                    label: "Value",
                    value: String::new(),
                    secret: false,
                },
            ],
            index: 0,
            error: None,
        });
    }

    pub(crate) fn open_edit_secret(&mut self) -> Result<()> {
        let Some(team) = self.selected_team() else {
            self.status = "请先创建团队".to_string();
            return Ok(());
        };
        if self.selected_access().is_none() {
            self.status = format!("请先解锁团队: {}", team.team_name);
            return Ok(());
        }
        let Some(key) = self.selected_key() else {
            self.status = "没有可更新的 key".to_string();
            return Ok(());
        };
        let value = tui_ops::get_secret(&self.paths, &team.team_name, key)?;
        self.dialog = Dialog::Form(FormDialog {
            title: "更新密钥",
            submit_label: "Enter 更新",
            kind: FormKind::EditSecret(team.team_name.clone()),
            fields: vec![
                InputField {
                    label: "Key",
                    value: key.to_string(),
                    secret: false,
                },
                InputField {
                    label: "Value",
                    value,
                    secret: false,
                },
            ],
            index: 0,
            error: None,
        });
        Ok(())
    }

    pub(crate) fn open_set_remote(&mut self) {
        let Some(team) = self.selected_team() else {
            self.status = "没有可设置远程的团队".to_string();
            return;
        };
        if self.selected_access().is_none() {
            self.status = format!("请先解锁团队: {}", team.team_name);
            return;
        }
        self.dialog = Dialog::Form(FormDialog {
            title: "设置远程仓库",
            submit_label: "Enter 保存",
            kind: FormKind::SetRemote(team.team_name.clone()),
            fields: vec![InputField {
                label: "Remote URL",
                value: team.git_remote.clone().unwrap_or_default(),
                secret: false,
            }],
            index: 0,
            error: None,
        });
    }

    pub(crate) fn open_delete_team(&mut self) {
        let Some(team) = self.selected_team() else {
            self.status = "没有可删除的团队".to_string();
            return;
        };
        self.dialog = Dialog::Form(FormDialog {
            title: "删除团队",
            submit_label: "Enter 删除",
            kind: FormKind::DeleteTeam(team.team_name.clone()),
            fields: vec![InputField {
                label: "团队密码",
                value: String::new(),
                secret: true,
            }],
            index: 0,
            error: None,
        });
    }

    pub(crate) fn open_delete_key(&mut self) {
        let (Some(team), Some(key)) = (self.selected_team(), self.selected_key()) else {
            self.status = "没有可删除的 key".to_string();
            return;
        };
        if self.selected_access().is_none() {
            self.status = format!("请先解锁团队: {}", team.team_name);
            return;
        }
        self.dialog = Dialog::ConfirmDeleteKey {
            team: team.team_name.clone(),
            key: key.to_string(),
        };
    }

    pub(crate) fn open_secret_view(&mut self) -> Result<()> {
        let (Some(team), Some(key)) = (self.selected_team(), self.selected_key()) else {
            self.status = "没有可查看的 key".to_string();
            return Ok(());
        };
        let value = tui_ops::get_secret(&self.paths, &team.team_name, key)?;
        self.dialog = Dialog::SecretView {
            key: key.to_string(),
            value,
        };
        Ok(())
    }

    pub(crate) fn sync_current_team(&mut self) -> Result<()> {
        let Some(access) = self.selected_access().cloned() else {
            self.status = "请先解锁团队，再执行同步".to_string();
            return Ok(());
        };
        if access.config.git_remote.is_none() {
            self.status = format!("错误: 团队未配置远程仓库: {}", access.config.team_name);
            return Ok(());
        }
        tui_ops::sync_team(&self.paths, &access)?;
        self.reload_teams()?;
        self.status = format!("已同步团队: {}", access.config.team_name);
        Ok(())
    }

    pub(crate) fn sync_all_teams(&mut self) -> Result<()> {
        if self.teams.is_empty() {
            self.status = "没有可同步的团队".to_string();
            return Ok(());
        }

        let locked = self
            .teams
            .iter()
            .filter(|team| !self.unlocked.contains_key(&team.team_name))
            .map(|team| team.team_name.clone())
            .collect::<Vec<_>>();
        if !locked.is_empty() {
            self.status = format!("错误: 请先解锁全部团队: {}", locked.join(", "));
            return Ok(());
        }

        let mut synced = 0_usize;
        let mut skipped = Vec::new();
        for team in &self.teams {
            if team.git_remote.is_none() {
                skipped.push(team.team_name.clone());
                continue;
            }
            if let Some(access) = self.unlocked.get(&team.team_name).cloned() {
                tui_ops::sync_team(&self.paths, &access)?;
                synced += 1;
            }
        }
        if synced == 0 {
            self.status = format!("错误: 没有已配置远程仓库的团队: {}", skipped.join(", "));
            return Ok(());
        }
        self.reload_teams()?;
        self.status = if skipped.is_empty() {
            format!("已同步全部团队: {synced}")
        } else {
            format!(
                "已同步 {synced} 个团队，未配置远程仓库: {}",
                skipped.join(", ")
            )
        };
        Ok(())
    }

    pub(crate) fn submit_form(&mut self) -> Result<()> {
        let Dialog::Form(mut dialog) = std::mem::replace(&mut self.dialog, Dialog::None) else {
            return Ok(());
        };
        let result: Result<()> = match &dialog.kind {
            FormKind::CreateTeam => {
                let team = dialog.fields[0].value.trim();
                let access = tui_ops::create_team(
                    &self.paths,
                    team,
                    &dialog.fields[1].value,
                    &dialog.fields[2].value,
                )?;
                self.unlocked.insert(team.to_string(), access);
                self.reload_teams()?;
                self.team_index = self
                    .teams
                    .iter()
                    .position(|item| item.team_name == team)
                    .unwrap_or(0);
                self.status = format!("已创建团队: {team}");
                Ok(())
            }
            FormKind::ImportTeam => {
                let remote = dialog.fields[0].value.trim();
                let password = dialog.fields[1].value.trim();
                if remote.is_empty() {
                    anyhow::bail!("remote url cannot be empty");
                }
                if password.is_empty() {
                    anyhow::bail!("password cannot be empty");
                }

                let team = app::import_team(
                    &self.paths,
                    TeamImportArgs {
                        args: vec![remote.to_string()],
                        team: None,
                        password: Some(password.to_string()),
                    },
                )?;
                let access = tui_ops::open_team(&self.paths, &team, None)?;
                self.unlocked.insert(team.clone(), access);
                self.reload_teams()?;
                self.team_index = self
                    .teams
                    .iter()
                    .position(|item| item.team_name == team)
                    .unwrap_or(0);
                self.status = format!("已导入团队: {team}");
                Ok(())
            }
            FormKind::UnlockTeam(team) => {
                let access = tui_ops::unlock_team(&self.paths, team, &dialog.fields[0].value)?;
                self.unlocked.insert(team.clone(), access);
                self.reload_keys()?;
                self.status = format!("已解锁团队: {team}");
                Ok(())
            }
            FormKind::AddSecret(team) => {
                let Some(access) = self.unlocked.get(team).cloned() else {
                    self.status = format!("请先解锁团队: {team}");
                    return Ok(());
                };
                let key = dialog.fields[0].value.trim();
                let value = &dialog.fields[1].value;
                if key.is_empty() {
                    anyhow::bail!("key cannot be empty");
                }
                tui_ops::set_secret(&self.paths, &access, key, value)?;
                self.reload_keys()?;
                if let Page::TeamDetail { key_index, .. } = &mut self.page {
                    *key_index = self.keys.iter().position(|item| item == key).unwrap_or(0);
                }
                self.status = format!("已新增 key: {key}");
                Ok(())
            }
            FormKind::EditSecret(team) => {
                let Some(access) = self.unlocked.get(team).cloned() else {
                    self.status = format!("请先解锁团队: {team}");
                    return Ok(());
                };
                let key = dialog.fields[0].value.trim();
                let value = &dialog.fields[1].value;
                if key.is_empty() {
                    anyhow::bail!("key cannot be empty");
                }
                tui_ops::set_secret(&self.paths, &access, key, value)?;
                self.reload_keys()?;
                if let Page::TeamDetail { key_index, .. } = &mut self.page {
                    *key_index = self.keys.iter().position(|item| item == key).unwrap_or(0);
                }
                self.status = format!("已更新 key: {key}");
                Ok(())
            }
            FormKind::SetRemote(team) => {
                let Some(access) = self.unlocked.get(team).cloned() else {
                    self.status = format!("请先解锁团队: {team}");
                    return Ok(());
                };
                let updated = tui_ops::set_remote(&self.paths, &access, &dialog.fields[0].value)?;
                self.unlocked.insert(team.clone(), updated);
                self.reload_teams()?;
                self.status = format!("已更新远程: {team}");
                Ok(())
            }
            FormKind::DeleteTeam(team) => {
                tui_ops::delete_team(&self.paths, team, &dialog.fields[0].value)?;
                self.unlocked.remove(team);
                self.reload_teams()?;
                self.status = format!("已删除团队: {team}");
                Ok(())
            }
        };

        match result {
            Ok(()) => {}
            Err(err) => {
                dialog.error = Some(err.to_string());
                self.dialog = Dialog::Form(dialog);
            }
        }
        Ok(())
    }

    pub(crate) fn confirm_delete_key(&mut self) -> Result<()> {
        let Dialog::ConfirmDeleteKey { team, key } = &self.dialog else {
            return Ok(());
        };
        let team = team.clone();
        let key = key.clone();
        let Some(access) = self.unlocked.get(&team).cloned() else {
            self.status = format!("请先解锁团队: {team}");
            self.dialog = Dialog::None;
            return Ok(());
        };
        tui_ops::delete_secret(&self.paths, &access, &key)?;
        self.reload_keys()?;
        self.status = format!("已删除 key: {key}");
        self.dialog = Dialog::None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use base64::{Engine as _, engine::general_purpose::STANDARD};

    use super::*;
    use crate::crypto::{derive_key, password_verifier};
    use crate::storage::AppPaths;

    fn test_paths() -> AppPaths {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!(
            "rupass-tui-actions-test-{}-{suffix}",
            std::process::id()
        ));
        AppPaths::from_dirs(base.join("config"), base.join("store"))
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

    fn init_remote_repo(team_name: &str, password: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo_dir = std::env::temp_dir().join(format!("rupass-tui-remote-repo-{suffix}"));
        fs::create_dir_all(&repo_dir).unwrap();
        run_git(&repo_dir, &["init", "-b", "main"]);
        run_git(&repo_dir, &["config", "user.email", "test@example.com"]);
        run_git(&repo_dir, &["config", "user.name", "rupass-test"]);

        let salt = [6_u8; 16];
        let key = derive_key(password, &salt).unwrap();
        let metadata = serde_json::json!({
            "team_name": team_name,
            "salt": STANDARD.encode(salt),
            "password_verifier": STANDARD.encode(password_verifier(&key)),
        });
        fs::write(
            repo_dir.join(".rupass-team.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .unwrap();
        run_git(&repo_dir, &["add", "."]);
        run_git(&repo_dir, &["commit", "-m", "init"]);

        repo_dir
    }

    #[test]
    fn imports_team_from_tui_form() {
        let paths = test_paths();
        paths.ensure_base_dirs().unwrap();
        let remote_repo = init_remote_repo("dev_team", "secret");
        let mut app = App::new(paths).unwrap();

        app.open_import_team();
        let Dialog::Form(dialog) = &mut app.dialog else {
            panic!("expected form dialog");
        };
        dialog.fields[0].value = remote_repo.display().to_string();
        dialog.fields[1].value = "secret".to_string();

        app.submit_form().unwrap();

        assert_eq!(app.teams.len(), 1);
        assert_eq!(app.teams[0].team_name, "dev_team");
        assert!(app.unlocked.contains_key("dev_team"));
        assert!(matches!(app.dialog, Dialog::None));
        assert_eq!(app.status, "已导入团队: dev_team");
    }
}
