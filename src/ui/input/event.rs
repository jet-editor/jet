#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Key(String),
    Mouse { x: u16, y: u16 },
    Paste(String),
}
