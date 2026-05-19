use crate::buffer::encoding::{self, Encoding};
use anyhow::{anyhow, Context, Result};
use memmap2::{Mmap, MmapOptions};
use ropey::Rope;
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const DEFAULT_VISIBLE_LINES: usize = 200;
const MAX_SCAN_BYTES_FOR_WINDOW: usize = 1024 * 1024;
const DEFAULT_CHUNK_BYTES: usize = 16 * 1024;

/// A lazily loaded text view backed by a memory-mapped file.
///
/// The mmap is the durable backing store. Opening maps virtual pages and copies
/// only the first visible window into a small rope for display/editing. It never
/// builds a whole-file Rope during open.
#[derive(Clone)]
pub struct MmapBuffer {
    path: PathBuf,
    mmap: Arc<Mmap>,
    window_start: usize,
    window_end: usize,
    window_rope: Rope,
    overlays: Vec<OverlayEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OverlayEdit {
    start: usize,
    end: usize,
    replacement: String,
}

impl MmapBuffer {
    /// Convert window contents to a contiguous string.
    pub fn to_window_string(&self) -> String {
        self.window_rope.to_string()
    }

    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
        let mmap = unsafe { MmapOptions::new().map(&file) }
            .with_context(|| format!("mmap {}", path.display()))?;
        let enc = encoding::detect(&mmap)?;
        let bom_offset = match enc {
            Encoding::Utf8Bom => 3usize,
            Encoding::Utf8 => 0,
        };
        let arc = Arc::new(mmap);
        let (end, text) = Self::visible_chunk(&arc, bom_offset, DEFAULT_VISIBLE_LINES)?;
        let window_rope = Rope::from_str(text);
        Ok(Self {
            path: path.to_path_buf(),
            mmap: arc,
            window_start: bom_offset,
            window_end: end,
            window_rope,
            overlays: Vec::new(),
        })
    }

