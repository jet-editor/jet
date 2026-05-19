use crate::{
    highlight::theme::Theme,
    lsp::types::{Diagnostic, DiagnosticSeverity},
};

pub fn render_count(diagnostics: &[Diagnostic]) -> String {
    match diagnostics.len() {
        0 => "no diagnostics".to_string(),
        1 => "1 diagnostic".to_string(),
        count => format!("{} diagnostics", count),
    }
}

pub fn severity_theme_key(severity: Option<DiagnosticSeverity>) -> &'static str {
    match severity {
        Some(DiagnosticSeverity::Error) => "diagnostic.error",
        Some(DiagnosticSeverity::Warning) => "diagnostic.warning",
        Some(DiagnosticSeverity::Information) => "diagnostic.info",
        Some(DiagnosticSeverity::Hint) => "diagnostic.hint",
        None => "diagnostic.hint",
    }
}

pub fn severity_marker(severity: Option<DiagnosticSeverity>) -> char {
    match severity {
        Some(DiagnosticSeverity::Error) => 'E',
        Some(DiagnosticSeverity::Warning) => 'W',
        Some(DiagnosticSeverity::Information) => 'I',
        Some(DiagnosticSeverity::Hint) => 'H',
        None => '?',
    }
}

pub fn picker_label(diagnostic: &Diagnostic, theme: &Theme, max_chars: usize) -> String {
    let line = diagnostic.range.start.line + 1;
    let col = diagnostic.range.start.character + 1;
    let marker = severity_marker(diagnostic.severity);
    let message = diagnostic.message.trim();
    let truncated = if message.chars().count() > max_chars {
        let short: String = message.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{short}…")
    } else {
        message.to_string()
    };
    format!(
        "{}{}:{}:{} {}{}\x1b[0m",
        theme.ansi_for_theme_key(severity_theme_key(diagnostic.severity), false),
        marker,
        line,
        col,
        truncated,
        theme.ansi_for_theme_key("popup", true)
    )
}

pub fn inline_suffix(diagnostic: &Diagnostic, theme: &Theme, max_chars: usize) -> String {
    let message = diagnostic.message.trim();
    let truncated = if message.chars().count() > max_chars {
        let short: String = message.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{short}…")
    } else {
        message.to_string()
    };
    format!(
        "  {}⎸ {}\x1b[0m",
        theme.ansi_for_theme_key(severity_theme_key(diagnostic.severity), false),
        truncated
    )
}
