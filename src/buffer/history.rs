use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const INSERT_GROUP_WINDOW_MS: u64 = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryEntry {
    Insert { idx: usize, text: String },
    Delete { idx: usize, text: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorSnapshot {
    pub selections: Vec<(usize, usize)>,
}

impl CursorSnapshot {
    pub fn cursor(position: usize) -> Self {
        Self {
            selections: vec![(position, position)],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryNode {
    pub id: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub timestamp_ms: u128,
    pub entry: Option<HistoryEntry>,
    pub before: CursorSnapshot,
    pub after: CursorSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct History {
    nodes: Vec<HistoryNode>,
    current: usize,
    last_child: Option<usize>,
}

impl History {
    pub fn new() -> Self {
        Self {
            nodes: vec![HistoryNode {
                id: 0,
                parent: None,
                children: Vec::new(),
                timestamp_ms: now_ms(),
                entry: None,
                before: CursorSnapshot::cursor(0),
                after: CursorSnapshot::cursor(0),
            }],
            current: 0,
            last_child: None,
        }
    }

    pub fn push(&mut self, entry: HistoryEntry) {
        self.push_with_cursor(entry, CursorSnapshot::cursor(0), CursorSnapshot::cursor(0));
    }

    pub fn push_with_cursor(
        &mut self,
        entry: HistoryEntry,
        before: CursorSnapshot,
        after: CursorSnapshot,
    ) {
        if self.try_group_insert(&entry, &after) {
            return;
        }

        let id = self.nodes.len();
        let node = HistoryNode {
            id,
            parent: Some(self.current),
            children: Vec::new(),
            timestamp_ms: now_ms(),
            entry: Some(entry),
            before,
            after,
        };
        self.nodes.push(node);
        self.nodes[self.current].children.push(id);
        self.current = id;
        self.last_child = None;
    }

    pub fn boundary(&mut self) {
        self.last_child = None;
    }

    pub fn undo_entry(&mut self) -> Option<HistoryEntry> {
        self.undo_step().map(|(entry, _)| entry)
    }

    pub fn redo_entry(&mut self) -> Option<HistoryEntry> {
        self.redo_step().map(|(entry, _)| entry)
    }

    pub fn undo_step(&mut self) -> Option<(HistoryEntry, CursorSnapshot)> {
        let node = self.nodes.get(self.current)?;
        let entry = node.entry.clone()?;
        let before = node.before.clone();
        self.last_child = Some(self.current);
        self.current = node.parent?;
        Some((entry, before))
    }

    pub fn redo_step(&mut self) -> Option<(HistoryEntry, CursorSnapshot)> {
        let redo = self
            .last_child
            .take()
            .or_else(|| self.nodes[self.current].children.last().copied())?;
        let node = self.nodes.get(redo)?;
        let entry = node.entry.clone()?;
        let after = node.after.clone();
        self.current = redo;
        Some((entry, after))
    }

    pub fn current_node(&self) -> &HistoryNode {
        &self.nodes[self.current]
    }

    pub fn nodes(&self) -> &[HistoryNode] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len().saturating_sub(1)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bincode::serialize(self)?)?;
        Ok(())
    }

    pub fn load_from(path: &Path) -> anyhow::Result<Self> {
        let bytes = fs::read(path)?;
        Ok(bincode::deserialize(&bytes)?)
    }

    fn try_group_insert(&mut self, entry: &HistoryEntry, after: &CursorSnapshot) -> bool {
        let HistoryEntry::Insert { idx, text } = entry else {
            self.boundary();
            return false;
        };

        let node = &mut self.nodes[self.current];
        let Some(HistoryEntry::Insert {
            idx: previous_idx,
            text: previous_text,
        }) = &mut node.entry
        else {
            return false;
        };

        let elapsed = now_ms().saturating_sub(node.timestamp_ms);
        if elapsed > Duration::from_millis(INSERT_GROUP_WINDOW_MS).as_millis() {
            return false;
        }

        if *previous_idx + previous_text.chars().count() == *idx {
            previous_text.push_str(text);
            node.timestamp_ms = now_ms();
            node.after = after.clone();
            true
        } else {
            false
        }
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
