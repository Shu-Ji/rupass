use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub(crate) fn muted() -> Style {
    Style::default().fg(Color::Rgb(140, 154, 171))
}

pub(crate) fn section_style() -> Style {
    Style::default()
        .fg(Color::Rgb(242, 208, 129))
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn primary_style() -> Style {
    Style::default().fg(Color::Rgb(242, 208, 129))
}

pub(crate) fn accent_style() -> Style {
    Style::default().fg(Color::Rgb(120, 188, 255))
}

pub(crate) fn success_style() -> Style {
    Style::default().fg(Color::Rgb(116, 196, 118))
}

pub(crate) fn danger_style() -> Style {
    Style::default().fg(Color::Rgb(245, 123, 113))
}

pub(crate) fn action_line(key: &'static str, label: &'static str, style: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{key}: "), style.add_modifier(Modifier::BOLD)),
        Span::styled(label, style),
    ])
}

pub(crate) fn status_style(line: &str) -> Style {
    if line.starts_with("错误:") {
        return danger_style().add_modifier(Modifier::BOLD);
    }
    if line.starts_with("已删除") {
        return danger_style();
    }
    if line.starts_with("已") {
        return success_style();
    }
    primary_style()
}

pub(crate) fn split_status_lines(status: &str) -> Vec<String> {
    if status.contains(" · ") {
        return status.split(" · ").map(ToOwned::to_owned).collect();
    }
    vec![status.to_string()]
}
