use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub foreground: String,
    pub background: String,
    pub accent: String,
    pub groups: HashMap<String, String>,
}

impl Default for Theme {
    fn default() -> Self {
        jet_dark()
    }
}

impl Theme {
    pub fn ansi_for_group(&self, group: &str) -> String {
        let hex = map_highlight_group(group)
            .and_then(|key| self.groups.get(key).map(String::as_str))
            .unwrap_or(self.foreground.as_str());
        let bold = group.starts_with("keyword") || group.contains("function");
        ansi_foreground(hex, bold)
    }

    pub fn ansi_status_mode(&self) -> String {
        self.ansi_for_theme_key("statusline.normal", true)
    }

    pub fn color_hex<'a>(&'a self, key: &str) -> &'a str {
        self.groups
            .get(key)
            .map(String::as_str)
            .unwrap_or(self.foreground.as_str())
    }

    pub fn ansi_for_theme_key(&self, key: &str, bold: bool) -> String {
        ansi_foreground(self.color_hex(key), bold)
    }

    pub fn ansi_background_for_theme_key(&self, key: &str) -> String {
        ansi_background(self.color_hex(key))
    }

    pub fn highlighted_line_with_theme(
        line: &str,
        spans: &[crate::highlight::treesitter::HighlightSpan],
        theme: &Theme,
    ) -> String {
        if spans.is_empty() {
            return line.to_string();
        }

        let mut ordered = spans.to_vec();
        ordered.sort_by_key(|span| (span.start, span.end));

        let mut out = String::with_capacity(line.len() + ordered.len() * 10);
        let mut cursor = 0usize;
        for span in ordered {
            if span.start < cursor
                || span.end > line.len()
                || span.start >= span.end
                || !line.is_char_boundary(span.start)
                || !line.is_char_boundary(span.end)
            {
                continue;
            }
            out.push_str(&line[cursor..span.start]);
            out.push_str(&theme.ansi_for_group(span.group));
            out.push_str(&line[span.start..span.end]);
            out.push_str("\x1b[0m");
            cursor = span.end;
        }
        out.push_str(&line[cursor..]);
        out
    }
}

pub fn map_highlight_group(group: &str) -> Option<&'static str> {
    match group {
        "keyword" | "keyword.control" | "keyword.function" | "keyword.operator"
        | "keyword.storage" => Some("keyword"),
        "function" | "function.builtin" | "function.macro" | "function.method" => Some("function"),
        "type" | "type.builtin" | "type.definition" | "constructor" | "module" => Some("type"),
        "string" | "string.escape" | "string.special" => Some("string"),
        "number" | "boolean" | "constant" | "constant.builtin" | "constant.character"
        | "constant.macro" => Some("number"),
        "comment" => Some("comment"),
        _ => None,
    }
}

pub fn ansi_background(hex: &str) -> String {
    let Some((r, g, b)) = parse_hex_color(hex) else {
        return String::new();
    };
    format!("\x1b[48;2;{r};{g};{b}m")
}

pub fn ansi_foreground(hex: &str, bold: bool) -> String {
    let Some((r, g, b)) = parse_hex_color(hex) else {
        return if bold {
            "\x1b[1m".to_string()
        } else {
            String::new()
        };
    };
    if bold {
        format!("\x1b[1;38;2;{r};{g};{b}m")
    } else {
        format!("\x1b[38;2;{r};{g};{b}m")
    }
}

fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some((r, g, b))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ThemeFile {
    name: String,
    foreground: String,
    background: String,
    accent: String,
    #[serde(default)]
    groups: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ThemeRegistry {
    themes: HashMap<String, Theme>,
    active: String,
}

impl ThemeRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            themes: HashMap::new(),
            active: "jet-dark".to_string(),
        };
        registry.register(jet_dark());
        registry.register(jet_light());
        registry.register(catppuccin_mocha());
        registry.register(gruvbox_dark());
        registry.register(tokyonight());
        registry.register(onedark());
        registry
    }

    pub fn register(&mut self, theme: Theme) {
        self.themes.insert(theme.name.clone(), theme);
    }

    pub fn load_dir(&mut self, dir: &Path) -> Result<usize> {
        if !dir.exists() {
            return Ok(0);
        }
        let mut count = 0usize;
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
                continue;
            }
            self.register(load_theme_file(&path)?);
            count += 1;
        }
        Ok(count)
    }

    pub fn set_active(&mut self, name: &str) -> Result<()> {
        if !self.themes.contains_key(name) {
            bail!("unknown theme: {name}");
        }
        self.active = name.to_string();
        Ok(())
    }

    pub fn active(&self) -> &Theme {
        self.themes
            .get(&self.active)
            .expect("active theme is always registered")
    }

    pub fn names(&self) -> Vec<String> {
        let mut names: Vec<_> = self.themes.keys().cloned().collect();
        names.sort();
        names
    }
}

