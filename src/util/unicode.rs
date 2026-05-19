use unicode_width::UnicodeWidthStr;

pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices()
        .nth(char_idx)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}
