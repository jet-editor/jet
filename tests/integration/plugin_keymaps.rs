use jet::config::keybindings::BindingTable;

#[test]
fn plugin_keymaps_merge_into_binding_table() {
    let mut table = BindingTable::default();
    table.extend_bindings([("space p".to_string(), "plugin-list".to_string())]);
    let chords = jet::config::keybindings::parse_sequence("space p").unwrap();
    assert!(matches!(
        table.match_sequence(&chords),
        jet::config::keybindings::BindingMatch::Complete("plugin-list")
    ));
}
