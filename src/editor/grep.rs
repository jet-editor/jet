use crate::editor::picker;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepMatch {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

pub fn grep_project(root: &Path, query: &str, limit: usize) -> Vec<GrepMatch> {
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }

    let Ok(paths) = picker::discover_files(root, 16) else {
        return Vec::new();
    };

    let mut matches = Vec::new();
    for path in paths.into_iter().take(1024) {
        if !is_probably_text(&path) {
            continue;
        }
        if !grep_file(&path, query, limit, &mut matches) {
            break;
        }
    }
    matches
}

fn grep_file(path: &Path, query: &str, limit: usize, matches: &mut Vec<GrepMatch>) -> bool {
    let Ok(file) = File::open(path) else {
        return true;
    };
    let reader = BufReader::new(file);
    for (line_idx, line) in reader.lines().enumerate() {
        let Ok(line) = line else {
            continue;
        };
        if let Some(column) = line.find(query) {
            matches.push(GrepMatch {
                path: path.to_path_buf(),
                line: line_idx,
                column,
                text: line,
            });
            if matches.len() >= limit {
                return false;
            }
        }
    }
    true
}

fn is_probably_text(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let mut reader = BufReader::new(file);
    let mut sample = [0u8; 8192];
    let Ok(read) = std::io::Read::read(&mut reader, &mut sample) else {
        return false;
    };
    !sample[..read].contains(&0)
}
