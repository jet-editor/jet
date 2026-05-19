use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub id: Uuid,
    pub name: String,
}

impl Peer {
    pub fn local(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
        }
    }
}
