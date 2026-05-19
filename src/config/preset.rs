use serde::Deserialize;
use std::collections::HashMap;

use crate::config::schema::JetConfig;

#[derive(Debug, Deserialize)]
struct PresetBindings {
    #[serde(default)]
    keybindings: HashMap<String, HashMap<String, String>>,
}

pub fn apply_preset_bindings(config: &mut JetConfig) {
    if !config.keybindings.is_empty() {
        return;
    }
    let Some(preset) = preset_source(&config.keymap) else {
        return;
    };
    let Ok(preset_config) = toml::from_str::<PresetBindings>(preset) else {
        return;
    };
    for (mode, bindings) in preset_config.keybindings {
        config.keybindings.entry(mode).or_default().extend(bindings);
    }
}

fn preset_source(keymap: &str) -> Option<&'static str> {
    match keymap {
        "helix" => Some(include_str!("presets/helix_bindings.toml")),
        "vscode" => Some(include_str!("presets/vscode_bindings.toml")),
        _ => None,
    }
}

pub fn preset_modes(config: &JetConfig) -> Vec<String> {
    config.keybindings.keys().cloned().collect()
}

pub fn preset_binding_count(config: &JetConfig) -> usize {
    config.keybindings.values().map(HashMap::len).sum()
}
