use crate::config::schema::JetConfig;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone, Default)]
pub struct BindingTable {
    bindings: Vec<(Vec<KeyChord>, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingMatch<'a> {
    None,
    Prefix,
    Complete(&'a str),
}

impl BindingTable {
    pub fn from_config(config: &JetConfig, mode: &str) -> Self {
        let mut table = Self::default();
        if let Some(mode_bindings) = config.keybindings.get(mode) {
            for (sequence, action) in mode_bindings {
                if let Some(chords) = parse_sequence(sequence) {
                    table.bindings.push((chords, action.clone()));
                }
            }
        }
        table
    }

    pub fn push_binding(&mut self, sequence: &str, action: String) {
        if let Some(chords) = parse_sequence(sequence) {
            self.bindings.push((chords, action));
        }
    }

    pub fn extend_bindings(&mut self, entries: impl IntoIterator<Item = (String, String)>) {
        for (key, action) in entries {
            self.push_binding(&key, action);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn match_sequence(&self, chords: &[KeyChord]) -> BindingMatch<'_> {
        if chords.is_empty() {
            return BindingMatch::None;
        }
        let mut complete = None;
        let mut prefix = false;
        for (sequence, action) in &self.bindings {
            if sequence.len() < chords.len() {
                continue;
            }
            if sequence.starts_with(chords) {
                if sequence.len() == chords.len() {
                    complete = Some(action.as_str());
                } else {
                    prefix = true;
                }
            }
        }
        match (complete, prefix) {
            (Some(action), _) => BindingMatch::Complete(action),
            (None, true) => BindingMatch::Prefix,
            _ => BindingMatch::None,
        }
    }

    pub fn starts_with(&self, chord: &KeyChord) -> bool {
        self.bindings
            .iter()
            .any(|(sequence, _)| sequence.first() == Some(chord))
    }

    pub fn which_key_entries(&self) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        for (sequence, action) in &self.bindings {
            if sequence.len() != 1 {
                continue;
            }
            entries.push((chord_label(&sequence[0]), action.clone()));
        }
        entries.sort_by(|(a, _), (b, _)| a.cmp(b));
        entries
    }
}

pub fn chord_label(chord: &KeyChord) -> String {
    let mut label = String::new();
    if chord.modifiers.contains(KeyModifiers::CONTROL) {
        label.push_str("C-");
    }
    if chord.modifiers.contains(KeyModifiers::ALT) {
        label.push_str("A-");
    }
    if chord.modifiers.contains(KeyModifiers::SHIFT) {
        label.push_str("S-");
    }
    match chord.code {
        KeyCode::Char(' ') => label.push_str("space"),
        KeyCode::Esc => label.push_str("esc"),
        KeyCode::Tab => label.push_str("tab"),
        KeyCode::Enter => label.push_str("enter"),
        KeyCode::Backspace => label.push_str("backspace"),
        KeyCode::Up => label.push_str("up"),
        KeyCode::Down => label.push_str("down"),
        KeyCode::Left => label.push_str("left"),
        KeyCode::Right => label.push_str("right"),
        KeyCode::Char(ch) => label.push(ch),
        _ => label.push('?'),
    }
    label
}

pub fn chord_from_event(key: KeyEvent) -> KeyChord {
    KeyChord {
        code: key.code,
        modifiers: key.modifiers,
    }
}

pub fn parse_sequence(sequence: &str) -> Option<Vec<KeyChord>> {
    let parts = sequence.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    parts.iter().map(|part| parse_part(part)).collect()
}

fn parse_part(part: &str) -> Option<KeyChord> {
    let lower = part.to_lowercase();
    let (mods, key_part) = split_modifiers(&lower);
    let code = match key_part.as_str() {
        "space" => KeyCode::Char(' '),
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "enter" => KeyCode::Enter,
        "backspace" => KeyCode::Backspace,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        key if key.len() == 1 => KeyCode::Char(key.chars().next()?),
        _ => return None,
    };
    Some(KeyChord {
        code,
        modifiers: mods,
    })
}

fn split_modifiers(part: &str) -> (KeyModifiers, String) {
    let mut modifiers = KeyModifiers::empty();
    let mut key = part.to_string();
    while let Some((head, tail)) = key.split_once('-') {
        match head {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            _ => break,
        }
        key = tail.to_string();
    }
    (modifiers, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn parses_space_sequence() {
        let chords = parse_sequence("space d").unwrap();
        assert_eq!(chords.len(), 2);
        assert_eq!(chords[0].code, KeyCode::Char(' '));
        assert_eq!(chords[1].code, KeyCode::Char('d'));
    }

    #[test]
    fn which_key_lists_single_chord_bindings() {
        let mut map = HashMap::new();
        map.insert("s".to_string(), "write".to_string());
        map.insert("space d".to_string(), "diagnostics".to_string());
        let config = JetConfig {
            keybindings: HashMap::from([(String::from("space"), map)]),
            ..JetConfig::default()
        };
        let table = BindingTable::from_config(&config, "space");
        let entries = table.which_key_entries();
        assert_eq!(entries, vec![("s".to_string(), "write".to_string())]);
    }

    #[test]
    fn matches_complete_binding() {
        let mut map = HashMap::new();
        map.insert("space d".to_string(), "diagnostics".to_string());
        let config = JetConfig {
            keybindings: HashMap::from([(String::from("normal"), map)]),
            ..JetConfig::default()
        };
        let table = BindingTable::from_config(&config, "normal");
        let chords = parse_sequence("space d").unwrap();
        assert!(matches!(
            table.match_sequence(&chords),
            BindingMatch::Complete("diagnostics")
        ));
    }
}
