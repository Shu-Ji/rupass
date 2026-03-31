use std::collections::HashMap;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::storage::AppPaths;
use crate::tui_ops::{self, TeamAccess, TeamSummary};

pub(crate) struct App {
    pub(crate) paths: AppPaths,
    pub(crate) teams: Vec<TeamSummary>,
    pub(crate) keys: Vec<String>,
    pub(crate) current_secret_value: Option<String>,
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
    Progress {
        title: &'static str,
        message: String,
    },
    Help,
}

pub(crate) enum PendingAction {
    Add {
        team: String,
        key: String,
        value: String,
    },
    Edit {
        team: String,
        original_key: String,
        new_key: String,
        value: String,
    },
    Delete {
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
    UnlockTeam(String),
    AddSecret(String),
    EditSecret { team: String, original_key: String },
    DeleteTeam(String),
}

pub(crate) struct InputField {
    pub(crate) label: &'static str,
    pub(crate) value: String,
    pub(crate) secret: bool,
    pub(crate) options: Option<Vec<&'static str>>,
}

impl App {
    pub(crate) fn new(paths: AppPaths) -> Result<Self> {
        let mut app = Self {
            paths,
            teams: Vec::new(),
            keys: Vec::new(),
            current_secret_value: None,
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
            Dialog::Help => {
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

    pub(crate) fn selected_secret_value(&self) -> Option<&str> {
        self.current_secret_value.as_deref()
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
            KeyCode::Char('u') => self.open_unlock_team(),
            KeyCode::Char('x') => self.open_delete_team(),
            _ => {}
        }
        Ok(())
    }

    fn handle_team_detail_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_key_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_key_selection(1),
            KeyCode::Char('a') => self.open_add_secret(),
            KeyCode::Char('e') => self.open_edit_secret()?,
            KeyCode::Char('d') => self.open_delete_key(),
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
        let is_select_field = dialog.fields[dialog.index].options.is_some();
        match key.code {
            KeyCode::Esc => self.dialog = Dialog::None,
            KeyCode::Tab => dialog.index = (dialog.index + 1) % dialog.fields.len(),
            KeyCode::Down => {
                if is_select_field {
                    cycle_field_option(&mut dialog.fields[dialog.index], 1);
                } else {
                    dialog.index = (dialog.index + 1) % dialog.fields.len();
                }
            }
            KeyCode::Up => {
                if is_select_field {
                    cycle_field_option(&mut dialog.fields[dialog.index], -1);
                } else {
                    dialog.index = if dialog.index == 0 {
                        dialog.fields.len() - 1
                    } else {
                        dialog.index - 1
                    };
                }
            }
            KeyCode::Backspace => {
                if dialog.fields[dialog.index].options.is_none() {
                    dialog.fields[dialog.index].value.pop();
                }
            }
            KeyCode::Enter => self.submit_form()?,
            KeyCode::Left => {
                if is_select_field {
                    cycle_field_option(&mut dialog.fields[dialog.index], -1);
                }
            }
            KeyCode::Right => {
                if is_select_field {
                    cycle_field_option(&mut dialog.fields[dialog.index], 1);
                }
            }
            KeyCode::Char(ch) => {
                if is_select_field {
                    return Ok(false);
                }
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
        self.refresh_selected_secret_value();
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
        self.current_secret_value = None;
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
            self.current_secret_value = None;
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
        self.load_selected_secret_value()
    }

    pub(crate) fn has_pending_action(&self) -> bool {
        self.pending_action.is_some()
    }

    pub(crate) fn run_pending_action(&mut self) -> Result<()> {
        let Some(action) = self.pending_action.take() else {
            return Ok(());
        };

        let result = match action {
            PendingAction::Add { team, key, value } => {
                self.save_secret_with_progress(&team, &key, &key, &value, false)
            }
            PendingAction::Edit {
                team,
                original_key,
                new_key,
                value,
            } => self.save_secret_with_progress(&team, &original_key, &new_key, &value, true),
            PendingAction::Delete { team, key } => self.delete_secret_with_progress(&team, &key),
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

    fn load_selected_secret_value(&mut self) -> Result<()> {
        let team_name = self.selected_team().map(|team| team.team_name.clone());
        let key = self.selected_key().map(str::to_string);
        if team_name.is_none() || key.is_none() || self.selected_access().is_none() {
            self.current_secret_value = None;
            return Ok(());
        }
        self.current_secret_value = Some(tui_ops::get_secret(
            &self.paths,
            &team_name.expect("checked"),
            &key.expect("checked"),
        )?);
        Ok(())
    }

    pub(crate) fn refresh_selected_secret_value(&mut self) {
        if let Err(err) = self.load_selected_secret_value() {
            self.current_secret_value = None;
            self.show_error(err.to_string());
        }
    }
}

fn wrap_index(current: usize, len: usize, delta: isize) -> usize {
    (((current as isize + delta).rem_euclid(len as isize)) as usize).min(len - 1)
}

fn cycle_field_option(field: &mut InputField, delta: isize) {
    let Some(options) = field.options.as_ref() else {
        return;
    };
    if options.is_empty() {
        return;
    }
    let current_index = options
        .iter()
        .position(|option| *option == field.value)
        .unwrap_or(0);
    let next_index = wrap_index(current_index, options.len(), delta);
    field.value = options[next_index].to_string();
}
