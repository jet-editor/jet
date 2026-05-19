use crate::buffer::crdt::{OperationId, TextOperation};
use anyhow::{anyhow, Result};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollabMessage {
    Hello {
        peer_id: Uuid,
        name: String,
    },
    SyncRequest {
        peer_id: Uuid,
    },
    SyncState {
        peer_id: Uuid,
        document: String,
        version: Vec<RemoteVersion>,
    },
    EncodedSync {
        peer_id: Uuid,
        bytes: Vec<u8>,
        version: Vec<RemoteVersion>,
    },
    Edit {
        peer_id: Uuid,
        operation: TextOperation,
    },
    Awareness {
        peer_id: Uuid,
        selections: Vec<(usize, usize)>,
        viewport: (usize, usize),
    },
    Chat {
        peer_id: Uuid,
        text: String,
    },
    Leave {
        peer_id: Uuid,
    },
    Ping {
        peer_id: Uuid,
        sent_at_ms: u64,
    },
    Pong {
        peer_id: Uuid,
        sent_at_ms: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteVersion {
    pub agent: String,
    pub seq: usize,
}

impl CollabMessage {
    pub fn peer_id(&self) -> Uuid {
        match self {
            Self::Hello { peer_id, .. }
            | Self::SyncRequest { peer_id }
            | Self::SyncState { peer_id, .. }
            | Self::EncodedSync { peer_id, .. }
            | Self::Edit { peer_id, .. }
            | Self::Awareness { peer_id, .. }
            | Self::Chat { peer_id, .. }
            | Self::Leave { peer_id }
            | Self::Ping { peer_id, .. }
            | Self::Pong { peer_id, .. } => *peer_id,
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Hello { peer_id, name } => serde_json::json!({
                "type": "hello",
                "peer_id": peer_id.to_string(),
                "name": name,
            }),
            Self::SyncRequest { peer_id } => serde_json::json!({
                "type": "sync_request",
                "peer_id": peer_id.to_string(),
            }),
            Self::SyncState {
                peer_id,
                document,
                version,
            } => serde_json::json!({
                "type": "sync_state",
                "peer_id": peer_id.to_string(),
                "document": document,
                "version": version_to_json(version),
            }),
            Self::EncodedSync {
                peer_id,
                bytes,
                version,
            } => serde_json::json!({
                "type": "encoded_sync",
                "peer_id": peer_id.to_string(),
                "bytes": bytes,
                "version": version_to_json(version),
            }),
            Self::Edit { peer_id, operation } => serde_json::json!({
                "type": "edit",
                "peer_id": peer_id.to_string(),
                "operation": operation_to_json(operation),
            }),
            Self::Awareness {
                peer_id,
                selections,
                viewport,
            } => serde_json::json!({
                "type": "awareness",
                "peer_id": peer_id.to_string(),
                "selections": selections,
                "viewport": [viewport.0, viewport.1],
            }),
            Self::Chat { peer_id, text } => serde_json::json!({
                "type": "chat",
                "peer_id": peer_id.to_string(),
                "text": text,
            }),
            Self::Leave { peer_id } => serde_json::json!({
                "type": "leave",
                "peer_id": peer_id.to_string(),
            }),
            Self::Ping {
                peer_id,
                sent_at_ms,
            } => serde_json::json!({
                "type": "ping",
                "peer_id": peer_id.to_string(),
                "sent_at_ms": sent_at_ms,
            }),
            Self::Pong {
                peer_id,
                sent_at_ms,
            } => serde_json::json!({
                "type": "pong",
                "peer_id": peer_id.to_string(),
                "sent_at_ms": sent_at_ms,
            }),
        }
    }

    pub fn from_json(value: &Value) -> Result<Self> {
        let kind = required_str(value, "type")?;
        let peer_id = Uuid::parse_str(required_str(value, "peer_id")?)?;
        Ok(match kind {
            "hello" => Self::Hello {
                peer_id,
                name: required_str(value, "name")?.to_string(),
            },
            "sync_request" => Self::SyncRequest { peer_id },
            "sync_state" => Self::SyncState {
                peer_id,
                document: required_str(value, "document")?.to_string(),
                version: version_from_json(value.get("version").unwrap_or(&Value::Null))?,
            },
            "encoded_sync" => Self::EncodedSync {
                peer_id,
                bytes: bytes_from_json(value.get("bytes").unwrap_or(&Value::Null))?,
                version: version_from_json(value.get("version").unwrap_or(&Value::Null))?,
            },
            "edit" => Self::Edit {
                peer_id,
                operation: operation_from_json(value.get("operation").unwrap_or(&Value::Null))?,
            },
            "awareness" => Self::Awareness {
                peer_id,
                selections: pairs_from_json(value.get("selections").unwrap_or(&Value::Null))?,
                viewport: pair_from_json(value.get("viewport").unwrap_or(&Value::Null))?,
            },
            "chat" => Self::Chat {
                peer_id,
                text: required_str(value, "text")?.to_string(),
            },
            "leave" => Self::Leave { peer_id },
            "ping" => Self::Ping {
                peer_id,
                sent_at_ms: required_u64(value, "sent_at_ms")?,
            },
            "pong" => Self::Pong {
                peer_id,
                sent_at_ms: required_u64(value, "sent_at_ms")?,
            },
            _ => return Err(anyhow!("unknown collaboration message type: {kind}")),
        })
    }
}

fn version_to_json(version: &[RemoteVersion]) -> Value {
    Value::Array(
        version
            .iter()
            .map(|entry| serde_json::json!({ "agent": entry.agent, "seq": entry.seq }))
            .collect(),
    )
}

fn version_from_json(value: &Value) -> Result<Vec<RemoteVersion>> {
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    items
        .iter()
        .map(|item| {
            Ok(RemoteVersion {
                agent: required_str(item, "agent")?.to_string(),
                seq: required_usize(item, "seq")?,
            })
        })
        .collect()
}

fn operation_to_json(operation: &TextOperation) -> Value {
    match operation {
        TextOperation::Insert { id, index, text } => serde_json::json!({
            "type": "insert",
            "id": operation_id_to_json(id),
            "index": index,
            "text": text,
        }),
        TextOperation::Delete { id, start, end } => serde_json::json!({
            "type": "delete",
            "id": operation_id_to_json(id),
            "start": start,
            "end": end,
        }),
    }
}

fn operation_from_json(value: &Value) -> Result<TextOperation> {
    let kind = required_str(value, "type")?;
    let id = operation_id_from_json(value.get("id").unwrap_or(&Value::Null))?;
    match kind {
        "insert" => Ok(TextOperation::Insert {
            id,
            index: required_usize(value, "index")?,
            text: required_str(value, "text")?.to_string(),
        }),
        "delete" => Ok(TextOperation::Delete {
            id,
            start: required_usize(value, "start")?,
            end: required_usize(value, "end")?,
        }),
        _ => Err(anyhow!("unknown text operation type: {kind}")),
    }
}

fn operation_id_to_json(id: &OperationId) -> Value {
    serde_json::json!({
        "peer": id.peer.to_string(),
        "seq": id.seq,
    })
}

fn operation_id_from_json(value: &Value) -> Result<OperationId> {
    Ok(OperationId {
        peer: Uuid::parse_str(required_str(value, "peer")?)?,
        seq: required_u64(value, "seq")?,
    })
}

fn pairs_from_json(value: &Value) -> Result<Vec<(usize, usize)>> {
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    items.iter().map(pair_from_json).collect()
}

fn pair_from_json(value: &Value) -> Result<(usize, usize)> {
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("expected pair array"))?;
    if items.len() != 2 {
        return Err(anyhow!("expected pair array with two items"));
    }
    Ok((value_to_usize(&items[0])?, value_to_usize(&items[1])?))
}

fn bytes_from_json(value: &Value) -> Result<Vec<u8>> {
    let items = value
        .as_array()
        .ok_or_else(|| anyhow!("expected byte array"))?;
    items
        .iter()
        .map(|item| {
            let byte = value_to_u64(item)?;
            u8::try_from(byte).map_err(|_| anyhow!("byte out of range: {byte}"))
        })
        .collect()
}

fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string field: {key}"))
}

fn required_usize(value: &Value, key: &str) -> Result<usize> {
    value
        .get(key)
        .map(value_to_usize)
        .transpose()?
        .ok_or_else(|| anyhow!("missing usize field: {key}"))
}

fn required_u64(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .map(value_to_u64)
        .transpose()?
        .ok_or_else(|| anyhow!("missing u64 field: {key}"))
}

fn value_to_usize(value: &Value) -> Result<usize> {
    let number = value_to_u64(value)?;
    usize::try_from(number).map_err(|_| anyhow!("number out of range: {number}"))
}

fn value_to_u64(value: &Value) -> Result<u64> {
    value
        .as_u64()
        .ok_or_else(|| anyhow!("expected unsigned integer"))
}
