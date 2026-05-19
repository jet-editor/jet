use crate::buffer::rope::EditorBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurroundPair {
    pub open: char,
    pub close: char,
    pub open_index: usize,
    pub close_index: usize,
}

pub fn pair_chars(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' | '\'' | '`' => Some(open),
        _ => None,
    }
}

fn char_at(buffer: &EditorBuffer, index: usize) -> Option<char> {
    buffer.slice_chars(index, index + 1).chars().next()
}

pub fn find_surrounding(buffer: &EditorBuffer, start: usize, end: usize) -> Option<SurroundPair> {
    if start == 0 {
        return None;
    }
    let open_index = start.saturating_sub(1);
    let open = char_at(buffer, open_index)?;
    let close = pair_chars(open)?;
    let close_index = end;
    if char_at(buffer, close_index)? != close {
        return None;
    }
    Some(SurroundPair {
        open,
        close,
        open_index,
        close_index,
    })
}

pub fn wrap_range(
    buffer: &EditorBuffer,
    start: usize,
    end: usize,
    open: char,
) -> Option<(String, usize, usize)> {
    let close = pair_chars(open)?;
    let selected = buffer.slice_chars(start, end);
    let text = format!("{open}{selected}{close}");
    Some((text, start, end))
}

pub fn change_surrounding(
    buffer: &EditorBuffer,
    start: usize,
    end: usize,
    new_open: char,
) -> Option<(String, usize, usize)> {
    let surrounding = find_surrounding(buffer, start, end)?;
    let new_close = pair_chars(new_open)?;
    let selected = buffer.slice_chars(start, end);
    let text = format!("{new_open}{selected}{new_close}");
    let replace_start = surrounding.open_index;
    let replace_end = surrounding.close_index + 1;
    Some((text, replace_start, replace_end))
}

pub fn delete_surrounding(buffer: &EditorBuffer, start: usize, end: usize) -> Option<SurroundPair> {
    find_surrounding(buffer, start, end)
}
