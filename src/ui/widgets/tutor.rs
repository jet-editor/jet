pub const STEPS: &[&str] = &[
    "1. Press i to enter insert mode, Esc to return to normal mode.",
    "2. Press v to extend a selection; C in select mode adds another cursor.",
    "3. Press d to delete the selection, y to yank, p to paste.",
    "4. Press : to run commands such as write, quit, grep, diagnostics.",
    "5. Press space for the menu (files, buffers, grep, diagnostics).",
    "6. Press g then d for LSP definition when a server is active.",
    "7. Use ]c and [c to jump between git hunks; :git-diff shows a diff.",
    "8. Run :tutor again or Esc to dismiss this overlay.",
];

pub fn overlay_lines(max_lines: usize) -> Vec<String> {
    let mut lines = vec!["── jet tutor ──".to_string()];
    lines.extend(
        STEPS
            .iter()
            .take(max_lines.saturating_sub(1))
            .map(|step| format!("  {step}")),
    );
    lines
}
