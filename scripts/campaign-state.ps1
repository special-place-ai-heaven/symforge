#Requires -Version 5.1
<#
.SYNOPSIS
  Print the live campaign state a handover document must NOT hand-write.

.DESCRIPTION
  Handover docs rot because volatile facts get typed into them. Reading
  specs/024-optimization-backlog/HANDOVER-2026-08-04.md on 2026-08-05 turned up
  three claims that had already gone stale:

    - "origin/main is at 506fe30"        -> was 78adcf3 (release-please cut 8.22.0)
    - "aap-embedder-reverse-asks.md is UNCOMMITTED"  -> landed in #509
    - "No campaign worktrees exist"      -> true when written, not when read

  None of those are documentation failures. They are facts that should never
  have been typed by hand. This script prints them from the source of truth, so
  a handover can carry durable knowledge (protocol, roster, measurements,
  gotchas) and cite this command for everything that moves.

.EXAMPLE
  pwsh scripts/campaign-state.ps1
  pwsh scripts/campaign-state.ps1 -Json    # machine-readable, for a bootstrap prompt
#>
param(
    [switch]$Json,
    [string]$RepoRoot = ""
)

$ErrorActionPreference = "Stop"
if (-not $RepoRoot) {
    $ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
    $RepoRoot = (Resolve-Path (Join-Path $ScriptDir "..")).Path
}
Push-Location $RepoRoot
try {
    git fetch origin --quiet 2>$null

    $originMain = (git rev-parse --short origin/main 2>$null)
    $originSubj = (git log -1 --format=%s origin/main 2>$null)
    $localBranch = (git rev-parse --abbrev-ref HEAD 2>$null)
    $ahead, $behind = (git rev-list --left-right --count "HEAD...origin/main" 2>$null) -split '\s+'

    # Worktrees, excluding the bare/main record noise.
    $worktrees = @()
    foreach ($line in (git worktree list 2>$null)) {
        if ($line -match '^(\S+)\s+(\S+)\s+\[(.+)\]$') {
            $worktrees += [ordered]@{ path = $Matches[1]; head = $Matches[2]; branch = $Matches[3] }
        }
    }

    # Only report files that genuinely differ from origin/main. Untracked files
    # that are byte-identical to main are stale-branch noise, not pending work --
    # that distinction is exactly what cost time reading the last handover.
    $pending = @()
    foreach ($line in (git status --porcelain --untracked-files=all 2>$null)) {
        if ($line.Length -lt 4) { continue }
        $code = $line.Substring(0, 2)
        $path = $line.Substring(3).Trim('"')
        git cat-file -e "origin/main:$path" 2>$null
        $onMain = ($LASTEXITCODE -eq 0)
        $state = if (-not $onMain) { "new" }
        else {
            $same = $false
            try {
                $a = (git show "origin/main:$path" 2>$null) -join "`n"
                $b = (Get-Content -Raw -LiteralPath $path -ErrorAction SilentlyContinue)
                if ($null -ne $b) { $same = (($a -replace "`r", "") -eq ($b -replace "`r", "").TrimEnd("`n")) }
            } catch {}
            if ($same) { "same-as-main" } else { "differs" }
        }
        if ($state -ne "same-as-main") { $pending += [ordered]@{ code = $code.Trim(); path = $path; state = $state } }
    }

    $prs = @()
    try {
        $raw = gh pr list --state open --json number,title,headRefName,statusCheckRollup 2>$null
        if ($raw) {
            foreach ($pr in ($raw | ConvertFrom-Json)) {
                $checks = @($pr.statusCheckRollup)
                $failing = @($checks | Where-Object { $_.conclusion -and $_.conclusion -notin @("SUCCESS", "NEUTRAL", "SKIPPED") }).Count
                $pending_c = @($checks | Where-Object { -not $_.conclusion }).Count
                $prs += [ordered]@{
                    number = $pr.number; title = $pr.title; branch = $pr.headRefName
                    ci = if ($checks.Count -eq 0) { "no checks" } elseif ($failing -gt 0) { "$failing FAILING" } elseif ($pending_c -gt 0) { "$pending_c pending" } else { "green" }
                }
            }
        }
    } catch {}

    # Read the version from origin/main, NOT the local checkout: a drafting
    # branch that is 40+ commits behind reports a version nobody is running.
    function Get-CargoVersion([string]$rev) {
        try {
            $toml = if ($rev) { git show "${rev}:Cargo.toml" 2>$null } else { Get-Content (Join-Path $RepoRoot "Cargo.toml") }
            foreach ($l in $toml) { if ($l -match '^version\s*=\s*"(.+)"') { return $Matches[1] } }
        } catch {}
        return "unknown"
    }
    $version = Get-CargoVersion "origin/main"
    $localVersion = Get-CargoVersion $null

    $state = [ordered]@{
        generatedAt = (Get-Date).ToUniversalTime().ToString("o")
        cargoVersion = $version
        originMain = [ordered]@{ sha = $originMain; subject = $originSubj }
        localBranch = [ordered]@{ name = $localBranch; ahead = $ahead; behind = $behind }
        worktrees = $worktrees
        pendingChanges = $pending
        openPRs = $prs
    }

    if ($Json) { $state | ConvertTo-Json -Depth 6; return }

    Write-Host "campaign state  (generated $($state.generatedAt))" -ForegroundColor Cyan
    Write-Host ("  version        : {0}" -f $version)
    Write-Host ("  origin/main    : {0}  {1}" -f $originMain, $originSubj)
    Write-Host ("  local branch   : {0}  (ahead {1}, behind {2})" -f $localBranch, $ahead, $behind)
    Write-Host "  worktrees      :"
    foreach ($w in $worktrees) { Write-Host ("      {0}  [{1}]  {2}" -f $w.path, $w.branch, $w.head) }
    if ($prs.Count) {
        Write-Host "  open PRs       :"
        foreach ($p in $prs) { Write-Host ("      #{0}  {1}  ({2})  {3}" -f $p.number, $p.branch, $p.ci, $p.title) }
    } else { Write-Host "  open PRs       : none" }
    if ($pending.Count) {
        Write-Host "  pending changes (differ from origin/main):"
        foreach ($p in $pending) { Write-Host ("      {0,-4} {1}  [{2}]" -f $p.code, $p.path, $p.state) }
    } else { Write-Host "  pending changes: none" }
}
finally { Pop-Location }
