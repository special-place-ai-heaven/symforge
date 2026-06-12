#Requires -Version 5.1
<#
.SYNOPSIS
  Measure MCP tools/list JSON byte size for SymForge v8 H1 / assumption A-005.

.DESCRIPTION
  Phase 0 stub (gap plan §12A): documents how to capture schema bytes before
  src/stel/ ships. Writes docs/research/A-005-schema-bytes.json when run from
  repo root with a working symforge binary.

  H1 gate: compact surface tools/list <= 5000 bytes UTF-8 JSON.

  Surfaces to measure (when compact stub exists):
    - full:     SYMFORGE_SURFACE=full (32-tool baseline, informational)
    - compact:  SYMFORGE_SURFACE=compact (3-tool target for v8)

  Until compact surface is implemented, this script records full-surface bytes
  and marks compact as TODO.

.EXAMPLE
  .\scripts\measure-schema-bytes.ps1
  .\scripts\measure-schema-bytes.ps1 -RepoRoot E:\project\symforge\tests\fixtures\tokio-mini
#>
param(
    [string]$RepoRoot = "",
    [string]$SymforgeBin = "",
    [string]$OutFile = "docs/research/A-005-schema-bytes.json"
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = if ($RepoRoot) { $RepoRoot } else { Resolve-Path (Join-Path $ScriptDir "..") }
$OutPath = Join-Path $RepoRoot $OutFile

function Resolve-Symforge {
    param([string]$Preferred)
    if ($Preferred -and (Test-Path $Preferred)) { return $Preferred }
    $cargo = Join-Path $RepoRoot "target/debug/symforge.exe"
    if (Test-Path $cargo) { return $cargo }
    $cargoRelease = Join-Path $RepoRoot "target/release/symforge.exe"
    if (Test-Path $cargoRelease) { return $cargoRelease }
    $cmd = Get-Command symforge -ErrorAction SilentlyContinue
    if ($cmd) { return $cmd.Source }
    return $null
}

function Measure-ToolsListBytes {
    param(
        [string]$Bin,
        [string]$Cwd,
        [string]$Surface
    )
    $env:SYMFORGE_SURFACE = $Surface
    $nodeScript = @'
const { spawn } = require("child_process");
const readline = require("readline");
const bin = process.argv[2];
const cwd = process.argv[3];
let id = 1;
const proc = spawn(bin, [], { cwd, stdio: ["pipe", "pipe", "inherit"] });
const pending = new Map();
readline.createInterface({ input: proc.stdout }).on("line", (line) => {
  if (!line.trim()) return;
  let msg; try { msg = JSON.parse(line); } catch { return; }
  if (msg.id != null && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    if (msg.error) reject(new Error(JSON.stringify(msg.error)));
    else resolve(msg.result);
  }
});
function request(method, params) {
  return new Promise((resolve, reject) => {
    const myId = id++;
    pending.set(myId, { resolve, reject });
    proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: myId, method, params }) + "\n");
    setTimeout(() => { if (pending.has(myId)) { pending.delete(myId); reject(new Error("timeout")); } }, 120000);
  });
}
(async () => {
  await request("initialize", { protocolVersion: "2024-11-05", capabilities: {}, clientInfo: { name: "measure-schema-bytes", version: "0.1" } });
  proc.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} }) + "\n");
  const list = await request("tools/list", {});
  const bytes = Buffer.byteLength(JSON.stringify(list), "utf8");
  console.log(JSON.stringify({ toolCount: (list.tools || []).length, schemaBytes: bytes }));
  proc.kill();
})().catch((e) => { console.error(e); process.exit(1); });
'@
    $temp = [System.IO.Path]::GetTempFileName() + ".cjs"
    Set-Content -Path $temp -Value $nodeScript -Encoding UTF8
    try {
        $raw = & node $temp $Bin $Cwd 2>$null
        return ($raw | ConvertFrom-Json)
    } finally {
        Remove-Item $temp -Force -ErrorAction SilentlyContinue
    }
}

$bin = Resolve-Symforge -Preferred $SymforgeBin
$measureCwd = if (Test-Path (Join-Path $RepoRoot "tests/fixtures/tokio-mini")) {
    Join-Path $RepoRoot "tests/fixtures/tokio-mini"
} else {
    $RepoRoot
}

$artifact = [ordered]@{
    measuredAt = (Get-Date).ToUniversalTime().ToString("o")
    method     = "Buffer.byteLength(JSON.stringify(tools/list result), utf8)"
    h1BudgetBytes = 5000
    symforgeBin = $bin
    repoRoot   = $measureCwd
    surfaces   = @{}
    notes      = @(
        "TODO: re-run after SYMFORGE_SURFACE=compact stub lands (Phase 0.7 / A-005).",
        "TODO: measure symforge_edit schema separately for A-025 (edit budget <= 1500 B).",
        "Battery uses schemaBytes from compare-results H1 on candidate results.json."
    )
}

if (-not $bin) {
    $artifact.status = "TODO"
    $artifact.error = "symforge binary not found; build with: cargo build -p symforge"
    Write-Warning $artifact.error
} else {
    foreach ($surface in @("full", "compact")) {
        try {
            $result = Measure-ToolsListBytes -Bin $bin -Cwd $measureCwd -Surface $surface
            $artifact.surfaces[$surface] = $result
        } catch {
            $artifact.surfaces[$surface] = @{ error = $_.Exception.Message; status = "TODO" }
        }
    }
    $compactBytes = $artifact.surfaces.compact.schemaBytes
    if ($null -ne $compactBytes) {
        $artifact.h1Pass = ($compactBytes -le 5000)
    } else {
        $artifact.h1Pass = $null
        $artifact.status = "PARTIAL"
    }
}

$outDir = Split-Path $OutPath -Parent
if (-not (Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir -Force | Out-Null }
$artifact | ConvertTo-Json -Depth 6 | Set-Content -Path $OutPath -Encoding UTF8
Write-Host "Wrote $OutPath"
if ($artifact.surfaces.full.schemaBytes) {
    Write-Host ("full surface: {0} B ({1} tools)" -f $artifact.surfaces.full.schemaBytes, $artifact.surfaces.full.toolCount)
}
if ($artifact.surfaces.compact.schemaBytes) {
    Write-Host ("compact surface: {0} B ({1} tools) H1={2}" -f $artifact.surfaces.compact.schemaBytes, $artifact.surfaces.compact.toolCount, $(if ($artifact.h1Pass) { 'PASS' } else { 'FAIL' }))
}
