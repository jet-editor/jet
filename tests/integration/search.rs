use jet::editor::search::SearchEngine;

#[test]
fn count_matches() {
    let engine = SearchEngine::new("ERROR");
    assert_eq!(engine.count_in_bytes(b"ERROR ok ERROR"), 2);
}

#[test]
fn count_single_byte_matches() {
    let engine = SearchEngine::new("x");
    assert_eq!(engine.count_in_bytes(b"axbxc"), 2);
}
