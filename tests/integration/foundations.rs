use jet::{
    buffer::{
        crdt::CrdtDocument,
        rope::{BufferEdit, EditorBuffer},
    },
    collab::{protocol::CollabMessage, session::CollaborationSession},
    highlight::{
        grammars::{detect_by_shebang, GrammarManager},
        treesitter::{HighlightEngine, Language, TreeSitterHighlighter},
    },
    lsp::{
        client::{
            parse_completion_result, parse_goto_result, parse_hover_result, parse_lsp_notification,
            parse_signature_help_result, LspClient, LspEvent,
        },
        servers, sync, transport,
        types::DiagnosticSeverity,
    },
    plugin::{
        api::{BufferSnapshot, HostCommand, PluginEvent},
        manager::PluginManager,
        manifest::PluginManifest,
    },
};
use lsp_types as lsp;
use serde_json::json;
use std::{env, time::Duration};
use tempfile::TempDir;
use tokio::io::{AsyncWriteExt, BufReader};
use uuid::Uuid;

fn guest_plugin_wasm(message: &str) -> Vec<u8> {
    let len = message.len();
    wat::parse_str(format!(
        r#"
(module
  (import "jet" "emit_message" (func $emit_message (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 16) "{message}")
  (func (export "on_buffer_open")
    i32.const 16
    i32.const {len}
    call $emit_message))
"#
    ))
    .unwrap()
}

#[test]
fn mmap_edit_uses_overlay_and_saves_full_file() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("large.txt");
    let text = "alpha\nbeta\ngamma\n";
    std::fs::write(&path, text).unwrap();

    let mut buffer = EditorBuffer::open(&path).unwrap();
    assert!(buffer.is_mapped());

    buffer.insert(0, ">");
    assert!(buffer.is_mapped());
    assert_eq!(buffer.mapped_overlay_count(), 1);

    let out = temp.path().join("out.txt");
    buffer.save_to(&out).unwrap();
    assert_eq!(
        std::fs::read_to_string(out).unwrap(),
        ">alpha\nbeta\ngamma\n"
    );
}

#[test]
fn mmap_scattered_window_overlays_save_full_file() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("large.txt");
    let mut content = String::new();
    for idx in 0..500 {
        content.push_str(&format!("line-{idx:04}\n"));
    }
    std::fs::write(&path, &content).unwrap();

    let mut mmap = jet::buffer::mmap::MmapBuffer::open(&path).unwrap();
    mmap.load_window_at(0, 50).unwrap();
    mmap.insert(0, ">");
    let mid = mmap.as_bytes().len() / 2;
    mmap.load_window_at(mid, 50).unwrap();
    mmap.insert(0, "<");

    let out = temp.path().join("out.txt");
    mmap.save_to(&out).unwrap();
    let saved = std::fs::read_to_string(out).unwrap();
    assert!(saved.starts_with(">line-0000"));
    assert!(saved.contains("<line-0250"));
}

#[test]
fn lsp_registry_maps_common_languages() {
    let rust = servers::server_definition_for_path(std::path::Path::new("src/main.rs")).unwrap();
    assert_eq!(rust.binary, "rust-analyzer");
    assert!(rust.root_markers.contains(&"Cargo.toml"));

    let ts = servers::server_definition_for_path(std::path::Path::new("app.ts")).unwrap();
    assert_eq!(ts.args, &["--stdio"]);
}

#[tokio::test]
async fn lsp_transport_reads_content_length_framed_json() {
    let value = json!({"jsonrpc": "2.0", "id": 7, "result": true});
    let body = value.to_string();
    let bytes = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    let mut reader = BufReader::new(bytes.as_bytes());

    let parsed = transport::read_message(&mut reader).await.unwrap().unwrap();
    assert_eq!(transport::response_id(&parsed).unwrap(), Some(7));

    let mut out = Vec::new();
    transport::write_message(&mut out, &value).await.unwrap();
    out.flush().await.unwrap();
    assert!(String::from_utf8(out)
        .unwrap()
        .starts_with("Content-Length: "));
}

