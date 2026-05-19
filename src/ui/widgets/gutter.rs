use crate::{git::LineStatus, highlight::theme::Theme, lsp::types::DiagnosticSeverity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GutterSign {
    #[default]
    None,
    GitAdded,
    GitModified,
    FoldClosed,
    DiagnosticError,
    DiagnosticWarning,
    DiagnosticInfo,
    DiagnosticHint,
}

pub fn number_width(line_count: usize) -> usize {
    line_count.max(1).to_string().len()
}

pub fn width(line_count: usize) -> usize {
    number_width(line_count) + 1
}

pub fn git_sign(status: Option<LineStatus>) -> GutterSign {
    match status {
        Some(LineStatus::Added) => GutterSign::GitAdded,
        Some(LineStatus::Modified) => GutterSign::GitModified,
        None => GutterSign::None,
    }
}

pub fn diagnostic_sign(severity: Option<DiagnosticSeverity>) -> GutterSign {
    match severity {
        Some(DiagnosticSeverity::Error) => GutterSign::DiagnosticError,
        Some(DiagnosticSeverity::Warning) => GutterSign::DiagnosticWarning,
        Some(DiagnosticSeverity::Information) => GutterSign::DiagnosticInfo,
        Some(DiagnosticSeverity::Hint) => GutterSign::DiagnosticHint,
        None => GutterSign::None,
    }
}

pub fn merge_signs(mut primary: GutterSign, secondary: GutterSign) -> GutterSign {
    if matches!(primary, GutterSign::None) {
        primary = secondary;
    }
    primary
}

pub fn render_line_number(line: usize, width: usize) -> String {
    format!("{:>width$} ", line + 1, width = width)
}

pub fn render_gutter(
    line: usize,
    width: usize,
    sign: GutterSign,
    current_line: bool,
    theme: &Theme,
) -> String {
    let number_width = width.saturating_sub(1);
    let marker = themed_sign_marker(sign, theme);
    let number = line + 1;
    if current_line {
        format!(
            "\x1b[7m{marker}{:>number_width$}\x1b[0m ",
            number,
            number_width = number_width
        )
    } else {
        format!(
            "{marker}{:>number_width$} ",
            number,
            number_width = number_width
        )
    }
}

fn themed_sign_marker(sign: GutterSign, theme: &Theme) -> String {
    let (ch, key) = match sign {
        GutterSign::None => return " ".to_string(),
        GutterSign::GitAdded => ('+', "git.added"),
        GutterSign::GitModified => ('~', "git.modified"),
        GutterSign::FoldClosed => ('▾', "fold.closed"),
        GutterSign::DiagnosticError => ('E', "diagnostic.error"),
        GutterSign::DiagnosticWarning => ('W', "diagnostic.warning"),
        GutterSign::DiagnosticInfo => ('I', "diagnostic.info"),
        GutterSign::DiagnosticHint => ('H', "diagnostic.hint"),
    };
    format!("{}{ch}\x1b[0m", theme.ansi_for_theme_key(key, true))
}
