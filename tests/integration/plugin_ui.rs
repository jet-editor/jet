use jet::{
    highlight::theme::jet_dark,
    plugin::api::{GutterMark, VirtualText},
    ui::widgets::plugin_ui,
};

#[test]
fn virtual_text_suffix_uses_theme_group() {
    let theme = jet_dark();
    let text = VirtualText {
        line: 1,
        text: "hint".to_string(),
        group: "popup".to_string(),
    };
    let suffix = plugin_ui::virtual_text_suffix(&text, &theme, 20);
    assert!(suffix.contains("hint"));
    assert!(suffix.contains("\x1b["));
}

#[test]
fn gutter_marker_renders_first_character() {
    let theme = jet_dark();
    let mark = GutterMark {
        line: 0,
        mark: "★".to_string(),
        group: "popup".to_string(),
    };
    let rendered = plugin_ui::gutter_marker(&mark, &theme);
    assert!(rendered.contains('★'));
    assert!(rendered.contains("\x1b["));
}
