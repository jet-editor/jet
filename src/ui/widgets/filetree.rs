use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeItem {
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
}

pub fn item(path: &Path) -> FileTreeItem {
    FileTreeItem {
        path: path.to_path_buf(),
        is_dir: path.is_dir(),
        depth: 0,
    }
}

pub fn build_tree(root: &Path, max_depth: usize, max_entries: usize) -> Vec<FileTreeItem> {
    let mut items = Vec::new();
    let ignore = load_gitignore_patterns(root);
    visit(
        root,
        root,
        0,
        max_depth,
        max_entries,
        &mut items,
        root,
        &ignore,
    );
    items
}

fn load_gitignore_patterns(root: &Path) -> Vec<String> {
    let path = root.join(".gitignore");
    std::fs::read_to_string(path)
        .map(|source| {
            source
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn path_ignored(rel: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| {
        if let Some(suffix) = pattern.strip_prefix('*') {
            rel.ends_with(suffix)
        } else if pattern.ends_with('/') {
            rel.starts_with(pattern.trim_end_matches('/'))
        } else {
            rel == pattern
                || rel.starts_with(&format!("{pattern}/"))
                || rel.ends_with(&format!("/{pattern}"))
        }
    })
}

#[allow(clippy::only_used_in_recursion, clippy::too_many_arguments)]
fn visit(
    _root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    out: &mut Vec<FileTreeItem>,
    project_root: &Path,
    ignore: &[String],
) {
    if out.len() >= max_entries || depth > max_depth {
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read_dir.filter_map(Result::ok).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if out.len() >= max_entries {
            return;
        }
        let path = entry.path();
        if let Ok(rel) = path.strip_prefix(project_root) {
            let rel = rel.to_string_lossy().replace('\\', "/");
            if path_ignored(&rel, ignore) {
                continue;
            }
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if should_skip(&name) {
            continue;
        }
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        out.push(FileTreeItem {
            path: path.clone(),
            is_dir,
            depth,
        });
        if is_dir {
            visit(
                _root,
                &path,
                depth + 1,
                max_depth,
                max_entries,
                out,
                project_root,
                ignore,
            );
        }
    }
}

pub fn render_lines(items: &[FileTreeItem], root: &Path, width: usize) -> Vec<String> {
    items
        .iter()
        .map(|item| {
            let indent = "  ".repeat(item.depth);
            let marker = if item.is_dir { "▸ " } else { "  " };
            let label = item
                .path
                .strip_prefix(root)
                .unwrap_or(&item.path)
                .to_string_lossy()
                .replace('\\', "/");
            let line = format!("{indent}{marker}{label}");
            if line.len() > width {
                format!("{}…", &line[..width.saturating_sub(1)])
            } else {
                line
            }
        })
        .collect()
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".hg" | ".svn" | ".DS_Store"
    )
}
