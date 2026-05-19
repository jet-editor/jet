#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalLine {
    pub text: String,
}

pub fn render(lines: &[TerminalLine]) -> String {
    lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}
