use jet::editor::commands;

#[test]
fn command_head_splits_name_and_argument() {
    let (head, arg) = commands::command_head("edit src/main.rs");
    assert_eq!(head, "edit");
    assert_eq!(arg, "src/main.rs");
}

#[test]
fn command_completion_finds_prefix_matches() {
    let matches = commands::matching_commands("git", 8);
    assert!(matches.contains(&"git-diff"));
    assert!(commands::complete_command("qui").is_some());
}

#[test]
fn tutor_overlay_includes_steps() {
    let lines = jet::ui::widgets::tutor::overlay_lines(4);
    assert!(lines[0].contains("tutor"));
    assert!(lines.len() >= 3);
}
