use jet::{
    buffer::rope::EditorBuffer,
    editor::{mode::SelectionSet, selection::Selection},
};

#[test]
fn insert_at_multiple_positions_from_end() {
    let mut buffer = EditorBuffer::from_text("abcdef");
    let mut set = SelectionSet::new();
    set.set_primary(Selection::cursor(1));
    set.push_selection(Selection::cursor(4));
    *buffer.selections_mut() = set;

    let mut positions: Vec<_> = buffer
        .selections()
        .selections()
        .iter()
        .map(|selection| selection.head)
        .collect();
    positions.sort_unstable();
    for idx in positions.into_iter().rev() {
        buffer.insert(idx, "X");
    }
    assert_eq!(buffer.to_string(), "aXbcdXef");
}
