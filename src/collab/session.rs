use crate::{
    buffer::crdt::{CrdtDocument, TextOperation},
    collab::{
        peer::Peer,
        presence::PeerPresence,
        protocol::{CollabMessage, RemoteVersion},
        sync::SyncState,
        ui::peer_color_index,
    },
};
use std::{
    collections::HashMap,
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

fn collab_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn collab_latency_ms(sent_at_ms: u64) -> Option<u64> {
    let now = collab_timestamp_ms();
    now.checked_sub(sent_at_ms)
}

#[derive(Debug)]
pub struct CollaborationSession {
    id: String,
    local_peer: Peer,
    peers: Vec<Peer>,
    document: CrdtDocument,
    sync: SyncState,
    chat: Vec<String>,
    latency_ms: Option<u64>,
    remote: HashMap<Uuid, PeerPresence>,
}

impl CollaborationSession {
    pub fn host(name: impl Into<String>, initial_text: &str) -> Self {
        let local_peer = Peer::local(name);
        let document = CrdtDocument::from_text(local_peer.id, initial_text);
        Self {
            id: format!("jet-{}", &local_peer.id.simple().to_string()[..6]),
            local_peer,
            peers: Vec::new(),
            document,
            sync: SyncState::default(),
            chat: Vec::new(),
            latency_ms: None,
            remote: HashMap::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn local_peer(&self) -> &Peer {
        &self.local_peer
    }

    pub fn document(&self) -> &CrdtDocument {
        &self.document
    }

    pub fn peers(&self) -> &[Peer] {
        &self.peers
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len() + 1
    }

    pub fn latency_ms(&self) -> Option<u64> {
        self.latency_ms
    }

    pub fn chat(&self) -> &[String] {
        &self.chat
    }

    pub fn remote_presence(&self) -> impl Iterator<Item = &PeerPresence> {
        self.remote.values()
    }

    pub fn set_latency_ms(&mut self, latency_ms: u64) {
        self.latency_ms = Some(latency_ms);
    }

    pub fn apply_local_insert(&mut self, index: usize, text: &str) -> TextOperation {
        let op = self.document.local_insert(index, text);
        self.sync.queue(op.clone());
        op
    }

    pub fn apply_local_delete(&mut self, start: usize, end: usize) -> Option<TextOperation> {
        let op = self.document.local_delete(start..end)?;
        self.sync.queue(op.clone());
        Some(op)
    }

    pub fn receive(&mut self, message: CollabMessage) {
        match message {
            CollabMessage::Hello { peer_id, name } => self.upsert_peer(peer_id, name),
            CollabMessage::SyncRequest { peer_id } => self.upsert_peer(peer_id, "peer".to_string()),
            CollabMessage::SyncState { document, .. } => {
                self.document = CrdtDocument::from_text(self.local_peer.id, &document);
            }
            CollabMessage::EncodedSync { bytes, .. } => {
                let _ = self.document.merge_encoded(&bytes);
            }
            CollabMessage::Edit { operation, .. } => self.document.apply(&operation),
            CollabMessage::Awareness {
                peer_id,
                selections,
                viewport,
            } => {
                let name = self
                    .peers
                    .iter()
                    .find(|peer| peer.id == peer_id)
                    .map(|peer| peer.name.clone())
                    .unwrap_or_else(|| "peer".to_string());
                self.upsert_peer(peer_id, name.clone());
                self.remote.insert(
                    peer_id,
                    PeerPresence {
                        peer_id,
                        name,
                        selections,
                        viewport,
                        color_index: peer_color_index(&peer_id),
                    },
                );
            }
            CollabMessage::Chat { peer_id, text } => {
                self.chat.push(format!("{peer_id}: {text}"));
            }
            CollabMessage::Leave { peer_id } => {
                self.peers.retain(|peer| peer.id != peer_id);
                self.remote.remove(&peer_id);
            }
            CollabMessage::Ping { .. } => {}
            CollabMessage::Pong { sent_at_ms, .. } => {
                if let Some(latency) = collab_latency_ms(sent_at_ms) {
                    self.latency_ms = Some(latency);
                }
            }
        }
    }

    pub fn ping_message(&self) -> CollabMessage {
        CollabMessage::Ping {
            peer_id: self.local_peer.id,
            sent_at_ms: collab_timestamp_ms(),
        }
    }

    pub fn pong_message(&self, sent_at_ms: u64) -> CollabMessage {
        CollabMessage::Pong {
            peer_id: self.local_peer.id,
            sent_at_ms,
        }
    }

    pub fn sync_state_message(&self) -> CollabMessage {
        CollabMessage::EncodedSync {
            peer_id: self.local_peer.id,
            bytes: self.document.encode_full(),
            version: self.remote_version(),
        }
    }

    pub fn awareness_message(
        &self,
        selections: Vec<(usize, usize)>,
        viewport: (usize, usize),
    ) -> CollabMessage {
        CollabMessage::Awareness {
            peer_id: self.local_peer.id,
            selections,
            viewport,
        }
    }

    pub fn chat_message(&self, text: String) -> CollabMessage {
        CollabMessage::Chat {
            peer_id: self.local_peer.id,
            text,
        }
    }

    pub fn drain_outgoing(&mut self) -> Vec<CollabMessage> {
        self.sync
            .drain()
            .into_iter()
            .map(|operation| CollabMessage::Edit {
                peer_id: self.local_peer.id,
                operation,
            })
            .collect()
    }

    fn upsert_peer(&mut self, peer_id: Uuid, name: String) {
        if let Some(peer) = self.peers.iter_mut().find(|peer| peer.id == peer_id) {
            peer.name = name;
        } else if peer_id != self.local_peer.id {
            self.peers.push(Peer { id: peer_id, name });
        }
    }

    fn remote_version(&self) -> Vec<RemoteVersion> {
        self.document
            .remote_version()
            .into_iter()
            .map(|(agent, seq)| RemoteVersion { agent, seq })
            .collect()
    }
}
