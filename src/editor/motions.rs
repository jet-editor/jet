use crate::buffer::rope::EditorBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    Move,
    Extend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharSearchMode {
    Inclusive,
    Exclusive,
}

pub fn word_forward(buffer: &EditorBuffer, char_idx: usize) -> usize {
    let len = buffer.len_chars();
    if char_idx >= len {
        return len;
    }

    let mut current = char_idx;
    let start_ch = buffer
        .slice_chars(current, current + 1)
        .chars()
        .next()
        .unwrap_or(' ');
    let start_is_word = is_word_char(start_ch);

    if start_is_word {
        // If we start on a word, skip to the end of it
        while current < len {
            let ch = buffer
                .slice_chars(current, current + 1)
                .chars()
                .next()
                .unwrap_or(' ');
            if !is_word_char(ch) {
                break;
            }
            current += 1;
        }
    }

    // Skip whitespace/non-word chars to find the start of the next word
    while current < len {
        let ch = buffer
            .slice_chars(current, current + 1)
            .chars()
            .next()
            .unwrap_or(' ');
        if is_word_char(ch) {
            return current;
        }
        current += 1;
    }

    len
}

pub fn word_backward(buffer: &EditorBuffer, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }

    let mut current = char_idx;

    // Skip whitespace/non-word chars to find the end of the previous word
    while current > 0 {
        let ch = buffer
            .slice_chars(current - 1, current)
            .chars()
            .next()
            .unwrap_or(' ');
        if is_word_char(ch) {
            break;
        }
        current -= 1;
    }

    // Find the start of that word
    while current > 0 {
        let ch = buffer
            .slice_chars(current - 1, current)
            .chars()
            .next()
            .unwrap_or(' ');
        if !is_word_char(ch) {
            break;
        }
        current -= 1;
    }

    current
}

pub fn word_end(buffer: &EditorBuffer, char_idx: usize) -> usize {
    let len = buffer.len_chars();
    if char_idx >= len {
        return len;
    }

    let mut current = char_idx;
    if current + 1 < len {
        current += 1;
    }

    // Skip whitespace to find the start of a word
    while current < len {
        let ch = buffer
            .slice_chars(current, current + 1)
            .chars()
            .next()
            .unwrap_or(' ');
        if is_word_char(ch) {
            break;
        }
        current += 1;
    }

    // Find the end of that word
    while current < len {
        let ch = buffer
            .slice_chars(current, current + 1)
            .chars()
            .next()
            .unwrap_or(' ');
        if !is_word_char(ch) {
            break;
        }
        current += 1;
    }

    current.saturating_sub(1)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn is_whitespace(ch: char) -> bool {
    ch.is_whitespace()
}

pub fn big_word_forward(buffer: &EditorBuffer, char_idx: usize) -> usize {
    let len = buffer.len_chars();
    let mut current = char_idx.min(len);

    while current < len && is_whitespace(char_at(buffer, current)) {
        current += 1;
    }
    while current < len && !is_whitespace(char_at(buffer, current)) {
        current += 1;
    }
    while current < len && is_whitespace(char_at(buffer, current)) {
        current += 1;
    }
    current.min(len)
}

pub fn big_word_backward(buffer: &EditorBuffer, char_idx: usize) -> usize {
    if char_idx == 0 {
        return 0;
    }
    let mut current = char_idx;

    while current > 0 && is_whitespace(char_at(buffer, current - 1)) {
        current -= 1;
    }
    while current > 0 && !is_whitespace(char_at(buffer, current - 1)) {
        current -= 1;
    }
    current
}

pub fn big_word_end(buffer: &EditorBuffer, char_idx: usize) -> usize {
    let len = buffer.len_chars();
    let mut current = char_idx.saturating_add(1).min(len);

    while current < len && is_whitespace(char_at(buffer, current)) {
        current += 1;
    }
    while current < len && !is_whitespace(char_at(buffer, current)) {
        current += 1;
    }
    current.saturating_sub(1).min(len.saturating_sub(1))
}

fn char_at(buffer: &EditorBuffer, char_idx: usize) -> char {
    buffer.char_at(char_idx)
}

pub fn line_start(buffer: &EditorBuffer, char_idx: usize) -> usize {
    let (row, _) = buffer.char_to_line_col(char_idx);
    buffer.char_idx(row, 0)
}

pub fn line_end(buffer: &EditorBuffer, char_idx: usize) -> usize {
    let (row, _) = buffer.char_to_line_col(char_idx);
    buffer.char_idx(row, buffer.line_len(row))
}

pub fn file_start() -> usize {
    0
}

pub fn file_end(buffer: &EditorBuffer) -> usize {
    buffer.len_chars()
}

pub fn find_char_forward(
    buffer: &EditorBuffer,
    char_idx: usize,
    target: char,
    mode: CharSearchMode,
) -> Option<usize> {
    let len = buffer.len_chars();
    let start = char_idx.saturating_add(1);
    for pos in start..len {
        if char_at(buffer, pos) == target {
            return Some(match mode {
                CharSearchMode::Inclusive => pos,
                CharSearchMode::Exclusive => pos.saturating_sub(1),
            });
        }
    }
    None
}

pub fn find_char_backward(
    buffer: &EditorBuffer,
    char_idx: usize,
    target: char,
    mode: CharSearchMode,
) -> Option<usize> {
    let start = char_idx;
    for pos in (0..start).rev() {
        if char_at(buffer, pos) == target {
            return Some(match mode {
                CharSearchMode::Inclusive => pos,
                CharSearchMode::Exclusive => pos.saturating_add(1),
            });
        }
    }
    None
}

pub fn word_under_cursor(buffer: &EditorBuffer, char_idx: usize) -> Option<(usize, usize)> {
    if buffer.len_chars() == 0 {
        return None;
    }
    let idx = char_idx.min(buffer.len_chars().saturating_sub(1));
    let mut start = idx;
    let mut end = idx.saturating_add(1);

    while start > 0 && is_word_char(char_at(buffer, start - 1)) {
        start -= 1;
    }
    while end < buffer.len_chars() && is_word_char(char_at(buffer, end)) {
        end += 1;
    }

    if start == end {
        return None;
    }
    Some((start, end))
}

#[derive(Debug, Default, Clone)]
pub struct JumpList {
    entries: Vec<Jump>,
    cursor: usize,
    capacity: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jump {
    pub file: Option<std::path::PathBuf>,
    pub position: usize,
}

impl JumpList {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
            cursor: 0,
            capacity: capacity.max(1),
        }
    }

    pub fn push(&mut self, jump: Jump) {
        if self.entries.last() == Some(&jump) {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.remove(0);
            self.cursor = self.cursor.saturating_sub(1);
        }
        if self.cursor + 1 < self.entries.len() {
            self.entries.truncate(self.cursor + 1);
        }
        self.entries.push(jump);
        self.cursor = self.entries.len().saturating_sub(1);
    }

    pub fn backward(&mut self) -> Option<&Jump> {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
        self.entries.get(self.cursor)
    }

    pub fn forward(&mut self) -> Option<&Jump> {
        if self.cursor + 1 < self.entries.len() {
            self.cursor += 1;
        }
        self.entries.get(self.cursor)
    }
}

pub fn char_index_from_byte(text: &str, byte: usize) -> usize {
    text.char_indices()
        .take_while(|(idx, _)| *idx < byte)
        .count()
}
