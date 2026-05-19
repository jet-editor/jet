use crate::editor::selection::Selection;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostCommand {
    ShowMessage(String),
    SetVirtualText {
        line: usize,
        text: String,
        group: String,
    },
    SetGutterMark {
        line: usize,
        mark: String,
        group: String,
    },
    RegisterCommand {
        name: String,
        description: String,
    },
    RegisterKeymap {
        mode: String,
        key: String,
        command: String,
    },
    Log(String),
    ApplyEdit {
        start: usize,
        end: usize,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredCommand {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredKeymap {
    pub mode: String,
    pub key: String,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualText {
    pub line: usize,
    pub text: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GutterMark {
    pub line: usize,
    pub mark: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferSnapshot {
    pub path: Option<PathBuf>,
    pub selections: Vec<Selection>,
    pub visible_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginEvent {
    BufferOpen(BufferSnapshot),
    BufferSave(BufferSnapshot),
    CursorMove(BufferSnapshot),
    ModeChange { from: String, to: String },
}

pub trait PluginHost {
    fn read_line(&self, line: usize) -> Option<String>;
    fn current_file(&self) -> Option<PathBuf>;
    fn selections(&self) -> Vec<Selection>;
    fn apply_command(&mut self, command: HostCommand);
}
