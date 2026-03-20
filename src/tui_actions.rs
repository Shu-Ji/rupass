use anyhow::Result;

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
                    label: "显示名",
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
                    secret: true,
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
                    secret: true,
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

        for team in &self.teams {
            if let Some(access) = self.unlocked.get(&team.team_name).cloned() {
                tui_ops::sync_team(&self.paths, &access)?;
            }
        }
        self.reload_teams()?;
        self.status = format!("已同步全部团队: {}", self.teams.len());
        Ok(())
    }

    pub(crate) fn submit_form(&mut self) -> Result<()> {
        let Dialog::Form(mut dialog) = std::mem::replace(&mut self.dialog, Dialog::None) else {
            return Ok(());
        };
        let result: Result<()> = match &dialog.kind {
            FormKind::CreateTeam => {
                let team = dialog.fields[0].value.trim();
                let display_name = dialog.fields[1].value.trim();
                let access = tui_ops::create_team(
                    &self.paths,
                    team,
                    (!display_name.is_empty()).then_some(display_name),
                    &dialog.fields[2].value,
                    &dialog.fields[3].value,
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
