use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const CONTENT_LENGTH: &str = "content-length";

pub async fn read_message<R>(reader: &mut R) -> Result<Option<Value>>
where
    R: AsyncBufRead + Unpin,
{
    let mut content_length = None;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).await?;
        if bytes == 0 {
            return Ok(None);
        }

        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }

        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        if name.trim().eq_ignore_ascii_case(CONTENT_LENGTH) {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .with_context(|| format!("invalid LSP Content-Length header: {value}"))?,
            );
        }
    }

    let content_length = content_length.ok_or_else(|| anyhow!("missing LSP Content-Length"))?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}

pub async fn write_message<W>(writer: &mut W, value: &Value) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let body = serde_json::to_vec(value)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

pub fn request(id: i64, method: &str, params: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

pub fn notification(method: &str, params: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

pub fn response_id(value: &Value) -> Result<Option<i64>> {
    let Some(id) = value.get("id") else {
        return Ok(None);
    };
    if let Some(id) = id.as_i64() {
        Ok(Some(id))
    } else {
        bail!("unsupported non-integer LSP response id: {id}")
    }
}
