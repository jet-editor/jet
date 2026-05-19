use crate::{
    highlight::theme::Theme,
    plugin::api::{GutterMark, VirtualText},
};

pub fn virtual_text_suffix(text: &VirtualText, theme: &Theme, max_chars: usize) -> String {
    let trimmed = text.text.trim();
    let truncated = if trimmed.chars().count() > max_chars {
        let short: String = trimmed.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{short}…")
    } else {
        trimmed.to_string()
    };
    let key = if text.group.is_empty() {
        "popup"
    } else {
        text.group.as_str()
    };
    format!(
        "  {}⎸ {}\x1b[0m",
        theme.ansi_for_theme_key(key, false),
        truncated
    )
}

pub fn gutter_marker(mark: &GutterMark, theme: &Theme) -> String {
    let ch = mark.mark.chars().next().unwrap_or('•');
    let key = if mark.group.is_empty() {
        "popup"
    } else {
        mark.group.as_str()
    };
    format!("{}{ch}\x1b[0m", theme.ansi_for_theme_key(key, true))
}
