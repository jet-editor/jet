pub fn render_context(mode: &str, message: &str) -> String {
    if message.is_empty() {
        mode.to_string()
    } else {
        format!("{}: {}", mode, message)
    }
}
