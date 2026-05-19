use crate::git::blame::path_relative_to;
use anyhow::{Context, Result};
use git2::Repository;
use std::{path::Path, process::Command};

pub fn file_diff_lines(project_root: &Path, file_path: &Path) -> Result<Vec<String>> {
    let repo = Repository::discover(project_root).context("git repository not found")?;
    let repo_root = repo
        .workdir()
        .context("cannot diff in a bare repository")?
        .to_path_buf();
    let relative = path_relative_to(&repo_root, file_path)
        .with_context(|| format!("file is outside repository: {}", file_path.display()))?;

    let head_diff = Command::new("git")
        .args(["diff", "HEAD", "--"])
        .arg(&relative)
        .current_dir(&repo_root)
        .output()
        .context("failed to run git diff HEAD")?;
    if head_diff.status.success() && !head_diff.stdout.is_empty() {
        return Ok(lines_from_output(&head_diff.stdout));
    }

    let worktree_diff = Command::new("git")
        .args(["diff", "--"])
        .arg(&relative)
        .current_dir(&repo_root)
        .output()
        .context("failed to run git diff")?;
    if worktree_diff.status.success() && !worktree_diff.stdout.is_empty() {
        return Ok(lines_from_output(&worktree_diff.stdout));
    }

    let staged_diff = Command::new("git")
        .args(["diff", "--cached", "--"])
        .arg(&relative)
        .current_dir(&repo_root)
        .output()
        .context("failed to run git diff --cached")?;
    if staged_diff.status.success() && !staged_diff.stdout.is_empty() {
        return Ok(lines_from_output(&staged_diff.stdout));
    }

    Ok(Vec::new())
}

fn lines_from_output(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::to_string)
        .collect()
}
