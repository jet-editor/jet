use crate::{
    buffer::rope::EditorBuffer,
    highlight::treesitter::{tree_sitter_language, Language},
};
use tree_sitter::{Node, Query, QueryCursor, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextObjectRange {
    pub start: usize,
    pub end: usize,
}

pub fn expand_to_parent(
    buffer: &EditorBuffer,
    _language: Language,
    tree: &Tree,
    char_idx: usize,
) -> Option<TextObjectRange> {
    let root = tree.root_node();
    let byte = buffer.char_to_byte(char_idx);
    let mut node = root.named_descendant_for_byte_range(byte, byte)?;
    if node.byte_range().contains(&byte) && node.parent().is_some() {
        node = node.parent()?;
    }
    while let Some(parent) = node.parent() {
        if parent.kind() == "source_file" || parent.kind() == "crate" {
            break;
        }
        node = parent;
    }
    Some(node_to_range(buffer, node))
}

pub fn function_around(
    buffer: &EditorBuffer,
    language: Language,
    tree: &Tree,
    char_idx: usize,
) -> Option<TextObjectRange> {
    let query_src = function_query(language)?;
    let language_ref = tree_sitter_language(language)?;
    let query = Query::new(&language_ref, query_src).ok()?;
    let byte = buffer.char_to_byte(char_idx);
    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let source = buffer.source_bytes();
    let matches = cursor.matches(&query, root, source.as_slice());
    let mut best: Option<TextObjectRange> = None;
    for m in matches {
        for capture in m.captures {
            let node = capture.node;
            let range = node.byte_range();
            if range.start <= byte && byte <= range.end {
                let candidate = node_to_range(buffer, node);
                if best.as_ref().is_none_or(|current| {
                    candidate.end - candidate.start > current.end - current.start
                }) {
                    best = Some(candidate);
                }
            }
        }
    }
    best
}

fn function_query(language: Language) -> Option<&'static str> {
    match language {
        Language::Rust => Some(
            r#"
(function_item) @function
"#,
        ),
        Language::Python => Some(
            r#"
(function_definition) @function
"#,
        ),
        Language::JavaScript | Language::TypeScript => Some(
            r#"
(function_declaration) @function
(method_definition) @function
"#,
        ),
        _ => None,
    }
}

fn node_to_range(buffer: &EditorBuffer, node: Node) -> TextObjectRange {
    let start = buffer.byte_to_char(node.start_byte());
    let end = buffer.byte_to_char(node.end_byte());
    TextObjectRange { start, end }
}
