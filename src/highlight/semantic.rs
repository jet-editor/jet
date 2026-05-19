#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticToken {
    pub line: usize,
    pub start: usize,
    pub len: usize,
    pub group: &'static str,
}

#[derive(Debug, Default, Clone)]
pub struct SemanticTokens {
    tokens: Vec<SemanticToken>,
}

impl SemanticTokens {
    pub fn push(&mut self, token: SemanticToken) {
        self.tokens.push(token);
    }

    pub fn tokens(&self) -> &[SemanticToken] {
        &self.tokens
    }

    pub fn clear(&mut self) {
        self.tokens.clear();
    }
}

/// Decode LSP `semanticTokens/full` `data` array (delta-encoded, 5 ints per token).
pub fn decode_semantic_tokens(data: &[u32]) -> SemanticTokens {
    let mut tokens = SemanticTokens::default();
    let mut line = 0usize;
    let mut col = 0usize;
    let mut idx = 0usize;
    while idx + 4 < data.len() {
        let delta_line = data[idx] as usize;
        let delta_start = data[idx + 1] as usize;
        let length = data[idx + 2] as usize;
        let token_type = data[idx + 3] as usize;
        let _modifiers = data[idx + 4];
        idx += 5;
        if delta_line > 0 {
            line += delta_line;
            col = delta_start;
        } else {
            col += delta_start;
        }
        if length == 0 {
            continue;
        }
        tokens.push(SemanticToken {
            line,
            start: col,
            len: length,
            group: semantic_group(token_type),
        });
    }
    tokens
}

fn semantic_group(token_type: usize) -> &'static str {
    match token_type {
        0 => "namespace",
        1 => "type",
        2 => "type",
        3 => "type",
        4 => "type",
        5 => "parameter",
        6 => "variable",
        7 => "function",
        8 => "function",
        9 => "property",
        10 => "function",
        11 => "keyword",
        12 => "type",
        13 => "keyword",
        14 => "keyword",
        15 => "keyword",
        16 => "keyword",
        17 => "keyword",
        18 => "string",
        19 => "number",
        20 => "keyword",
        21 => "keyword",
        _ => "variable",
    }
}
