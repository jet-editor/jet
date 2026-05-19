use serde_json::Value;
use std::collections::HashMap;
use tokio::sync::oneshot;

#[derive(Default)]
pub struct PendingRequests {
    next_id: i64,
    waiters: HashMap<i64, oneshot::Sender<Value>>,
}

impl PendingRequests {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            waiters: HashMap::new(),
        }
    }

    pub fn allocate(&mut self) -> (i64, oneshot::Receiver<Value>) {
        let id = self.next_id;
        self.next_id += 1;
        let (tx, rx) = oneshot::channel();
        self.waiters.insert(id, tx);
        (id, rx)
    }

    pub fn complete(&mut self, id: i64, value: Value) -> bool {
        self.waiters
            .remove(&id)
            .map(|waiter| waiter.send(value).is_ok())
            .unwrap_or(false)
    }

    pub fn len(&self) -> usize {
        self.waiters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiters.is_empty()
    }
}
