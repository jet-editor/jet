# Jet Roadmap Status — v1.0.0

All features are wired in the running editor with tests. Last verified: `cargo test --locked` (89 tests), `cargo clippy --all-targets -- -D warnings`.

## Feature Completion

| Area | Status | Details |
|------|--------|---------|
| **Core editor** | Complete | Modes, splits, pickers, undo tree, surround, multi-selection, folds, word wrap, viewport rendering |
| **Tree-sitter** | Partial (5/9 compiled) | Rust, Python, JSON, JavaScript, Bash compiled-in; TypeScript, Go, TOML, Markdown dynamic/lexical |
| **LSP** | Complete | Completion with resolve, hover, goto, references, rename, format-on-save, code actions, diagnostics with inline decorations, incremental sync |
| **Plugins** | Partial | wasmtime runtime with memory/fuel limits; `jet-plugin-sdk` workspace member; install/list/remove/update; guest WASM hooks not enabled |
| **Collab** | Complete | TCP + WebSocket transport; diamond-types CRDT; peer carets/selection overlay; chat; reconnect; relay support |
| **Config/themes** | Complete | XDG + project config with hot reload; serde schema with defaults; `[editor]` section (scrolloff, inlay_hints, format_on_save, word_wrap, cursor_style, line_numbers, relative_numbers, color_column, mouse); per-language overrides; keybinding presets (helix/vscode); 2 built-in themes + custom TOML themes |
| **Large files** | Complete | mmap lazy loading — only visible window into Rope; overlay edits; ~10ms open for 16MB; flat RSS across 10MB–10GB |
| **Search** | Complete | SIMD-accelerated (memchr/memmem); ~36 GiB/s throughput; project grep; buffer search |
| **Distribution** | Complete | Install scripts (sh/ps1); GitHub Actions cross-platform builds (Linux musl, macOS x86_64 + aarch64, Windows x86_64) |
| **Docs** | Partial | `README.md`, `docs/GETTING_STARTED.md`, `:tutor` overlay, `BENCHMARKS.md` |

## Key Architecture

- **12 source modules** in `src/`: app, buffer, collab, config, editor, git, highlight, lsp, plugin, terminal, ui, util
- **Workspace member**: `jet-plugin-sdk/` for WASM plugin authoring
- **47+ dependencies**: tree-sitter (5 compiled grammars), wasmtime, diamond-types, tokio, crossterm, ratatui, git2, nucleo, memmap2
- **~92 source files**, ~13k+ lines Rust
- **89 tests**: 84 integration + 5 unit

## Benchmarks (Windows release artifact)

| Benchmark | Result |
|-----------|--------|
| `empty_buffer_startup` | 85 ns |
| `rope/from_text/97000` | 31 µs (2.9 GiB/s) |
| `rope/line_lookup/1000` | 4.4 µs (20.5 GiB/s) |
| `open_lazy_16mb` | 10.1 ms |
| `render_frame_p50` | 10.8 µs |
| `search_simd/count_error_64mb` | 1.71 ms (36.5 GiB/s) |
| `insert_10_chars_10000_lines` | 99 µs |

## Future work (post-v1.0.0)

1. Compiled TypeScript/Go/TOML/Markdown grammars with injection queries
2. Guest WASM hook execution in plugin runtime
3. Plugin registry with network fetch
4. P2P relay server product with human-readable session codes
5. Competitor benchmarks in CI (Helix/Kakoune/micro)
6. Homebrew/crates.io/AUR distribution
7. `[editor.file-picker]`, `[keys.*]` per-mode keybindings, more config fields
8. Full docs website, expanded `:tutor` lessons
