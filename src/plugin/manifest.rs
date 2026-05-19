use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_engine")]
    pub engine: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub hooks: PluginHooks,
    #[serde(default)]
    pub commands: Vec<PluginCommandSpec>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginHooks {
    #[serde(default)]
    pub on_buffer_open: bool,
    #[serde(default)]
    pub on_buffer_save: bool,
    #[serde(default)]
    pub on_cursor_move: bool,
    #[serde(default)]
    pub on_mode_change: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCommandSpec {
    pub event: String,
    pub kind: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub line: usize,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub mark: String,
}

impl PluginManifest {
    pub fn from_toml(source: &str) -> Result<Self> {
        let manifest: Self = toml::from_str(source)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("plugin manifest is missing name");
        }
        if self.version.trim().is_empty() {
            bail!("plugin {} is missing version", self.name);
        }
        for command in &self.commands {
            command.validate(&self.name)?;
        }
        Ok(())
    }

    pub fn needs_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|entry| entry == permission)
    }
}

impl PluginCommandSpec {
    fn validate(&self, plugin_name: &str) -> Result<()> {
        if self.event.trim().is_empty() {
            bail!("plugin {plugin_name} command is missing event");
        }
        match self.kind.as_str() {
            "message" if self.text.trim().is_empty() => {
                bail!("plugin {plugin_name} message command is missing text")
            }
            "virtual_text" if self.text.trim().is_empty() => {
                bail!("plugin {plugin_name} virtual_text command is missing text")
            }
            "gutter_mark" if self.mark.trim().is_empty() => {
                bail!("plugin {plugin_name} gutter_mark command is missing mark")
            }
            "command" if self.name.trim().is_empty() => {
                bail!("plugin {plugin_name} command registration is missing name")
            }
            "keymap" if self.mode.trim().is_empty() || self.key.trim().is_empty() => {
                bail!("plugin {plugin_name} keymap command is missing mode or key")
            }
            "message" | "virtual_text" | "gutter_mark" | "command" | "keymap" => {}
            other => bail!("plugin {plugin_name} has unknown host command kind: {other}"),
        }
        Ok(())
    }
}

fn default_engine() -> String {
    "jet >= 1.0.0".to_string()
}
