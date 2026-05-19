pub mod keybindings;
pub mod preset;
pub mod schema;
pub mod watch;

pub use schema::EffectiveLanguageSettings;

use anyhow::{Context, Result};
use schema::JetConfig;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub fn load_default() -> Result<JetConfig> {
    Ok(JetConfig::default())
}

pub fn load() -> Result<JetConfig> {
    let mut config = load_default()?;
    for path in config_paths()? {
        if path.exists() {
            let loaded = load_file(&path)?;
            config = config.merge(loaded);
        }
    }
    config.validate()?;
    preset::apply_preset_bindings(&mut config);
    Ok(config)
}

pub fn load_file(path: &Path) -> Result<JetConfig> {
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut config: JetConfig =
        toml::from_str(&source).with_context(|| format!("parse {}", path.display()))?;
    let defaults = JetConfig::default();
    if config.theme.is_empty() {
        config.theme = defaults.theme;
    }
    if config.keymap.is_empty() {
        config.keymap = defaults.keymap;
    }
    if config.tab_width == 0 {
        config.tab_width = defaults.tab_width;
    }
    config.validate()?;
    preset::apply_preset_bindings(&mut config);
    Ok(config)
}

pub fn themes_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join(".config")
            .join("jet")
            .join("themes")
    })
}

pub fn config_paths() -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    if let Some(home) = env::var_os("HOME") {
        paths.push(
            PathBuf::from(home)
                .join(".config")
                .join("jet")
                .join("config.toml"),
        );
    }
    paths.push(env::current_dir()?.join(".jet").join("config.toml"));
    Ok(paths)
}
