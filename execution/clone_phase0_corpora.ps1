# Shallow-clone the phase0 battery repos for STEL golden replay, for Windows
# jobs that run PowerShell only. The corpus list is read from
# clone_phase0_corpora.sh so there is one list to maintain.
# Idempotent: skips any corpus whose marker file already exists.
param([string]$Root = 'tests/fixtures/phase0-corpus')
$ErrorActionPreference = 'Stop'

$list = Join-Path $PSScriptRoot 'clone_phase0_corpora.sh'
$pattern = '^clone_if_missing "\$\{ROOT\}/(?<dir>[^"]+)" (?<url>\S+) (?<marker>\S+)$'
$corpora = Get-Content -LiteralPath $list | ForEach-Object {
  if ($_ -match $pattern) { [pscustomobject]@{ Dir = $Matches.dir; Url = $Matches.url; Marker = $Matches.marker } }
}
if (-not $corpora) { throw "no corpora found in $list" }

New-Item -ItemType Directory -Force -Path $Root | Out-Null
foreach ($corpus in $corpora) {
  $dir = Join-Path $Root $corpus.Dir
  if (Test-Path -LiteralPath (Join-Path $dir $corpus.Marker)) {
    Write-Output "phase0 corpus ready: $dir ($($corpus.Marker))"
    continue
  }
  $cloned = $false
  for ($attempt = 1; $attempt -le 3 -and -not $cloned; $attempt++) {
    Write-Output "cloning $($corpus.Url) -> $dir (attempt $attempt)"
    git clone --depth 1 $corpus.Url $dir
    if ($LASTEXITCODE -eq 0) { $cloned = $true; break }
    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds (3 * $attempt)
  }
  if (-not $cloned) { throw "could not clone $($corpus.Url) after 3 attempts" }
}
