use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorConfig {
    #[serde(default = "default_scrolloff")]
    pub scrolloff: usize,
    #[serde(default)]
    pub format_on_save: bool,
    #[serde(default = "default_true")]
    pub inlay_hints: bool,
    #[serde(default = "default_true")]
    pub mouse: bool,
    #[serde(default)]
    pub word_wrap: bool,
    #[serde(default)]
    pub cursor_style: CursorStyle,
    #[serde(default = "default_true")]
    pub line_numbers: bool,
    #[serde(default)]
    pub relative_numbers: bool,
    #[serde(default)]
    pub color_column: usize,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            scrolloff: default_scrolloff(),
            format_on_save: false,
            inlay_hints: true,
            mouse: true,
            word_wrap: false,
            cursor_style: CursorStyle::Block,
            line_numbers: true,
            relative_numbers: false,
            color_column: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStyle {
    #[default]
    Block,
    Bar,
    Underline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JetConfig {
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_keymap")]
    pub keymap: String,
    #[serde(default = "default_tab_width")]
    pub tab_width: usize,
    #[serde(default = "default_true")]
    pub lsp: bool,
    #[serde(default = "default_true")]
    pub highlight: bool,
    #[serde(default = "default_true")]
    pub auto_pairs: bool,
    #[serde(default)]
    pub language: HashMap<String, LanguageConfig>,
    #[serde(default)]
    pub keybindings: HashMap<String, HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageConfig {
    #[serde(default)]
    pub tab_width: Option<usize>,
    #[serde(default)]
    pub lsp: Option<bool>,
    #[serde(default)]
    pub highlight: Option<bool>,
}

impl Default for JetConfig {
    fn default() -> Self {
        Self {
            editor: EditorConfig::default(),
            theme: default_theme(),
            keymap: default_keymap(),
            tab_width: default_tab_width(),
            lsp: default_true(),
            highlight: default_true(),
            auto_pairs: default_true(),
            language: HashMap::new(),
            keybindings: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveLanguageSettings {
    pub tab_width: usize,
    pub lsp: bool,
    pub highlight: bool,
}

impl JetConfig {
    pub fn effective_for_language(&self, language_name: Option<&str>) -> EffectiveLanguageSettings {
        let mut tab_width = self.tab_width;
        let mut lsp = self.lsp;
        let mut highlight = self.highlight;
        if let Some(name) = language_name {
            if let Some(language) = self.language.get(name) {
                if let Some(width) = language.tab_width {
                    tab_width = width;
                }
                if let Some(enabled) = language.lsp {
                    lsp = enabled;
                }
                if let Some(enabled) = language.highlight {
                    highlight = enabled;
                }
            }
        }
        EffectiveLanguageSettings {
            tab_width: tab_width.clamp(1, 16),
            lsp,
            highlight,
        }
    }

    pub fn merge(mut self, override_config: JetConfig) -> Self {
        self.theme = override_config.theme;
        self.keymap = override_config.keymap;
        self.tab_width = override_config.tab_width;
        self.lsp = override_config.lsp;
        self.highlight = override_config.highlight;
        self.auto_pairs = override_config.auto_pairs;
        self.editor = override_config.editor;
        self.language.extend(override_config.language);
        for (mode, bindings) in override_config.keybindings {
            self.keybindings.entry(mode).or_default().extend(bindings);
        }
        self
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.tab_width == 0 || self.tab_width > 16 {
            anyhow::bail!("tab_width must be between 1 and 16");
        }
        if self.theme.trim().is_empty() {
            anyhow::bail!("theme must not be empty");
        }
        if self.keymap.trim().is_empty() {
            anyhow::bail!("keymap must not be empty");
        }
        Ok(())
    }
}

fn default_theme() -> String {
    "jet-dark".to_string()
}

fn default_keymap() -> String {
    "default".to_string()
}

fn default_tab_width() -> usize {
    4
}

fn default_true() -> bool {
    true
}

fn default_scrolloff() -> usize {
    5
}
