use crate::editor::cursor::Cursor;

/// Rows reserved below the editor for bufferline + status.
pub const CHROME_ROWS: usize = 2;

#[derive(Debug, Clone, Copy)]
pub struct View {
    pub top_line: usize,
    pub left_col: usize,
    pub width: usize,
    pub height: usize,
    pub scrolloff: usize,
}

impl View {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            top_line: 0,
            left_col: 0,
            width,
            height: height.max(1),
            scrolloff: 5,
        }
    }

    pub fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height.max(1);
    }

    pub fn ensure_cursor_visible(
        &mut self,
        buffer: &crate::buffer::rope::EditorBuffer,
        cursor: &Cursor,
    ) {
        let scrolloff = self.scrolloff;
        if cursor.row < self.top_line.saturating_add(scrolloff) {
            self.top_line = cursor.row.saturating_sub(scrolloff);
        }
        if cursor.row + 1 + scrolloff > self.top_line + self.height {
            self.top_line = cursor
                .row
                .saturating_add(1 + scrolloff)
                .saturating_sub(self.height);
        }

        let line = buffer.line_string(cursor.row);
        let cursor_x =
            crate::util::unicode::display_width(&line.chars().take(cursor.col).collect::<String>());

        if cursor_x < self.left_col {
            self.left_col = cursor_x;
        }
        if cursor_x >= self.left_col + self.width {
            self.left_col = cursor_x.saturating_sub(self.width.saturating_sub(1));
        }
    }
}