#[test]
fn rust_highlighter_uses_tree_sitter_captures_for_core_tokens() {
    let mut highlighter = TreeSitterHighlighter::new(Language::Rust);
    assert!(highlighter.tree_sitter_available());
    assert_eq!(highlighter.last_engine(), HighlightEngine::Lexical);

    let lines = vec![
        "pub fn greet(name: &str) -> String {".to_string(),
        "    let message = \"hello\"; // comment".to_string(),
        "    message.to_string()".to_string(),
        "}".to_string(),
    ];
    let highlighted = highlighter.highlight_visible_window(&lines, 0, 0..lines.len(), 1);
    assert_eq!(highlighted.engine, HighlightEngine::TreeSitter);
    assert_eq!(highlighter.last_engine(), HighlightEngine::TreeSitter);

    let groups: Vec<_> = highlighted
        .spans
        .iter()
        .map(|(_, span)| span.group)
        .collect();
    assert!(
        groups.contains(&"keyword"),
        "expected Rust keyword captures: {groups:?}"
    );
    assert!(
        groups.contains(&"function"),
        "expected Rust function captures: {groups:?}"
    );
    assert!(
        groups.contains(&"string"),
        "expected Rust string captures: {groups:?}"
    );
    assert!(
        groups.contains(&"comment"),
        "expected Rust comment captures: {groups:?}"
    );
}

#[test]
fn highlighter_detects_languages_and_compiled_rust_grammar_availability() {
    assert_eq!(
        TreeSitterHighlighter::detect(std::path::Path::new("main.py"), None),
        Language::Python
    );
    assert_eq!(
        detect_by_shebang("#!/usr/bin/env python3"),
        Some(Language::Python)
    );

    let temp = TempDir::new().unwrap();
    let manager = GrammarManager::new(temp.path().join("grammars"));
    assert!(manager.is_available(Language::Rust));
    assert!(manager
        .package_path("rust")
        .to_string_lossy()
        .contains("rust"));
    assert!(manager.is_available(Language::Python));
    assert!(manager.is_available(Language::Json));
}

#[test]
fn lsp_incremental_change_uses_document_range() {
    let buffer = EditorBuffer::from_text("fn main() {}\n");
    let edit = BufferEdit {
        start_char: 3,
        old_end_char: 3,
        new_end_char: 4,
        old_text: String::new(),
        new_text: "x".to_string(),
    };
    let change = sync::incremental_change(&buffer, &edit);
    assert_eq!(change.text, "x");
    assert!(change.range.is_some());
    let range = change.range.unwrap();
    assert_eq!(range.start.line, 0);
    assert_eq!(range.start.character, 3);
}

