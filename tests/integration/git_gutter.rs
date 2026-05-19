use jet::{
    buffer::rope::EditorBuffer,
    git::{self, blame::LineBlame, LineStatus},
    highlight::theme::jet_dark,
    lsp::types::{Diagnostic, DiagnosticSeverity, Position, Range},
    ui::widgets::{
        diagnostics,
        gutter::{self, GutterSign},
    },
};
use std::process::Command;
use tempfile::TempDir;

fn init_repo(path: &std::path::Path) {
    Command::new("git")
        .args(["init", "-b", "main"])
        .current_dir(path)
        .output()
        .expect("git init");
    Command::new("git")
        .args(["config", "user.email", "jet@test.local"])
        .current_dir(path)
        .output()
        .expect("git config email");
    Command::new("git")
        .args(["config", "user.name", "jet"])
        .current_dir(path)
        .output()
        .expect("git config name");
}

#[test]
fn themed_gutter_uses_ansi_colors() {
    let theme = jet_dark();
    let rendered = gutter::render_gutter(0, 4, GutterSign::DiagnosticError, false, &theme);
    assert!(rendered.contains("\x1b["));
    assert!(rendered.contains('E'));
}

#[test]
fn diagnostic_picker_label_uses_severity_colors() {
    let theme = jet_dark();
    let diagnostic = Diagnostic {
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
        message: "unused variable".to_string(),
        severity: Some(DiagnosticSeverity::Warning),
        source: None,
        code: None,
    };
    let label = diagnostics::picker_label(&diagnostic, &theme, 40);
    assert!(label.contains('W'));
    assert!(label.contains("5:3"));
    assert!(label.contains("unused variable"));
    assert!(label.contains("\x1b["));
}

#[test]
fn diagnostic_inline_suffix_uses_theme() {
    let theme = jet_dark();
    let diagnostic = Diagnostic {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 1,
            },
        },
        message: "expected semicolon".to_string(),
        severity: Some(DiagnosticSeverity::Error),
        source: None,
        code: None,
    };
    let suffix = diagnostics::inline_suffix(&diagnostic, &theme, 40);
    assert!(suffix.contains("expected semicolon"));
    assert!(suffix.contains("\x1b["));
}

#[test]
fn git_unstage_removes_file_from_index() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    let path = root.join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    git::stage_file(root, &path).expect("stage");
    git::unstage_file(root, &path).expect("unstage");
    let status = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(root)
        .output()
        .unwrap();
    let staged = String::from_utf8_lossy(&status.stdout);
    assert!(!staged.contains("main.rs"));
}

#[test]
fn git_diff_reports_modified_lines() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    let path = root.join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    Command::new("git")
        .args(["add", "main.rs"])
        .current_dir(root)
        .status()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .env("GIT_AUTHOR_NAME", "jet")
        .env("GIT_AUTHOR_EMAIL", "jet@test")
        .env("GIT_COMMITTER_NAME", "jet")
        .env("GIT_COMMITTER_EMAIL", "jet@test")
        .current_dir(root)
        .status()
        .unwrap();
    std::fs::write(&path, "fn main() { println!(\"hi\"); }\n").unwrap();
    let lines = git::file_diff_lines(root, &path).expect("diff");
    assert!(lines
        .iter()
        .any(|line| line.contains('+') || line.contains('-')));
}

#[test]
fn git_stage_adds_tracked_file() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    let path = root.join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    git::stage_file(root, &path).expect("stage");
    let status = Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(root)
        .output()
        .unwrap();
    let staged = String::from_utf8_lossy(&status.stdout);
    assert!(staged.contains("main.rs"));
}

#[test]
fn gutter_sign_chars_map_git_and_diagnostics() {
    assert_eq!(
        gutter::git_sign(Some(LineStatus::Added)),
        GutterSign::GitAdded
    );
    assert_eq!(
        gutter::git_sign(Some(LineStatus::Modified)),
        GutterSign::GitModified
    );
}

#[test]
fn git_status_marks_modified_lines_in_tracked_file() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    let path = root.join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    Command::new("git")
        .args(["add", "main.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(&path, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
    let status = git::status_for_file(root, &path).expect("git status");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(status
        .marks
        .values()
        .any(|mark| *mark == LineStatus::Modified));
}

#[test]
fn git_status_marks_untracked_file_lines_as_added() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    let path = root.join("new.rs");
    std::fs::write(&path, "fn new() {}\n").unwrap();
    let status = git::status_for_file(root, &path).expect("git status");
    assert!(status.marks.values().all(|mark| *mark == LineStatus::Added));
    let _buffer = EditorBuffer::from_text("fn new() {}\n");
}

#[test]
fn hunks_for_modified_file_support_navigation() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    let path = root.join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    Command::new("git")
        .args(["add", "main.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output()
        .unwrap();
    std::fs::write(&path, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();

    let hunks = git::hunks_for_file(root, &path).expect("hunks");
    assert!(!hunks.is_empty());
    let first = hunks[0];
    let next = git::adjacent_hunk(&hunks, 0, 1).expect("next hunk");
    assert_eq!(next.start_line, first.start_line);
    let wrap = git::adjacent_hunk(&hunks, usize::MAX, 1).expect("wrap");
    assert_eq!(wrap.start_line, first.start_line);
}

#[test]
fn blame_maps_lines_to_commits() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    init_repo(root);

    let path = root.join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    Command::new("git")
        .args(["add", "main.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(&path, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
    Command::new("git")
        .args(["add", "main.rs"])
        .current_dir(root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "add print"])
        .current_dir(root)
        .output()
        .unwrap();

    let blame = git::blame_for_file(root, &path).expect("blame");
    assert!(blame.len() >= 3);
    assert_eq!(
        git::format_annotation(blame.get(&0).expect("line 0")),
        git::format_annotation(blame.get(&2).expect("line 2"))
    );
    let _line_blame = LineBlame {
        author: "jet".to_string(),
        commit_short: "abc1234".to_string(),
    };
}

#[test]
fn adjacent_hunk_steps_backward() {
    let hunks = [
        git::GitHunk {
            start_line: 2,
            end_line: 4,
            status: LineStatus::Modified,
        },
        git::GitHunk {
            start_line: 10,
            end_line: 11,
            status: LineStatus::Added,
        },
    ];
    let prev = git::adjacent_hunk(&hunks, 10, -1).expect("previous");
    assert_eq!(prev.start_line, 2);
}
