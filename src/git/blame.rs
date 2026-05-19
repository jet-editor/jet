use anyhow::Result;
use git2::Repository;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineBlame {
    pub author: String,
    pub commit_short: String,
}

pub fn blame_for_file(project_root: &Path, file_path: &Path) -> Option<HashMap<usize, LineBlame>> {
    let repo = Repository::discover(project_root).ok()?;
    let repo_root = repo.workdir()?.to_path_buf();
    let relative = path_relative_to(&repo_root, file_path)?;
    blame_tracked_file(&repo, &relative).ok()
}

pub fn format_annotation(blame: &LineBlame) -> String {
    format!("{} {}", blame.commit_short, blame.author)
}

pub(crate) fn path_relative_to(base: &Path, path: &Path) -> Option<PathBuf> {
    let base = base.canonicalize().ok()?;
    let path = path.canonicalize().ok()?;
    path.strip_prefix(&base).ok().map(|p| p.to_path_buf())
}

fn blame_tracked_file(repo: &Repository, relative: &Path) -> Result<HashMap<usize, LineBlame>> {
    let blame = repo.blame_file(relative, None)?;
    let mut lines = HashMap::new();
    for hunk in blame.iter() {
        let signature = hunk.final_signature();
        let author = signature.name().unwrap_or("unknown").to_string();
        let commit_short = format_short_oid(hunk.final_commit_id().to_string());
        let start = hunk.final_start_line();
        for offset in 0..hunk.lines_in_hunk() {
            let line = start.saturating_add(offset).saturating_sub(1);
            lines.insert(
                line,
                LineBlame {
                    author: author.clone(),
                    commit_short: commit_short.clone(),
                },
            );
        }
    }
    Ok(lines)
}

fn format_short_oid(oid: String) -> String {
    oid.chars().take(7).collect()
}
