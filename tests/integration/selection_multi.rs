use jet::editor::{mode::SelectionSet, selection::Selection};

#[test]
fn multiple_selections_track_count() {
    let mut set = SelectionSet::new();
    set.push_selection(Selection::new(0, 3));
    set.push_selection(Selection::new(10, 15));
    assert_eq!(set.selections().len(), 3);
}
