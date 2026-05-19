use crate::buffer::crdt::TextOperation;

#[derive(Debug, Default, Clone)]
pub struct SyncState {
    pending: Vec<TextOperation>,
}

impl SyncState {
    pub fn queue(&mut self, op: TextOperation) {
        self.pending.push(op);
    }

    pub fn drain(&mut self) -> Vec<TextOperation> {
        std::mem::take(&mut self.pending)
    }
}
