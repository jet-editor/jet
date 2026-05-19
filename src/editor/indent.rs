pub fn leading_indent(line: &str) -> &str {
    let end = line
        .char_indices()
        .find_map(|(idx, ch)| (!ch.is_whitespace()).then_some(idx))
        .unwrap_or(line.len());
    &line[..end]
}

pub fn indent_after_newline(previous_line: &str) -> String {
    leading_indent(previous_line).to_string()
}
