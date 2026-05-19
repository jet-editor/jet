use jet::{buffer::rope::EditorBuffer, editor::surround};

#[test]
fn wrap_selection_adds_delimiters() {
    let buffer = EditorBuffer::from_text("hello");
    let (text, start, end) = surround::wrap_range(&buffer, 0, 5, '"').unwrap();
    assert_eq!(text, "\"hello\"");
    assert_eq!((start, end), (0, 5));
}

#[test]
fn change_surrounding_replaces_delimiters() {
    let buffer = EditorBuffer::from_text("(hello)");
    let (text, start, end) = surround::change_surrounding(&buffer, 1, 6, '"').unwrap();
    assert_eq!(text, "\"hello\"");
    assert_eq!((start, end), (0, 7));
}

#[test]
fn delete_surrounding_removes_pair() {
    let buffer = EditorBuffer::from_text("(hello)");
    let pair = surround::delete_surrounding(&buffer, 1, 6).unwrap();
    assert_eq!(pair.open, '(');
    assert_eq!(pair.close, ')');
}
