use crate::buffer::rope::{BufferEdit, EditorBuffer};
use anyhow::Result;
use lsp_types as lsp;
use std::path::Path;

pub fn position_from_char(buffer: &EditorBuffer, char_idx: usize) -> lsp::Position {
    let (row, col) = buffer.char_to_line_col(char_idx);
    lsp::Position::new(row as u32, col as u32)
}

pub fn range_from_chars(buffer: &EditorBuffer, start: usize, end: usize) -> lsp::Range {
    lsp::Range::new(
        position_from_char(buffer, start),
        position_from_char(buffer, end),
    )
}

pub fn incremental_change(
    buffer: &EditorBuffer,
    edit: &BufferEdit,
) -> lsp::TextDocumentContentChangeEvent {
    lsp::TextDocumentContentChangeEvent {
        range: Some(range_from_chars(buffer, edit.start_char, edit.old_end_char)),
        range_length: None,
        text: edit.new_text.clone(),
    }
}

pub fn did_change_params(
    path: &Path,
    version: i32,
    changes: Vec<lsp::TextDocumentContentChangeEvent>,
) -> Result<lsp::DidChangeTextDocumentParams> {
    Ok(lsp::DidChangeTextDocumentParams {
        text_document: lsp::VersionedTextDocumentIdentifier {
            uri: super::client::path_to_url(path)?,
            version,
        },
        content_changes: changes,
    })
}
