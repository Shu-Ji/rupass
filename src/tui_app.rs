use std::collections::HashMap;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::storage::AppPaths;
use crate::tui_ops::{self, TeamAccess, TeamSummary};

pub(crate) struct App {
    pub(crate) paths: AppPaths,
    pub(crate) teams: Vec<TeamSummary>,
    pub(crate) keys: Vec<String>,
    pub(crate) unlocked: HashMap<String, TeamAccess>,
    pub(crate) team_index: usize,
    pub(crate) page: Page,
    pub(crate) dialog: Dialog,
    pub(crate) pending_action: Option<PendingAction>,
    pub(crate) status: String,
}

pub(crate) enum Page {
    TeamList,
    TeamDetail { team_name: String, key_index: usize },
}

pub(crate) enum Dialog {
    None,
    Form(FormDialog),
    ConfirmDeleteKey {
        team: String,
        key: String,
    },
    SecretView {
        key: String,
        value: String,
    },
    Progress {
        title: &'static str,
        message: String,
    },
    Help,
}

pub(crate) enum PendingAction {
    SyncCurrentTeam,
    SyncAllTeams,
    ImportTeam {
        remote: String,
        password: String,
    },
    AddSecret {
        team: String,
        key: String,
        value: String,
    },
    EditSecret {
        team: String,
        original_key: String,
        new_key: String,
        value: String,
    },
    SetRemote {
        team: String,
        url: String,
    },
    DeleteSecret {
        team: String,
        key: String,
    },
}

pub(crate) struct FormDialog {
    pub(crate) title: &'static str,
    pub(crate) submit_label: &'static str,
    pub(crate) kind: FormKind,
    pub(crate) fields: Vec<InputField>,
    pub(crate) index: usize,
    pub(crate) error: Option<String>,
}

pub(crate) enum FormKind {
    CreateTeam,
    ImportTeam,
    UnlockTeam(String),
    AddSecret(String),
    EditSecret { team: String, original_key: String },
    SetRemote(String),
    DeleteTeam(String),
}

pub(crate) struct InputField {
    pub(crate) label: &'static str,
    pub(crate) value: String,
    pub(crate) secret: bool,
}

