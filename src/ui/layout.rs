#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn inner(self, margin: u16) -> Self {
        let double = margin.saturating_mul(2);
        Self {
            x: self.x.saturating_add(margin),
            y: self.y.saturating_add(margin),
            width: self.width.saturating_sub(double),
            height: self.height.saturating_sub(double),
        }
    }
}

pub fn split_status(area: Rect) -> (Rect, Rect) {
    let status_height = u16::from(area.height > 0);
    let editor = Rect {
        height: area.height.saturating_sub(status_height),
        ..area
    };
    let status = Rect {
        x: area.x,
        y: area.y + editor.height,
        width: area.width,
        height: status_height,
    };
    (editor, status)
}
