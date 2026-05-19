use crate::{buffer::rope::EditorBuffer, editor::cursor::Cursor};
use anyhow::Result;
use std::{
    collections::VecDeque,
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BufferId(pub usize);

pub struct BufferEntry {
    pub id: BufferId,
    pub path: Option<PathBuf>,
    pub buffer: EditorBuffer,
    pub cursor: Cursor,
    pub modified: bool,
    pub read_only: bool,
    last_used_tick: u64,
}

pub struct BufferManager {
    buffers: Vec<BufferEntry>,
    current: Option<BufferId>,
    max_buffers: usize,
    tick: u64,
    recent: VecDeque<BufferId>,
}

impl BufferManager {
    pub fn new(max_buffers: usize) -> Self {
        Self {
            buffers: Vec::new(),
            current: None,
            max_buffers: max_buffers.max(1),
            tick: 0,
            recent: VecDeque::new(),
        }
    }

    pub fn open(&mut self, path: impl AsRef<Path>) -> Result<BufferId> {
        let path = normalize_path(path.as_ref());
        if let Some(id) = self.find_by_path(&path) {
            self.switch_to(id);
            return Ok(id);
        }

        let id = BufferId(self.next_id());
        let entry = BufferEntry {
            id,
            path: Some(path.clone()),
            buffer: EditorBuffer::open(&path)?,
            cursor: Cursor::default(),
            modified: false,
            read_only: false,
            last_used_tick: self.next_tick(),
        };
        self.buffers.push(entry);
        self.current = Some(id);
        self.push_recent(id);
        self.evict_if_needed();
        Ok(id)
    }

    pub fn new_scratch(&mut self) -> BufferId {
        let id = BufferId(self.next_id());
        let entry = BufferEntry {
            id,
            path: None,
            buffer: EditorBuffer::new(),
            cursor: Cursor::default(),
            modified: false,
            read_only: false,
            last_used_tick: self.next_tick(),
        };
        self.buffers.push(entry);
        self.current = Some(id);
        self.push_recent(id);
        self.evict_if_needed();
        id
    }

    pub fn switch_to(&mut self, id: BufferId) -> bool {
        let tick = self.next_tick();
        if let Some(entry) = self.entry_mut(id) {
            entry.last_used_tick = tick;
            self.current = Some(id);
            self.push_recent(id);
            true
        } else {
            false
        }
    }

    pub fn next_buffer(&mut self) -> Option<BufferId> {
        let current = self.current?;
        let idx = self.buffers.iter().position(|entry| entry.id == current)?;
        let next = self.buffers[(idx + 1) % self.buffers.len()].id;
        self.switch_to(next);
        Some(next)
    }

    pub fn previous_buffer(&mut self) -> Option<BufferId> {
        let current = self.current?;
        let idx = self.buffers.iter().position(|entry| entry.id == current)?;
        let prev = if idx == 0 {
            self.buffers.len() - 1
        } else {
            idx - 1
        };
        let id = self.buffers[prev].id;
        self.switch_to(id);
        Some(id)
    }

    pub fn current(&self) -> Option<&BufferEntry> {
        self.current.and_then(|id| self.entry(id))
    }

    pub fn current_mut(&mut self) -> Option<&mut BufferEntry> {
        let id = self.current?;
        self.entry_mut(id)
    }

    pub fn buffers(&self) -> &[BufferEntry] {
        &self.buffers
    }

    pub fn close(&mut self, id: BufferId) -> Option<BufferEntry> {
        let index = self.buffers.iter().position(|entry| entry.id == id)?;
        let removed = self.buffers.remove(index);
        self.recent.retain(|recent| *recent != id);
        if self.current == Some(id) {
            self.current = self
                .recent
                .back()
                .copied()
                .or_else(|| self.buffers.first().map(|b| b.id));
        }
        Some(removed)
    }

    fn entry(&self, id: BufferId) -> Option<&BufferEntry> {
        self.buffers.iter().find(|entry| entry.id == id)
    }

    fn entry_mut(&mut self, id: BufferId) -> Option<&mut BufferEntry> {
        self.buffers.iter_mut().find(|entry| entry.id == id)
    }

    fn find_by_path(&self, path: &Path) -> Option<BufferId> {
        self.buffers
            .iter()
            .find(|entry| entry.path.as_deref() == Some(path))
            .map(|entry| entry.id)
    }

    fn evict_if_needed(&mut self) {
        while self.buffers.len() > self.max_buffers {
            let victim = self
                .buffers
                .iter()
                .filter(|entry| !entry.modified && Some(entry.id) != self.current)
                .min_by_key(|entry| entry.last_used_tick)
                .map(|entry| entry.id);
            if let Some(id) = victim {
                self.close(id);
            } else {
                break;
            }
        }
    }

    fn push_recent(&mut self, id: BufferId) {
        self.recent.retain(|recent| *recent != id);
        self.recent.push_back(id);
    }

    fn next_id(&self) -> usize {
        self.buffers
            .iter()
            .map(|entry| entry.id.0)
            .max()
            .map(|id| id + 1)
            .unwrap_or(0)
    }

    fn next_tick(&mut self) -> u64 {
        self.tick += 1;
        self.tick
    }
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new(64)
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let lexical = normalize_path_lexically(path);
    lexical.canonicalize().unwrap_or(lexical)
}

fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::Normal(part) => normalized.push(part),
        }
    }
    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}
