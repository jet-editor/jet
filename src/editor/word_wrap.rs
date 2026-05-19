pub fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    if line.is_empty() {
        return vec![String::new()];
    }

    let mut out = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0usize;

    for ch in line.chars() {
        let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);

        if current_width + ch_width > width && !current_line.is_empty() {
            out.push(current_line);
            current_line = String::new();
            current_width = 0;
        }

        current_line.push(ch);
        current_width += ch_width;
    }

    if !current_line.is_empty() {
        out.push(current_line);
    }

    if out.is_empty() {
        out.push(String::new());
    }

    out
}

/// Wraps a list of lines to the given width.
/// Returns (wrapped_lines, is_continuation) where is_continuation[i]
/// is true if wrapped_lines[i] is a continuation of the previous logical line.
pub fn wrap_lines(lines: Vec<String>, width: usize) -> (Vec<String>, Vec<bool>) {
    let mut wrapped = Vec::new();
    let mut continuation = Vec::new();
    for line in lines {
        let sub = wrap_line(&line, width);
        for (i, s) in sub.into_iter().enumerate() {
            continuation.push(i > 0);
            wrapped.push(s);
        }
    }
    (wrapped, continuation)
}
