#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    pub text: String,
}

pub fn render_hover(hover: Option<&Hover>) -> String {
    hover.map(|item| item.text.clone()).unwrap_or_default()
}
