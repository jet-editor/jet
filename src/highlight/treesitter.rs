use crate::{
    buffer::rope::{BufferEdit, EditorBuffer},
    highlight::grammars,
    lsp::servers::language_for_path,
};
use std::{
    ops::Range,
    path::Path,
    time::{Duration, Instant},
};
use tree_sitter::{InputEdit, Parser, Point, Tree};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const VIEWPORT_OVERSCAN_LINES: usize = 32;
const PARSE_FRAME_BUDGET: Duration = Duration::from_millis(4);
const TREE_SITTER_PARSE_TIMEOUT_MICROS: u64 = 2_000;

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "comment",
    "constant",
    "constant.builtin",
    "constant.character",
    "constant.macro",
    "constructor",
    "embedded",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "keyword.control",
    "keyword.function",
    "keyword.operator",
    "keyword.storage",
    "label",
    "module",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.escape",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "type.definition",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Python,
    Go,
    Json,
    Toml,
    Markdown,
    Bash,
    PlainText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HighlightEngine {
    TreeSitter,
    Lexical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub group: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedSpans {
    pub spans: Vec<(usize, HighlightSpan)>,
    pub engine: HighlightEngine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseBudgetStatus {
    Parsed,
    Deferred,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start_byte: usize,
    pub old_end_byte: usize,
    pub new_end_byte: usize,
    pub start_position: Point,
    pub old_end_position: Point,
    pub new_end_position: Point,
}

pub struct TreeSitterHighlighter {
    language: Language,
    parser: Option<Parser>,
    tree: Option<Tree>,
    pending_tree: Option<Tree>,
    highlighter: Option<Highlighter>,
    highlight_config: Option<HighlightConfiguration>,
    last_engine: HighlightEngine,
    last_parse_deferred: bool,
}

impl TreeSitterHighlighter {
    pub fn new(language: Language) -> Self {
        Self {
            language,
            parser: None,
            tree: None,
            pending_tree: None,
            highlighter: None,
            highlight_config: None,
            last_engine: HighlightEngine::Lexical,
            last_parse_deferred: false,
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    pub fn last_engine(&self) -> HighlightEngine {
        self.last_engine
    }

    pub fn tree_sitter_available(&self) -> bool {
        tree_sitter_language(self.language).is_some()
    }

    pub fn viewport_overscan_lines() -> usize {
        VIEWPORT_OVERSCAN_LINES
    }

    pub fn detect(path: &Path, first_line: Option<&str>) -> Language {
        if let Some(modeline) = first_line.and_then(parse_modeline_language) {
            return modeline;
        }
        if let Some(shebang) = first_line.and_then(grammars::detect_by_shebang) {
            return shebang;
        }

        match language_for_path(path) {
            Some("rust") => Language::Rust,
            Some("typescript") => Language::TypeScript,
            Some("javascript") => Language::JavaScript,
            Some("python") => Language::Python,
            Some("go") => Language::Go,
            Some("json") => Language::Json,
            Some("toml") => Language::Toml,
            Some("markdown") => Language::Markdown,
            _ => grammars::detect_by_path(path),
        }
    }

    pub fn parse(&mut self, source: &str) -> bool {
        if !self.ensure_parser() {
            return false;
        }
        if let Some(parser) = &mut self.parser {
            self.tree = parser.parse(source, self.tree.as_ref());
            self.last_parse_deferred = false;
            self.tree.is_some()
        } else {
            false
        }
    }

    pub fn parse_buffer(&mut self, buffer: &EditorBuffer) -> bool {
        if !self.ensure_parser() {
            return false;
        }
        let provider = buffer.text_provider();
        if let Some(parser) = &mut self.parser {
            parser.set_timeout_micros(0);
            let mut read = |byte: usize, _position: Point| provider.chunk_at(byte);
            self.tree = parser.parse_with(&mut read, self.tree.as_ref());
            self.last_parse_deferred = false;
            self.tree.is_some()
        } else {
            false
        }
    }

    pub fn parse_buffer_with_budget(
        &mut self,
        buffer: &EditorBuffer,
        budget: Duration,
    ) -> ParseBudgetStatus {
        if !self.ensure_parser() {
            return ParseBudgetStatus::Unavailable;
        }

        let provider = buffer.text_provider();
        let old_tree = self.tree.as_ref();
        if let Some(parser) = &mut self.parser {
            parser.set_timeout_micros(budget.as_micros().max(1) as u64);
            let mut read = |byte: usize, _position: Point| provider.chunk_at(byte);
            match parser.parse_with(&mut read, old_tree) {
                Some(tree) => {
                    parser.set_timeout_micros(0);
                    self.tree = Some(tree);
                    self.pending_tree = None;
                    self.last_parse_deferred = false;
                    ParseBudgetStatus::Parsed
                }
                None => {
                    parser.set_timeout_micros(0);
                    self.last_parse_deferred = true;
                    ParseBudgetStatus::Deferred
                }
            }
        } else {
            ParseBudgetStatus::Unavailable
        }
    }

    pub fn parse_buffer_snapshot(
        language: Language,
        source: String,
        old_tree: Option<Tree>,
        timeout: Duration,
    ) -> Option<Tree> {
        let language = tree_sitter_language(language)?;
        let mut parser = Parser::new();
        parser.set_language(&language).ok()?;
        parser.set_timeout_micros(timeout.as_micros().max(1) as u64);
        parser.parse(source, old_tree.as_ref())
    }

    pub fn parse_editor_buffer_snapshot(
        language: Language,
        buffer: EditorBuffer,
        old_tree: Option<Tree>,
        timeout: Duration,
    ) -> Option<Tree> {
        let language = tree_sitter_language(language)?;
        let mut parser = Parser::new();
        parser.set_language(&language).ok()?;
        parser.set_timeout_micros(timeout.as_micros().max(1) as u64);
        let provider = buffer.text_provider();
        let mut read = |byte: usize, _position: Point| provider.chunk_at(byte);
        parser.parse_with(&mut read, old_tree.as_ref())
    }

    pub fn editable_tree(&self) -> Option<Tree> {
        self.tree.clone()
    }

    pub fn queue_parsed_tree(&mut self, tree: Tree) {
        self.pending_tree = Some(tree);
    }

    pub fn install_pending_tree_with_budget(&mut self, frame_start: Instant) -> bool {
        if frame_start.elapsed() > PARSE_FRAME_BUDGET {
            self.last_parse_deferred = self.pending_tree.is_some();
            return false;
        }
        if let Some(tree) = self.pending_tree.take() {
            self.tree = Some(tree);
            self.last_parse_deferred = false;
            true
        } else {
            false
        }
    }

    pub fn last_parse_deferred(&self) -> bool {
        self.last_parse_deferred
    }

    pub fn frame_budget() -> Duration {
        PARSE_FRAME_BUDGET
    }

    pub fn background_parse_timeout() -> Duration {
        Duration::from_micros(TREE_SITTER_PARSE_TIMEOUT_MICROS)
    }

    pub fn apply_edit(&mut self, edit: TextEdit) {
        if let Some(tree) = &mut self.tree {
            tree.edit(&InputEdit {
                start_byte: edit.start_byte,
                old_end_byte: edit.old_end_byte,
                new_end_byte: edit.new_end_byte,
                start_position: edit.start_position,
                old_end_position: edit.old_end_position,
                new_end_position: edit.new_end_position,
            });
        }
        self.pending_tree = None;
    }

    pub fn apply_buffer_edit(&mut self, buffer: &EditorBuffer, edit: &BufferEdit) {
        let start_byte = buffer.char_to_byte(edit.start_char);
        let old_end_byte = start_byte + edit.old_text.len();
        let new_end_byte = start_byte + edit.new_text.len();
        let start_position = point_for_char(buffer, edit.start_char);
        let old_end_position = point_after_text(start_position, &edit.old_text);
        let new_end_position = point_after_text(start_position, &edit.new_text);
        self.apply_edit(TextEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        });
    }

    pub fn has_tree(&self) -> bool {
        self.tree.is_some()
    }

    pub fn tree(&self) -> Option<&Tree> {
        self.tree.as_ref()
    }

    pub fn highlight_line(&self, line: &str) -> Vec<(usize, usize, &'static str)> {
        self.highlight_line_spans(line)
            .into_iter()
            .map(|span| (span.start, span.end, span.group))
            .collect()
    }

    pub fn highlight_line_spans(&self, line: &str) -> Vec<HighlightSpan> {
        lexical_highlight(self.language, line)
    }

    pub fn highlight_viewport(
        &mut self,
        lines: &[String],
        line_range: Range<usize>,
        overscan: usize,
    ) -> Vec<(usize, HighlightSpan)> {
        self.highlight_window(lines, 0, line_range, overscan).spans
    }

    pub fn highlight_visible_window(
        &mut self,
        lines: &[String],
        first_line: usize,
        visible_range: Range<usize>,
        overscan: usize,
    ) -> HighlightedSpans {
        self.highlight_window(lines, first_line, visible_range, overscan)
    }

    fn highlight_window(
        &mut self,
        lines: &[String],
        first_line: usize,
        visible_range: Range<usize>,
        overscan: usize,
    ) -> HighlightedSpans {
        if lines.is_empty() || visible_range.start >= visible_range.end {
            return HighlightedSpans {
                spans: Vec::new(),
                engine: self.last_engine,
            };
        }

        let available_range = first_line..first_line.saturating_add(lines.len());
        let start = visible_range
            .start
            .saturating_sub(overscan)
            .max(available_range.start);
        let end = visible_range
            .end
            .saturating_add(overscan)
            .min(available_range.end);

        if start >= end {
            return HighlightedSpans {
                spans: Vec::new(),
                engine: self.last_engine,
            };
        }

        let start_rel = start - first_line;
        let end_rel = end - first_line;
        let window_lines = &lines[start_rel..end_rel];

        if let Some(mut spans) = self.highlight_tree_sitter_window(window_lines) {
            for (line, _) in &mut spans {
                *line += start;
            }
            spans.retain(|(line, _)| visible_range.contains(line));
            spans.sort_by_key(|(line, span)| (*line, span.start, span.end));
            self.last_engine = HighlightEngine::TreeSitter;
            return HighlightedSpans {
                spans,
                engine: HighlightEngine::TreeSitter,
            };
        }

        let spans = (start..end)
            .flat_map(|line_idx| {
                let rel = line_idx - first_line;
                lexical_highlight(self.language, &lines[rel])
                    .into_iter()
                    .map(move |span| (line_idx, span))
            })
            .filter(|(line, _)| visible_range.contains(line))
            .collect();
        self.last_engine = HighlightEngine::Lexical;
        HighlightedSpans {
            spans,
            engine: HighlightEngine::Lexical,
        }
    }

    fn highlight_tree_sitter_window(
        &mut self,
        lines: &[String],
    ) -> Option<Vec<(usize, HighlightSpan)>> {
        self.ensure_highlight_config()?;
        let (source, line_starts) = joined_source(lines);
        if source.is_empty() {
            return Some(Vec::new());
        }

        let config = self.highlight_config.as_ref()?;
        let highlighter = self.highlighter.as_mut()?;
        let events = highlighter
            .highlight(config, source.as_bytes(), None, |_| None)
            .ok()?;
        let mut active: Vec<Option<&'static str>> = Vec::new();
        let mut spans = Vec::new();

        for event in events {
            match event.ok()? {
                HighlightEvent::HighlightStart(highlight) => {
                    active.push(highlight_group(HIGHLIGHT_NAMES.get(highlight.0).copied()));
                }
                HighlightEvent::HighlightEnd => {
                    active.pop();
                }
                HighlightEvent::Source { start, end } => {
                    if let Some(group) = active.iter().rev().find_map(|group| *group) {
                        push_range_spans(start, end, group, &line_starts, source.len(), &mut spans);
                    }
                }
            }
        }

        spans.sort_by_key(|(line, span)| (*line, span.start, span.end));
        Some(spans)
    }

    fn ensure_parser(&mut self) -> bool {
        if self.parser.is_some() {
            return true;
        }
        let Some(language) = tree_sitter_language(self.language) else {
            return false;
        };
        let mut parser = Parser::new();
        if parser.set_language(&language).is_err() {
            return false;
        }
        self.parser = Some(parser);
        true
    }

    fn ensure_highlight_config(&mut self) -> Option<()> {
        if self.highlight_config.is_some() {
            if self.highlighter.is_none() {
                self.highlighter = Some(Highlighter::new());
            }
            return Some(());
        }

        let language = tree_sitter_language(self.language)?;
        let (highlights, injections, locals) = highlight_queries(self.language)?;
        let mut config = HighlightConfiguration::new(
            language,
            language_name(self.language),
            highlights,
            injections,
            locals,
        )
        .ok()?;
        config.configure(HIGHLIGHT_NAMES);
        self.highlight_config = Some(config);
        self.highlighter = Some(Highlighter::new());
        Some(())
    }
}

fn point_for_char(buffer: &EditorBuffer, char_idx: usize) -> Point {
    let (row, col) = buffer.char_to_line_col(char_idx);
    Point { row, column: col }
}

fn point_after_text(start: Point, text: &str) -> Point {
    let mut row = start.row;
    let mut column = start.column;
    for ch in text.chars() {
        if ch == '\n' {
            row += 1;
            column = 0;
        } else {
            column += ch.len_utf8();
        }
    }
    Point { row, column }
}

pub fn available_grammar_languages() -> &'static [Language] {
    &[
        Language::Rust,
        Language::Python,
        Language::Json,
        Language::TypeScript,
        Language::Go,
        Language::Toml,
    ]
}

fn highlight_queries(language: Language) -> Option<(&'static str, &'static str, &'static str)> {
    match language {
        Language::Rust => Some((
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        )),
        Language::Python => Some((tree_sitter_python::HIGHLIGHTS_QUERY, "", "")),
        Language::Json => Some((tree_sitter_json::HIGHLIGHTS_QUERY, "", "")),
        Language::Bash => Some((tree_sitter_bash::HIGHLIGHT_QUERY, "", "")),
        Language::JavaScript => Some((tree_sitter_javascript::HIGHLIGHT_QUERY, "", "")),
        Language::TypeScript => Some((tree_sitter_typescript::HIGHLIGHTS_QUERY, "", "")),
        Language::Go => Some((tree_sitter_go::HIGHLIGHTS_QUERY, "", "")),
        _ => None,
    }
}

pub fn tree_sitter_language(language: Language) -> Option<tree_sitter::Language> {
    match language {
        Language::Rust => Some(tree_sitter_rust::language()),
        Language::Python => Some(tree_sitter_python::language()),
        Language::Json => Some(tree_sitter_json::language()),
        Language::Bash => Some(tree_sitter_bash::language()),
        Language::JavaScript => {
            let lang_fn = tree_sitter_javascript::LANGUAGE.into_raw();
            let ptr = unsafe { lang_fn() as *const tree_sitter::ffi::TSLanguage };
            Some(unsafe { tree_sitter::Language::from_raw(ptr) })
        }
        Language::TypeScript => {
            let lang_fn = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into_raw();
            let ptr = unsafe { lang_fn() as *const tree_sitter::ffi::TSLanguage };
            Some(unsafe { tree_sitter::Language::from_raw(ptr) })
        }
        Language::Go => Some(tree_sitter_go::language()),
        _ => None,
    }
}

fn language_name(language: Language) -> &'static str {
    grammars::language_spec(language)
        .map(|spec| spec.name)
        .unwrap_or("text")
}

fn joined_source(lines: &[String]) -> (String, Vec<usize>) {
    let source_len = lines.iter().map(String::len).sum::<usize>() + lines.len().saturating_sub(1);
    let mut source = String::with_capacity(source_len);
    let mut line_starts = Vec::with_capacity(lines.len());
    for (idx, line) in lines.iter().enumerate() {
        line_starts.push(source.len());
        source.push_str(line);
        if idx + 1 < lines.len() {
            source.push('\n');
        }
    }
    (source, line_starts)
}

fn highlight_group(name: Option<&'static str>) -> Option<&'static str> {
    let name = name?;
    if name == "comment" {
        Some("comment")
    } else if name == "string" || name.starts_with("string.") {
        Some("string")
    } else if name == "keyword" || name.starts_with("keyword.") {
        Some("keyword")
    } else if name == "function" || name.starts_with("function.") {
        Some("function")
    } else if name == "type" || name.starts_with("type.") {
        Some("type")
    } else if name == "number" {
        Some("number")
    } else if name == "boolean" {
        Some("boolean")
    } else if name == "constant" || name.starts_with("constant.") {
        Some("constant")
    } else {
        None
    }
}

fn push_range_spans(
    start: usize,
    end: usize,
    group: &'static str,
    line_starts: &[usize],
    source_len: usize,
    spans: &mut Vec<(usize, HighlightSpan)>,
) {
    if start >= end || line_starts.is_empty() || end > source_len {
        return;
    }

    let first_line = line_for_byte(start, line_starts);
    let last_line = line_for_byte(end.saturating_sub(1).max(start), line_starts);
    for line in first_line..=last_line {
        let line_start = line_starts[line];
        let next_line_start = line_starts.get(line + 1).copied().unwrap_or(source_len);

        // The line content ends before the newline character
        let line_content_end = if line + 1 < line_starts.len() {
            next_line_start.saturating_sub(1)
        } else {
            source_len
        };

        let span_start = if line == first_line {
            start.saturating_sub(line_start)
        } else {
            0
        };

        let span_end = if line == last_line {
            end.saturating_sub(line_start)
        } else {
            line_content_end.saturating_sub(line_start)
        };

        // Clamp span_end to line_content_end
        let span_end = span_end.min(line_content_end.saturating_sub(line_start));

        if span_start < span_end {
            spans.push((
                line,
                HighlightSpan {
                    start: span_start,
                    end: span_end,
                    group,
                },
            ));
        }
    }
}

fn line_for_byte(byte: usize, line_starts: &[usize]) -> usize {
    line_starts
        .partition_point(|line_start| *line_start <= byte)
        .saturating_sub(1)
}

fn lexical_highlight(language: Language, line: &str) -> Vec<HighlightSpan> {
    let mut spans = Vec::new();
    highlight_comments(language, line, &mut spans);
    highlight_strings(line, &mut spans);
    highlight_numbers(line, &mut spans);
    highlight_keywords(language, line, &mut spans);
    spans.sort_by_key(|span| (span.start, span.end));
    spans
}

fn highlight_comments(language: Language, line: &str, spans: &mut Vec<HighlightSpan>) {
    let marker = match language {
        Language::Python => "#",
        Language::Toml => "#",
        Language::Markdown => "<!--",
        _ => "//",
    };
    if let Some(start) = line.find(marker) {
        spans.push(HighlightSpan {
            start,
            end: line.len(),
            group: "comment",
        });
    }
}

fn highlight_strings(line: &str, spans: &mut Vec<HighlightSpan>) {
    let mut quote = None;
    let mut start = 0usize;
    let mut escape = false;
    for (idx, ch) in line.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' {
            escape = true;
            continue;
        }
        match quote {
            Some(current) if current == ch => {
                spans.push(HighlightSpan {
                    start,
                    end: idx + ch.len_utf8(),
                    group: "string",
                });
                quote = None;
            }
            None if matches!(ch, '"' | '\'' | '`') => {
                quote = Some(ch);
                start = idx;
            }
            _ => {}
        }
    }
}

