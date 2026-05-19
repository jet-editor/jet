#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub detail: String,
}

pub fn filter(items: &[CompletionItem], query: &str) -> Vec<CompletionItem> {
    items
        .iter()
        .filter(|item| item.label.contains(query))
        .cloned()
        .collect()
}
