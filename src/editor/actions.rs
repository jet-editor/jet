use crate::{
    buffer::{history::HistoryEntry, rope::EditorBuffer},
    editor::{mode::SelectionSet, selection::Selection},
};

pub fn apply_history_forward(buffer: &mut EditorBuffer, entry: &HistoryEntry) {
    match entry {
        HistoryEntry::Insert { idx, text } => {
            buffer.insert(*idx, text);
        }
        HistoryEntry::Delete { idx, text } => {
            buffer.remove(*idx..idx + text.chars().count());
        }
    }
}

pub fn apply_history_backward(buffer: &mut EditorBuffer, entry: &HistoryEntry) {
    match entry {
        HistoryEntry::Insert { idx, text } => {
            buffer.remove(*idx..idx + text.chars().count());
        }
        HistoryEntry::Delete { idx, text } => {
            buffer.insert(*idx, text);
        }
    }
}

pub fn restore_selections(buffer: &mut EditorBuffer, snapshot: &[(usize, usize)]) {
    if snapshot.is_empty() {
        return;
    }
    let selections = snapshot
        .iter()
        .map(|(anchor, head)| Selection::new(*anchor, *head))
        .collect();
    buffer.set_selections(SelectionSet::from_vec(selections));
}

pub fn toggle_case(text: &str) -> String {
    text.chars()
        .map(|ch| {
            if ch.is_uppercase() {
                ch.to_lowercase().collect::<String>()
            } else if ch.is_lowercase() {
                ch.to_uppercase().collect::<String>()
            } else {
                ch.to_string()
            }
        })
        .collect()
}

pub fn indent_lines(text: &str, width: usize) -> String {
    let pad = " ".repeat(width);
    text.lines()
        .map(|line| format!("{pad}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn dedent_lines(text: &str, width: usize) -> String {
    text.lines()
        .map(|line| {
            let mut removed = 0;
            let trimmed: String = line
                .chars()
                .take_while(|ch| {
                    if removed < width && matches!(ch, ' ' | '\t') {
                        removed += 1;
                        true
                    } else {
                        false
                    }
                })
                .collect();
            line[trimmed.len()..].to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
