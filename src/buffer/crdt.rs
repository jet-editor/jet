use diamond_types::list::ListCRDT;
use std::ops::Range;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OperationId {
    pub peer: Uuid,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextOperation {
    Insert {
        id: OperationId,
        index: usize,
        text: String,
    },
    Delete {
        id: OperationId,
        start: usize,
        end: usize,
    },
}

pub struct CrdtDocument {
    peer: Uuid,
    seq: u64,
    doc: ListCRDT,
    agent: diamond_types::AgentId,
    cached_text: String,
}

impl std::fmt::Debug for CrdtDocument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrdtDocument")
            .field("peer", &self.peer)
            .field("seq", &self.seq)
            .field("cached_text", &self.cached_text)
            .finish_non_exhaustive()
    }
}

impl CrdtDocument {
    pub fn new(peer: Uuid) -> Self {
        let mut doc = ListCRDT::new();
        let agent = doc.get_or_create_agent_id(&peer.to_string());
        Self {
            peer,
            seq: 0,
            doc,
            agent,
            cached_text: String::new(),
        }
    }

    pub fn from_text(peer: Uuid, text: &str) -> Self {
        let mut this = Self::new(peer);
        if !text.is_empty() {
            this.doc.insert(this.agent, 0, text);
            this.refresh_text();
        }
        this
    }

    pub fn text(&self) -> &str {
        &self.cached_text
    }

    pub fn local_insert(&mut self, index: usize, text: &str) -> TextOperation {
        self.seq += 1;
        let index = index.min(self.doc.len());
        self.doc.insert(self.agent, index, text);
        self.refresh_text();
        TextOperation::Insert {
            id: OperationId {
                peer: self.peer,
                seq: self.seq,
            },
            index,
            text: text.to_string(),
        }
    }

    pub fn local_delete(&mut self, range: Range<usize>) -> Option<TextOperation> {
        let start = range.start.min(self.doc.len());
        let end = range.end.min(self.doc.len());
        if start >= end {
            return None;
        }
        self.seq += 1;
        self.doc.delete(self.agent, start..end);
        self.refresh_text();
        Some(TextOperation::Delete {
            id: OperationId {
                peer: self.peer,
                seq: self.seq,
            },
            start,
            end,
        })
    }

    pub fn apply(&mut self, op: &TextOperation) {
        match op {
            TextOperation::Insert { id, index, text } => {
                let agent = self.doc.get_or_create_agent_id(&id.peer.to_string());
                self.doc.insert(agent, (*index).min(self.doc.len()), text);
            }
            TextOperation::Delete { id, start, end } => {
                let agent = self.doc.get_or_create_agent_id(&id.peer.to_string());
                let start = (*start).min(self.doc.len());
                let end = (*end).min(self.doc.len());
                if start < end {
                    self.doc.delete(agent, start..end);
                }
            }
        }
        self.refresh_text();
    }

    pub fn merge_encoded(&mut self, bytes: &[u8]) -> anyhow::Result<()> {
        self.doc.merge_data_and_ff(bytes)?;
        self.refresh_text();
        Ok(())
    }

    pub fn encode_full(&self) -> Vec<u8> {
        self.doc
            .oplog
            .encode(diamond_types::list::encoding::EncodeOptions::default())
    }

    pub fn remote_version(&self) -> Vec<(String, usize)> {
        self.doc
            .branch
            .remote_version(&self.doc.oplog)
            .into_iter()
            .map(|id| (id.agent.to_string(), id.seq))
            .collect()
    }

    fn refresh_text(&mut self) {
        self.cached_text = self.doc.branch.content().to_string();
    }
}
