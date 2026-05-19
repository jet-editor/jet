#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterId {
    Unnamed,
    Blackhole,
    LastYank,
    SmallDelete,
    Named(u8),
}

impl RegisterId {
    pub fn from_char(ch: char) -> Option<Self> {
        match ch {
            '"' => Some(Self::Unnamed),
            '_' => Some(Self::Blackhole),
            '0' => Some(Self::LastYank),
            '-' => Some(Self::SmallDelete),
            'a'..='z' => Some(Self::Named((ch as u8) - b'a')),
            'A'..='Z' => Some(Self::Named((ch.to_ascii_lowercase() as u8) - b'a')),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RegisterBank {
    named: [String; 26],
    unnamed: String,
    last_yank: String,
    small_delete: String,
}

impl RegisterBank {
    pub fn get(&self, id: RegisterId) -> &str {
        match id {
            RegisterId::Unnamed => &self.unnamed,
            RegisterId::LastYank => &self.last_yank,
            RegisterId::SmallDelete => &self.small_delete,
            RegisterId::Named(index) => &self.named[index as usize],
            RegisterId::Blackhole => "",
        }
    }

    pub fn yank(&mut self, id: RegisterId, text: String) {
        if matches!(id, RegisterId::Blackhole) {
            return;
        }
        if !text.is_empty() {
            self.last_yank = text.clone();
        }
        match id {
            RegisterId::Unnamed => self.unnamed = text,
            RegisterId::Named(index) => self.named[index as usize] = text,
            RegisterId::SmallDelete => self.small_delete = text,
            RegisterId::LastYank => self.last_yank = text,
            RegisterId::Blackhole => {}
        }
    }

    pub fn small_delete(&mut self, text: String) {
        if text.chars().count() < 120 {
            self.small_delete = text;
        }
    }
}
