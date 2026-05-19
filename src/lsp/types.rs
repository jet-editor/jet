use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range,
    pub message: String,
    pub severity: Option<DiagnosticSeverity>,
    pub source: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
    pub kind: Option<u32>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HoverInfo {
    pub markdown: String,
    pub range: Option<Range>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SignatureHelpInfo {
    pub label: String,
    pub documentation: Option<String>,
    pub active_parameter: Option<usize>,
    pub parameters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub kind: u32,
    pub range: Range,
    pub selection_range: Range,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeActionItem {
    pub title: String,
    pub kind: Option<String>,
    pub raw: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlayHintItem {
    pub line: u32,
    pub character: u32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldRange {
    pub start_line: u32,
    pub end_line: u32,
    pub kind: Option<String>,
}

impl From<lsp_types::Position> for Position {
    fn from(position: lsp_types::Position) -> Self {
        Self {
            line: position.line,
            character: position.character,
        }
    }
}

impl From<Position> for lsp_types::Position {
    fn from(position: Position) -> Self {
        Self {
            line: position.line,
            character: position.character,
        }
    }
}

impl From<lsp_types::Range> for Range {
    fn from(range: lsp_types::Range) -> Self {
        Self {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

impl From<Range> for lsp_types::Range {
    fn from(range: Range) -> Self {
        Self {
            start: range.start.into(),
            end: range.end.into(),
        }
    }
}

impl From<lsp_types::DiagnosticSeverity> for DiagnosticSeverity {
    fn from(severity: lsp_types::DiagnosticSeverity) -> Self {
        if severity == lsp_types::DiagnosticSeverity::ERROR {
            Self::Error
        } else if severity == lsp_types::DiagnosticSeverity::WARNING {
            Self::Warning
        } else if severity == lsp_types::DiagnosticSeverity::INFORMATION {
            Self::Information
        } else {
            Self::Hint
        }
    }
}

impl From<lsp_types::Diagnostic> for Diagnostic {
    fn from(diagnostic: lsp_types::Diagnostic) -> Self {
        Self {
            range: diagnostic.range.into(),
            message: diagnostic.message,
            severity: diagnostic.severity.map(Into::into),
            source: diagnostic.source,
            code: diagnostic.code.map(|code| match code {
                lsp_types::NumberOrString::Number(n) => n.to_string(),
                lsp_types::NumberOrString::String(s) => s,
            }),
        }
    }
}

impl From<Diagnostic> for lsp_types::Diagnostic {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            range: diagnostic.range.into(),
            severity: diagnostic.severity.map(Into::into),
            code: diagnostic.code.map(lsp_types::NumberOrString::String),
            source: diagnostic.source,
            message: diagnostic.message,
            ..Default::default()
        }
    }
}

impl From<DiagnosticSeverity> for lsp_types::DiagnosticSeverity {
    fn from(severity: DiagnosticSeverity) -> Self {
        match severity {
            DiagnosticSeverity::Error => lsp_types::DiagnosticSeverity::ERROR,
            DiagnosticSeverity::Warning => lsp_types::DiagnosticSeverity::WARNING,
            DiagnosticSeverity::Information => lsp_types::DiagnosticSeverity::INFORMATION,
            DiagnosticSeverity::Hint => lsp_types::DiagnosticSeverity::HINT,
        }
    }
}

impl From<lsp_types::Location> for Location {
    fn from(location: lsp_types::Location) -> Self {
        Self {
            uri: location.uri.to_string(),
            range: location.range.into(),
        }
    }
}
