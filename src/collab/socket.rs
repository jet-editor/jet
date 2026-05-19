use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketAddress(pub String);

pub trait CollaborationSocket {
    fn send(&mut self, message: &[u8]) -> Result<()>;
    fn recv(&mut self) -> Result<Vec<u8>>;
}
