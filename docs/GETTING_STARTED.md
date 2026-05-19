# Getting Started with Jet

Jet is a terminal editor focused on fast startup, lazy large-file opening, and
selection-first editing.

## Quick start

```sh
cargo build --release --locked
target/release/jet README.md
```

Inside the editor:

- `i` — insert mode
- `Esc` — normal mode
- `v` — extend selection; `C` in select mode adds another selection
- With multiple selections, `d`/`y`/`p` apply to every selection
- `:` — command mode (Tab completes; Up/Down history)
- `space` — menu (files, buffers, grep, diagnostics)
- `Ctrl-Q` — quit

## Configuration

User config: `~/.config/jet/config.toml`  
Project config: `.jet/config.toml`  
Themes: `~/.config/jet/themes/*.toml`

```toml
theme = "jet-dark"
keymap = "helix"
tab_width = 4
lsp = true
highlight = true

[editor]
scrolloff = 3
inlay_hints = true
format_on_save = false
word_wrap = false
cursor_style = "block"
line_numbers = true
relative_numbers = false
color_column = 0

[language.rust]
tab_width = 2
lsp = true

[keybindings.normal]
"space d" = "diagnostics"
```

Set `keymap = "helix"` or `keymap = "vscode"` to load bundled preset bindings
when you do not define your own `[keybindings]`.

## Useful commands

| Command | Action |
| --- | --- |
| `:help` | Command reference picker |
| `:grep pattern` | Project search picker |
| `:git-diff` | Diff for current file |
| `:git-stage` / `:git-unstage` | Stage or unstage file |
| `:surround "` | Wrap selection |
| `:plugin-list` | Installed plugins |
| `:tutor` | In-editor tutorial overlay |
| `:config-reload` | Reload config and keybindings |

## LSP

LSP starts lazily when you run an LSP action (or enable `lsp = true` in config).
Use `:lsp-start` to connect manually. Supported actions include completion,
hover (`K`), goto, references, rename, format, and code actions.

## Benchmarks

See [BENCHMARKS.md](../BENCHMARKS.md). Run the smoke harness:

```sh
sh scripts/run_benchmarks.sh
```

On Windows:

```powershell
.\scripts\run_benchmarks.ps1
```
