#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== environment =="
rustc --version
cargo --version

echo "== build release =="
cargo build --release --locked

echo "== compile benches =="
cargo bench --no-run --locked

JET="${ROOT}/target/release/jet"
if [ -x "$JET" ]; then
  echo "== headless smoke =="
  "$JET" --headless --quit README.md
  echo "== latency sample =="
  "$JET" --headless --bench-latency 200
fi

echo "Benchmark harness ready. Run individual benches with:"
echo "  cargo bench --bench startup -- --sample-size 10"
