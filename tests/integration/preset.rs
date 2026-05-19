use jet::config::{preset, schema::JetConfig};

#[test]
fn helix_preset_installs_default_bindings() {
    let mut config = JetConfig {
        keymap: "helix".to_string(),
        ..JetConfig::default()
    };
    preset::apply_preset_bindings(&mut config);
    assert_eq!(config.keybindings["normal"]["space g"], "grep".to_string());
}