impl Default for ThemeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn load_theme_file(path: &Path) -> Result<Theme> {
    let source = fs::read_to_string(path)?;
    let file: ThemeFile = toml::from_str(&source)?;
    if file.name.trim().is_empty() {
        bail!("theme name must not be empty");
    }
    Ok(Theme {
        name: file.name,
        foreground: file.foreground,
        background: file.background,
        accent: file.accent,
        groups: file.groups,
    })
}

pub fn jet_dark() -> Theme {
    Theme {
        name: "jet-dark".to_string(),
        foreground: "#d8dee9".to_string(),
        background: "#1f2329".to_string(),
        accent: "#7aa2f7".to_string(),
        groups: default_groups(),
    }
}

pub fn jet_light() -> Theme {
    let mut groups = default_groups();
    groups.insert("statusline.normal".to_string(), "#005f87".to_string());
    Theme {
        name: "jet-light".to_string(),
        foreground: "#20242a".to_string(),
        background: "#f7f8fa".to_string(),
        accent: "#006d9c".to_string(),
        groups,
    }
}

pub fn catppuccin_mocha() -> Theme {
    Theme {
        name: "catppuccin-mocha".to_string(),
        foreground: "#cdd6f4".to_string(),
        background: "#1e1e2e".to_string(),
        accent: "#89b4fa".to_string(),
        groups: HashMap::from([
            ("keyword".to_string(), "#cba6f7".to_string()),
            ("function".to_string(), "#89b4fa".to_string()),
            ("type".to_string(), "#f9e2af".to_string()),
            ("string".to_string(), "#a6e3a1".to_string()),
            ("number".to_string(), "#fab387".to_string()),
            ("comment".to_string(), "#6c7086".to_string()),
            ("statusline.normal".to_string(), "#89b4fa".to_string()),
        ]),
    }
}

pub fn gruvbox_dark() -> Theme {
    Theme {
        name: "gruvbox-dark".to_string(),
        foreground: "#ebdbb2".to_string(),
        background: "#282828".to_string(),
        accent: "#83a598".to_string(),
        groups: HashMap::from([
            ("keyword".to_string(), "#fb4934".to_string()),
            ("function".to_string(), "#83a598".to_string()),
            ("type".to_string(), "#fabd2f".to_string()),
            ("string".to_string(), "#b8bb26".to_string()),
            ("number".to_string(), "#d3869b".to_string()),
            ("comment".to_string(), "#928374".to_string()),
            ("statusline.normal".to_string(), "#b8bb26".to_string()),
        ]),
    }
}

pub fn tokyonight() -> Theme {
    Theme {
        name: "tokyonight".to_string(),
        foreground: "#c0caf5".to_string(),
        background: "#1a1b26".to_string(),
        accent: "#7aa2f7".to_string(),
        groups: default_groups(),
    }
}

pub fn onedark() -> Theme {
    Theme {
        name: "onedark".to_string(),
        foreground: "#abb2bf".to_string(),
        background: "#282c34".to_string(),
        accent: "#61afef".to_string(),
        groups: HashMap::from([
            ("keyword".to_string(), "#c678dd".to_string()),
            ("function".to_string(), "#61afef".to_string()),
            ("type".to_string(), "#e5c07b".to_string()),
            ("string".to_string(), "#98c379".to_string()),
            ("number".to_string(), "#d19a66".to_string()),
            ("comment".to_string(), "#5c6370".to_string()),
            ("statusline.normal".to_string(), "#61afef".to_string()),
        ]),
    }
}

fn default_groups() -> HashMap<String, String> {
    HashMap::from([
        ("keyword".to_string(), "#7aa2f7".to_string()),
        ("function".to_string(), "#e0af68".to_string()),
        ("type".to_string(), "#2ac3de".to_string()),
        ("string".to_string(), "#9ece6a".to_string()),
        ("number".to_string(), "#bb9af7".to_string()),
        ("comment".to_string(), "#737aa2".to_string()),
        ("diagnostic.error".to_string(), "#f7768e".to_string()),
        ("diagnostic.warning".to_string(), "#e0af68".to_string()),
        ("diagnostic.info".to_string(), "#7dcfff".to_string()),
        ("diagnostic.hint".to_string(), "#73daca".to_string()),
        ("git.added".to_string(), "#9ece6a".to_string()),
        ("git.modified".to_string(), "#e0af68".to_string()),
        ("fold.closed".to_string(), "#7aa2f7".to_string()),
        ("selection".to_string(), "#394b70".to_string()),
        ("cursorline".to_string(), "#2a2f3a".to_string()),
        ("search.highlight".to_string(), "#3a4a6a".to_string()),
        ("statusline.normal".to_string(), "#7aa2f7".to_string()),
        ("popup".to_string(), "#c0caf5".to_string()),
    ])
}
