use crate::{
    editor::{buffers::BufferId, cursor::Cursor, view::View},
    ui::layout::Rect,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SplitId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
pub struct Split {
    pub id: SplitId,
    pub buffer: BufferId,
    pub cursor: Cursor,
    pub view: View,
}

pub struct SplitManager {
    splits: Vec<Split>,
    focused: SplitId,
    layout_direction: SplitDirection,
}

impl SplitManager {
    pub fn new(buffer: BufferId, width: usize, height: usize) -> Self {
        let focused = SplitId(0);
        Self {
            splits: vec![Split {
                id: focused,
                buffer,
                cursor: Cursor::default(),
                view: View::new(width, height),
            }],
            focused,
            layout_direction: SplitDirection::Horizontal,
        }
    }

    pub fn focused(&self) -> &Split {
        self.splits
            .iter()
            .find(|split| split.id == self.focused)
            .expect("focused split exists")
    }

    pub fn focused_mut(&mut self) -> &mut Split {
        self.splits
            .iter_mut()
            .find(|split| split.id == self.focused)
            .expect("focused split exists")
    }

    pub fn split(&mut self, direction: SplitDirection) -> SplitId {
        let source = self.focused().clone();
        let id = SplitId(self.next_id());
        let mut new_split = source;
        new_split.id = id;
        self.layout_direction = direction;
        match direction {
            SplitDirection::Horizontal => {
                new_split.view.height = new_split.view.height.saturating_div(2).max(1);
            }
            SplitDirection::Vertical => {
                new_split.view.width = new_split.view.width.saturating_div(2).max(1);
            }
        }
        self.splits.push(new_split);
        self.focused = id;
        id
    }

    pub fn focus_next(&mut self) -> SplitId {
        let idx = self
            .splits
            .iter()
            .position(|split| split.id == self.focused)
            .unwrap_or(0);
        let next = self.splits[(idx + 1) % self.splits.len()].id;
        self.focused = next;
        next
    }

    pub fn close_focused(&mut self) -> bool {
        if self.splits.len() == 1 {
            return false;
        }
        let focused = self.focused;
        self.splits.retain(|split| split.id != focused);
        self.focused = self.splits[0].id;
        true
    }

    pub fn close_others(&mut self) {
        let focused = self.focused;
        self.splits.retain(|split| split.id == focused);
    }

    pub fn layout(&self, area: Rect) -> Vec<(SplitId, Rect)> {
        if self.splits.is_empty() {
            return Vec::new();
        }
        match self.layout_direction {
            SplitDirection::Horizontal => self.layout_rows(area),
            SplitDirection::Vertical => self.layout_columns(area),
        }
    }

    pub fn splits(&self) -> &[Split] {
        &self.splits
    }

    fn next_id(&self) -> usize {
        self.splits
            .iter()
            .map(|split| split.id.0)
            .max()
            .map(|id| id + 1)
            .unwrap_or(0)
    }

    fn layout_rows(&self, area: Rect) -> Vec<(SplitId, Rect)> {
        let height_each = area.height / self.splits.len() as u16;
        self.splits
            .iter()
            .enumerate()
            .map(|(idx, split)| {
                let y = area.y + idx as u16 * height_each;
                let height = if idx + 1 == self.splits.len() {
                    area.height.saturating_sub(height_each * idx as u16)
                } else {
                    height_each
                };
                (
                    split.id,
                    Rect {
                        x: area.x,
                        y,
                        width: area.width,
                        height,
                    },
                )
            })
            .collect()
    }

    fn layout_columns(&self, area: Rect) -> Vec<(SplitId, Rect)> {
        let width_each = area.width / self.splits.len() as u16;
        self.splits
            .iter()
            .enumerate()
            .map(|(idx, split)| {
                let x = area.x + idx as u16 * width_each;
                let width = if idx + 1 == self.splits.len() {
                    area.width.saturating_sub(width_each * idx as u16)
                } else {
                    width_each
                };
                (
                    split.id,
                    Rect {
                        x,
                        y: area.y,
                        width,
                        height: area.height,
                    },
                )
            })
            .collect()
    }
}
