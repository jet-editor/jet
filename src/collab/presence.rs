use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerPresence {
    pub peer_id: Uuid,
    pub name: String,
    pub selections: Vec<(usize, usize)>,
    pub viewport: (usize, usize),
    pub color_index: usize,
}

impl PeerPresence {
    pub fn primary_head(&self) -> Option<usize> {
        self.selections.first().map(|(_, head)| *head)
    }
}
