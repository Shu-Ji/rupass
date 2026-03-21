use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::tui_app::{App, Dialog, FormDialog, Page};
use crate::tui_style::{
    accent_style, action_line, danger_style, muted, primary_style, section_style,
    split_status_lines, status_style, success_style,
};

pub(crate) fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(68), Constraint::Percentage(32)])
        .split(sections[1]);

    frame.render_widget(header(app), sections[0]);
    match &app.page {
        Page::TeamList => draw_team_page(frame, app, body[0]),
        Page::TeamDetail { .. } => draw_team_detail_page(frame, app, body[0]),
    }
    frame.render_widget(help_panel(app), body[1]);

    match &app.dialog {
        Dialog::None => {}
        Dialog::Form(dialog) => render_form_dialog(frame, dialog),
        Dialog::ConfirmDeleteKey { team, key } => render_confirm_dialog(frame, team, key),
        Dialog::SecretView { key, value } => render_secret_dialog(frame, key, value),
        Dialog::Help => render_help_dialog(frame, app),
    }
}

fn draw_team_page(frame: &mut Frame, app: &App, area: Rect) {
    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    frame.render_stateful_widget(team_list(app), content[0], &mut list_state(app.team_index));
    frame.render_widget(team_detail_panel(app), content[1]);
}

fn draw_team_detail_page(frame: &mut Frame, app: &App, area: Rect) {
    let content = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);

    frame.render_stateful_widget(key_list(app), content[0], &mut list_state(app.key_index()));
    frame.render_widget(secret_detail_panel(app), content[1]);
}

fn header(app: &App) -> Paragraph<'static> {
    let page_name = match app.page {
        Page::TeamList => "团队列表",
        Page::TeamDetail { .. } => "团队详情",
    };
    let title = Line::from(vec![
        Span::styled(
            "RUPASS TUI",
            Style::default()
                .fg(Color::Rgb(242, 208, 129))
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(page_name, Style::default().fg(Color::Rgb(159, 173, 189))),
    ]);
    Paragraph::new(title).block(block("终端交互管理台", true))
}

fn team_list(app: &App) -> List<'_> {
    let mut items: Vec<ListItem<'_>> = app
        .teams
        .iter()
        .map(|team| {
            let unlocked = app.unlocked.contains_key(&team.team_name);
            let status_text = if unlocked { "已解锁" } else { "未解锁" };
            let status_color = if unlocked {
                Color::Rgb(116, 196, 118)
            } else {
                Color::Rgb(245, 123, 113)
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    &team.team_name,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled("  •  ", Style::default().fg(Color::Rgb(104, 117, 133))),
                Span::styled(status_text, Style::default().fg(status_color)),
            ]))
        })
        .collect();
    items.push(ListItem::new(Line::from(vec![
        Span::styled("+", Style::default().fg(Color::Rgb(242, 208, 129))),
        Span::raw(" "),
        Span::styled("添加团队", Style::default().add_modifier(Modifier::BOLD)),
    ])));
    List::new(items)
        .block(block("团队", true))
        .highlight_style(Style::default().bg(Color::Rgb(43, 52, 65)).fg(Color::White))
        .highlight_symbol("▌ ")
}

fn team_detail_panel(app: &App) -> Paragraph<'_> {
    let mut lines = Vec::new();
    if let Some(team) = app.selected_team() {
        let unlocked = app.unlocked.contains_key(&team.team_name);
        lines.push(Line::from(vec![
            Span::styled("团队名: ", muted()),
            Span::styled(
                &team.team_name,
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("状态: ", muted()),
            Span::styled(
                if unlocked { "已解锁" } else { "未解锁" },
                if unlocked {
                    success_style().add_modifier(Modifier::BOLD)
                } else {
                    danger_style().add_modifier(Modifier::BOLD)
                },
            ),
        ]));
        lines.push(Line::from(format!(
            "Remote: {}",
            team.git_remote.as_deref().unwrap_or("未设置")
        )));
    }
    Paragraph::new(Text::from(lines))
        .block(block("团队信息", false))
        .wrap(Wrap { trim: false })
}

