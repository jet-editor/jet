use jet::{
    app::App,
    buffer::{
        history::{CursorSnapshot, History, HistoryEntry},
        rope::EditorBuffer,
    },
    editor::{
        actions,
        buffers::{BufferId, BufferManager},
        mode::SelectionSet,
        motions::{self, CharSearchMode, Jump, JumpList},
        picker,
        registers::{RegisterBank, RegisterId},
        selection::Selection,
        splits::{SplitDirection, SplitManager},
    },
    ui::layout::Rect,
};
use tempfile::TempDir;

#[test]
fn selection_tracks_anchor_head_and_primary_rotation() {
    let mut selections = SelectionSet::from_vec(vec![Selection::new(2, 5), Selection::cursor(9)]);

    assert_eq!(selections.primary().range(), 2..5);
    selections.rotate_forward();
    assert_eq!(selections.primary().head, 9);
    selections.rotate_backward();
    assert_eq!(selections.primary().range(), 2..5);

    selections.collapse_to_primary();
    assert_eq!(selections.selections().len(), 1);
}

#[test]
fn motions_are_unicode_word_aware() {
    let buffer = EditorBuffer::from_text("alpha beta\n日本語 gamma");

    assert_eq!(motions::word_forward(&buffer, 0), 6);
    assert_eq!(motions::word_backward(&buffer, 8), 6);
    assert_eq!(motions::line_end(&buffer, 0), 10);
    assert_eq!(
        motions::find_char_forward(&buffer, 0, 'b', CharSearchMode::Inclusive),
        Some(6)
    );
    assert_eq!(motions::big_word_forward(&buffer, 0), 6);
    assert_eq!(motions::word_under_cursor(&buffer, 7), Some((6, 10)));
}

#[test]
fn registers_store_named_and_default_yanks() {
    let mut bank = RegisterBank::default();
    bank.yank(RegisterId::Unnamed, "alpha".to_string());
    assert_eq!(bank.get(RegisterId::Unnamed), "alpha");
    bank.yank(RegisterId::Named(0), "beta".to_string());
    assert_eq!(bank.get(RegisterId::Named(0)), "beta");
    assert_eq!(bank.get(RegisterId::LastYank), "beta");
}

#[test]
fn undo_step_restores_cursor_snapshot() {
    let mut history = History::new();
    history.push_with_cursor(
        HistoryEntry::Insert {
            idx: 0,
            text: "hi".to_string(),
        },
        CursorSnapshot::cursor(0),
        CursorSnapshot {
            selections: vec![(0, 2)],
        },
    );
    let mut buffer = EditorBuffer::from_text("");
    let (entry, snapshot) = history.undo_step().unwrap();
    actions::apply_history_backward(&mut buffer, &entry);
    actions::restore_selections(&mut buffer, &snapshot.selections);
    assert_eq!(buffer.to_string(), "");
    assert_eq!(buffer.selections().primary().head, 0);
}

#[test]
fn persistent_history_round_trips_to_disk() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("history.bin");
    let mut history = History::new();
    history.push_with_cursor(
        HistoryEntry::Insert {
            idx: 0,
            text: "x".to_string(),
        },
        CursorSnapshot::cursor(0),
        CursorSnapshot::cursor(1),
    );
    history.save_to(&path).unwrap();
    let loaded = History::load_from(&path).unwrap();
    assert_eq!(loaded.len(), history.len());
}

#[test]
fn jump_list_keeps_branches_navigable() {
    let mut jumps = JumpList::new(8);
    jumps.push(Jump {
        file: None,
        position: 1,
    });
    jumps.push(Jump {
        file: None,
        position: 20,
    });
    assert_eq!(jumps.backward().unwrap().position, 1);
    assert_eq!(jumps.forward().unwrap().position, 20);
}

#[test]
fn history_groups_adjacent_insertions_and_preserves_redo_branch() {
    let mut history = History::new();
    history.push_with_cursor(
        HistoryEntry::Insert {
            idx: 0,
            text: "h".to_string(),
        },
        CursorSnapshot::cursor(0),
        CursorSnapshot::cursor(1),
    );
    history.push_with_cursor(
        HistoryEntry::Insert {
            idx: 1,
            text: "i".to_string(),
        },
        CursorSnapshot::cursor(1),
        CursorSnapshot::cursor(2),
    );

    assert_eq!(history.len(), 1);
    assert_eq!(
        history.undo_entry(),
        Some(HistoryEntry::Insert {
            idx: 0,
            text: "hi".to_string(),
        })
    );
    assert_eq!(
        history.redo_entry(),
        Some(HistoryEntry::Insert {
            idx: 0,
            text: "hi".to_string(),
        })
    );
}

