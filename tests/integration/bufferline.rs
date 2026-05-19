use jet::{
    highlight::theme::jet_dark,
    ui::widgets::bufferline::{self, BufferTab},
};

#[test]
fn bufferline_renders_multiple_tabs() {
    let tabs = vec![
        BufferTab {
            label: "main.rs".to_string(),
            active: true,
            modified: true,
        },
        BufferTab {
            label: "lib.rs".to_string(),
            active: false,
            modified: false,
        },
    ];
    let line = bufferline::render(&tabs, 80);
    assert!(line.contains("[main.rs*]"));
    assert!(line.contains(" lib.rs"));
}

#[test]
fn themed_bufferline_uses_ansi_colors() {
    let tabs = vec![
        BufferTab {
            label: "a.rs".to_string(),
            active: true,
            modified: false,
        },
        BufferTab {
            label: "b.rs".to_string(),
            active: false,
            modified: false,
        },
    ];
    let line = bufferline::render_themed(&tabs, 80, &jet_dark());
    assert!(line.contains("\x1b["));
    assert!(line.contains("a.rs"));
}

#[test]
fn bufferline_hides_for_single_buffer() {
    let tabs = vec![BufferTab {
        label: "only.rs".to_string(),
        active: true,
        modified: false,
    }];
    assert!(bufferline::render(&tabs, 80).is_empty());
}
