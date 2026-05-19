use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerDefinition {
    pub language: &'static str,
    pub server_name: &'static str,
    pub binary: &'static str,
    pub args: &'static [&'static str],
    pub root_markers: &'static [&'static str],
    pub trigger_characters: &'static [&'static str],
    pub install_hint: &'static str,
}

pub fn registry() -> &'static [ServerDefinition] {
    &[
        ServerDefinition {
            language: "rust",
            server_name: "rust-analyzer",
            binary: "rust-analyzer",
            args: &[],
            root_markers: &["Cargo.toml", "rust-project.json", ".git"],
            trigger_characters: &[".", "::"],
            install_hint: "rustup component add rust-analyzer",
        },
        ServerDefinition {
            language: "typescript",
            server_name: "typescript-language-server",
            binary: "typescript-language-server",
            args: &["--stdio"],
            root_markers: &["package.json", "tsconfig.json", "jsconfig.json", ".git"],
            trigger_characters: &[".", "\"", "'", "/"],
            install_hint: "npm install -g typescript typescript-language-server",
        },
        ServerDefinition {
            language: "javascript",
            server_name: "typescript-language-server",
            binary: "typescript-language-server",
            args: &["--stdio"],
            root_markers: &["package.json", "jsconfig.json", ".git"],
            trigger_characters: &[".", "\"", "'", "/"],
            install_hint: "npm install -g typescript typescript-language-server",
        },
        ServerDefinition {
            language: "python",
            server_name: "pyright",
            binary: "pyright-langserver",
            args: &["--stdio"],
            root_markers: &["pyproject.toml", "setup.py", "requirements.txt", ".git"],
            trigger_characters: &["."],
            install_hint: "npm install -g pyright",
        },
        ServerDefinition {
            language: "go",
            server_name: "gopls",
            binary: "gopls",
            args: &[],
            root_markers: &["go.mod", "go.work", ".git"],
            trigger_characters: &["."],
            install_hint: "go install golang.org/x/tools/gopls@latest",
        },
        ServerDefinition {
            language: "c",
            server_name: "clangd",
            binary: "clangd",
            args: &[],
            root_markers: &["compile_commands.json", "compile_flags.txt", ".git"],
            trigger_characters: &[".", "->", "::"],
            install_hint: "install clangd from LLVM",
        },
        ServerDefinition {
            language: "cpp",
            server_name: "clangd",
            binary: "clangd",
            args: &[],
            root_markers: &["compile_commands.json", "compile_flags.txt", ".git"],
            trigger_characters: &[".", "->", "::"],
            install_hint: "install clangd from LLVM",
        },
        ServerDefinition {
            language: "lua",
            server_name: "lua-language-server",
            binary: "lua-language-server",
            args: &[],
            root_markers: &[".luarc.json", ".luacheckrc", ".git"],
            trigger_characters: &[".", ":"],
            install_hint: "install lua-language-server",
        },
        ServerDefinition {
            language: "bash",
            server_name: "bash-language-server",
            binary: "bash-language-server",
            args: &["start"],
            root_markers: &[".git"],
            trigger_characters: &["$"],
            install_hint: "npm install -g bash-language-server",
        },
        ServerDefinition {
            language: "json",
            server_name: "vscode-json-languageserver",
            binary: "vscode-json-languageserver",
            args: &["--stdio"],
            root_markers: &["package.json", ".git"],
            trigger_characters: &["\"", ":"],
            install_hint: "npm install -g vscode-langservers-extracted",
        },
        ServerDefinition {
            language: "yaml",
            server_name: "yaml-language-server",
            binary: "yaml-language-server",
            args: &["--stdio"],
            root_markers: &[".git"],
            trigger_characters: &[":"],
            install_hint: "npm install -g yaml-language-server",
        },
        ServerDefinition {
            language: "toml",
            server_name: "taplo",
            binary: "taplo",
            args: &["lsp", "stdio"],
            root_markers: &["taplo.toml", ".git"],
            trigger_characters: &[".", "="],
            install_hint: "cargo install taplo-cli --locked",
        },
        ServerDefinition {
            language: "markdown",
            server_name: "marksman",
            binary: "marksman",
            args: &["server"],
            root_markers: &[".marksman.toml", ".git"],
            trigger_characters: &["[", "#"],
            install_hint: "install marksman",
        },
    ]
}

pub fn server_for_language(language: &str) -> Option<&'static ServerDefinition> {
    registry().iter().find(|server| server.language == language)
}

pub fn server_for_path(path: &Path) -> Option<&'static str> {
    server_definition_for_path(path).map(|server| server.binary)
}

pub fn server_definition_for_path(path: &Path) -> Option<&'static ServerDefinition> {
    let language = language_for_path(path)?;
    server_for_language(language)
}

pub fn language_for_path(path: &Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let mut map = HashMap::new();
    map.insert("rs", "rust");
    map.insert("ts", "typescript");
    map.insert("tsx", "typescript");
    map.insert("js", "javascript");
    map.insert("jsx", "javascript");
    map.insert("py", "python");
    map.insert("go", "go");
    map.insert("c", "c");
    map.insert("h", "c");
    map.insert("cc", "cpp");
    map.insert("cpp", "cpp");
    map.insert("hpp", "cpp");
    map.insert("lua", "lua");
    map.insert("sh", "bash");
    map.insert("bash", "bash");
    map.insert("json", "json");
    map.insert("yaml", "yaml");
    map.insert("yml", "yaml");
    map.insert("toml", "toml");
    map.insert("md", "markdown");
    map.get(ext).copied().or(match filename {
        "Dockerfile" => Some("dockerfile"),
        "Makefile" => Some("make"),
        _ => None,
    })
}
