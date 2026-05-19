# jet — fast terminal text editor

<p align="center">
  <img src="docs/screenshots/hero.gif" alt="jet editor demo" width="720">
</p>

<p align="center">
  <a href="#install">Install</a> •
  <a href="#features">Features</a> •
  <a href="#performance">Performance</a> •
  <a href="docs/GETTING_STARTED.md">Getting Started</a> •
  <a href="BENCHMARKS.md">Benchmarks</a>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/language-Rust-orange" alt="Rust">
  <img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT">
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey" alt="Platform">
</p>

Jet is a terminal text editor for the performance-obsessed. It opens 10 GB files
in milliseconds, searches at 15 GB/s, and starts up in under 10 ms.

## Install

```sh
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/jet-editor/jet/main/scripts/install.sh | sh

# Windows PowerShell
irm https://raw.githubusercontent.com/jet-editor/jet/main/scripts/install.ps1 | iex
```

Or build from source:

```sh
cargo build --release --locked
target/release/jet README.md
```

[Full install docs with version pinning, custom directories, and uninstall &rarr;](#install)

## Features

### Performance

- **Lazy mmap file loading** — first render builds only the visible 200-line
  window. A 10 GB file uses ~1.9 MB RSS.
- **SIMD search** — `memchr`/`memmem` for literal search at 15–45 GB/s
  throughput. Parallel counting for files >64 MB.
- **Sub-millisecond keystroke latency** — render frame in ~11 µs (p50).
  Headless benchmark mode for reproducible measurements.

### Editing

- **Selection-first modal editing** (Kakoune/Helix paradigm). Select then act.
- **Multi-selection** — `C` in select mode adds cursors. `d`/`y`/`p` apply to
  all selections simultaneously.
- **Surround** — `:surround (`, `:surround "`, `ds(` to delete, `cs([` to change.
- **Text objects** — `mif` selects inside function, `maf` around function (Rust).
- **Undo tree** — branches preserved. `:undo` / `:redo` navigates history.
- **Word wrap** — config-gated with continuation marks in gutter.
- **Folding** — LSP `foldingRange`, `:fold`/`:zc`, `:unfold`/`:zo`.

### Syntax highlighting

Five compiled-in Tree-sitter grammars (Rust, Python, JSON, JavaScript, Bash)
with viewport + 32-line overscan parsing on a background thread with 4 ms frame
budget. Lexical fallback for all other languages.

### LSP integration

Completion, hover (`K`), signature help (`(`), goto definition/references,
rename, format-on-save, code actions, diagnostics with inline decorations.
Incremental document sync with a 4 ms frame budget.

### Git

Inline blame (`:git-blame`), hunk navigation (`]c`/`[c`), stage/unstage
(`:git-stage`/`:git-unstage`), diff (`:git-diff`). Gutter signs for
added/modified/deleted/untracked lines. Theme-colored inline diagnostics.

### Collaboration

Real-time CRDT-based editing over TCP or WebSocket (`:collab-host`,
`:collab-join host:port`, `:collab-join ws://host:port`). Peer carets,
selection highlights, chat, auto-reconnect, and relay support.

### Plugin system

WASM sandboxed runtime with memory limits (64 MB), fuel metering (1M
instructions/hook), and import whitelist. SDK crate (`jet-plugin-sdk`) for
authoring plugins in Rust. Host-side hooks: `emit_message`, `emit_virtual_text`,
`emit_gutter_mark`, `register_command`, `register_keymap`. Guest hooks:
`host_log`, `host_read_line`, `host_apply_edit`.

### Customization

XDG + project-local config with hot reload. `[editor]` section: scrolloff,
inlay_hints, format_on_save, word_wrap, cursor_style (block/bar/underline),
line_numbers, relative_numbers, color_column, mouse. Per-language overrides.
Keybinding presets (helix/vscode). Custom TOML themes.

### Finder & navigation

- File picker (Space `f`) with fuzzy matching and preview
- Buffer picker (Space `b`) with buffer tabs
- Grep picker (Space `g`) — line-scanned, no full-file load
- Diagnostics picker with severity colors
- Git picker, symbol picker

## Architecture

```
┌──────────────────────────────────────────────────┐
│                    App (app.rs)                   │
├──────────┬─────────┬────────┬────────┬───────────┤
│  Buffer  │ Editor  │  LSP   │ Plugin │  Collab   │
│ (mmap /  │ (mode,  │(client,│ (wasm, │ (CRDT,    │
│  ropey)  │ cursor, │  diag, │  SDK)  │  TCP, WS) │
│          │  view)  │  fmt)  │        │           │
├──────────┴─────────┴────────┴────────┴───────────┤
│          UI (ratatui + crossterm)                 │
└──────────────────────────────────────────────────┘
```

12 source modules, ~92 source files, ~13k lines Rust. 0 unsafe dependencies
(other than wasmtime and tree-sitter grammar bindings).

## Performance

| Metric | Result | How to reproduce |
|--------|--------|-----------------|
| Cold start (empty buffer) | <10 ms | `cargo bench --bench cold_start` |
| Open 10 GB file | <15 ms | `cargo bench --bench cold_start -- binary_cold_start_10gb` |
| RSS at rest (10 MB–10 GB) | ~1.9 MB | `cargo bench --bench rss_bench` |
| Search 405 MB | 15.7 GB/s | `cargo bench --bench search_simd -- count_error_405mb` |
| Search 2 GB | TBD (Linux) | `cargo bench --bench search_simd -- count_error_2gb` |
| Render frame p50 | 10.8 µs | `cargo bench --bench render_frame` |
| Binary size (stripped) | 16.6 MB (PE) / TBD (ELF) | `ls -lh target/release/jet` |

Full benchmark tables in [BENCHMARKS.md](BENCHMARKS.md).

## Usage

```sh
jet file.txt              # Open file
jet +42 file.txt          # Open at line 42
jet --read-only file.txt  # Read-only mode
jet --headless --quit     # Startup benchmark
jet --search ERROR log    # Count matches, print throughput
jet --bench-latency 1000  # Keystroke latency benchmark
```

| Flag | Purpose |
| --- | --- |
| `--headless` | Run without the interactive TUI |
| `--quit` | Exit after startup (for open/startup benchmarks) |
| `--search PATTERN` | Count matches, print throughput |
| `--bench-latency N` | Simulate N keystrokes, print latency percentiles |
| `+LINE`, `--line LINE` | Open at a line number |
| `-R`, `--read-only` | Open without write operations |
| `--keymap PRESET` | Override keymap (helix, vscode) |
| `--theme THEME` | Override theme |
| `--no-lsp` | Disable LSP client |
| `--no-highlight` | Disable syntax highlighting |

## In-editor commands

| Area | Commands / keys |
| --- | --- |
| Files | `:e`, `:w`, `:q`, Space `f` file picker |
| Buffers | `:buffers`, `:bn`/`:bp`, bufferline |
| Search | `/` `?` `n` `N`, Space `g` grep picker |
| LSP | Ctrl-Space completion, `K` hover, `(` signature help, `gd` goto, `:code-actions` |
| Git | `:git-blame`, `]c`/`[c` hunks, `:git-stage`, `:git-diff` |
| Collab | `:collab-host`, `:collab-join host:port`, `:collab-leave`, `:collab-chat` |
| Config | `:config-reload`, `:theme name`, `:theme` |
| Plugins | `:plugin-list`, `:plugin-install`, `:plugin-remove`, `:plugin-update` |

Normal-mode chords: `g` goto, `]`/`[` prefixes, Space menu, `v` select.

## Screenshots

<!--
  Screenshots and GIFs are stored in docs/screenshots/.
  Record with asciinema + agg (CLI) or terminalizer (GUI).
  See docs/screenshots/README.md for recording instructions.
-->

<p align="center">
  <i>Screenshots coming soon. See the table below for planned captures.</i>
</p>

### Planned screenshots / GIFs

| # | What | Duration | Tool | Shows |
|---|------|----------|------|-------|
| 1 | **Hero demo** — open `main.rs`, navigate with selection-first motions, insert text, save, quit | 15 s | asciinema + agg | Mode indicator, selections, editing flow |
| 2 | **10 GB file in htop** — `jet --headless 10gb.bin` with htop in second pane showing 1.9 MB RSS | 10 s | terminalizer | Lazy mmap, flat RSS |
| 3 | **Search speed** — `jet --headless --search ERROR 405mb.log` showing `15.7 GB/s` | 5 s | terminalizer | SIMD search throughput |
| 4 | **LSP completion** — open Rust file, Ctrl-Space, cycle items, select, see docs | 12 s | asciinema + agg | LSP completion with resolve |
| 5 | **CRDT collaboration** — two terminals side-by-side, host on left, join on right, type in one, see in other | 20 s | terminalizer (tiled) | Peer carets, sync |
| 6 | **Large file scroll** — 200 MB log file, rapid `Ctrl-D`/`Ctrl-U` scrolling | 8 s | asciinema + agg | No frame drops |
| 7 | **Plugin system** — `:plugin-list`, install demo, see gutter marks + virtual text | 15 s | asciinema + agg | Plugin hooks |
| 8 | **Git blame + hunk nav** — `:git-blame`, `]c`/`[c` hunks, `:git-stage` | 12 s | asciinema + agg | Git integration |

### Recording tools

| Tool | Best for | Install |
|------|----------|---------|
| [asciinema](https://asciinema.org) + [agg](https://github.com/asciinema/agg) | Single-terminal GIFs, text-accurate | `apt install asciinema`, `cargo install agg` |
| [terminalizer](https://terminalizer.com) | Tiled/side-by-side, window chrome | `npm install -g terminalizer` |
| [vhs](https://github.com/charmbracelet/vhs) | Tape-driven CI pipelines | `go install github.com/charmbracelet/vhs@latest` |

## Build

```sh
cargo build --release --locked
```

The release profile enables LTO, single codegen unit, symbol stripping, and
abort-on-panic.

## Test

```sh
cargo test --locked
cargo clippy --all-targets -- -D warnings
cargo bench --no-run --locked
```

89 tests (84 integration + 5 unit). 0 allowed failures.

## Known limits

- TypeScript/Go/TOML/Markdown Tree-sitter grammars are not compiled-in (loaded
  dynamically or use lexical fallback).
- Plugin guest WASM hooks are enabled (`host_log`, `host_read_line`,
  `host_apply_edit`) but no published plugin registry exists yet.

## Release Artifacts

GitHub Actions builds these exact archive names on `v*` tag pushes:

| Target | Archive |
|--------|--------|
| `x86_64-unknown-linux-musl` | `jet-linux-x86_64.tar.gz` |
| `aarch64-unknown-linux-musl` | `jet-linux-aarch64.tar.gz` |
| `x86_64-apple-darwin` | `jet-macos-x86_64.tar.gz` |
| `aarch64-apple-darwin` | `jet-macos-aarch64.tar.gz` |
| `x86_64-pc-windows-msvc` | `jet-windows-x86_64.zip` |

The installer scripts depend on these archive names.

## Install details

### Linux / macOS

```sh
# Default install
curl -fsSL https://raw.githubusercontent.com/jet-editor/jet/main/scripts/install.sh | sh

# Specific version
curl -fsSL https://raw.githubusercontent.com/jet-editor/jet/main/scripts/install.sh | sh -s -- --version v1.0.0

# Custom directory
sh scripts/install.sh --install-dir "$HOME/.local/bin"

# Uninstall
sh scripts/install.sh --uninstall
```

### Windows PowerShell

```powershell
# Default install
irm https://raw.githubusercontent.com/jet-editor/jet/main/scripts/install.ps1 | iex

# Specific version
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/jet-editor/jet/main/scripts/install.ps1))) -Version v1.0.0

# Custom directory
.\scripts\install.ps1 -InstallDir "$env:USERPROFILE\bin"

# Uninstall
.\scripts\install.ps1 -Uninstall
```

> Do not pipe the Linux/macOS `install.sh` into PowerShell. Native Windows
> does not include `sh`; use `install.ps1` or download the `.zip` from
> GitHub Releases.

## Documentation

- [Getting Started](docs/GETTING_STARTED.md) — config, keybindings, commands, LSP setup
- [Benchmarks](BENCHMARKS.md) — full benchmark tables and reproduction commands
- `:tutor` — in-editor interactive tutorial for selection-first editing
