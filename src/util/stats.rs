use crate::buffer::rope::EditorBuffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BufferStats {
    pub bytes: usize,
    pub lines: usize,
    pub chars: usize,
}

impl BufferStats {
    pub fn from_buffer(buffer: &EditorBuffer) -> Self {
        Self {
            bytes: buffer.len_bytes(),
            lines: buffer.len_lines(),
            chars: buffer.len_chars(),
        }
    }
}
