use crate::git::blame::path_relative_to;
use anyhow::{bail, Context, Result};
use git2::Repository;
use std::{path::Path, process::Command};

pub fn stage_file(project_root: &Path, file_path: &Path) -> Result<()> {
    let repo = Repository::discover(project_root).context("git repository not found")?;
    let repo_root = repo
        .workdir()
        .context("cannot stage in a bare repository")?
        .to_path_buf();
    let relative = path_relative_to(&repo_root, file_path)
        .with_context(|| format!("file is outside repository: {}", file_path.display()))?;
    let status = Command::new("git")
        .args(["add", "--"])
        .arg(&relative)
        .current_dir(&repo_root)
        .status()
        .context("failed to run git add")?;
    if !status.success() {
        bail!("git add failed with status {}", status);
    }
    Ok(())
}

pub fn unstage_file(project_root: &Path, file_path: &Path) -> Result<()> {
    let repo = Repository::discover(project_root).context("git repository not found")?;
    let repo_root = repo
        .workdir()
        .context("cannot unstage in a bare repository")?
        .to_path_buf();
    let relative = path_relative_to(&repo_root, file_path)
        .with_context(|| format!("file is outside repository: {}", file_path.display()))?;
    let mut command = Command::new("git");
    command
        .args(["restore", "--staged", "--"])
        .arg(&relative)
        .current_dir(&repo_root);
    let status = command
        .status()
        .context("failed to run git restore --staged")?;
    if status.success() {
        return Ok(());
    }
    let status = Command::new("git")
        .args(["rm", "--cached", "--force", "--"])
        .arg(&relative)
        .current_dir(&repo_root)
        .status()
        .context("failed to run git rm --cached")?;
    if !status.success() {
        bail!("git unstage failed with status {}", status);
    }
    Ok(())
}
