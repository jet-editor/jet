# Jet Benchmarks

This file records reproducible benchmark commands. Do not publish competitor
claims unless the competing editors are installed and run by the same script in
CI with hardware details captured beside the results.

## Environment

Record these before publishing numbers:

```sh
rustc --version
cargo --version
target/release/jet --version
```

## Latest Results — v1.0.0

### Native Linux (x86_64-unknown-linux-musl)

Record from a GitHub Actions runner (Ubuntu latest, x86_64). Run `cargo bench --locked` on the `jet-linux-x86_64.tar.gz` artifact.

> **Note:** Linux CI run pending for v1.0.0. Non-RSS results below are from the
> Windows baseline run (AMD Ryzen 9 6900HX); RSS and binary size require a
> native Linux build and are listed as TBD.

| Metric | Result | Notes |
|--------|--------|-------|
| Cold start (empty buffer) | **16.9 ms** | `cargo bench --bench cold_start -- binary_cold_start_empty` |
| Cold start (10 GB file) | **6.51 s** | `cargo bench --bench cold_start -- binary_cold_start_10gb` |
| Peak RSS (10 MB file) | TBD | `cargo bench --bench rss_bench -- rss_10mb` |
| Peak RSS (100 MB file) | TBD | `cargo bench --bench rss_bench -- rss_100mb` |
| Peak RSS (1 GB file) | TBD | `cargo bench --bench rss_bench -- rss_1gb` |
| Peak RSS (10 GB file) | TBD | `cargo bench --bench rss_bench -- rss_10gb` |
| Search (1 MB) | TBD | `cargo bench --bench core -- search/.*/1048576` |
| Search (16 MB) | TBD | `cargo bench --bench core -- search/.*/16777216` |
| Search (64 MB) | **4.30 ms**, 14.5 GiB/s | `cargo bench --bench core -- search/.*/67108864` |
| Search (405 MB) | **10.5 ms**, 35.9 GiB/s | `cargo bench --bench search_simd -- count_error_405mb` |
| Search (2 GB) | **51.3 ms**, 36.3 GiB/s | `cargo bench --bench search_simd -- count_error_2gb` |
| Render frame p50 | **3.68 µs** | `cargo bench --bench render_frame` |
| Rope insert (10K lines) | **101 µs** | `cargo bench --bench treesitter -- insert_10_chars_10000` |
| Rope insert (100K lines) | **1.53 ms** | `cargo bench --bench treesitter -- insert_10_chars_100000` |
| Binary size (stripped) | TBD | `ls -lh target/release/jet` |

### Windows PE (AMD Ryzen 9 6900HX) — v1.0.0

> rss_bench skipped on Windows (requires Linux procfs). All 8 other suites
> completed without errors or panics.

| Benchmark | Result | Notes |
|-----------|--------|-------|
| `empty_buffer_startup` | **78.8 ns** | |
| `cold_start/empty_buffer` | **16.9 ms** | |
| `cold_start/10gb_file` | **6.51 s** | |
| `rope/from_text/97000` (97 KB) | **31.7 µs**, 2.85 GiB/s | |
| `rope/from_text/2425000` (2.4 MB) | **0.935 ms**, 2.42 GiB/s | -9.2% vs pre-release |
| `rope/line_lookup/1000` | **3.47 µs**, 26.0 GiB/s | |
| `rope/line_lookup/25000` | **55.5 µs**, 40.7 GiB/s | noise-stabilised vs pre-release |
| `open_lazy_16mb` | **9.42 ms** | -16.6% via MmapBuffer allocation reduction |
| `render_frame_p50` | **3.68 µs** | -64.6% via buffer reuse in `truncate_to_width` |
| `search/sequential_memmem/67108864` (64 MB) | **4.30 ms**, 14.5 GiB/s | |
| `search/parallel_memmem/67108864` (64 MB) | **4.37 ms**, 14.3 GiB/s | |
| `search_simd/count_error_64mb` | **1.77 ms**, 35.3 GiB/s | |
| `search_simd/count_error_405mb` | **10.5 ms**, 35.9 GiB/s | 405,000,000 bytes (SI) |
| `search_simd/count_error_2gb` | **51.3 ms**, 36.3 GiB/s | 2,000,000,000 bytes (SI); near hardware limit |
| `treesitter/insert_10_chars_10000_lines` | **101 µs** | |
| `treesitter/insert_10_chars_100000_lines` | **1.53 ms** | |

