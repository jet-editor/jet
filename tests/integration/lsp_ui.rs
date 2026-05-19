use jet::{
    buffer::rope::EditorBuffer,
    editor::lsp_ui,
    lsp::types::{CodeActionItem, CompletionItem, Location, Position, Range, SignatureHelpInfo},
};
#[test]
fn completion_filter_prefers_prefix_matches() {
    let items = vec![
        CompletionItem {
            label: "println!".to_string(),
            detail: None,
            documentation: None,
            insert_text: None,
            kind: Some(3),
            raw: serde_json::json!({}),
        },
        CompletionItem {
            label: "print".to_string(),
            detail: None,
            documentation: None,
            insert_text: None,
            kind: Some(3),
            raw: serde_json::json!({}),
        },
        CompletionItem {
            label: "Vec".to_string(),
            detail: None,
            documentation: None,
            insert_text: None,
            kind: Some(22),
            raw: serde_json::json!({}),
        },
    ];
    let filtered = lsp_ui::filter_completions(&items, "pr");
    assert_eq!(filtered.len(), 2);
    assert_eq!(items[filtered[0]].label, "print");
}

#[test]
fn word_prefix_at_cursor_extracts_identifier_fragment() {
    let buffer = EditorBuffer::from_text("let foo_bar = 1;\n");
    let (start, prefix) = lsp_ui::word_prefix_at(&buffer, 0, 8);
    assert_eq!(start, 4);
    assert_eq!(prefix, "foo_");
}

#[test]
fn replace_completion_range_inserts_selected_label() {
    let mut buffer = EditorBuffer::from_text("hello\n");
    let after = lsp_ui::replace_completion_range(&mut buffer, 0, 1, 4, "ip");
    assert_eq!(buffer.to_string(), "hipo\n");
    assert_eq!(after, 3);
}

#[test]
fn code_action_workspace_edit_applies_to_matching_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("main.rs");
    let url = lsp_types::Url::from_file_path(&file).expect("file url");
    let raw = serde_json::json!({
        "title": "insert x",
        "edit": {
            "changes": {
                url.as_str(): [{
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 0}
                    },
                    "newText": "x"
                }]
            }
        }
    });
    let action = CodeActionItem {
        title: "insert x".to_string(),
        kind: None,
        raw,
    };
    let edits = lsp_ui::text_edits_from_action(&action, Some(&file));
    assert_eq!(edits.len(), 1);
    let mut buffer = EditorBuffer::from_text("abc\n");
    lsp_ui::apply_text_edits(&mut buffer, &edits);
    assert_eq!(buffer.to_string(), "xabc\n");
}

#[test]
fn code_action_ignores_edits_for_other_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("main.rs");
    let other = dir.path().join("other.rs");
    let other_url = lsp_types::Url::from_file_path(&other).expect("other url");
    let raw = serde_json::json!({
        "title": "remote",
        "edit": {
            "changes": {
                other_url.as_str(): [{
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 0}
                    },
                    "newText": "z"
                }]
            }
        }
    });
    let action = CodeActionItem {
        title: "remote".to_string(),
        kind: None,
        raw,
    };
    let edits = lsp_ui::text_edits_from_action(&action, Some(&file));
    assert!(edits.is_empty());
}

#[test]
fn signature_help_lines_highlight_active_parameter() {
    let signature = SignatureHelpInfo {
        label: "fn foo(bar: i32, baz: str)".to_string(),
        documentation: Some("docs".to_string()),
        active_parameter: Some(1),
        parameters: vec!["bar: i32".to_string(), "baz: str".to_string()],
    };
    let lines = lsp_ui::signature_help_lines(&signature);
    assert!(lines[0].contains("fn foo"));
    assert!(lines.iter().any(|line| line.contains("baz: str")));
}

#[test]
fn rename_workspace_edit_applies_to_current_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("main.rs");
    let url = lsp_types::Url::from_file_path(&file).expect("file url");
    let raw = serde_json::json!({
        "changes": {
            url.as_str(): [{
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 3}
                },
                "newText": "new"
            }]
        }
    });
    let edits = lsp_ui::text_edits_from_workspace_value(&raw, Some(&file));
    assert_eq!(edits.len(), 1);
    let mut buffer = EditorBuffer::from_text("old\n");
    lsp_ui::apply_text_edits(&mut buffer, &edits);
    assert_eq!(buffer.to_string(), "new\n");
}

#[test]
fn location_label_formats_uri_and_position() {
    let location = Location {
        uri: "file:///tmp/main.rs".to_string(),
        range: Range {
            start: Position {
                line: 4,
                character: 2,
            },
            end: Position {
                line: 4,
                character: 5,
            },
        },
    };
    assert_eq!(lsp_ui::location_label(&location), "file:///tmp/main.rs:5:3");
}
