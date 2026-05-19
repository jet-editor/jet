use jet::editor::picker;
use std::io::Write;
use tempfile::TempDir;

#[test]
fn preview_reads_first_lines_of_file() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("sample.txt");
    let mut file = std::fs::File::create(&path).unwrap();
    writeln!(file, "alpha").unwrap();
    writeln!(file, "beta").unwrap();
    writeln!(file, "gamma").unwrap();

    let lines = picker::preview_file_lines(&path, 2);
    assert_eq!(lines, vec!["alpha".to_string(), "beta".to_string()]);
}
