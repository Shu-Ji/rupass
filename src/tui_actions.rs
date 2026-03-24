use anyhow::Result;

use crate::app;
use crate::cli::TeamImportArgs;
use crate::storage::{SyncBackend, TeamS3Config};
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
                    options: None,
                },
                InputField {
                    label: "密码",
                    value: String::new(),
                    secret: true,
                    options: None,
                },
                InputField {
                    label: "确认密码",
                    value: String::new(),
                    secret: true,
                    options: None,
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
                    options: None,
                },
                InputField {
                    label: "密码",
                    value: String::new(),
                    secret: true,
                    options: None,
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
                options: None,
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
                    options: None,
                },
                InputField {
                    label: "Value",
                    value: String::new(),
                    secret: false,
                    options: None,
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
            kind: FormKind::EditSecret {
                team: team.team_name.clone(),
                original_key: key.to_string(),
            },
            fields: vec![
                InputField {
                    label: "Key",
                    value: key.to_string(),
                    secret: false,
                    options: None,
                },
                InputField {
                    label: "Value",
                    value,
                    secret: false,
                    options: None,
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
            title: "设置 Git 远程仓库",
            submit_label: "Enter 保存",
            kind: FormKind::SetRemote(team.team_name.clone()),
            fields: vec![InputField {
                label: "Remote URL",
                value: team.git_remote.clone().unwrap_or_default(),
                secret: false,
                options: None,
            }],
            index: 0,
            error: None,
        });
    }

    pub(crate) fn open_choose_sync_backend(&mut self) {
        let Some(team) = self.selected_team() else {
            self.status = "没有可配置同步方式的团队".to_string();
            return;
        };
        let Some(access) = self.selected_access() else {
            self.status = format!("请先解锁团队: {}", team.team_name);
            return;
        };
        let mode = match access.config.effective_sync_backend() {
            Some(SyncBackend::Git) => "git",
            Some(SyncBackend::S3) => "s3",
            None => {
                if access.config.git_remote.is_some() {
                    "git"
                } else {
                    "s3"
                }
            }
        };
        self.dialog = Dialog::Form(FormDialog {
            title: "选择同步方式",
            submit_label: "Enter 下一步",
            kind: FormKind::ChooseSyncBackend(team.team_name.clone()),
            fields: vec![InputField {
                label: "同步方式",
                value: mode.to_string(),
                secret: false,
                options: Some(vec!["git", "s3"]),
            }],
            index: 0,
            error: None,
        });
    }

    pub(crate) fn open_set_s3(&mut self) {
        let Some(team) = self.selected_team() else {
            self.status = "没有可设置 S3 的团队".to_string();
            return;
        };
        let Some(access) = self.selected_access() else {
            self.status = format!("请先解锁团队: {}", team.team_name);
            return;
        };
        let s3 = access.config.s3.clone();
        self.dialog = Dialog::Form(FormDialog {
            title: "设置 S3 远程",
            submit_label: "Enter 保存",
            kind: FormKind::SetS3(team.team_name.clone()),
            fields: vec![
                InputField {
                    label: "Endpoint",
                    value: s3.as_ref().map(|it| it.endpoint.clone()).unwrap_or_default(),
                    secret: false,
                    options: None,
                },
                InputField {
                    label: "Region",
                    value: s3.as_ref().map(|it| it.region.clone()).unwrap_or_else(|| "us-east-1".to_string()),
                    secret: false,
                    options: None,
                },
                InputField {
                    label: "Bucket",
                    value: s3.as_ref().map(|it| it.bucket.clone()).unwrap_or_default(),
                    secret: false,
                    options: None,
                },
                InputField {
                    label: "Access Key ID",
                    value: s3.as_ref().map(|it| it.access_key_id.clone()).unwrap_or_default(),
                    secret: false,
                    options: None,
                },
                InputField {
                    label: "Secret Access Key",
                    value: s3.as_ref().map(|it| it.secret_access_key.clone()).unwrap_or_default(),
                    secret: true,
                    options: None,
                },
                InputField {
                    label: "Root",
                    value: s3.as_ref().map(|it| it.root.clone()).unwrap_or_default(),
                    secret: false,
                    options: None,
                },
                InputField {
                    label: "Force Path Style",
                    value: s3
                        .as_ref()
                        .map(|it| it.force_path_style.to_string())
                        .unwrap_or_else(|| "true".to_string()),
                    secret: false,
                    options: None,
                },
            ],
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
                options: None,
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
        if !access.config.has_remote() {
            self.status = format!("错误: 团队未配置远程: {}", access.config.team_name);
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
            if team.sync_backend.is_none() {
                skipped.push(team.team_name.clone());
                continue;
            }
            if let Some(access) = self.unlocked.get(&team.team_name).cloned() {
                tui_ops::sync_team(&self.paths, &access)?;
                synced += 1;
            }
        }
        if synced == 0 {
            self.status = format!("错误: 没有已配置远程的团队: {}", skipped.join(", "));
            return Ok(());
        }
        self.reload_teams()?;
        self.status = if skipped.is_empty() {
            format!("已同步全部团队: {synced}")
        } else {
            format!(
                "已同步 {synced} 个团队，未配置远程: {}",
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
                self.queue_import_team(remote, password)
            }
            FormKind::UnlockTeam(team) => {
                let access = tui_ops::unlock_team(&self.paths, team, &dialog.fields[0].value)?;
                self.unlocked.insert(team.clone(), access);
                self.reload_keys()?;
                self.status = format!("已解锁团队: {team}");
                Ok(())
            }
            FormKind::AddSecret(team) => {
                if !self.unlocked.contains_key(team) {
                    self.status = format!("请先解锁团队: {team}");
                    return Ok(());
                }
                let key = dialog.fields[0].value.trim();
                let value = &dialog.fields[1].value;
                if key.is_empty() {
                    anyhow::bail!("key cannot be empty");
                }
                self.queue_add_secret(team, key, value)?;
                Ok(())
            }
            FormKind::EditSecret { team, original_key } => {
                if !self.unlocked.contains_key(team) {
                    self.status = format!("请先解锁团队: {team}");
                    return Ok(());
                }
                let key = dialog.fields[0].value.trim();
                let value = &dialog.fields[1].value;
                if key.is_empty() {
                    anyhow::bail!("key cannot be empty");
                }
                self.queue_edit_secret(team, original_key, key, value)?;
                Ok(())
            }
            FormKind::ChooseSyncBackend(team) => {
                if !self.unlocked.contains_key(team) {
                    self.status = format!("请先解锁团队: {team}");
                    return Ok(());
                }
                let mode = dialog.fields[0].value.trim().to_ascii_lowercase();
                let access = self.unlocked.get(team).cloned().expect("checked unlocked");
                match mode.as_str() {
                    "git" => {
                        if access.config.git_remote.is_some() {
                            self.queue_set_sync_backend(team, SyncBackend::Git)?;
                        } else {
                            self.dialog = Dialog::None;
                            self.open_set_remote();
                        }
                        Ok(())
                    }
                    "s3" => {
                        if access.config.s3.is_some() {
                            self.queue_set_sync_backend(team, SyncBackend::S3)?;
                        } else {
                            self.dialog = Dialog::None;
                            self.open_set_s3();
                        }
                        Ok(())
                    }
                    _ => anyhow::bail!("同步方式必须是 git 或 s3"),
                }
            }
            FormKind::SetRemote(team) => {
                if !self.unlocked.contains_key(team) {
                    self.status = format!("请先解锁团队: {team}");
                    return Ok(());
                }
                self.queue_set_remote(team, &dialog.fields[0].value)?;
                Ok(())
            }
            FormKind::SetS3(team) => {
                if !self.unlocked.contains_key(team) {
                    self.status = format!("请先解锁团队: {team}");
                    return Ok(());
                }
                let endpoint = dialog.fields[0].value.trim();
                let region = dialog.fields[1].value.trim();
                let bucket = dialog.fields[2].value.trim();
                let access_key_id = dialog.fields[3].value.trim();
                let secret_access_key = dialog.fields[4].value.trim();
                let root = dialog.fields[5].value.trim().trim_matches('/').to_string();
                let force_path_style = dialog.fields[6]
                    .value
                    .trim()
                    .parse::<bool>()
                    .map_err(|_| anyhow::anyhow!("Force Path Style 必须是 true 或 false"))?;

                if endpoint.is_empty()
                    && region.is_empty()
                    && bucket.is_empty()
                    && access_key_id.is_empty()
                    && secret_access_key.is_empty()
                    && root.is_empty()
                {
                    self.queue_set_s3(team, None)?;
                    return Ok(());
                }

                if endpoint.is_empty()
                    || region.is_empty()
                    || bucket.is_empty()
                    || access_key_id.is_empty()
                    || secret_access_key.is_empty()
                {
                    anyhow::bail!("S3 配置不完整");
                }

                self.queue_set_s3(
                    team,
                    Some(TeamS3Config {
                        endpoint: endpoint.to_string(),
                        region: region.to_string(),
                        bucket: bucket.to_string(),
                        access_key_id: access_key_id.to_string(),
                        secret_access_key: secret_access_key.to_string(),
                        root,
                        force_path_style,
                    }),
                )?;
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
        if !self.unlocked.contains_key(&team) {
            self.status = format!("请先解锁团队: {team}");
            self.dialog = Dialog::None;
            return Ok(());
        }
        self.queue_delete_secret(&team, &key);
        Ok(())
    }

    fn queue_import_team(&mut self, remote: &str, password: &str) -> Result<()> {
        if remote.is_empty() {
            anyhow::bail!("remote url cannot be empty");
        }
        if password.is_empty() {
            anyhow::bail!("password cannot be empty");
        }

        self.queue_progress_action(
            "导入中",
            "正在导入团队并同步远程元数据，完成后会自动关闭。".to_string(),
            crate::tui_app::PendingAction::ImportTeam {
                remote: remote.to_string(),
                password: password.to_string(),
            },
        );
        Ok(())
    }

    fn queue_add_secret(&mut self, team: &str, key: &str, value: &str) -> Result<()> {
        self.queue_progress_action(
            "保存中",
            format!("正在新增 key {key} 并同步团队 {team}，完成后会自动关闭。"),
            crate::tui_app::PendingAction::AddSecret {
                team: team.to_string(),
                key: key.to_string(),
                value: value.to_string(),
            },
        );
        Ok(())
    }

    fn queue_edit_secret(
        &mut self,
        team: &str,
        original_key: &str,
        new_key: &str,
        value: &str,
    ) -> Result<()> {
        let action_text = if original_key == new_key {
            format!("正在更新 key {new_key} 并同步团队 {team}，完成后会自动关闭。")
        } else {
            format!(
                "正在将 key {original_key} 重命名为 {new_key} 并同步团队 {team}，完成后会自动关闭。"
            )
        };
        self.queue_progress_action(
            "保存中",
            action_text,
            crate::tui_app::PendingAction::EditSecret {
                team: team.to_string(),
                original_key: original_key.to_string(),
                new_key: new_key.to_string(),
                value: value.to_string(),
            },
        );
        Ok(())
    }

    fn queue_set_remote(&mut self, team: &str, url: &str) -> Result<()> {
        self.queue_progress_action(
            "更新中",
            format!("正在更新团队 {team} 的 Git 远程配置，完成后会自动关闭。"),
            crate::tui_app::PendingAction::SetRemote {
                team: team.to_string(),
                url: url.to_string(),
            },
        );
        Ok(())
    }

    fn queue_set_s3(&mut self, team: &str, config: Option<TeamS3Config>) -> Result<()> {
        self.queue_progress_action(
            "更新中",
            if config.is_some() {
                format!("正在更新团队 {team} 的 S3 远程配置，完成后会自动关闭。")
            } else {
                format!("正在清空团队 {team} 的 S3 远程配置，完成后会自动关闭。")
            },
            crate::tui_app::PendingAction::SetS3 {
                team: team.to_string(),
                config,
            },
        );
        Ok(())
    }

    fn queue_set_sync_backend(&mut self, team: &str, backend: SyncBackend) -> Result<()> {
        let target = match backend {
            SyncBackend::Git => "Git",
            SyncBackend::S3 => "S3",
        };
        self.queue_progress_action(
            "更新中",
            format!("正在切换团队 {team} 的同步方式为 {target}，完成后会自动关闭。"),
            crate::tui_app::PendingAction::SetSyncBackend {
                team: team.to_string(),
                backend,
            },
        );
        Ok(())
    }

    fn queue_delete_secret(&mut self, team: &str, key: &str) {
        self.queue_progress_action(
            "删除中",
            format!("正在删除 key {key} 并同步团队 {team}，完成后会自动关闭。"),
            crate::tui_app::PendingAction::DeleteSecret {
                team: team.to_string(),
                key: key.to_string(),
            },
        );
    }

    pub(crate) fn import_team_with_progress(&mut self, remote: &str, password: &str) -> Result<()> {
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

    pub(crate) fn save_secret_with_progress(
        &mut self,
        team: &str,
        original_key: &str,
        new_key: &str,
        value: &str,
        is_edit: bool,
    ) -> Result<()> {
        let Some(access) = self.unlocked.get(team).cloned() else {
            self.status = format!("请先解锁团队: {team}");
            return Ok(());
        };
        if is_edit {
            tui_ops::update_secret(&self.paths, &access, original_key, new_key, value)?;
        } else {
            tui_ops::set_secret(&self.paths, &access, new_key, value)?;
        }
        self.reload_keys()?;
        if let Page::TeamDetail { key_index, .. } = &mut self.page {
            *key_index = self
                .keys
                .iter()
                .position(|item| item == new_key)
                .unwrap_or(0);
        }
        self.status = if is_edit {
            if original_key == new_key {
                format!("已更新 key: {new_key}")
            } else {
                format!("已重命名 key: {original_key} -> {new_key}")
            }
        } else {
            format!("已新增 key: {new_key}")
        };
        Ok(())
    }

    pub(crate) fn set_remote_with_progress(&mut self, team: &str, url: &str) -> Result<()> {
        let Some(access) = self.unlocked.get(team).cloned() else {
            self.status = format!("请先解锁团队: {team}");
            return Ok(());
        };
        let updated = tui_ops::set_remote(&self.paths, &access, url)?;
        self.unlocked.insert(team.to_string(), updated);
        self.reload_teams()?;
        self.status = format!("已更新 Git 远程: {team}");
        Ok(())
    }

    pub(crate) fn set_s3_with_progress(
        &mut self,
        team: &str,
        config: Option<TeamS3Config>,
    ) -> Result<()> {
        let Some(access) = self.unlocked.get(team).cloned() else {
            self.status = format!("请先解锁团队: {team}");
            return Ok(());
        };
        let updated = tui_ops::set_s3(&self.paths, &access, config)?;
        self.unlocked.insert(team.to_string(), updated);
        self.reload_teams()?;
        self.status = if self
            .unlocked
            .get(team)
            .and_then(|access| access.config.s3.as_ref())
            .is_some()
        {
            format!("已更新 S3 远程: {team}")
        } else {
            format!("已清空 S3 远程: {team}")
        };
        Ok(())
    }

    pub(crate) fn set_sync_backend_with_progress(
        &mut self,
        team: &str,
        backend: SyncBackend,
    ) -> Result<()> {
        let Some(access) = self.unlocked.get(team).cloned() else {
            self.status = format!("请先解锁团队: {team}");
            return Ok(());
        };
        let updated = tui_ops::set_sync_backend(&self.paths, &access, backend)?;
        self.unlocked.insert(team.to_string(), updated);
        self.reload_teams()?;
        self.status = format!(
            "已切换同步方式: {} -> {}",
            team,
            match backend {
                SyncBackend::Git => "Git",
                SyncBackend::S3 => "S3",
            }
        );
        Ok(())
    }

    pub(crate) fn delete_secret_with_progress(&mut self, team: &str, key: &str) -> Result<()> {
        let Some(access) = self.unlocked.get(team).cloned() else {
            self.status = format!("请先解锁团队: {team}");
            return Ok(());
        };
        tui_ops::delete_secret(&self.paths, &access, key)?;
        self.reload_keys()?;
        self.status = format!("已删除 key: {key}");
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
            repo_dir.join("rupass-team.json"),
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
        assert!(matches!(
            app.dialog,
            Dialog::Progress {
                title: "导入中",
                ..
            }
        ));
        assert!(matches!(
            app.pending_action,
            Some(crate::tui_app::PendingAction::ImportTeam { .. })
        ));

        app.run_pending_action().unwrap();

        assert_eq!(app.teams.len(), 1);
        assert_eq!(app.teams[0].team_name, "dev_team");
        assert!(app.unlocked.contains_key("dev_team"));
        assert!(matches!(app.dialog, Dialog::None));
        assert_eq!(app.status, "已导入团队: dev_team");
    }

    #[test]
    fn add_secret_from_form_shows_progress_before_running() {
        let paths = test_paths();
        paths.ensure_base_dirs().unwrap();
        let mut app = App::new(paths).unwrap();
        let access = tui_ops::create_team(&app.paths, "dev_team", "secret", "secret").unwrap();
        app.unlocked.insert("dev_team".to_string(), access);
        app.reload_teams().unwrap();
        app.page = Page::TeamDetail {
            team_name: "dev_team".to_string(),
            key_index: 0,
        };

        app.open_add_secret();
        let Dialog::Form(dialog) = &mut app.dialog else {
            panic!("expected form dialog");
        };
        dialog.fields[0].value = "db_password".to_string();
        dialog.fields[1].value = "hello123".to_string();

        app.submit_form().unwrap();
        assert!(matches!(
            app.dialog,
            Dialog::Progress {
                title: "保存中",
                ..
            }
        ));
        assert!(matches!(
            app.pending_action,
            Some(crate::tui_app::PendingAction::AddSecret { .. })
        ));

        app.run_pending_action().unwrap();

        assert_eq!(
            tui_ops::get_secret(&app.paths, "dev_team", "db_password").unwrap(),
            "hello123"
        );
        assert_eq!(app.status, "已新增 key: db_password");
    }

    #[test]
    fn delete_key_confirmation_shows_progress_before_running() {
        let paths = test_paths();
        paths.ensure_base_dirs().unwrap();
        let mut app = App::new(paths).unwrap();
        let access = tui_ops::create_team(&app.paths, "dev_team", "secret", "secret").unwrap();
        tui_ops::set_secret(&app.paths, &access, "db_password", "hello123").unwrap();
        app.unlocked.insert("dev_team".to_string(), access);
        app.reload_teams().unwrap();
        app.page = Page::TeamDetail {
            team_name: "dev_team".to_string(),
            key_index: 0,
        };
        app.reload_keys().unwrap();
        app.dialog = Dialog::ConfirmDeleteKey {
            team: "dev_team".to_string(),
            key: "db_password".to_string(),
        };

        app.confirm_delete_key().unwrap();
        assert!(matches!(
            app.dialog,
            Dialog::Progress {
                title: "删除中",
                ..
            }
        ));
        assert!(matches!(
            app.pending_action,
            Some(crate::tui_app::PendingAction::DeleteSecret { .. })
        ));

        app.run_pending_action().unwrap();

        assert!(tui_ops::get_secret(&app.paths, "dev_team", "db_password").is_err());
        assert_eq!(app.status, "已删除 key: db_password");
    }

    #[test]
    fn edit_secret_can_rename_key() {
        let paths = test_paths();
        paths.ensure_base_dirs().unwrap();
        let mut app = App::new(paths).unwrap();
        let access = tui_ops::create_team(&app.paths, "dev_team", "secret", "secret").unwrap();
        tui_ops::set_secret(&app.paths, &access, "db_password", "hello123").unwrap();
        app.unlocked.insert("dev_team".to_string(), access);
        app.reload_teams().unwrap();
        app.page = Page::TeamDetail {
            team_name: "dev_team".to_string(),
            key_index: 0,
        };
        app.reload_keys().unwrap();

        app.open_edit_secret().unwrap();
        let Dialog::Form(dialog) = &mut app.dialog else {
            panic!("expected form dialog");
        };
        dialog.fields[0].value = "db_password_v2".to_string();
        dialog.fields[1].value = "hello456".to_string();

        app.submit_form().unwrap();
        assert!(matches!(
            app.pending_action,
            Some(crate::tui_app::PendingAction::EditSecret { .. })
        ));

        app.run_pending_action().unwrap();

        assert!(tui_ops::get_secret(&app.paths, "dev_team", "db_password").is_err());
        assert_eq!(
            tui_ops::get_secret(&app.paths, "dev_team", "db_password_v2").unwrap(),
            "hello456"
        );
        assert_eq!(app.status, "已重命名 key: db_password -> db_password_v2");
    }
}
