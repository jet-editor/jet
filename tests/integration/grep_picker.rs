use jet::editor::grep;
use tempfile::TempDir;

#[test]
fn grep_project_finds_matches_line_by_line() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    std::fs::write(root.join("a.txt"), "hello\nneedle here\n").unwrap();
    std::fs::write(root.join("b.txt"), "nothing\n").unwrap();

    let hits = grep::grep_project(root, "needle", 16);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 1);
    assert_eq!(hits[0].column, 0);
    assert!(hits[0].text.contains("needle"));
}

#[test]
fn grep_skips_binary_files() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let data = vec![0u8, b'a', b'b', b'c'];
    std::fs::write(root.join("bin.dat"), data).unwrap();

    let hits = grep::grep_project(root, "abc", 8);
    assert!(hits.is_empty());
}
