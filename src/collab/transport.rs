use crate::collab::protocol::CollabMessage;
use anyhow::{anyhow, Context, Result};
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    time::Duration,
};
use tungstenite::{connect, Message};

pub trait CollaborationTransport {
    fn send(&mut self, message: CollabMessage) -> Result<()>;
    fn try_recv(&mut self) -> Result<Option<CollabMessage>>;
}

#[derive(Debug, Default)]
pub struct MemoryTransport {
    inbox: std::collections::VecDeque<CollabMessage>,
}

impl MemoryTransport {
    pub fn push_incoming(&mut self, message: CollabMessage) {
        self.inbox.push_back(message);
    }
}

impl CollaborationTransport for MemoryTransport {
    fn send(&mut self, message: CollabMessage) -> Result<()> {
        self.inbox.push_back(message);
        Ok(())
    }

    fn try_recv(&mut self) -> Result<Option<CollabMessage>> {
        Ok(self.inbox.pop_front())
    }
}

pub struct TcpTransport {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl TcpTransport {
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self> {
        let stream = TcpStream::connect(addr)?;
        Self::from_stream(stream)
    }

    pub fn accept(listener: &TcpListener) -> Result<Self> {
        let (stream, _) = listener.accept()?;
        Self::from_stream(stream)
    }

    pub fn bind(addr: impl ToSocketAddrs) -> Result<TcpListener> {
        Ok(TcpListener::bind(addr)?)
    }

    pub fn from_stream(stream: TcpStream) -> Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_read_timeout(Some(Duration::from_millis(5)))?;
        let writer = stream.try_clone()?;
        Ok(Self {
            reader: BufReader::new(stream),
            writer,
        })
    }
}

#[allow(clippy::large_enum_variant)]
pub enum CollabLink {
    Tcp(TcpTransport),
    Ws(WebSocketTransport),
}

impl CollaborationTransport for CollabLink {
    fn send(&mut self, message: CollabMessage) -> Result<()> {
        match self {
            Self::Tcp(transport) => transport.send(message),
            Self::Ws(transport) => transport.send(message),
        }
    }

    fn try_recv(&mut self) -> Result<Option<CollabMessage>> {
        match self {
            Self::Tcp(transport) => transport.try_recv(),
            Self::Ws(transport) => transport.try_recv(),
        }
    }
}

pub struct WebSocketTransport {
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>,
}

impl WebSocketTransport {
    pub fn connect(url: &str) -> Result<Self> {
        let (mut socket, _) = connect(url).context("websocket connect")?;
        if let tungstenite::stream::MaybeTlsStream::Plain(stream) = socket.get_mut() {
            stream.set_nonblocking(true)?;
            stream.set_read_timeout(Some(Duration::from_millis(5)))?;
        } else if let tungstenite::stream::MaybeTlsStream::Rustls(stream) = socket.get_mut() {
            stream.get_mut().set_nonblocking(true)?;
            stream
                .get_mut()
                .set_read_timeout(Some(Duration::from_millis(5)))?;
        }
        Ok(Self { socket })
    }
}

impl CollaborationTransport for WebSocketTransport {
    fn send(&mut self, message: CollabMessage) -> Result<()> {
        let mut json = message.to_json().to_string();
        json.push('\n');
        self.socket
            .send(Message::Text(json))
            .map_err(|err| anyhow!(err))
    }

    fn try_recv(&mut self) -> Result<Option<CollabMessage>> {
        match self.socket.read() {
            Ok(Message::Text(text)) => {
                let value = serde_json::from_str(text.trim())?;
                Ok(Some(CollabMessage::from_json(&value)?))
            }
            Ok(Message::Binary(bytes)) => {
                let value = serde_json::from_slice(&bytes)?;
                Ok(Some(CollabMessage::from_json(&value)?))
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => Ok(None),
            Ok(Message::Close(_)) => Err(anyhow!("disconnected")),
            Ok(Message::Frame(_)) => Ok(None),
            Err(tungstenite::Error::Io(err)) if err.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(None)
            }
            Err(err) => Err(anyhow!(err)),
        }
    }
}

impl CollaborationTransport for TcpTransport {
    fn send(&mut self, message: CollabMessage) -> Result<()> {
        let mut json = message.to_json().to_string();
        json.push('\n');
        self.writer.write_all(json.as_bytes())?;
        self.writer.flush()?;
        Ok(())
    }

    fn try_recv(&mut self) -> Result<Option<CollabMessage>> {
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => Err(anyhow!("disconnected")),
            Ok(_) => {
                let value = serde_json::from_str(line.trim_end())?;
                Ok(Some(CollabMessage::from_json(&value)?))
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(err) => Err(anyhow!(err)),
        }
    }
}
