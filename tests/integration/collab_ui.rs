use jet::{
    buffer::rope::EditorBuffer,
    collab::{
        protocol::CollabMessage,
        session::CollaborationSession,
        transport::{CollaborationTransport, MemoryTransport},
        ui as collab_ui,
    },
};
use uuid::Uuid;

#[test]
fn awareness_stores_remote_peer_presence() {
    let mut host = CollaborationSession::host("host", "hello");
    let peer_id = Uuid::new_v4();
    host.receive(CollabMessage::Awareness {
        peer_id,
        selections: vec![(4, 4)],
        viewport: (0, 24),
    });
    let presence: Vec<_> = host.remote_presence().collect();
    assert_eq!(presence.len(), 1);
    assert_eq!(presence[0].primary_head(), Some(4));
}

#[test]
fn overlay_highlights_remote_selection_range() {
    let buffer = EditorBuffer::from_text("hello world\n");
    let peer_id = Uuid::new_v4();
    let presence = jet::collab::presence::PeerPresence {
        peer_id,
        name: "bob".to_string(),
        selections: vec![(0, 5)],
        viewport: (0, 1),
        color_index: 0,
    };
    let lines = collab_ui::overlay_remote_selections(
        &buffer,
        0,
        0,
        vec!["hello world".to_string()],
        &[presence],
    );
    assert!(lines[0].contains("\x1b[7m"));
}

#[test]
fn overlay_inserts_colored_peer_caret_on_visible_line() {
    let buffer = EditorBuffer::from_text("hello world\n");
    let peer_id = Uuid::new_v4();
    let presence = jet::collab::presence::PeerPresence {
        peer_id,
        name: "alice".to_string(),
        selections: vec![(0, 0)],
        viewport: (0, 1),
        color_index: 0,
    };
    let lines = collab_ui::overlay_remote_carets(
        &buffer,
        0,
        0,
        vec!["hello world".to_string()],
        &[presence],
    );
    assert!(lines[0].contains('▏'));
    assert!(lines[0].contains("hello"));
}

#[test]
fn memory_transport_applies_remote_edits_to_document() {
    let mut transport = MemoryTransport::default();
    let mut host = CollaborationSession::host("host", "abc");
    let mut guest = CollaborationSession::host("guest", "abc");

    transport
        .send(CollabMessage::Hello {
            peer_id: guest.local_peer().id,
            name: "guest".to_string(),
        })
        .unwrap();
    host.receive(transport.try_recv().unwrap().unwrap());

    let op = host.apply_local_insert(3, "!");
    transport
        .send(CollabMessage::Edit {
            peer_id: host.local_peer().id,
            operation: op,
        })
        .unwrap();
    guest.receive(transport.try_recv().unwrap().unwrap());
    assert_eq!(guest.document().text(), "abc!");
}
