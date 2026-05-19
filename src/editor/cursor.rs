use crate::buffer::rope::EditorBuffer;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
}

impl Cursor {
    pub fn move_up(&mut self, buffer: &EditorBuffer) {
        self.row = self.row.saturating_sub(1);
        self.clamp_col(buffer);
    }

    pub fn move_down(&mut self, buffer: &EditorBuffer) {
        self.row = (self.row + 1).min(buffer.len_lines().saturating_sub(1));
        self.clamp_col(buffer);
    }

    pub fn move_left(&mut self, buffer: &EditorBuffer) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = buffer.line_len(self.row);
        }
    }

    pub fn move_right(&mut self, buffer: &EditorBuffer) {
        let line_len = buffer.line_len(self.row);
        if self.col < line_len {
            self.col += 1;
        } else if self.row + 1 < buffer.len_lines() {
            self.row += 1;
            self.col = 0;
        }
    }

    pub fn clamp_col(&mut self, buffer: &EditorBuffer) {
        self.col = self.col.min(buffer.line_len(self.row));
    }
}
