#!/usr/bin/env sh
set -eu

ROOT="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/benchdata"
mkdir -p "$OUT"

write_file() {
  size="$1"
  path="$2"
  printf 'Generating %s (%s bytes)\n' "$path" "$size"
  if command -v truncate >/dev/null 2>&1; then
    truncate -s "$size" "$path" 2>/dev/null || dd if=/dev/zero of="$path" bs=1 count=0 seek="$size"
  else
    dd if=/dev/zero of="$path" bs=1M count=$((size / 1024 / 1024)) 2>/dev/null || true
  fi
}

printf 'ERROR sample line for search benchmarks\n' >"$OUT/bench_search.txt"
for count in $(seq 1 2000); do
  printf 'ERROR line %s\n' "$count" >>"$OUT/bench_search.txt"
done

write_file $((10 * 1024 * 1024)) "$OUT/bench_10mb.txt"
write_file $((100 * 1024 * 1024)) "$OUT/bench_100mb.txt"
write_file $((1024 * 1024 * 1024)) "$OUT/bench_1gb.txt"

if [ "${SKIP_10GB:-0}" != "1" ]; then
  write_file $((10 * 1024 * 1024 * 1024)) "$OUT/bench_10gb.txt"
fi

printf 'Fixtures written to %s\n' "$OUT"