impl App {
    pub(crate) fn new(paths: AppPaths) -> Result<Self> {
        let mut app = Self {
            paths,
            teams: Vec::new(),
            keys: Vec::new(),
            unlocked: HashMap::new(),
            team_index: 0,
            page: Page::TeamList,
            dialog: Dialog::None,
            pending_action: None,
            status: "先选择一个团队，或创建新团队。".to_string(),
        };
        app.reload_teams()?;
        Ok(app)
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> Result<bool> {
        match &self.dialog {
            Dialog::None => self.handle_page_key(key),
            Dialog::Form(_) => self.handle_form_key(key),
            Dialog::ConfirmDeleteKey { .. } => self.handle_confirm_key(key),
            Dialog::SecretView { .. } | Dialog::Help => {
                if matches!(key.code, KeyCode::Esc) {
                    self.dialog = Dialog::None;
                }
                Ok(false)
            }
            Dialog::Progress { .. } => Ok(false),
        }
    }

    pub(crate) fn show_error(&mut self, message: impl Into<String>) {
        self.status = format!("错误: {}", message.into());
    }

    pub(crate) fn selected_team(&self) -> Option<&TeamSummary> {
        match &self.page {
            Page::TeamList => self.teams.get(self.team_index),
            Page::TeamDetail { team_name, .. } => {
                self.teams.iter().find(|team| team.team_name == *team_name)
            }
        }
    }

    pub(crate) fn selected_access(&self) -> Option<&TeamAccess> {
        self.selected_team()
            .and_then(|team| self.unlocked.get(&team.team_name))
    }

    pub(crate) fn is_add_team_selected(&self) -> bool {
        matches!(self.page, Page::TeamList) && self.team_index >= self.teams.len()
    }

    pub(crate) fn selected_key(&self) -> Option<&str> {
        let Page::TeamDetail { key_index, .. } = self.page else {
            return None;
        };
        self.keys.get(key_index).map(String::as_str)
    }

    pub(crate) fn key_index(&self) -> usize {
        match self.page {
            Page::TeamDetail { key_index, .. } => key_index,
            Page::TeamList => 0,
        }
    }

    fn handle_page_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('h') => self.dialog = Dialog::Help,
            _ => match self.page {
                Page::TeamList => self.handle_team_page_key(key)?,
                Page::TeamDetail { .. } => self.handle_team_detail_key(key)?,
            },
        }
        Ok(false)
    }

    fn handle_team_page_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_team_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_team_selection(1),
            KeyCode::Enter => self.enter_team(),
            KeyCode::Char('c') => self.open_create_team(),
            KeyCode::Char('i') => self.open_import_team(),
            KeyCode::Char('u') => self.open_unlock_team(),
            KeyCode::Char('r') => self.open_set_remote(),
            KeyCode::Char('s') => self.queue_sync_all_teams(),
            KeyCode::Char('x') => self.open_delete_team(),
            _ => {}
        }
        Ok(())
    }

    fn handle_team_detail_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_key_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_key_selection(1),
            KeyCode::Enter => self.open_secret_view()?,
            KeyCode::Char('a') => self.open_add_secret(),
            KeyCode::Char('e') => self.open_edit_secret()?,
            KeyCode::Char('d') => self.open_delete_key(),
            KeyCode::Char('s') => self.queue_sync_current_team(),
            KeyCode::Char('u') => self.open_unlock_team(),
            KeyCode::Esc => self.back_to_team_list(),
            _ => {}
        }
        Ok(())
    }

    fn handle_form_key(&mut self, key: KeyEvent) -> Result<bool> {
        let Dialog::Form(dialog) = &mut self.dialog else {
            return Ok(false);
        };
        match key.code {
            KeyCode::Esc => self.dialog = Dialog::None,
            KeyCode::Tab | KeyCode::Down => dialog.index = (dialog.index + 1) % dialog.fields.len(),
            KeyCode::Up => {
                dialog.index = if dialog.index == 0 {
                    dialog.fields.len() - 1
                } else {
                    dialog.index - 1
                };
            }
            KeyCode::Backspace => {
                dialog.fields[dialog.index].value.pop();
            }
            KeyCode::Enter => self.submit_form()?,
            KeyCode::Char(ch) => {
                if key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER)
                {
                    return Ok(false);
                }
                dialog.fields[dialog.index].value.push(ch);
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_confirm_key(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => self.dialog = Dialog::None,
            KeyCode::Enter => self.confirm_delete_key()?,
            _ => {}
        }
        Ok(false)
    }

    fn move_team_selection(&mut self, delta: isize) {
        let total = self.teams.len() + 1;
        self.team_index = wrap_index(self.team_index, total, delta);
    }

    fn move_key_selection(&mut self, delta: isize) {
        if self.keys.is_empty() {
            return;
        }
        if let Page::TeamDetail { key_index, .. } = &mut self.page {
            *key_index = wrap_index(*key_index, self.keys.len(), delta);
        }
    }

    fn enter_team(&mut self) {
        let Some(team) = self.teams.get(self.team_index) else {
            self.open_create_team();
            return;
        };
        let team_name = team.team_name.clone();
        self.page = Page::TeamDetail {
            team_name: team_name.clone(),
            key_index: 0,
        };
        let _ = self.reload_keys();
        self.status = format!("已进入团队: {team_name}");
    }

    fn back_to_team_list(&mut self) {
        self.page = Page::TeamList;
        self.keys.clear();
        self.status = "已返回团队列表。".to_string();
    }

    pub(crate) fn reload_teams(&mut self) -> Result<()> {
        self.teams = tui_ops::list_teams(&self.paths)?;
        if self.team_index >= self.teams.len() && !self.teams.is_empty() {
            self.team_index = self.teams.len() - 1;
        }
        if self.teams.is_empty() {
            self.team_index = 0;
        }
        self.unlocked
            .retain(|team, _| self.teams.iter().any(|item| item.team_name == *team));

        if let Page::TeamDetail { team_name, .. } = &self.page
            && !self.teams.iter().any(|team| team.team_name == *team_name)
        {
            self.page = Page::TeamList;
            self.keys.clear();
        }

        self.reload_keys()
    }

    pub(crate) fn reload_keys(&mut self) -> Result<()> {
        self.keys.clear();
        let Some(access) = self.selected_access().cloned() else {
            return Ok(());
        };
        self.keys = tui_ops::list_keys(&self.paths, &access)?;
        if let Page::TeamDetail { key_index, .. } = &mut self.page
            && *key_index >= self.keys.len()
            && !self.keys.is_empty()
        {
            *key_index = self.keys.len() - 1;
        }
        Ok(())
    }

    pub(crate) fn has_pending_action(&self) -> bool {
        self.pending_action.is_some()
    }

    pub(crate) fn run_pending_action(&mut self) -> Result<()> {
        let Some(action) = self.pending_action.take() else {
            return Ok(());
        };

        let result = match action {
            PendingAction::SyncCurrentTeam => self.sync_current_team(),
            PendingAction::SyncAllTeams => self.sync_all_teams(),
            PendingAction::ImportTeam { remote, password } => {
                self.import_team_with_progress(&remote, &password)
            }
            PendingAction::AddSecret { team, key, value } => {
                self.save_secret_with_progress(&team, &key, &key, &value, false)
            }
            PendingAction::EditSecret {
                team,
                original_key,
                new_key,
                value,
            } => {
                self.save_secret_with_progress(&team, &original_key, &new_key, &value, true)
            }
            PendingAction::SetRemote { team, url } => self.set_remote_with_progress(&team, &url),
            PendingAction::DeleteSecret { team, key } => {
                self.delete_secret_with_progress(&team, &key)
            }
        };
        self.dialog = Dialog::None;
        result
    }

    pub(crate) fn queue_progress_action(
        &mut self,
        title: &'static str,
        message: String,
        action: PendingAction,
    ) {
        if self.pending_action.is_some() {
            self.status = "错误: 已有操作正在进行中，请等待完成".to_string();
            return;
        }

        self.dialog = Dialog::Progress { title, message };
        self.pending_action = Some(action);
    }

    fn queue_sync_current_team(&mut self) {
        let Some(access) = self.selected_access() else {
            self.status = "请先解锁团队，再执行同步".to_string();
            return;
        };
        if access.config.git_remote.is_none() {
            self.status = format!("错误: 团队未配置远程仓库: {}", access.config.team_name);
            return;
        }

        self.queue_progress_action(
            "同步中",
            format!(
                "正在同步团队 {}，完成后会自动关闭。",
                access.config.team_name
            ),
            PendingAction::SyncCurrentTeam,
        );
    }

    fn queue_sync_all_teams(&mut self) {
        if self.teams.is_empty() {
            self.status = "没有可同步的团队".to_string();
            return;
        }

        let locked = self
            .teams
            .iter()
            .filter(|team| !self.unlocked.contains_key(&team.team_name))
            .map(|team| team.team_name.clone())
            .collect::<Vec<_>>();
        if !locked.is_empty() {
            self.status = format!("错误: 请先解锁全部团队: {}", locked.join(", "));
            return;
        }

        let ready = self
            .teams
            .iter()
            .filter(|team| team.git_remote.is_some())
            .count();
        if ready == 0 {
            let names = self
                .teams
                .iter()
                .map(|team| team.team_name.clone())
                .collect::<Vec<_>>();
            self.status = format!("错误: 没有已配置远程仓库的团队: {}", names.join(", "));
            return;
        }

        self.queue_progress_action(
            "同步中",
            format!("正在同步 {ready} 个团队，完成后会自动关闭。"),
            PendingAction::SyncAllTeams,
        );
    }
}

fn wrap_index(current: usize, len: usize, delta: isize) -> usize {
    (((current as isize + delta).rem_euclid(len as isize)) as usize).min(len - 1)
}
