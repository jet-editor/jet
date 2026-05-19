pub const COMMAND_HELP: &[(&str, &str)] = &[
    ("buffers", "list open buffers"),
    ("diagnostics", "open diagnostics picker"),
    ("files", "fuzzy-find project files"),
    ("grep", "search project for text"),
    ("git-diff", "show git diff for current file"),
    ("git-stage", "stage current file"),
    ("git-unstage", "unstage current file"),
    ("help", "show command reference"),
    ("lang", "show language settings"),
    ("quit", "exit jet"),
    ("rename", "LSP rename symbol at cursor"),
    ("surround", "wrap selection with a delimiter"),
    ("delsurround", "remove surrounding delimiters"),
    ("theme", "pick a color theme"),
    ("tutor", "show the interactive tutorial"),
    ("terminal", "toggle integrated terminal panel"),
    ("file-tree", "toggle project file tree sidebar"),
    ("write", "save current buffer"),
];

pub const COMMAND_NAMES: &[&str] = &[
    "buffers",
    "bdelete",
    "bnext",
    "bprev",
    "bprevious",
    "code-actions",
    "collab-chat",
    "collab-host",
    "collab-join",
    "collab-leave",
    "complete",
    "completion",
    "config",
    "config-reload",
    "definition",
    "diagnostic-list",
    "diagnostics",
    "delsurround",
    "diff",
    "ds",
    "edit",
    "files",
    "find",
    "format",
    "git-blame",
    "git-diff",
    "git-hunk-next",
    "git-hunk-prev",
    "git-next",
    "git-prev",
    "git-stage",
    "goto-definition",
    "goto-implementation",
    "goto-type",
    "grammar",
    "grammar-info",
    "grep",
    "help",
    "hover",
    "implementation",
    "lang",
    "language",
    "lsp",
    "lsp-info",
    "lsp-start",
    "ls",
    "noh",
    "nohlsearch",
    "only",
    "plugin-install",
    "plugin-list",
    "plugins",
    "plugin-remove",
    "plugin-update",
    "plugins",
    "q",
    "quit",
    "references",
    "rename",
    "signature",
    "signature-help",
    "sp",
    "split",
    "stage",
    "surround",
    "symbol-picker",
    "symbols",
    "theme",
    "tutor",
    "unstage",
    "vs",
    "vsplit",
    "w",
    "fold",
    "zc",
    "unfold",
    "zo",
    "write",
    "wq",
    "x",
];

pub fn matching_commands(prefix: &str, limit: usize) -> Vec<&'static str> {
    let trimmed = prefix.trim();
    if trimmed.is_empty() {
        return COMMAND_NAMES.iter().copied().take(limit).collect();
    }
    let lower = trimmed.to_lowercase();
    let mut matches: Vec<_> = COMMAND_NAMES
        .iter()
        .copied()
        .filter(|name| name.starts_with(&lower))
        .collect();
    if matches.is_empty() {
        matches = COMMAND_NAMES
            .iter()
            .copied()
            .filter(|name| name.contains(&lower))
            .collect();
    }
    matches.truncate(limit);
    matches
}

pub fn command_head(input: &str) -> (&str, &str) {
    let trimmed = input.trim_start();
    let Some((head, tail)) = trimmed.split_once(' ') else {
        return (trimmed, "");
    };
    (head, tail.trim_start())
}

pub fn help_entries(query: &str, limit: usize) -> Vec<String> {
    let lower = query.trim().to_lowercase();
    let mut entries: Vec<String> = COMMAND_HELP
        .iter()
        .filter(|(name, _)| lower.is_empty() || name.contains(&lower))
        .map(|(name, description)| format!("{name} — {description}"))
        .collect();
    for name in COMMAND_NAMES {
        if entries.len() >= limit {
            break;
        }
        if !lower.is_empty() && !name.contains(&lower) {
            continue;
        }
        if entries.iter().any(|entry| entry.starts_with(name)) {
            continue;
        }
        entries.push((*name).to_string());
    }
    entries.truncate(limit);
    entries
}

pub fn complete_command(prefix: &str) -> Option<&'static str> {
    let mut matches = matching_commands(prefix, 2);
    if matches.len() == 1 {
        Some(matches.remove(0))
    } else {
        None
    }
}
