pub mod blame;
pub mod diff;
pub mod stage;

use anyhow::Result;
use git2::{Delta, DiffOptions, Repository, Status, StatusOptions};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

pub use blame::{blame_for_file, format_annotation, LineBlame};
pub use diff::file_diff_lines;
pub use stage::{stage_file, unstage_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineStatus {
    Added,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitHunk {
    pub start_line: usize,
    pub end_line: usize,
    pub status: LineStatus,
}

#[derive(Debug, Clone, Default)]
pub struct FileGitStatus {
    pub branch: Option<String>,
    pub marks: HashMap<usize, LineStatus>,
    pub hunks: Vec<GitHunk>,
}

pub fn status_for_file(project_root: &Path, file_path: &Path) -> Option<FileGitStatus> {
    let hunks = hunks_for_file(project_root, file_path)?;
    let branch = Repository::discover(project_root)
        .ok()
        .and_then(|repo| head_branch(&repo));
    Some(FileGitStatus {
        branch,
        marks: marks_from_hunks(&hunks),
        hunks,
    })
}

pub fn hunks_for_file(project_root: &Path, file_path: &Path) -> Option<Vec<GitHunk>> {
    let repo = Repository::discover(project_root).ok()?;
    let repo_root = repo.workdir()?.to_path_buf();
    let relative = path_relative_to(&repo_root, file_path)?;
    if is_untracked(&repo, &relative).ok()? {
        Some(untracked_hunks(file_path).ok()?)
    } else {
        Some(diff_hunks(&repo, &relative).ok()?)
    }
}

pub fn adjacent_hunk(hunks: &[GitHunk], current_line: usize, direction: i32) -> Option<&GitHunk> {
    if hunks.is_empty() {
        return None;
    }
    if direction >= 0 {
        hunks
            .iter()
            .find(|hunk| hunk.start_line > current_line)
            .or_else(|| hunks.first())
    } else {
        hunks
            .iter()
            .rev()
            .find(|hunk| hunk.start_line < current_line)
            .or_else(|| hunks.last())
    }
}

fn head_branch(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    head.shorthand().map(str::to_string)
}

fn is_untracked(repo: &Repository, relative: &Path) -> Result<bool> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    opts.include_ignored(false);
    let statuses = repo.statuses(Some(&mut opts))?;
    for entry in statuses.iter() {
        let entry_path = entry.path().map(Path::new);
        if entry_path == Some(relative) && entry.status() == Status::WT_NEW {
            return Ok(true);
        }
    }
    Ok(false)
}

fn untracked_hunks(file_path: &Path) -> Result<Vec<GitHunk>> {
    let text = std::fs::read_to_string(file_path).unwrap_or_default();
    let line_count = text.lines().count().max(1);
    Ok(vec![GitHunk {
        start_line: 0,
        end_line: line_count.saturating_sub(1),
        status: LineStatus::Added,
    }])
}

fn diff_hunks(repo: &Repository, relative: &Path) -> Result<Vec<GitHunk>> {
    let head = repo.head()?.peel_to_commit()?;
    let tree = head.tree()?;
    let mut opts = DiffOptions::new();
    opts.pathspec(relative.to_string_lossy().as_ref());
    let diff = repo.diff_tree_to_workdir(Some(&tree), Some(&mut opts))?;
    let mut hunks = Vec::new();

    diff.foreach(
        &mut |_, _| true,
        None,
        Some(&mut |delta, hunk| {
            let status = hunk_status(delta.status(), &hunk);
            let start_line = hunk.new_start().saturating_sub(1) as usize;
            let end_line = if hunk.new_lines() == 0 {
                start_line
            } else {
                (hunk.new_start() + hunk.new_lines() - 1).saturating_sub(1) as usize
            };
            hunks.push(GitHunk {
                start_line,
                end_line,
                status,
            });
            true
        }),
        None,
    )?;

    hunks.sort_by_key(|hunk| hunk.start_line);
    Ok(hunks)
}

fn hunk_status(delta: Delta, hunk: &git2::DiffHunk<'_>) -> LineStatus {
    match delta {
        Delta::Added => LineStatus::Added,
        Delta::Modified if hunk.old_lines() > 0 && hunk.new_lines() > 0 => LineStatus::Modified,
        _ => LineStatus::Added,
    }
}

fn marks_from_hunks(hunks: &[GitHunk]) -> HashMap<usize, LineStatus> {
    let mut marks = HashMap::new();
    for hunk in hunks {
        for line in hunk.start_line..=hunk.end_line {
            marks.insert(line, hunk.status);
        }
    }
    marks
}

fn path_relative_to(base: &Path, path: &Path) -> Option<PathBuf> {
    blame::path_relative_to(base, path)
}
