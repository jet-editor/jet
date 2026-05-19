use std::path::{Path, PathBuf};

pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(path)
}
