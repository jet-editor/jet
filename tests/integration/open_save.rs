use jet::buffer::rope::EditorBuffer;

#[test]
fn open_existing_file_lazily() {
    let file = tempfile::NamedTempFile::new().expect("temp file");
    std::fs::write(file.path(), b"one\ntwo\nthree\n").expect("write fixture");
    let buffer = EditorBuffer::open(file.path()).expect("open buffer");
    assert_eq!(buffer.len_bytes(), 14);
    assert_eq!(buffer.line_string(0), "one");
}

#[test]
fn save_heap_buffer() {
    let file = tempfile::NamedTempFile::new().expect("temp file");
    let buffer = EditorBuffer::from_text("saved\n");
    let bytes = buffer.save_to(file.path()).expect("save buffer");
    assert_eq!(bytes, 6);
    assert_eq!(std::fs::read_to_string(file.path()).unwrap(), "saved\n");
}
