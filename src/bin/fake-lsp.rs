use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();
    loop {
        let Some(message) = read_message(&mut reader)? else {
            break;
        };
        if let Some(response) = respond(&message) {
            write_message(&mut stdout, &response)?;
        }
    }
    Ok(())
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().ok();
            }
        }
    }
    let length = content_length.unwrap_or(0);
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body)?;
    Ok(Some(serde_json::from_slice(&body)?))
}

fn respond(message: &Value) -> Option<Value> {
    if message.get("method").is_some() {
        let method = message.get("method")?.as_str()?;
        return match method {
            "initialize" => Some(json!({
                "jsonrpc": "2.0",
                "id": message.get("id").cloned().unwrap_or(Value::Null),
                "result": {
                    "capabilities": { "textDocumentSync": 2 },
                    "serverInfo": { "name": "fake-lsp", "version": "0.1.0" }
                }
            })),
            "shutdown" => Some(json!({
                "jsonrpc": "2.0",
                "id": message.get("id").cloned().unwrap_or(Value::Null),
                "result": null
            })),
            _ => None,
        };
    }
    None
}

fn write_message(stdout: &mut impl Write, value: &Value) -> io::Result<()> {
    let body = value.to_string();
    write!(stdout, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    stdout.flush()
}