#[tokio::test]
async fn lsp_client_starts_fake_server_and_initializes() {
    let fake_lsp = env::var("CARGO_BIN_EXE_fake-lsp")
        .expect("fake-lsp binary must be built for integration tests");
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();

    let mut client = LspClient::new(temp.path().to_path_buf(), fake_lsp);
    client.start().expect("fake-lsp should start");
    for _ in 0..100 {
        if client.is_initialized() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(client.is_initialized());
    client
        .did_open(&path, "rust", "fn main() {}\n".to_string(), 1)
        .expect("didOpen should succeed");
}

#[test]
fn plugin_manager_parses_manifest_and_dispatches_hooks() {
    let temp = TempDir::new().unwrap();
    let plugin_dir = temp.path().join("plugins").join("demo");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    std::fs::write(
        plugin_dir.join("plugin.wasm"),
        guest_plugin_wasm("guest-opened"),
    )
    .unwrap();
    std::fs::write(
        plugin_dir.join("plugin.toml"),
        r#"
name = "demo"
version = "0.1.0"
permissions = ["filesystem:read", "ui:write"]

[hooks]
on_buffer_open = true
"#,
    )
    .unwrap();

    let manifest = PluginManifest::from_toml(
        r#"
name = "inline"
version = "1.2.3"
"#,
    )
    .unwrap();
    assert_eq!(manifest.engine, "jet >= 1.0.0");

    let mut manager = PluginManager::new(temp.path().join("plugins"));
    assert_eq!(manager.discover().unwrap(), 1);
    assert_eq!(manager.plugin_count(), 1);
    let called = manager
        .dispatch(&PluginEvent::BufferOpen(BufferSnapshot {
            path: None,
            selections: Vec::new(),
            visible_lines: Vec::new(),
        }))
        .unwrap();
    assert_eq!(called, 1);
    assert_eq!(manager.messages(), &["guest-opened".to_string()]);

    manager.apply_host_command(HostCommand::RegisterCommand {
        name: "demo.hello".to_string(),
        description: "Say hello".to_string(),
    });
    manager.apply_host_command(HostCommand::RegisterKeymap {
        mode: "normal".to_string(),
        key: "space x".to_string(),
        command: "demo.hello".to_string(),
    });
    manager.apply_host_command(HostCommand::SetVirtualText {
        line: 2,
        text: "hint".to_string(),
        group: "hint".to_string(),
    });
    assert_eq!(manager.registered_commands()[0].name, "demo.hello");
    assert_eq!(manager.keymaps()[0].command, "demo.hello");
    assert_eq!(manager.virtual_text()[0].line, 2);
}

#[test]
fn crdt_document_and_collab_session_emit_edits() {
    let peer = Uuid::new_v4();
    let mut doc = CrdtDocument::from_text(peer, "abc");
    let op = doc.local_insert(1, "Z");
    assert_eq!(doc.text(), "aZbc");

    let mut other = CrdtDocument::from_text(Uuid::new_v4(), "abc");
    other.apply(&op);
    assert_eq!(other.text(), "aZbc");

    let mut session = CollaborationSession::host("me", "hello");
    session.apply_local_insert(5, "!");
    let outgoing = session.drain_outgoing();
    assert_eq!(outgoing.len(), 1);
    assert!(matches!(outgoing[0], CollabMessage::Edit { .. }));

    let sync = session.sync_state_message();
    let mut joined = CollaborationSession::host("you", "");
    joined.receive(sync);
    assert_eq!(joined.document().text(), "hello!");

    let json = outgoing[0].to_json();
    let decoded = CollabMessage::from_json(&json).unwrap();
    assert_eq!(decoded.peer_id(), outgoing[0].peer_id());
    assert!(matches!(decoded, CollabMessage::Edit { .. }));

    let ping = CollabMessage::Ping {
        peer_id: Uuid::new_v4(),
        sent_at_ms: 1_000,
    };
    let ping_json = ping.to_json();
    let decoded_ping = CollabMessage::from_json(&ping_json).unwrap();
    assert!(matches!(
        decoded_ping,
        CollabMessage::Ping {
            sent_at_ms: 1_000,
            ..
        }
    ));

    let mut host = CollaborationSession::host("host", "ping");
    host.receive(CollabMessage::Pong {
        peer_id: Uuid::new_v4(),
        sent_at_ms: collab_timestamp_ms_for_test(),
    });
    assert!(host.latency_ms().is_some());
}

fn collab_timestamp_ms_for_test() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().saturating_sub(5) as u64)
        .unwrap_or(0)
}

