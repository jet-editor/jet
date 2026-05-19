pub fn render_statusbar(filename: &str, row: usize, col: usize, dirty: bool) -> String {
    let marker = if dirty { " +" } else { "" };
    format!("{}{}  {}:{}", filename, marker, row + 1, col + 1)
}
