use crate::highlight::treesitter::Language;
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageSpec {
    pub language: Language,
    pub name: &'static str,
    pub extensions: &'static [&'static str],
    pub shebangs: &'static [&'static str],
    pub compiled_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrammarPackage {
    pub language: String,
    pub path: PathBuf,
    pub installed_at: Option<SystemTime>,
}

#[derive(Debug, Clone)]
pub struct GrammarManager {
    root: PathBuf,
}

impl GrammarManager {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn package_path(&self, language: &str) -> PathBuf {
        let extension = std::env::consts::DLL_EXTENSION;
        self.root.join(format!("{language}.{extension}"))
    }

    pub fn installed(&self, language: &str) -> Option<GrammarPackage> {
        let path = self.package_path(language);
        path.exists().then(|| GrammarPackage {
            language: language.to_string(),
            installed_at: path.metadata().and_then(|meta| meta.modified()).ok(),
            path,
        })
    }

    pub fn is_available(&self, language: Language) -> bool {
        language_spec(language)
            .map(|spec| spec.compiled_in || self.installed(spec.name).is_some())
            .unwrap_or(false)
    }
}

pub fn language_specs() -> &'static [LanguageSpec] {
    &[
        LanguageSpec {
            language: Language::Rust,
            name: "rust",
            extensions: &["rs"],
            shebangs: &[],
            compiled_in: true,
        },
        LanguageSpec {
            language: Language::TypeScript,
            name: "typescript",
            extensions: &["ts", "tsx"],
            shebangs: &[],
            compiled_in: true,
        },
        LanguageSpec {
            language: Language::JavaScript,
            name: "javascript",
            extensions: &["js", "jsx", "mjs", "cjs"],
            shebangs: &["node"],
            compiled_in: true,
        },
        LanguageSpec {
            language: Language::Python,
            name: "python",
            extensions: &["py", "pyw"],
            shebangs: &["python", "python3"],
            compiled_in: true,
        },
        LanguageSpec {
            language: Language::Go,
            name: "go",
            extensions: &["go"],
            shebangs: &[],
            compiled_in: true,
        },
        LanguageSpec {
            language: Language::Json,
            name: "json",
            extensions: &["json", "jsonc"],
            shebangs: &[],
            compiled_in: true,
        },
        LanguageSpec {
            language: Language::Toml,
            name: "toml",
            extensions: &["toml"],
            shebangs: &[],
            compiled_in: false,
        },
        LanguageSpec {
            language: Language::Markdown,
            name: "markdown",
            extensions: &["md", "markdown"],
            shebangs: &[],
            compiled_in: false,
        },
        LanguageSpec {
            language: Language::Bash,
            name: "bash",
            extensions: &["sh", "bash"],
            shebangs: &["bash", "sh"],
            compiled_in: true,
        },
    ]
}

pub fn language_spec(language: Language) -> Option<&'static LanguageSpec> {
    language_specs()
        .iter()
        .find(|spec| spec.language == language)
}

pub fn language_spec_by_name(name: &str) -> Option<&'static LanguageSpec> {
    language_specs().iter().find(|spec| spec.name == name)
}

pub fn detect_by_path(path: &Path) -> Language {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    language_specs()
        .iter()
        .find(|spec| spec.extensions.contains(&ext))
        .map(|spec| spec.language)
        .unwrap_or(Language::PlainText)
}

pub fn detect_by_shebang(first_line: &str) -> Option<Language> {
    let shebang = first_line.strip_prefix("#!")?;
    let mut parts = shebang.split_whitespace();
    let command = parts
        .next()
        .and_then(|path| path.rsplit('/').next())
        .unwrap_or_default();
    let command = if command == "env" {
        parts.next().unwrap_or_default()
    } else {
        command
    };
    language_specs()
        .iter()
        .find(|spec| spec.shebangs.contains(&command))
        .map(|spec| spec.language)
}

pub fn compiled_languages() -> Vec<Language> {
    language_specs()
        .iter()
        .filter(|spec| spec.compiled_in)
        .map(|spec| spec.language)
        .collect()
}