#[test]
fn buffer_manager_opens_switches_and_closes_buffers() {
    let temp = TempDir::new().unwrap();
    let first = temp.path().join("first.txt");
    let second = temp.path().join("second.txt");
    std::fs::write(&first, "alpha\n").unwrap();
    std::fs::write(&second, "beta\n").unwrap();

    let mut buffers = BufferManager::new(8);
    let first_id = buffers.open(&first).unwrap();
    let second_id = buffers.open(&second).unwrap();

    assert_eq!(buffers.current().unwrap().id, second_id);
    assert!(buffers.switch_to(first_id));
    let normalized_first = first.canonicalize().unwrap();
    assert_eq!(
        buffers.current().unwrap().path.as_deref(),
        Some(normalized_first.as_path())
    );

    let reopened = buffers.open(first.join("..").join("first.txt")).unwrap();
    assert_eq!(reopened, first_id);
    assert_eq!(buffers.buffers().len(), 2);

    let closed = buffers.close(first_id).unwrap();
    assert_eq!(closed.id, first_id);
    assert_eq!(buffers.current().unwrap().id, second_id);
}

#[test]
fn split_manager_tracks_focus_and_respects_split_direction() {
    let mut splits = SplitManager::new(BufferId(0), 120, 40);
    let second = splits.split(SplitDirection::Vertical);

    assert_eq!(splits.focused().id, second);
    assert_eq!(splits.splits().len(), 2);

    let layout = splits.layout(Rect {
        x: 0,
        y: 0,
        width: 120,
        height: 40,
    });
    assert_eq!(layout.len(), 2);
    assert_eq!(layout[0].1.width, 60);
    assert_eq!(layout[1].1.x, 60);
    assert_eq!(layout[1].1.height, 40);

    splits.focus_next();
    assert!(splits.close_focused());
    assert_eq!(splits.splits().len(), 1);
    assert!(!splits.close_focused());
}

#[test]
fn file_picker_discovers_and_fuzzy_filters_project_files() {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    let target = temp.path().join("target");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(src.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(src.join("lib.rs"), "pub fn run() {}\n").unwrap();
    std::fs::write(target.join("ignored.rs"), "").unwrap();

    let discovered = picker::discover_files(temp.path(), 10).unwrap();
    assert_eq!(discovered.len(), 2);
    assert!(discovered.iter().all(|path| !path.ends_with("ignored.rs")));

    let matches = picker::fuzzy_files(temp.path(), "MR", 10, 5).unwrap();
    assert_eq!(matches[0].display, "src/main.rs");
}

#[test]
fn app_commands_open_switch_close_buffers() {
    let temp = TempDir::new().unwrap();
    let first = temp.path().join("first.txt");
    let second = temp.path().join("second.txt");
    std::fs::write(&first, "alpha\n").unwrap();
    std::fs::write(&second, "beta\n").unwrap();

    let mut app = App::from_args(
        vec![first.clone()],
        None,
        false,
        None::<String>,
        None::<String>,
        true,
        true,
    )
    .unwrap();
    assert_eq!(
        app.current_path(),
        Some(first.canonicalize().unwrap().as_path())
    );

    app.execute_command(&format!("e {}", second.display()))
        .unwrap();
    assert_eq!(
        app.current_path(),
        Some(second.canonicalize().unwrap().as_path())
    );

    app.execute_command("bp").unwrap();
    assert_eq!(
        app.current_path(),
        Some(first.canonicalize().unwrap().as_path())
    );

    app.execute_command("bn").unwrap();
    assert_eq!(
        app.current_path(),
        Some(second.canonicalize().unwrap().as_path())
    );

    app.execute_command("bd").unwrap();
    assert_eq!(
        app.current_path(),
        Some(first.canonicalize().unwrap().as_path())
    );
}

#[test]
fn app_commands_manage_split_state() {
    let mut app = App::from_args(
        Vec::new(),
        None,
        false,
        None::<String>,
        None::<String>,
        true,
        true,
    )
    .unwrap();

    assert_eq!(app.split_count(), 1);
    app.execute_command("split").unwrap();
    assert_eq!(app.split_count(), 2);
    app.execute_command("vsplit").unwrap();
    assert_eq!(app.split_count(), 3);
    app.execute_command("only").unwrap();
    assert_eq!(app.split_count(), 1);
}

#[test]
fn app_reports_language_grammar_lsp_and_plugins() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();

    let mut app = App::from_args(
        vec![path],
        None,
        false,
        None::<String>,
        None::<String>,
        true,
        true,
    )
    .unwrap();

    app.execute_command("lang").unwrap();
    assert!(app.status.contains("language: rust"));
    app.execute_command("grammar").unwrap();
    assert!(app.status.contains("grammar: rust"));
    app.execute_command("lsp").unwrap();
    assert!(app.status.contains("rust-analyzer"));
    app.execute_command("plugin-list").unwrap();
    assert_eq!(app.mode, jet::editor::mode::Mode::Picker);
}
