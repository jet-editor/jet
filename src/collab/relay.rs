#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayConfig {
    pub bind: String,
    pub max_sessions: usize,
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:3478".to_string(),
            max_sessions: 128,
        }
    }
}

#[derive(Debug, Default)]
pub struct RelayState {
    sessions: std::collections::HashMap<String, usize>,
}

impl RelayState {
    pub fn register_session(&mut self, id: impl Into<String>) {
        self.sessions.entry(id.into()).or_insert(0);
    }

    pub fn join(&mut self, id: &str) -> bool {
        if let Some(count) = self.sessions.get_mut(id) {
            *count += 1;
            true
        } else {
            false
        }
    }

    pub fn peer_count(&self, id: &str) -> usize {
        self.sessions.get(id).copied().unwrap_or(0)
    }
}