fn key_list(app: &App) -> List<'_> {
    let items = if let Some(team) = app.selected_team() {
        if app.selected_access().is_none() {
            vec![ListItem::new(format!(
                "团队 {} 尚未解锁，按 u 解锁",
                team.team_name
            ))]
        } else if app.keys.is_empty() {
            vec![ListItem::new("当前团队没有 key，按 a 新建")]
        } else {
            app.keys
                .iter()
                .map(|key| ListItem::new(Line::from(vec![Span::raw(key)])))
                .collect()
        }
    } else {
        vec![ListItem::new("没有可用团队")]
    };
    List::new(items)
        .block(block("密钥", true))
        .highlight_style(Style::default().bg(Color::Rgb(43, 52, 65)).fg(Color::White))
        .highlight_symbol("▌ ")
}

fn secret_detail_panel(app: &App) -> Paragraph<'_> {
    let mut lines = Vec::new();
    if let Some(team) = app.selected_team() {
        let unlocked = app.unlocked.contains_key(&team.team_name);
        lines.push(Line::from(vec![
            Span::styled("团队: ", muted()),
            Span::styled(
                &team.team_name,
                Style::default().add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("状态: ", muted()),
            Span::styled(
                if unlocked { "已解锁" } else { "未解锁" },
                if unlocked {
                    success_style().add_modifier(Modifier::BOLD)
                } else {
                    danger_style().add_modifier(Modifier::BOLD)
                },
            ),
        ]));
        lines.push(Line::from(""));
        if let Some(key) = app.selected_key() {
            lines.push(Line::from(vec![
                Span::styled("当前 Key: ", muted()),
                Span::styled(key, primary_style().add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(Span::styled(
                "当前 key 已选中，可直接查看 value。",
                accent_style(),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                "还没有可用的 key。",
                accent_style(),
            )));
        }
    } else {
        lines.push(Line::from("没有可用团队"));
    }
    Paragraph::new(Text::from(lines))
        .block(block("详情", false))
        .wrap(Wrap { trim: false })
}

fn help_panel(app: &App) -> Paragraph<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    lines.push(Line::from(Span::styled("当前最重要", section_style())));
    for line in primary_action_lines(app) {
        lines.push(line);
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("其他操作", section_style())));
    for line in shortcut_lines(app) {
        lines.push(line);
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("状态", section_style())));
    lines.push(Line::from(""));
    for line in split_status_lines(&app.status) {
        let style = status_style(&line);
        lines.push(Line::from(Span::styled(line, style)));
    }
    Paragraph::new(Text::from(lines))
        .block(block("快捷键说明", false))
        .wrap(Wrap { trim: false })
}

fn render_form_dialog(frame: &mut Frame, dialog: &FormDialog) {
    let height = (dialog.fields.len() as u16) * 3 + 6;
    let area = popup_area(
        frame.area(),
        64,
        height.min(frame.area().height.saturating_sub(2)),
    );
    frame.render_widget(Clear, area);
    let mut lines = Vec::new();
    for (index, field) in dialog.fields.iter().enumerate() {
        let marker = if index == dialog.index { "›" } else { " " };
        let marker_style = if index == dialog.index {
            primary_style().add_modifier(Modifier::BOLD)
        } else {
            muted()
        };
        let value = if field.secret {
            "•".repeat(field.value.chars().count())
        } else {
            field.value.clone()
        };
        lines.push(Line::from(vec![
            Span::styled(marker, marker_style),
            Span::raw(" "),
            Span::styled(field.label, muted()),
            Span::raw(": "),
            Span::styled(
                value,
                if index == dialog.index {
                    Style::default().fg(Color::White)
                } else {
                    muted()
                },
            ),
        ]));
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled("提交: ", muted()),
        Span::styled(
            dialog.submit_label,
            primary_style().add_modifier(Modifier::BOLD),
        ),
    ]));
    if let Some(error) = &dialog.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            error.clone(),
            danger_style().add_modifier(Modifier::BOLD),
        )));
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block(dialog.title, true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_confirm_dialog(frame: &mut Frame, team: &str, key: &str) {
    let area = popup_area(frame.area(), 52, 8);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled("团队: ", muted()),
                Span::raw(team.to_string()),
            ]),
            Line::from(vec![
                Span::styled("危险操作: ", danger_style().add_modifier(Modifier::BOLD)),
                Span::styled(format!("删除 key {key}"), danger_style()),
            ]),
            Line::from(""),
            Line::from(Span::styled("确认后不可恢复", danger_style())),
            Line::from(""),
            Line::from(vec![
                Span::styled("Enter", primary_style().add_modifier(Modifier::BOLD)),
                Span::raw(" 确认"),
            ]),
        ]))
        .block(block("删除确认", true)),
        area,
    );
}

