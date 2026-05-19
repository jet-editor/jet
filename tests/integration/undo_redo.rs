use jet::buffer::history::{History, HistoryEntry};

#[test]
fn history_round_trip() {
    let mut history = History::default();
    history.push(HistoryEntry::Insert {
        idx: 0,
        text: "abc".to_string(),
    });
    assert_eq!(history.len(), 1);
    assert!(history.undo_entry().is_some());
    assert!(history.redo_entry().is_some());
}
