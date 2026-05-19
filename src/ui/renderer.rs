use std::fmt::Write as _;
use unicode_width::UnicodeWidthChar;

/// Small differential renderer used by the TUI and frame benchmarks.
pub struct FrameRenderer {
    width: u16,
    height: u16,
    previous: Vec<String>,
    scratch: String,
}

impl FrameRenderer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            previous: Vec::new(),
            scratch: String::with_capacity(width as usize * height as usize),
        }
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        if self.width != width || self.height != height {
            self.width = width;
            self.height = height;
            self.previous.clear();
        }
    }

    pub fn render_to_string<'a>(&mut self, lines: impl IntoIterator<Item = &'a str>) -> &str {
        self.scratch.clear();
        let width = self.width as usize;
        let height = self.height as usize;
        let mut display_buf = String::new();

        for (row, line) in lines.into_iter().take(height).enumerate() {
            truncate_to_width_into(line, width, &mut display_buf);
            if self.previous.get(row).map(String::as_str) != Some(display_buf.as_str()) {
                let _ = writeln!(self.scratch, "\x1b[{};1H\x1b[2K{}", row + 1, display_buf);
                if row >= self.previous.len() {
                    self.previous.push(display_buf.clone());
                } else {
                    self.previous[row].clear();
                    self.previous[row].push_str(&display_buf);
                }
            }
        }

        self.scratch.as_str()
    }

    pub fn previous_len(&self) -> usize {
        self.previous.len()
    }
}

fn truncate_to_width_into(line: &str, width: usize, out: &mut String) {
    out.clear();
    let mut visible_width = 0usize;
    let mut idx = 0usize;
    let mut truncated = false;

    while idx < line.len() {
        if let Some(end) = ansi_escape_end(line, idx) {
            out.push_str(&line[idx..end]);
            idx = end;
            continue;
        }

        let ch = line[idx..].chars().next().expect("idx is a char boundary");
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if visible_width + char_width > width {
            truncated = true;
            break;
        }
        out.push(ch);
        visible_width += char_width;
        idx += ch.len_utf8();
    }

    if truncated && idx < line.len() && line.as_bytes().contains(&0x1b) {
        out.push_str("\x1b[0m");
    }
}

fn ansi_escape_end(line: &str, idx: usize) -> Option<usize> {
    let rest = line.get(idx..)?;
    if !rest.starts_with("\x1b[") {
        return None;
    }
    rest.find('m').map(|end| idx + end + 1)
}