fn render_secret_dialog(frame: &mut Frame, key: &str, value: &str) {
    let area = popup_area(frame.area(), 68, 12);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(Text::from(vec![
            Line::from(vec![
                Span::styled("Key: ", muted()),
                Span::styled(
                    key.to_string(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled("Value:", muted())),
            Line::from(""),
            Line::from(Span::styled(
                value.to_string(),
                Style::default().fg(Color::Rgb(242, 208, 129)),
            )),
            Line::from(""),
            Line::from("Esc 关闭"),
        ]))
        .block(block("密钥值", true))
        .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_help_dialog(frame: &mut Frame, app: &App) {
    let area = popup_area(frame.area(), 72, 16);
    frame.render_widget(Clear, area);
    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled("当前最重要", section_style())));
    for line in primary_action_lines(app) {
        lines.push(line);
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("全部操作", section_style())));
    for line in shortcut_lines(app) {
        lines.push(line);
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(block("帮助", true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn block(title: &'static str, active: bool) -> Block<'static> {
    let border = if active {
        Color::Rgb(242, 208, 129)
    } else {
        Color::Rgb(78, 92, 110)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .title(title)
}

fn list_state(selected: usize) -> ListState {
    let mut state = ListState::default();
    state.select(Some(selected));
    state
}

fn popup_area(area: Rect, width_percent: u16, height: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(width_percent)])
        .flex(Flex::Center)
        .split(vertical[0])[0]
}

fn primary_action_lines(app: &App) -> Vec<Line<'static>> {
    match &app.page {
        Page::TeamList => {
            if app.is_add_team_selected() || app.selected_team().is_none() {
                vec![action_line("c", "创建团队", primary_style())]
            } else if let Some(team) = app.selected_team() {
                if !app.unlocked.contains_key(&team.team_name) {
                    vec![action_line("u", "先解锁当前团队", primary_style())]
                } else {
                    vec![action_line("s", "同步全部团队", primary_style())]
                }
            } else {
                Vec::new()
            }
        }
        Page::TeamDetail { .. } => {
            if let Some(team) = app.selected_team() {
                if !app.unlocked.contains_key(&team.team_name) {
                    vec![action_line("u", "先解锁当前团队", primary_style())]
                } else if app.keys.is_empty() {
                    vec![action_line("a", "新增第一个 key", primary_style())]
                } else {
                    vec![
                        action_line("e", "更新当前 key", primary_style()),
                        action_line("s", "同步当前团队", accent_style()),
                    ]
                }
            } else {
                Vec::new()
            }
        }
    }
}

fn shortcut_lines(app: &App) -> Vec<Line<'static>> {
    match app.page {
        Page::TeamList => vec![
            action_line("c", "创建团队", primary_style()),
            action_line("u", "解锁团队", accent_style()),
            action_line("r", "设置/清空 remote", accent_style()),
            action_line("s", "同步全部团队", success_style()),
            action_line("x", "删除当前团队", danger_style()),
            action_line("h", "帮助", muted()),
            action_line("q", "退出", muted()),
        ],
        Page::TeamDetail { .. } => vec![
            action_line("a", "新增 key", primary_style()),
            action_line("e", "更新 key", accent_style()),
            action_line("d", "删除当前 key", danger_style()),
            action_line("s", "同步当前团队", success_style()),
            action_line("u", "解锁当前团队", accent_style()),
            action_line("h", "帮助", muted()),
            action_line("q", "退出", muted()),
        ],
    }
}
