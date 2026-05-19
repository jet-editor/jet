use crate::editor::selection::Selection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Select,
    Goto,
    Match,
    Space,
    View,
    Command,
    Picker,
    Search,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
            Self::Select => "SELECT",
            Self::Goto => "GOTO",
            Self::Match => "MATCH",
            Self::Space => "SPACE",
            Self::View => "VIEW",
            Self::Command => "COMMAND",
            Self::Picker => "PICKER",
            Self::Search => "SEARCH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSet {
    selections: Vec<Selection>,
    primary: usize,
}

impl SelectionSet {
    pub fn new() -> Self {
        Self {
            selections: vec![Selection::cursor(0)],
            primary: 0,
        }
    }

    pub fn from_vec(mut selections: Vec<Selection>) -> Self {
        if selections.is_empty() {
            selections.push(Selection::cursor(0));
        }
        Self {
            selections,
            primary: 0,
        }
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }

    pub fn selections_mut(&mut self) -> &mut Vec<Selection> {
        &mut self.selections
    }

    pub fn primary(&self) -> Selection {
        self.selections[self.primary]
    }

    pub fn set_primary(&mut self, selection: Selection) {
        self.selections[self.primary] = selection;
    }

    pub fn collapse_to_primary(&mut self) {
        let primary = self.primary();
        self.selections.clear();
        self.selections.push(primary);
        self.primary = 0;
    }

    pub fn rotate_forward(&mut self) {
        if !self.selections.is_empty() {
            self.primary = (self.primary + 1) % self.selections.len();
        }
    }

    pub fn rotate_backward(&mut self) {
        if !self.selections.is_empty() {
            self.primary = if self.primary == 0 {
                self.selections.len() - 1
            } else {
                self.primary - 1
            };
        }
    }

    pub fn push_selection(&mut self, selection: Selection) {
        self.selections.push(selection);
    }
}

impl Default for SelectionSet {
    fn default() -> Self {
        Self::new()
    }
}
