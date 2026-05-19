use crate::ui::widgets::fuzzy::{fuzzy_match, fuzzy_score};
use anyhow::Result;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    pub path: PathBuf,
    pub display: String,
    pub score: i64,
}

pub fn discover_files(root: &Path, max_depth: usize) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    visit(root, root, max_depth, &mut out)?;
    out.sort();
    Ok(out)
}

pub fn preview_file_lines(path: &Path, max_lines: usize) -> Vec<String> {
    if max_lines == 0 {
        return Vec::new();
    }
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(err) => return vec![format!("(cannot read: {err})")],
    };
    let mut lines = Vec::new();
    for line in BufReader::new(file).lines().take(max_lines) {
        match line {
            Ok(text) => lines.push(text),
            Err(err) => {
                lines.push(format!("(read error: {err})"));
                break;
            }
        }
    }
    if lines.is_empty() {
        lines.push("(empty file)".to_string());
    }
    lines
}

pub fn fuzzy_files(
    root: &Path,
    query: &str,
    max_depth: usize,
    limit: usize,
) -> Result<Vec<PickerItem>> {
    let mut items: Vec<_> = discover_files(root, max_depth)?
        .into_iter()
        .filter_map(|path| {
            let display = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            fuzzy_match(&display, query).then(|| PickerItem {
                score: fuzzy_score(&display, query).unwrap_or(0) as i64,
                path,
                display,
            })
        })
        .collect();
    items.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.display.cmp(&b.display))
    });
    items.truncate(limit);
    Ok(items)
}

fn visit(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
    if depth == 0 {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if should_skip(&name) {
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            visit(root, &path, depth - 1, out)?;
        } else if file_type.is_file() && path.strip_prefix(root).is_ok() {
            out.push(path);
        }
    }
    Ok(())
}

fn should_skip(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | "node_modules" | ".hg" | ".svn" | ".DS_Store"
    )
}
