pub fn render_searchbar(query: &str, matches: usize) -> String {
    format!("/{}  {} matches", query, matches)
}