#[test]
fn lsp_feature_parsers_normalize_common_responses() {
    let completion_json =
        serde_json::to_value(lsp::CompletionResponse::Array(vec![lsp::CompletionItem {
            label: "println!".to_string(),
            detail: Some("macro".to_string()),
            documentation: Some(lsp::Documentation::String("Prints text".to_string())),
            kind: Some(lsp::CompletionItemKind::FUNCTION),
            ..Default::default()
        }]))
        .unwrap();
    let completions = parse_completion_result(completion_json).unwrap();
    assert_eq!(completions[0].label, "println!");
    assert_eq!(completions[0].documentation.as_deref(), Some("Prints text"));

    let hover_json = serde_json::to_value(lsp::Hover {
        contents: lsp::HoverContents::Markup(lsp::MarkupContent {
            kind: lsp::MarkupKind::Markdown,
            value: "**docs**".to_string(),
        }),
        range: Some(lsp::Range::new(
            lsp::Position::new(1, 2),
            lsp::Position::new(1, 5),
        )),
    })
    .unwrap();
    let hover = parse_hover_result(hover_json).unwrap().unwrap();
    assert_eq!(hover.markdown, "**docs**");
    assert_eq!(hover.range.unwrap().start.line, 1);

    let signature_json = serde_json::to_value(lsp::SignatureHelp {
        signatures: vec![lsp::SignatureInformation {
            label: "fn foo(bar: i32)".to_string(),
            documentation: Some(lsp::Documentation::String("Call foo".to_string())),
            parameters: Some(vec![lsp::ParameterInformation {
                label: lsp::ParameterLabel::Simple("bar: i32".to_string()),
                documentation: None,
            }]),
            active_parameter: None,
        }],
        active_signature: Some(0),
        active_parameter: Some(0),
    })
    .unwrap();
    let signature = parse_signature_help_result(signature_json)
        .unwrap()
        .unwrap();
    assert_eq!(signature.label, "fn foo(bar: i32)");
    assert_eq!(signature.active_parameter, Some(0));
    assert_eq!(signature.parameters[0], "bar: i32");

    let location = lsp::Location::new(
        lsp::Url::parse("file:///tmp/main.rs").unwrap(),
        lsp::Range::new(lsp::Position::new(3, 0), lsp::Position::new(3, 4)),
    );
    let goto_json = serde_json::to_value(lsp::GotoDefinitionResponse::Scalar(location)).unwrap();
    let locations = parse_goto_result(goto_json).unwrap();
    assert_eq!(locations[0].uri, "file:///tmp/main.rs");
}

#[test]
fn lsp_client_converts_publish_diagnostics() {
    let mut client = LspClient::new(std::path::PathBuf::from("."), "test-lsp");
    client
        .update_diagnostics_from_json(
            serde_json::to_value(lsp::PublishDiagnosticsParams {
                uri: lsp::Url::parse("file:///tmp/main.rs").unwrap(),
                diagnostics: vec![lsp::Diagnostic {
                    range: lsp::Range::new(lsp::Position::new(2, 1), lsp::Position::new(2, 4)),
                    severity: Some(lsp::DiagnosticSeverity::WARNING),
                    source: Some("rust-analyzer".to_string()),
                    message: "careful".to_string(),
                    ..Default::default()
                }],
                version: Some(1),
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        client.diagnostics()[0].severity,
        Some(DiagnosticSeverity::Warning)
    );
    assert_eq!(client.diagnostics()[0].message, "careful");
}

#[test]
fn lsp_notifications_are_converted_to_events() {
    let event = parse_lsp_notification(
        "textDocument/publishDiagnostics",
        serde_json::to_value(lsp::PublishDiagnosticsParams {
            uri: lsp::Url::parse("file:///tmp/main.rs").unwrap(),
            diagnostics: vec![lsp::Diagnostic {
                range: lsp::Range::new(lsp::Position::new(0, 0), lsp::Position::new(0, 1)),
                severity: Some(lsp::DiagnosticSeverity::ERROR),
                source: Some("server".to_string()),
                message: "broken".to_string(),
                ..Default::default()
            }],
            version: None,
        })
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    match event {
        LspEvent::PublishDiagnostics {
            uri, diagnostics, ..
        } => {
            assert_eq!(uri, "file:///tmp/main.rs");
            assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::Error));
        }
        other => panic!("unexpected event: {other:?}"),
    }

    let message = parse_lsp_notification(
        "window/showMessage",
        serde_json::to_value(lsp::ShowMessageParams {
            typ: lsp::MessageType::WARNING,
            message: "heads up".to_string(),
        })
        .unwrap(),
    )
    .unwrap()
    .unwrap();
    assert!(matches!(
        message,
        LspEvent::ShowMessage { typ: "warning", .. }
    ));
}
