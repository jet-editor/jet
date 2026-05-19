# Changelog

## 1.0.0 - 2026-05-18

Initial public release.

### Added

- **Lazy mmap-backed file opening**: builds only the first visible window
  (200 lines) into a Rope; never loads whole-file; overlay edits for modifications;
  ~10ms open for 16MB; flat RSS across 10MB–10GB files
- **Encoding detection**: UTF-8 BOM detection/stripping on open
- **SIMD search**: memchr/memmem literal search with parallel counting; ~36 GiB/s
  throughput on 64MB files
- **Tree-sitter syntax highlighting**: 5 compiled grammars (Rust, Python, JSON,
  JavaScript, Bash) for viewport + overscan; lexical fallback for others;
  incremental parsing with 4ms frame budget
- **LSP client**: completion with resolve, hover, signature help, goto
  definition/references/implementation, rename, format-on-save, code actions,
  diagnostics with inline decorations, incremental document sync
- **Word wrap**: config-gated, with continuation tracking in gutter
- **Folding**: LSP `foldingRange` support, `:fold`/`:zc`/`:unfold`/`:zo`,
  fold gutter markers (`▾`)
- **CRDT collaboration**: diamond-types-based real-time editing over TCP or
  WebSocket; peer carets, selection overlay, chat, reconnect, relay support
- **Plugin system**: wasmtime sandboxed runtime with memory/fuel limits;
  `jet-plugin-sdk` workspace member crate with safe Rust bindings for 5 host
  imports (emit_message, emit_virtual_text, emit_gutter_mark, register_command,
  register_keymap); install/remove/update/list commands
- **Git integration**: gutter signs, inline blame, hunk navigation (`]c`/`[c`),
  `:git-stage`/`:git-unstage`/`:git-diff`, untracked file detection
- **Config schema**: XDG + project-local config with hot reload; `[editor]`
  section (scrolloff, inlay_hints, format_on_save, word_wrap, cursor_style,
  line_numbers, relative_numbers, color_column, mouse); per-language overrides;
  keybinding presets (helix/vscode); custom TOML themes
- **UI**: buffer tabs/statusline, which-key popup, command completion/history,
  file picker with preview, grep picker, diagnostics picker with severity colors,
  git picker, symbol picker, tutor overlay, theme switching
- **CLI**: `--headless`, `--quit`, `--search`, `--bench-latency`, `--line`,
  `--read-only`, `--keymap`, `--theme`, `--no-lsp`, `--no-highlight`
- **Cross-platform distribution**: GitHub Actions builds for Linux musl (x86_64 +
  aarch64), macOS (x86_64 + aarch64), Windows (x86_64); install scripts for
  POSIX sh and PowerShell
- **89 tests**: 84 integration + 5 unit; clippy clean on all workspace members
- **6 benchmark suites**: startup, core, open_large_file, render_frame,
  search_simd, treesitter

### Binary

- Release binary size: 16.6 MB (Windows PE, stripped, with wasmtime)
- 1 codegen unit, LTO, symbol stripping, abort-on-panic

### Pending

- Native Linux ELF startup and 10GB open-time numbers must be recorded from the
  `jet-linux-x86_64.tar.gz` release artifact after GitHub Actions builds it
