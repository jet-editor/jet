use crate::{
    buffer::rope::{BufferEdit, EditorBuffer},
    lsp::types::{CodeActionItem, CompletionItem, Location, SignatureHelpInfo},
};
use lsp_types::{TextEdit, Url, WorkspaceEdit};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn completion_kind_label(kind: Option<u32>) -> &'static str {
    match kind {
        Some(3) => "fn",
        Some(6) => "var",
        Some(7) => "kw",
        Some(5) => "fld",
        Some(8) => "mod",
        Some(22) => "typ",
        Some(14) => "mac",
        Some(15) => "sym",
        _ => "  ",
    }
}

pub fn filter_completions(items: &[CompletionItem], prefix: &str) -> Vec<usize> {
    if prefix.is_empty() {
        return (0..items.len()).collect();
    }
    let lower = prefix.to_lowercase();
    let mut indices: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.label.to_lowercase().contains(&lower))
        .map(|(idx, _)| idx)
        .collect();
    indices.sort_by_key(|idx| {
        let label = &items[*idx].label;
        (!label.to_lowercase().starts_with(&lower), label.len())
    });
    indices
}

pub fn word_prefix_at(buffer: &EditorBuffer, row: usize, col: usize) -> (usize, String) {
    let line = buffer.line_string(row);
    let chars: Vec<char> = line.chars().collect();
    let col = col.min(chars.len());
    let mut word_start = col;
    while word_start > 0 && is_identifier_char(chars[word_start - 1]) {
        word_start -= 1;
    }
    let prefix: String = chars[word_start..col].iter().collect();
    (word_start, prefix)
}

pub fn replace_completion_range(
    buffer: &mut EditorBuffer,
    row: usize,
    start_col: usize,
    end_col: usize,
    text: &str,
) -> usize {
    let start = buffer.char_idx(row, start_col);
    let end = buffer.char_idx(row, end_col);
    if start < end {
        buffer.remove(start..end);
    }
    buffer.insert(start, text);
    start + text.chars().count()
}

pub fn signature_help_lines(signature: &SignatureHelpInfo) -> Vec<String> {
    let mut lines = vec![format!("  {}", signature.label.trim())];
    if let Some(active) = signature.active_parameter {
        if let Some(param) = signature.parameters.get(active) {
            lines.push(format!("  ▸ {param}"));
        }
    }
    if let Some(docs) = &signature.documentation {
        for line in docs.lines().take(3) {
            lines.push(format!("  {}", line.trim()));
        }
    }
    lines
}

pub fn location_label(location: &Location) -> String {
    format!(
        "{}:{}:{}",
        location.uri,
        location.range.start.line + 1,
        location.range.start.character + 1
    )
}

pub fn same_file_location(current: Option<&Path>, location: &Location) -> bool {
    let Some(path) = file_path_from_uri(&location.uri) else {
        return false;
    };
    current.is_some_and(|current| paths_equal(current, &path))
}

pub fn file_path_from_uri(uri: &str) -> Option<PathBuf> {
    lsp_types::Url::parse(uri).ok()?.to_file_path().ok()
}

pub fn paths_equal(left: &Path, right: &Path) -> bool {
    left.canonicalize().unwrap_or_else(|_| left.to_path_buf())
        == right.canonicalize().unwrap_or_else(|_| right.to_path_buf())
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

pub fn text_edits_for_file(raw: &Value, current: Option<&Path>) -> Vec<TextEdit> {
    let Ok(action) = serde_json::from_value::<lsp_types::CodeAction>(raw.clone()) else {
        return Vec::new();
    };
    action
        .edit
        .map(|edit| workspace_text_edits_for_file(edit, current))
        .unwrap_or_default()
}

pub fn text_edits_from_action(action: &CodeActionItem, current: Option<&Path>) -> Vec<TextEdit> {
    text_edits_for_file(&action.raw, current)
}

pub fn text_edits_from_workspace_value(raw: &Value, current: Option<&Path>) -> Vec<TextEdit> {
    let Ok(edit) = serde_json::from_value::<WorkspaceEdit>(raw.clone()) else {
        return Vec::new();
    };
    workspace_text_edits_for_file(edit, current)
}

fn workspace_text_edits_for_file(edit: WorkspaceEdit, current: Option<&Path>) -> Vec<TextEdit> {
    let mut out = Vec::new();
    if let Some(changes) = edit.changes {
        for (uri, edits) in changes {
            if uri_matches_file(&uri, current) {
                out.extend(edits);
            }
        }
    }
    if let Some(changes) = edit.document_changes {
        match changes {
            lsp_types::DocumentChanges::Edits(edits) => {
                for text_edit in edits {
                    push_text_document_edits(&mut out, text_edit, current);
                }
            }
            lsp_types::DocumentChanges::Operations(ops) => {
                for op in ops {
                    if let lsp_types::DocumentChangeOperation::Edit(text_edit) = op {
                        push_text_document_edits(&mut out, text_edit, current);
                    }
                }
            }
        }
    }
    out
}

fn push_text_document_edits(
    out: &mut Vec<TextEdit>,
    text_edit: lsp_types::TextDocumentEdit,
    current: Option<&Path>,
) {
    if !uri_matches_file(&text_edit.text_document.uri, current) {
        return;
    }
    for edit in text_edit.edits {
        match edit {
            lsp_types::OneOf::Left(text_edit) => out.push(text_edit),
            lsp_types::OneOf::Right(annotated) => out.push(annotated.text_edit),
        }
    }
}

fn uri_matches_file(uri: &Url, current: Option<&Path>) -> bool {
    let Some(path) = file_path_from_uri(uri.as_str()) else {
        return false;
    };
    current.is_some_and(|current| paths_equal(current, &path))
}

pub fn apply_text_edits(buffer: &mut EditorBuffer, edits: &[TextEdit]) -> Vec<BufferEdit> {
    let mut applied = Vec::new();
    for edit in edits.iter().rev() {
        let start = buffer.char_idx(
            edit.range.start.line as usize,
            edit.range.start.character as usize,
        );
        let end = buffer.char_idx(
            edit.range.end.line as usize,
            edit.range.end.character as usize,
        );
        if start > end {
            continue;
        }
        if let Some(buffer_edit) = buffer.remove_with_edit(start..end) {
            applied.push(buffer_edit);
        }
        applied.push(buffer.insert_with_edit(start, &edit.new_text));
    }
    applied
}
