# Verify terminal-commander SymForge findings fixes locally before git.
# Runs release build + fmt/clippy + targeted + full test gates.
$ErrorActionPreference = "Stop"
Set-Location (Split-Path $PSScriptRoot -Parent)

Write-Host "== symforge TC findings verification ==" -ForegroundColor Cyan

Write-Host "`n[1/6] cargo fmt --check" -ForegroundColor Yellow
cargo fmt --check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n[2/6] cargo check" -ForegroundColor Yellow
cargo check
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n[3/6] cargo clippy --all-targets -- -D warnings" -ForegroundColor Yellow
cargo clippy --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n[4/6] cargo build --release" -ForegroundColor Yellow
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$bin = Join-Path (Resolve-Path "." ) "target\release\symforge.exe"
if (-not (Test-Path $bin)) {
    Write-Error "Release binary missing at $bin"
    exit 1
}
$ver = & $bin --version 2>&1
Write-Host "Release binary: $bin" -ForegroundColor Green
Write-Host "Version: $ver" -ForegroundColor Green

Write-Host "`n[5/6] targeted regression tests (TC findings)" -ForegroundColor Yellow
cargo test --test impact_body_diff -- --test-threads=1
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --test edit_plan_symbol_line -- --test-threads=1
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test symbol_body_bytes --lib -- --test-threads=1
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`n[6/6] full test suite (single-threaded)" -ForegroundColor Yellow
cargo test --all-targets -- --test-threads=1
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "`nAll verification gates passed." -ForegroundColor Green
