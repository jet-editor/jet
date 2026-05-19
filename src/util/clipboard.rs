use std::io::{self, Write};

/// Copy text to the system clipboard via OSC 52 (works over SSH in many terminals).
pub fn copy_osc52(text: &str) -> io::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let encoded = base64_encode(text.as_bytes());
    let mut stdout = io::stdout();
    write!(stdout, "\x1b]52;c;{encoded}\x07")?;
    stdout.flush()
}

fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut idx = 0;
    while idx < input.len() {
        let b0 = input[idx] as u32;
        let b1 = input.get(idx + 1).copied().unwrap_or(0) as u32;
        let b2 = input.get(idx + 2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        out.push(if idx + 1 < input.len() {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if idx + 2 < input.len() {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
        idx += 3;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_roundtrip_ascii() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
    }
}