#### v1.0.0 Hot-path summary

| Path | Metric | Context |
|------|--------|---------|
| Rendering | 3.68 µs/frame | 0.022% of a 60 fps frame budget |
| Search (SIMD memchr) | 35–36 GiB/s | near hardware limit |
| Large file open (lazy mmap) | 16 MB in 9.42 ms | |
| Rope line lookup | 40.7 GiB/s | 25 K-line buffer |
| Rope insert (text) | 2.85 GiB/s | 97 KB buffer |

#### v1.0.0 Optimizations applied

- **Rendering (−64.6%)** — replaced per-line `String` allocation in
  `truncate_to_width` with buffer reuse via `truncate_to_width_into`
  (10.4 µs → 3.68 µs).
- **MmapBuffer allocations (−16.6%)** — added `char_at()`, `char_to_byte()`,
  `byte_to_char()`, `to_window_string()` to avoid `.to_string()` calls in the
  hot path (11.3 ms → 9.42 ms).
- **Code hygiene** — removed redundant clone in `refresh_search_matches`;
  deduplicated leftover code in `rope.rs`.
- **CI / musl builds** — switched `git2` dependency to include
  `vendored-libgit2` feature (required for musl cross-compilation; the
  `vendored` feature was renamed in git2 0.18).

---

## Build

```sh
cargo build --release --locked
```

## All Benchmarks (full suite)

```sh
# Run all benchmarks at once
cargo bench --locked
```

## Individual benchmarks

```sh
# Startup (empty buffer, in-process)
cargo bench --bench startup -- --sample-size 10

# Cold start (full binary subprocess)
cargo bench --bench cold_start -- --sample-size 10

# Large file open (lazy mmap)
cargo bench --bench open_large_file -- --sample-size 10

# Search throughput (1MB, 16MB, 64MB in-memory; 405MB, 2GB via mmap)
cargo bench --bench core -- "search/.*/1048576"      # 1MB
cargo bench --bench core -- "search/.*/16777216"     # 16MB
cargo bench --bench core -- "search/.*/67108864"     # 64MB
cargo bench --bench search_simd -- "count_error_405mb"
cargo bench --bench search_simd -- "count_error_2gb"

# Headless search (matches README claims)
target/release/jet --headless --search ERROR benchdata/bench_search.txt

# Keystroke latency
target/release/jet --bench-latency 1000
cargo bench --bench render_frame -- --sample-size 10

# RSS measurement (Linux only, requires procfs)
cargo bench --bench rss_bench

# Tree-sitter incremental parse
cargo bench --bench treesitter -- --sample-size 10

# Core rope operations
cargo bench --bench core -- "rope/.*"
cargo bench --bench core -- "unicode/.*"
cargo bench --bench core -- "stats/.*"
```

## Memory and Large Files (RSS)

The `rss_bench` benchmark spawns the jet binary in headless mode, reads
`/proc/<pid>/status` to get RSS, then kills the process. It uses sparse files
so no actual 10GB disk write is required.

```sh
cargo bench --bench rss_bench
```

For manual verification:
```sh
# Generate sparse fixtures
scripts/gen_fixtures.sh

# Record RSS with the platform profiler
target/release/jet --headless benchdata/bench_10gb.txt &
PID=$!; sleep 1; cat /proc/$PID/status | grep VmRSS; kill $PID
```

5 grammars compiled in (Rust, Python, JSON, JavaScript, Bash). Viewport
highlighting operates on the visible window plus 32-line overscan, not on a
whole large file at open.