    pub fn empty(path: PathBuf) -> Self {
        let file = tempfile::tempfile().expect("create empty mmap backing file");
        file.set_len(0).expect("set empty mmap length");
        let mmap = unsafe { MmapOptions::new().len(0).map(&file) }
            .unwrap_or_else(|_| panic!("empty mmap is not supported on this platform"));
        Self {
            path,
            mmap: Arc::new(mmap),
            window_start: 0,
            window_end: 0,
            window_rope: Rope::new(),
            overlays: Vec::new(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        let delta = self.overlays.iter().fold(0isize, |delta, edit| {
            delta + edit.replacement.len() as isize - (edit.end - edit.start) as isize
        });
        if delta.is_negative() {
            self.mmap.len().saturating_sub(delta.unsigned_abs())
        } else {
            self.mmap.len().saturating_add(delta as usize)
        }
    }

    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }

    pub fn has_overlays(&self) -> bool {
        !self.overlays.is_empty()
    }

    pub fn overlay_count(&self) -> usize {
        self.overlays.len()
    }

    pub fn window_start(&self) -> usize {
        self.window_start
    }

    pub fn window_end(&self) -> usize {
        self.window_end
    }

    pub fn visible_line_count(&self) -> usize {
        self.window_rope.len_lines()
    }

    pub fn line_string(&self, line: usize) -> String {
        if line >= self.window_rope.len_lines() {
            String::new()
        } else {
            self.window_rope.line(line).to_string()
        }
    }

    pub fn char_to_byte(&self, char_idx: usize) -> usize {
        self.window_rope
            .char_to_byte(char_idx.min(self.window_rope.len_chars()))
    }

    pub fn byte_to_char(&self, byte: usize) -> usize {
        self.window_rope
            .byte_to_char(byte.min(self.window_rope.len_bytes()))
    }

    pub fn char_at(&self, char_idx: usize) -> char {
        self.window_rope
            .get_char(char_idx.min(self.window_rope.len_chars().saturating_sub(1)))
            .unwrap_or(' ')
    }

    pub fn line_char_len(&self, line: usize) -> usize {
        if line >= self.window_rope.len_lines() {
            0
        } else {
            let slice = self.window_rope.line(line);
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

    pub fn total_char_count(&self) -> usize {
        self.window_rope.len_chars()
    }

    pub fn visible_lines(&self, start: usize, count: usize) -> Vec<String> {
        let end = (start + count).min(self.window_rope.len_lines());
        (start..end)
            .map(|idx| trim_line_end(self.window_rope.line(idx).to_string()))
            .collect()
    }

    pub fn chunk_at_byte(&self, byte_offset: usize) -> String {
        self.chunk_at_byte_with_limit(byte_offset, DEFAULT_CHUNK_BYTES)
    }

    pub fn chunk_at_byte_with_limit(&self, byte_offset: usize, max_bytes: usize) -> String {
        let text_len = self.len();
        let target = byte_offset.min(text_len);
        if target >= text_len || max_bytes == 0 {
            return String::new();
        }

        let mut overlays = self.overlays.clone();
        overlays.sort_by_key(|edit| edit.start);

        let mut current_cursor = 0usize;
        let mut original_cursor = 0usize;
        let mut out = String::new();
        let mut started = false;

        for edit in overlays {
            let edit_start = edit.start.min(self.mmap.len());
            let edit_end = edit.end.min(self.mmap.len()).max(edit_start);

            if original_cursor < edit_start {
                let original_len = edit_start - original_cursor;
                if started || target < current_cursor + original_len {
                    let start = if started {
                        original_cursor
                    } else {
                        original_cursor + target.saturating_sub(current_cursor)
                    };
                    started = true;
                    append_utf8_bytes(&mut out, &self.mmap[start..edit_start], max_bytes);
                    if out.len() >= max_bytes {
                        return out;
                    }
                }
                current_cursor += original_len;
            }

            let replacement_len = edit.replacement.len();
            if replacement_len > 0 && (started || target < current_cursor + replacement_len) {
                let start = if started {
                    0
                } else {
                    target.saturating_sub(current_cursor)
                };
                started = true;
                append_str_bytes(&mut out, &edit.replacement, start, max_bytes);
                if out.len() >= max_bytes {
                    return out;
                }
            }
            current_cursor += replacement_len;
            original_cursor = edit_end;
        }

        if original_cursor < self.mmap.len() {
            let original_len = self.mmap.len() - original_cursor;
            if started || target < current_cursor + original_len {
                let start = if started {
                    original_cursor
                } else {
                    original_cursor + target.saturating_sub(current_cursor)
                };
                append_utf8_bytes(&mut out, &self.mmap[start..], max_bytes);
            }
        }

        out
    }

    pub fn slice_chars(&self, start: usize, end: usize) -> String {
        self.window_rope
            .slice(start.min(self.window_rope.len_chars())..end.min(self.window_rope.len_chars()))
            .to_string()
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        let idx = char_idx.min(self.window_rope.len_chars());
        self.window_rope.insert(idx, text);
        self.replace_visible_window_overlay();
    }

    pub fn remove(&mut self, range: std::ops::Range<usize>) {
        let start = range.start.min(self.window_rope.len_chars());
        let end = range.end.min(self.window_rope.len_chars());
        if start < end {
            self.window_rope.remove(start..end);
            self.replace_visible_window_overlay();
        }
    }

    pub fn save_to(&self, path: &Path) -> Result<usize> {
        let file = File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut writer = BufWriter::new(file);
        let mut written = 0usize;
        let mut cursor = 0usize;

        let mut overlays = self.overlays.clone();
        overlays.sort_by_key(|edit| edit.start);

        for edit in overlays {
            if edit.start < cursor {
                continue;
            }
            if cursor < edit.start {
                writer.write_all(&self.mmap[cursor..edit.start])?;
                written += edit.start - cursor;
            }
            writer.write_all(edit.replacement.as_bytes())?;
            written += edit.replacement.len();
            cursor = edit.end.min(self.mmap.len());
        }

        if cursor < self.mmap.len() {
            writer.write_all(&self.mmap[cursor..])?;
            written += self.mmap.len() - cursor;
        }
        writer.flush()?;
        Ok(written)
    }

    pub fn load_window_at(&mut self, byte_offset: usize, lines: usize) -> Result<()> {
        let start = previous_line_boundary(&self.mmap, byte_offset.min(self.mmap.len()));
        let (end, _) = Self::visible_chunk(&self.mmap, start, lines)?;
        let text = self.render_original_range(start, end)?;
        self.window_start = start;
        self.window_end = end;
        self.window_rope = Rope::from_str(&text);
        Ok(())
    }

    pub fn first_visible_chunk(mmap: &Mmap, max_lines: usize) -> Result<&str> {
        let (_, text) = Self::visible_chunk(mmap, 0, max_lines)?;
        Ok(text)
    }

    fn visible_chunk(mmap: &Mmap, start: usize, max_lines: usize) -> Result<(usize, &str)> {
        if mmap.is_empty() {
            return Ok((0, ""));
        }
        if start >= mmap.len() {
            return Ok((mmap.len(), ""));
        }

        let bytes = &mmap[start..];
        let scan_len = bytes.len().min(MAX_SCAN_BYTES_FOR_WINDOW);
        let mut line_count = 0usize;
        let mut end = start + scan_len;

        for (idx, byte) in bytes[..scan_len].iter().enumerate() {
            if *byte == b'\n' {
                line_count += 1;
                if line_count >= max_lines {
                    end = start + idx + 1;
                    break;
                }
            }
        }

        // Ensure end is at a valid UTF-8 character boundary
        while end > start && (mmap[end - 1] & 0xC0) == 0x80 {
            end -= 1;
        }
        if end > start && (mmap[end - 1] & 0x80) != 0 {
            // Check if the last byte is a start of a multi-byte sequence that is truncated
            let last_byte = mmap[end - 1];
            let needed = if (last_byte & 0xE0) == 0xC0 {
                2
            } else if (last_byte & 0xF0) == 0xE0 {
                3
            } else if (last_byte & 0xF8) == 0xF0 {
                4
            } else {
                1
            };
            if needed > 1 {
                end -= 1;
            }
        }

        let text = std::str::from_utf8(&mmap[start..end]).map_err(|err| {
            anyhow!(
                "UTF-8 error in visible mmap window at {}-{}: {}",
                start,
                end,
                err
            )
        })?;
        Ok((end, text))
    }

    fn replace_visible_window_overlay(&mut self) {
        let replacement = self.window_rope.to_string();
        let original =
            std::str::from_utf8(&self.mmap[self.window_start..self.window_end]).unwrap_or_default();

        self.overlays
            .retain(|edit| edit.end <= self.window_start || edit.start >= self.window_end);

        if replacement != original {
            self.overlays.push(OverlayEdit {
                start: self.window_start,
                end: self.window_end,
                replacement,
            });
            self.overlays.sort_by_key(|edit| edit.start);
        }
        self.coalesce_overlays();
    }

    fn coalesce_overlays(&mut self) {
        if self.overlays.len() < 2 {
            return;
        }
        self.overlays.sort_by_key(|edit| edit.start);
        let mut merged: Vec<OverlayEdit> = Vec::new();
        for edit in self.overlays.drain(..) {
            if let Some(last) = merged.last_mut() {
                if edit.start <= last.end {
                    last.end = last.end.max(edit.end);
                    last.replacement.push_str(&edit.replacement);
                    continue;
                }
            }
            merged.push(edit);
        }
        self.overlays = merged;
    }

    fn render_original_range(&self, start: usize, end: usize) -> Result<String> {
        if self.overlays.is_empty() {
            return Ok(std::str::from_utf8(&self.mmap[start..end])?.to_string());
        }

        let mut out = String::new();
        let mut cursor = start;
        for edit in self
            .overlays
            .iter()
            .filter(|edit| edit.end > start && edit.start < end)
        {
            if cursor < edit.start {
                out.push_str(std::str::from_utf8(
                    &self.mmap[cursor..edit.start.min(end)],
                )?);
            }
            if edit.start >= start && edit.start < end {
                out.push_str(&edit.replacement);
            }
            cursor = edit.end.min(end);
        }
        if cursor < end {
            out.push_str(std::str::from_utf8(&self.mmap[cursor..end])?);
        }
        Ok(out)
    }
}

fn previous_line_boundary(bytes: &[u8], mut offset: usize) -> usize {
    if offset >= bytes.len() {
        offset = bytes.len();
    }
    while offset > 0 && bytes[offset - 1] != b'\n' {
        offset -= 1;
    }
    offset
}

fn trim_line_end(mut line: String) -> String {
    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    line
}

fn append_str_bytes(out: &mut String, text: &str, start: usize, max_bytes: usize) {
    if out.len() >= max_bytes || start >= text.len() {
        return;
    }
    let start = next_char_boundary(text, start);
    append_utf8_bytes(out, &text.as_bytes()[start..], max_bytes);
}

fn append_utf8_bytes(out: &mut String, bytes: &[u8], max_bytes: usize) {
    if out.len() >= max_bytes || bytes.is_empty() {
        return;
    }
    let remaining = max_bytes - out.len();
    let mut end = bytes.len().min(remaining);
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    if end == 0 {
        return;
    }
    if let Ok(text) = std::str::from_utf8(&bytes[..end]) {
        out.push_str(text);
    }
}

fn next_char_boundary(text: &str, mut idx: usize) -> usize {
    idx = idx.min(text.len());
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}
