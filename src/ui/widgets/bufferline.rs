use crate::highlight::theme::Theme;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct BufferTab {
    pub label: String,
    pub active: bool,
    pub modified: bool,
}

pub fn render_themed(tabs: &[BufferTab], max_width: usize, theme: &Theme) -> String {
    if tabs.len() <= 1 {
        return String::new();
    }

    let active = theme.ansi_for_theme_key("status.mode", false);
    let inactive = theme.ansi_for_theme_key("popup", false);
    let reset = theme.ansi_for_theme_key("popup", true);
    let mut line = tabs
        .iter()
        .map(|tab| {
            let color = if tab.active { &active } else { &inactive };
            let dirty = if tab.modified { "*" } else { "" };
            format!("{color} {}{dirty} {reset}", tab.label)
        })
        .collect::<Vec<_>>()
        .join(" ");

    if line.chars().count() > max_width {
        line = line.chars().take(max_width.saturating_sub(1)).collect();
        line.push('…');
    }
    line
}

pub fn render(tabs: &[BufferTab], max_width: usize) -> String {
    if tabs.len() <= 1 {
        return String::new();
    }

    let mut line = tabs
        .iter()
        .map(|tab| {
            let marker = if tab.active { "[" } else { " " };
            let end = if tab.active { "]" } else { "" };
            let dirty = if tab.modified { "*" } else { "" };
            format!("{marker}{}{dirty}{end}", tab.label, dirty = dirty)
        })
        .collect::<Vec<_>>()
        .join(" ");

    if line.chars().count() > max_width {
        line = line.chars().take(max_width.saturating_sub(1)).collect();
        line.push('…');
    }
    line
}

pub fn label_for_path(path: Option<&Path>) -> String {
    path.and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "[scratch]".to_string())
}
