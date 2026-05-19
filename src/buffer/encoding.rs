use anyhow::{anyhow, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf8Bom,
}

pub fn detect(bytes: &[u8]) -> Result<Encoding> {
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        let rest = &bytes[3..];
        std::str::from_utf8(rest)
            .map(|_| Encoding::Utf8Bom)
            .map_err(|err| anyhow!("UTF-8 BOM file with invalid content: {}", err))
    } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
        Err(anyhow!("UTF-16 LE encoding is not supported yet"))
    } else if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        Err(anyhow!("UTF-16 BE encoding is not supported yet"))
    } else {
        std::str::from_utf8(bytes)
            .map(|_| Encoding::Utf8)
            .map_err(|err| anyhow!("unsupported non-UTF-8 file: {}", err))
    }
}

pub fn decode_utf8(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|err| anyhow!("UTF-8 decode error: {}", err))
}

pub fn strip_bom(bytes: &[u8], encoding: Encoding) -> &[u8] {
    match encoding {
        Encoding::Utf8Bom if bytes.len() >= 3 => &bytes[3..],
        _ => bytes,
    }
}
