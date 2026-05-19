#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhichKeyEntry {
    pub key: String,
    pub label: String,
}

pub fn render(entries: &[WhichKeyEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{} {}", entry.key, entry.label))
        .collect::<Vec<_>>()
        .join("  ")
}
