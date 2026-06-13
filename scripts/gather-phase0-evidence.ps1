# scripts/gather-phase0-evidence.ps1
# In-repo Phase 0 §12A evidence collection (no external sf-bench required).
#Requires -Version 5.1
param(
    [string]$RepoRoot = ""
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = if ($RepoRoot) { $RepoRoot } else { Resolve-Path (Join-Path $ScriptDir "..") }
Set-Location $RepoRoot

Write-Host "=== Phase 0 in-repo evidence gather ==="

# A-005 / schema bytes (2 runs for repeatability proxy)
& "$ScriptDir\measure-schema-bytes.ps1" | Out-Host
& "$ScriptDir\measure-schema-bytes.ps1" -OutFile "docs/research/A-005-schema-bytes-run2.json" | Out-Host

# Rust unit evidence
cargo test -p symforge --lib -- surface_probe --test-threads=1
if ($LASTEXITCODE -ne 0) { throw "surface_probe tests failed" }
cargo test -p symforge --lib -- competent_manual --test-threads=1
if ($LASTEXITCODE -ne 0) { throw "competent_manual tests failed" }

# Release shakedown (A-003): MCP initialize + tools/list on compact surface
$fixture = Join-Path $RepoRoot "tests/fixtures/compression_ratio/rust"
$bin = Join-Path $RepoRoot "target/release/symforge.exe"
if (-not (Test-Path $bin)) {
    Write-Host "Building release binary..."
    cargo build --release -p symforge 2>&1 | Out-Host
}
$env:SYMFORGE_NO_DAEMON = "1"
$env:RUST_LOG = "off"
$env:SYMFORGE_SURFACE = "compact"
$lines = @(
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"phase0-gather","version":"0.1"}}}',
    '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}',
    '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
)
$shakedown = $lines | & $bin 2>$null
$shakedownPath = Join-Path $RepoRoot "docs/research/A-003-mcp-shakedown.jsonl"
$shakedown | Set-Content -Path $shakedownPath -Encoding UTF8
Write-Host "Wrote $shakedownPath"

# H1 preflight summary from A-005 artifacts
$a005 = Get-Content "docs/research/A-005-schema-bytes.json" -Raw | ConvertFrom-Json
$a005run2 = Get-Content "docs/research/A-005-schema-bytes-run2.json" -Raw | ConvertFrom-Json
$variance = if ($a005.surfaces.compact.schemaBytes -gt 0) {
    [math]::Abs($a005run2.surfaces.compact.schemaBytes - $a005.surfaces.compact.schemaBytes) / $a005.surfaces.compact.schemaBytes * 100
} else { 0 }

$preflight = [ordered]@{
    gatheredAt = (Get-Date).ToUniversalTime().ToString("o")
    source = "in-repo symforge (no external sf-bench)"
    H1 = @{ schemaBytes = $a005.surfaces.compact.schemaBytes; budget = 5000; pass = $a005.h1Pass }
    measurementRepeatability = @{
        run1Bytes = $a005.surfaces.compact.schemaBytes
        run2Bytes = $a005run2.surfaces.compact.schemaBytes
        variancePercent = $variance
        pass = ($variance -le 2)
    }
    fullSurfaceBytes = $a005.surfaces.full.schemaBytes
    shakedownPath = $shakedownPath
}
$preflightPath = Join-Path $RepoRoot "docs/research/G-005-inrepo-preflight.json"
$preflight | ConvertTo-Json -Depth 5 | Set-Content -Path $preflightPath -Encoding UTF8
Write-Host "Wrote $preflightPath"

# A-001 session_net battery (2 runs)
$binDebug = Join-Path $RepoRoot "target/debug/symforge.exe"
if (-not (Test-Path $binDebug)) {
    Write-Host "Building debug binary for battery..."
    cargo build -p symforge 2>&1 | Out-Host
}
node (Join-Path $ScriptDir "phase0-mcp-battery.cjs") $binDebug (Join-Path $RepoRoot "docs/research/A-001-tool-battery-run1.json") 2>&1 | Out-Host
node (Join-Path $ScriptDir "phase0-mcp-battery.cjs") $binDebug (Join-Path $RepoRoot "docs/research/A-001-tool-battery-run2.json") 2>&1 | Out-Host

# A-028 golden routes
node (Join-Path $ScriptDir "seed-routes-golden.cjs") 2>&1 | Out-Host
node (Join-Path $ScriptDir "validate-routes-golden.cjs") 2>&1 | Out-Host

Write-Host "=== Done ==="
