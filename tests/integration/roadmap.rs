use jet::{
    app::App,
    buffer::rope::EditorBuffer,
    collab::{
        protocol::CollabMessage,
        session::CollaborationSession,
        transport::{CollaborationTransport, MemoryTransport},
    },
    config,
    highlight::{
        theme::{ansi_foreground, jet_light, load_theme_file, map_highlight_group, ThemeRegistry},
        treesitter::{Language, TreeSitterHighlighter},
    },
    plugin::{
        api::{BufferSnapshot, PluginEvent},
        manager::PluginManager,
        runtime::wasm_runtime_available,
    },
    ui::widgets::{diagnostics, gutter, whichkey, whichkey::WhichKeyEntry},
};
use tempfile::TempDir;

fn roadmap_plugin_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
(module
  (import "jet" "emit_message" (func $emit_message (param i32 i32)))
  (import "jet" "emit_virtual_text" (func $emit_virtual_text (param i32 i32 i32)))
  (import "jet" "register_command" (func $register_command (param i32 i32 i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 16) "opened")
  (data (i32.const 32) "hint")
  (data (i32.const 48) "roadmap.hello")
  (data (i32.const 72) "Hello")
  (func (export "on_buffer_open")
    i32.const 16
    i32.const 6
    call $emit_message
    i32.const 1
    i32.const 32
    i32.const 4
    call $emit_virtual_text
    i32.const 48
    i32.const 13
    i32.const 72
    i32.const 5
    call $register_command))
"#,
    )
    .unwrap()
}

#[test]
fn incremental_tree_sitter_edit_path_updates_existing_tree() {
    let mut buffer = EditorBuffer::from_text("fn main() {\n    let x = 1;\n}\n");
    let mut highlighter = TreeSitterHighlighter::new(Language::Rust);
    assert!(highlighter.parse(&buffer.to_string()));
    assert!(highlighter.has_tree());

    let edit = buffer.insert_with_edit(17, "mut ");
    highlighter.apply_buffer_edit(&buffer, &edit);
    assert!(highlighter.parse(&buffer.to_string()));
    assert!(highlighter.has_tree());
}

#[test]
fn config_and_theme_files_load_and_validate() {
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");
    std::fs::write(
        &config_path,
        r#"
theme = "jet-light"
keymap = "default"
tab_width = 2
lsp = false
highlight = true

[keybindings.normal]
"space d" = "diagnostics"
"#,
    )
    .unwrap();
    let config = config::load_file(&config_path).unwrap();
    assert_eq!(config.theme, "jet-light");
    assert_eq!(config.tab_width, 2);
    assert!(!config.lsp);
    assert_eq!(
        config.keybindings["normal"]["space d"],
        "diagnostics".to_string()
    );

    let theme_path = temp.path().join("theme.toml");
    std::fs::write(
        &theme_path,
        r##"
name = "custom"
foreground = "#111111"
background = "#ffffff"
accent = "#005f87"

[groups]
keyword = "#005f87"
"##,
    )
    .unwrap();
    let theme = load_theme_file(&theme_path).unwrap();
    assert_eq!(theme.groups["keyword"], "#005f87");

    let mut registry = ThemeRegistry::new();
    registry.register(theme);
    registry.set_active("custom").unwrap();
    assert_eq!(registry.active().name, "custom");

    assert_eq!(map_highlight_group("keyword.control"), Some("keyword"));
    let ansi = ansi_foreground("#ff0000", false);
    assert!(ansi.contains("38;2;255;0;0"));
    let light = jet_light();
    assert!(light.ansi_for_group("string").contains("38;2"));
}

#[test]
fn plugin_manager_installs_removes_and_emits_declared_host_commands() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join("source");
    let install_root = temp.path().join("installed");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("plugin.wasm"), roadmap_plugin_wasm()).unwrap();
    std::fs::write(
        source.join("plugin.toml"),
        r#"
name = "roadmap"
version = "0.1.0"
permissions = ["ui:write"]

[hooks]
on_buffer_open = true
"#,
    )
    .unwrap();

    assert!(wasm_runtime_available());
    let mut manager = PluginManager::new(install_root);
    let name = manager.install_local(&source).unwrap();
    assert_eq!(name, "roadmap");
    let called = manager
        .dispatch(&PluginEvent::BufferOpen(BufferSnapshot {
            path: None,
            selections: Vec::new(),
            visible_lines: Vec::new(),
        }))
        .unwrap();
    assert_eq!(called, 1);
    assert_eq!(manager.messages(), &["opened".to_string()]);
    assert_eq!(manager.virtual_text()[0].text, "hint");
    assert_eq!(manager.registered_commands()[0].name, "roadmap.hello");
    assert!(manager.remove("roadmap").unwrap());
}

#[test]
fn collaboration_transport_frames_json_messages_and_sessions_track_presence() {
    let mut transport = MemoryTransport::default();
    let mut host = CollaborationSession::host("host", "hello");
    let mut peer = CollaborationSession::host("peer", "hello");

    transport
        .send(CollabMessage::Hello {
            peer_id: peer.local_peer().id,
            name: "peer".to_string(),
        })
        .unwrap();
    host.receive(transport.try_recv().unwrap().unwrap());
    assert_eq!(host.peer_count(), 2);

    let op = host.apply_local_insert(5, "!");
    peer.receive(CollabMessage::Edit {
        peer_id: host.local_peer().id,
        operation: op,
    });
    assert_eq!(peer.document().text(), "hello!");
}

#[test]
fn app_commands_cover_daily_driver_surfaces() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    let mut app = App::from_args(
        vec![path],
        None,
        false,
        None::<String>,
        None::<String>,
        false,
        true,
    )
    .unwrap();

    app.execute_command("theme jet-light").unwrap();
    assert!(app.status.contains("theme: jet-light"));
    app.execute_command("diagnostics").unwrap();
    app.execute_command("collab-host").unwrap();
    assert!(app.status.contains("collab host:"));
    app.execute_command("collab-chat hello").unwrap();
    assert!(app.status.contains("collab chat"));
    app.execute_command("collab-leave").unwrap();
    assert_eq!(app.status, "collab left");
    app.execute_command("tutor").unwrap();
    assert!(app.status.contains("tutor:"));
}

#[test]
fn ui_widgets_render_stable_daily_driver_text() {
    assert_eq!(
        gutter::render_line_number(9, gutter::number_width(100)),
        " 10 "
    );
    assert_eq!(diagnostics::render_count(&[]), "no diagnostics");
    let text = whichkey::render(&[WhichKeyEntry {
        key: "d".to_string(),
        label: "diagnostics".to_string(),
    }]);
    assert_eq!(text, "d diagnostics");
}
