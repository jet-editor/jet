$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

Write-Host "== environment =="
rustc --version
cargo --version

Write-Host "== build release =="
cargo build --release --locked

Write-Host "== compile benches =="
cargo bench --no-run --locked

$Jet = Join-Path $Root "target\release\jet.exe"
if (Test-Path $Jet) {
    Write-Host "== headless smoke =="
    & $Jet --headless --quit README.md
    Write-Host "== latency sample =="
    & $Jet --headless --bench-latency 200
}

Write-Host "Benchmark harness ready. Run individual benches with:"
Write-Host "  cargo bench --bench startup -- --sample-size 10"
