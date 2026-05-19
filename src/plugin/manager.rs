use crate::plugin::{
    api::{GutterMark, HostCommand, PluginEvent, RegisteredCommand, RegisteredKeymap, VirtualText},
    manifest::PluginManifest,
    runtime::{RuntimeLimits, WasmPlugin},
};
use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

type EditOp = (usize, usize, String);

#[derive(Debug)]
pub struct PluginManager {
    root: PathBuf,
    plugins: HashMap<String, WasmPlugin>,
    limits: RuntimeLimits,
    commands: HashMap<String, RegisteredCommand>,
    keymaps: Vec<RegisteredKeymap>,
    virtual_text: Vec<VirtualText>,
    gutter_marks: Vec<GutterMark>,
    messages: Vec<String>,
    pending_edits: Vec<EditOp>,
}

impl PluginManager {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            plugins: HashMap::new(),
            limits: RuntimeLimits::default(),
            commands: HashMap::new(),
            keymaps: Vec::new(),
            virtual_text: Vec::new(),
            gutter_marks: Vec::new(),
            messages: Vec::new(),
            pending_edits: Vec::new(),
        }
    }

    pub fn discover(&mut self) -> Result<usize> {
        if !self.root.exists() {
            return Ok(0);
        }

        let mut loaded = 0usize;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let plugin_dir = entry.path();
            let manifest_path = plugin_dir.join("plugin.toml");
            let wasm_path = plugin_dir.join("plugin.wasm");
            if manifest_path.exists() {
                self.register_from_paths(&manifest_path, &wasm_path)
                    .with_context(|| format!("load plugin manifest {}", manifest_path.display()))?;
                loaded += 1;
            }
        }
        Ok(loaded)
    }

    pub fn register_from_paths(&mut self, manifest_path: &Path, wasm_path: &Path) -> Result<()> {
        let source = fs::read_to_string(manifest_path)?;
        let manifest = PluginManifest::from_toml(&source)?;
        self.register_manifest(manifest, wasm_path.to_path_buf());
        Ok(())
    }

    pub fn register_manifest(&mut self, manifest: PluginManifest, wasm_path: PathBuf) {
        let plugin = WasmPlugin::new(manifest.clone(), wasm_path, self.limits.clone());
        self.plugins.insert(manifest.name.clone(), plugin);
    }

    pub fn list(&self) -> Vec<&PluginManifest> {
        let mut manifests: Vec<_> = self
            .plugins
            .values()
            .map(|plugin| &plugin.manifest)
            .collect();
        manifests.sort_by(|a, b| a.name.cmp(&b.name));
        manifests
    }

    pub fn clear_ephemeral(&mut self) {
        self.virtual_text.clear();
        self.gutter_marks.clear();
    }

    pub fn dispatch(&mut self, event: &PluginEvent) -> Result<usize> {
        let mut called = 0usize;
        let mut emitted = Vec::new();
        for plugin in self.plugins.values_mut() {
            if wants_event(&plugin.manifest, event) {
                let outcome = plugin.call_hook(event)?;
                emitted.extend(outcome.commands);
                called += 1;
            }
        }
        for command in emitted {
            self.apply_host_command(command);
        }
        Ok(called)
    }

    pub fn install_local(&mut self, source: &Path) -> Result<String> {
        let manifest_path = source.join("plugin.toml");
        let wasm_path = source.join("plugin.wasm");
        let manifest_source = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read plugin manifest {}", manifest_path.display()))?;
        let manifest = PluginManifest::from_toml(&manifest_source)?;
        let plugin_dir = self.root.join(&manifest.name);
        fs::create_dir_all(&plugin_dir)?;
        fs::copy(&manifest_path, plugin_dir.join("plugin.toml"))?;
        fs::copy(&wasm_path, plugin_dir.join("plugin.wasm"))?;
        let name = manifest.name.clone();
        self.register_manifest(manifest, plugin_dir.join("plugin.wasm"));
        Ok(name)
    }

    pub fn remove(&mut self, name: &str) -> Result<bool> {
        self.plugins.remove(name);
        let plugin_dir = self.root.join(name);
        if plugin_dir.exists() {
            fs::remove_dir_all(plugin_dir)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn update_local(&mut self, source: &Path) -> Result<String> {
        self.install_local(source)
    }

    pub fn apply_host_command(&mut self, command: HostCommand) {
        match command {
            HostCommand::ShowMessage(message) => self.messages.push(message),
            HostCommand::SetVirtualText { line, text, group } => {
                self.virtual_text.push(VirtualText { line, text, group });
            }
            HostCommand::SetGutterMark { line, mark, group } => {
                self.gutter_marks.push(GutterMark { line, mark, group });
            }
            HostCommand::RegisterCommand { name, description } => {
                self.commands
                    .insert(name.clone(), RegisteredCommand { name, description });
            }
            HostCommand::RegisterKeymap { mode, key, command } => {
                self.keymaps.push(RegisteredKeymap { mode, key, command });
            }
            HostCommand::Log(message) => self.messages.push(message),
            HostCommand::ApplyEdit { start, end, text } => {
                self.pending_edits.push((start, end, text));
            }
        }
    }

    pub fn drain_edits(&mut self) -> Vec<EditOp> {
        std::mem::take(&mut self.pending_edits)
    }

    pub fn registered_commands(&self) -> Vec<&RegisteredCommand> {
        let mut commands: Vec<_> = self.commands.values().collect();
        commands.sort_by(|a, b| a.name.cmp(&b.name));
        commands
    }

    pub fn keymaps(&self) -> &[RegisteredKeymap] {
        &self.keymaps
    }

    pub fn virtual_text(&self) -> &[VirtualText] {
        &self.virtual_text
    }

    pub fn gutter_marks(&self) -> &[GutterMark] {
        &self.gutter_marks
    }

    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn wants_event(manifest: &PluginManifest, event: &PluginEvent) -> bool {
    match event {
        PluginEvent::BufferOpen(_) => manifest.hooks.on_buffer_open,
        PluginEvent::BufferSave(_) => manifest.hooks.on_buffer_save,
        PluginEvent::CursorMove(_) => manifest.hooks.on_cursor_move,
        PluginEvent::ModeChange { .. } => manifest.hooks.on_mode_change,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_ephemeral_removes_overlay_state() {
        let mut manager = PluginManager::new(std::env::temp_dir());
        manager.apply_host_command(HostCommand::SetVirtualText {
            line: 1,
            text: "note".into(),
            group: "test".into(),
        });
        manager.apply_host_command(HostCommand::SetGutterMark {
            line: 2,
            mark: "!".into(),
            group: "test".into(),
        });
        assert!(!manager.virtual_text().is_empty());
        manager.clear_ephemeral();
        assert!(manager.virtual_text().is_empty());
        assert!(manager.gutter_marks().is_empty());
    }
}
