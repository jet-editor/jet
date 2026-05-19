use crate::{buffer::mmap::MmapBuffer, editor::mode::SelectionSet};
use anyhow::Result;
use ropey::Rope;
use std::{fs, ops::Range, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferEdit {
    pub start_char: usize,
    pub old_end_char: usize,
    pub new_end_char: usize,
    pub old_text: String,
    pub new_text: String,
}

#[derive(Clone)]
enum BufferStorage {
    Heap(Rope),
    Mapped(MmapBuffer),
}

#[derive(Clone)]
pub struct EditorBuffer {
    storage: BufferStorage,
    selections: SelectionSet,
}

impl EditorBuffer {
    pub fn new() -> Self {
        Self {
            storage: BufferStorage::Heap(Rope::new()),
            selections: SelectionSet::new(),
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        if path.exists() && path.metadata()?.len() > 0 {
            Ok(Self {
                storage: BufferStorage::Mapped(MmapBuffer::open(path)?),
                selections: SelectionSet::new(),
            })
        } else {
            Ok(Self::new())
        }
    }

    pub fn from_text(text: &str) -> Self {
        Self {
            storage: BufferStorage::Heap(Rope::from_str(text)),
            selections: SelectionSet::new(),
        }
    }

    pub fn len_lines(&self) -> usize {
        match &self.storage {
            BufferStorage::Heap(rope) => rope.len_lines(),
            BufferStorage::Mapped(mmap) => mmap.visible_line_count(),
        }
    }

    pub fn len_chars(&self) -> usize {
        match &self.storage {
            BufferStorage::Heap(rope) => rope.len_chars(),
            BufferStorage::Mapped(mmap) => mmap.total_char_count(),
        }
    }

    pub fn len_bytes(&self) -> usize {
        match &self.storage {
            BufferStorage::Heap(rope) => rope.len_bytes(),
            BufferStorage::Mapped(mmap) => mmap.len(),
        }
    }

    pub fn visible_line_count(&self) -> usize {
        self.len_lines()
    }

    pub fn selections(&self) -> &SelectionSet {
        &self.selections
    }

    pub fn selections_mut(&mut self) -> &mut SelectionSet {
        &mut self.selections
    }

    pub fn set_selections(&mut self, selections: SelectionSet) {
        self.selections = selections;
    }

    pub fn line_string(&self, line_idx: usize) -> String {
        match &self.storage {
            BufferStorage::Heap(rope) => {
                if line_idx >= rope.len_lines() {
                    String::new()
                } else {
                    trim_line_end(rope.line(line_idx).to_string())
                }
            }
            BufferStorage::Mapped(mmap) => trim_line_end(mmap.line_string(line_idx)),
        }
    }

    pub fn visible_lines(&self, start: usize, count: usize) -> Vec<String> {
        match &self.storage {
            BufferStorage::Heap(rope) => {
                let end = (start + count).min(rope.len_lines());
                (start..end)
                    .map(|idx| trim_line_end(rope.line(idx).to_string()))
                    .collect()
            }
            BufferStorage::Mapped(mmap) => mmap.visible_lines(start, count),
        }
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        match &mut self.storage {
            BufferStorage::Heap(rope) => {
                rope.insert(char_idx.min(rope.len_chars()), text);
            }
            BufferStorage::Mapped(mmap) => mmap.insert(char_idx, text),
        }
    }

    pub fn insert_with_edit(&mut self, char_idx: usize, text: &str) -> BufferEdit {
        let start = char_idx.min(self.len_chars());
        self.insert(start, text);
        BufferEdit {
            start_char: start,
            old_end_char: start,
            new_end_char: start + text.chars().count(),
            old_text: String::new(),
            new_text: text.to_string(),
        }
    }

    pub fn remove(&mut self, range: Range<usize>) {
        match &mut self.storage {
            BufferStorage::Heap(rope) => {
                let start = range.start.min(rope.len_chars());
                let end = range.end.min(rope.len_chars());
                if start < end {
                    rope.remove(start..end);
                }
            }
            BufferStorage::Mapped(mmap) => mmap.remove(range),
        }
    }

    pub fn remove_with_edit(&mut self, range: Range<usize>) -> Option<BufferEdit> {
        let start = range.start.min(self.len_chars());
        let end = range.end.min(self.len_chars());
        if start >= end {
            return None;
        }
        let old_text = self.slice_chars(start, end);
        self.remove(start..end);
        Some(BufferEdit {
            start_char: start,
            old_end_char: end,
            new_end_char: start,
            old_text,
            new_text: String::new(),
        })
    }

    pub fn slice_chars(&self, start: usize, end: usize) -> String {
        match &self.storage {
            BufferStorage::Heap(rope) => rope
                .slice(start.min(rope.len_chars())..end.min(rope.len_chars()))
                .to_string(),
            BufferStorage::Mapped(mmap) => mmap.slice_chars(start, end),
        }
    }

    pub fn char_idx(&self, line_idx: usize, col_idx: usize) -> usize {
        let mut idx = 0usize;
        for line in 0..line_idx.min(self.len_lines()) {
            idx += self.line_char_len(line).saturating_add(1);
        }
        idx + col_idx.min(self.line_char_len(line_idx))
    }

    pub fn char_to_line_col(&self, char_idx: usize) -> (usize, usize) {
        let mut remaining = char_idx;
        for line in 0..self.len_lines() {
            let len = self.line_char_len(line);
            if remaining <= len {
                return (line, remaining);
            }
            remaining = remaining.saturating_sub(len + 1);
        }
        (self.len_lines().saturating_sub(1), 0)
    }

    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        match &self.storage {
            BufferStorage::Heap(rope) => rope.char_to_byte(char_idx.min(rope.len_chars())),
            BufferStorage::Mapped(mmap) => mmap.char_to_byte(char_idx),
        }
    }

    pub fn byte_to_char(&self, byte: usize) -> usize {
        match &self.storage {
            BufferStorage::Heap(rope) => rope.byte_to_char(byte.min(rope.len_bytes())),
            BufferStorage::Mapped(mmap) => mmap.byte_to_char(byte),
        }
    }

    pub fn char_at(&self, char_idx: usize) -> char {
        match &self.storage {
            BufferStorage::Heap(rope) => rope
                .get_char(char_idx.min(rope.len_chars().saturating_sub(1)))
                .unwrap_or(' '),
            BufferStorage::Mapped(mmap) => mmap.char_at(char_idx),
        }
    }

    pub fn line_char_len(&self, line_idx: usize) -> usize {
        match &self.storage {
            BufferStorage::Heap(rope) => {
                if line_idx >= rope.len_lines() {
                    0
                } else {
                    let slice = rope.line(line_idx);
                    let mut len = slice.len_chars();
                    while len > 0 {
                        match slice.get_char(len - 1) {
                            Some('\n' | '\r') => len -= 1,
                            _ => break,
                        }
                    }
                    len
                }
            }
            BufferStorage::Mapped(mmap) => mmap.line_char_len(line_idx),
        }
    }

    pub fn source_bytes(&self) -> Vec<u8> {
        match &self.storage {
            BufferStorage::Heap(rope) => rope.to_string().into_bytes(),
            BufferStorage::Mapped(mmap) => mmap.as_bytes().to_vec(),
        }
    }

    pub fn text_provider(&self) -> BufferTextProvider<'_> {
        BufferTextProvider { buffer: self }
    }

    pub fn line_len(&self, line_idx: usize) -> usize {
        self.line_char_len(line_idx)
    }

    pub fn save_to(&self, path: &Path) -> Result<usize> {
        match &self.storage {
            BufferStorage::Heap(rope) => {
                let text = rope.to_string();
                fs::write(path, text.as_bytes())?;
                Ok(text.len())
            }
            BufferStorage::Mapped(mmap) => mmap.save_to(path),
        }
    }

    pub fn is_mapped(&self) -> bool {
        matches!(self.storage, BufferStorage::Mapped(_))
    }

    pub fn mapped_overlay_count(&self) -> usize {
        match &self.storage {
            BufferStorage::Heap(_) => 0,
            BufferStorage::Mapped(mmap) => mmap.overlay_count(),
        }
    }
}

pub struct BufferTextProvider<'a> {
    buffer: &'a EditorBuffer,
}

impl<'a> BufferTextProvider<'a> {
    pub fn chunk_at(&self, byte: usize) -> String {
        match &self.buffer.storage {
            BufferStorage::Heap(rope) => {
                let byte = byte.min(rope.len_bytes());
                let (chunk, chunk_byte, _, _) = rope.chunk_at_byte(byte);
                let relative = byte.saturating_sub(chunk_byte);
                chunk.get(relative..).unwrap_or_default().to_string()
            }
            BufferStorage::Mapped(mmap) => mmap.chunk_at_byte(byte),
        }
    }
}

impl Default for EditorBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for EditorBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.storage {
            BufferStorage::Heap(rope) => write!(f, "{}", rope),
            BufferStorage::Mapped(mmap) => {
                write!(f, "{}", mmap.to_window_string())
            }
        }
    }
}

fn trim_line_end(mut line: String) -> String {
    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    line
}