fn highlight_numbers(line: &str, spans: &mut Vec<HighlightSpan>) {
    let mut start = None;
    for (idx, ch) in line.char_indices() {
        if ch.is_ascii_digit() {
            start.get_or_insert(idx);
        } else if let Some(s) = start.take() {
            spans.push(HighlightSpan {
                start: s,
                end: idx,
                group: "number",
            });
        }
    }
    if let Some(s) = start {
        spans.push(HighlightSpan {
            start: s,
            end: line.len(),
            group: "number",
        });
    }
}

fn highlight_keywords(language: Language, line: &str, spans: &mut Vec<HighlightSpan>) {
    for (start, word) in words(line) {
        if let Some(group) = keyword_group(language, word) {
            spans.push(HighlightSpan {
                start,
                end: start + word.len(),
                group,
            });
        }
    }
}

fn words(line: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (idx, ch) in line.char_indices() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            start.get_or_insert(idx);
        } else if let Some(s) = start.take() {
            out.push((s, &line[s..idx]));
        }
    }
    if let Some(s) = start {
        out.push((s, &line[s..]));
    }
    out.into_iter()
}

fn keyword_group(language: Language, word: &str) -> Option<&'static str> {
    match language {
        Language::Rust => match word {
            "fn" | "let" | "mut" | "pub" | "impl" | "trait" | "struct" | "enum" | "use" | "mod"
            | "match" | "if" | "else" | "while" | "for" | "loop" | "return" | "async" | "await"
            | "move" | "const" | "static" | "where" => Some("keyword"),
            "true" | "false" => Some("boolean"),
            "Self" | "usize" | "String" | "Result" | "Option" => Some("type"),
            _ => None,
        },
        Language::Python => match word {
            "def" | "class" | "import" | "from" | "if" | "elif" | "else" | "while" | "for"
            | "return" | "async" | "await" | "try" | "except" | "finally" | "with" | "as"
            | "lambda" => Some("keyword"),
            "True" | "False" => Some("boolean"),
            "None" => Some("constant"),
            _ => None,
        },
        Language::Go => match word {
            "func" | "package" | "import" | "var" | "const" | "type" | "struct" | "interface"
            | "if" | "else" | "for" | "range" | "return" | "go" | "defer" | "select" | "switch"
            | "case" => Some("keyword"),
            "true" | "false" => Some("boolean"),
            _ => None,
        },
        Language::TypeScript | Language::JavaScript => match word {
            "function" | "const" | "let" | "var" | "class" | "interface" | "type" | "import"
            | "export" | "from" | "if" | "else" | "for" | "while" | "return" | "async"
            | "await" | "new" | "extends" | "implements" => Some("keyword"),
            "true" | "false" => Some("boolean"),
            "null" | "undefined" => Some("constant"),
            _ => None,
        },
        Language::Json => match word {
            "true" | "false" => Some("boolean"),
            "null" => Some("constant"),
            _ => None,
        },
        _ => None,
    }
}

fn parse_modeline_language(line: &str) -> Option<Language> {
    let language = line.split_whitespace().find_map(|part| {
        part.strip_prefix("ft=")
            .or_else(|| part.strip_prefix("lang="))
    })?;
    match language {
        "rust" | "rs" => Some(Language::Rust),
        "typescript" | "ts" => Some(Language::TypeScript),
        "javascript" | "js" => Some(Language::JavaScript),
        "python" | "py" => Some(Language::Python),
        "go" => Some(Language::Go),
        "json" => Some(Language::Json),
        "toml" => Some(Language::Toml),
        "markdown" | "md" => Some(Language::Markdown),
        _ => None,
    }
}
