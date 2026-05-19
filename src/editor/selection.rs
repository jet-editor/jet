#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub fn cursor(position: usize) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }

    pub fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    pub fn start(self) -> usize {
        self.anchor.min(self.head)
    }

    pub fn end(self) -> usize {
        self.anchor.max(self.head)
    }

    pub fn range(self) -> std::ops::Range<usize> {
        self.start()..self.end()
    }

    pub fn is_cursor(self) -> bool {
        self.anchor == self.head
    }

    pub fn len(self) -> usize {
        self.end() - self.start()
    }

    pub fn is_empty(self) -> bool {
        self.is_cursor()
    }

    pub fn move_to(&mut self, position: usize) {
        self.anchor = position;
        self.head = position;
    }

    pub fn extend_to(&mut self, position: usize) {
        self.head = position;
    }
}
