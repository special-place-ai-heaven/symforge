# SFBENCH-1.0 execution runbook

This runbook executes the frozen SymForge 8.14.0 benchmark without modifying
the SymForge checkout, source mirrors, fixtures, frozen inputs, or prior
artifacts. Run every PowerShell block with PowerShell 7 from the repository
root. Never add cleanup commands. A path collision is a reason to choose a new
campaign ID, not to delete evidence.

Sources of truth:

- [benchmark protocol](../../docs/dogfood/2026-07-12-symforge-8.14.0-full-surface-benchmark-protocol.md)
- [cases](./cases.json), [campaign config](./campaign.config.json), and
  [corpus lock](./corpus.lock.json)
- [asset lock](./assets.lock.json) and
  [frozen baseline patches](./baseline-patches.json)
- [adjudication specification](./adjudication.md),
  [executable adjudicator](./adjudicate_results.py), and
  [happy-v2 human decisions](./happy-v2-evaluator.json)

`case_complete` means capture completed. It does not mean the result is
correct or economical. Only adjudicated, correctness-valid evidence may enter
token-savings totals.

## 0. Establish paths and safety policy

Prerequisites are PowerShell 7, Git, `uv`, the asset-locked `symforge`
executable on `PATH`, and enough disk for independent clones. Network is
allowed only while resuming the corpus. Disable it for measured runs. Do not
print environment variables, auth state, credentials, raw secrets, or
unsanitized model output.

```powershell
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true

$ProjectRoot = [IO.Path]::GetFullPath((git rev-parse --show-toplevel).Trim())
$BenchRoot = [IO.Path]::GetFullPath('C:\AI_STUFF\BENCHMARKS\symforge-8.14.0-surface')
$ProjectPrefix = $ProjectRoot.TrimEnd('\') + [IO.Path]::DirectorySeparatorChar
if ($BenchRoot.Equals($ProjectRoot, [StringComparison]::OrdinalIgnoreCase) -or
    $BenchRoot.StartsWith($ProjectPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Benchmark root must be outside the SymForge checkout.'
}
Set-Location -LiteralPath $ProjectRoot

$BenchDir = Join-Path $ProjectRoot 'research/full-surface-benchmark'
$SetupCorpus = Join-Path $BenchDir 'setup_corpus.ps1'
$FixtureGenerator = Join-Path $BenchDir 'fixture_generator.py'
$DirectRunner = Join-Path $BenchDir 'direct_case_runner.py'
$NonToolRunner = Join-Path $BenchDir 'non_tool_surface_smoke.py'
$ClaudeRunner = Join-Path $BenchDir 'claude_task_runner.py'
$Harness = Join-Path $BenchDir 'mcp_harness.py'
$Cases = Join-Path $BenchDir 'cases.json'
$Campaign = Join-Path $BenchDir 'campaign.config.json'
$CorpusLock = Join-Path $BenchDir 'corpus.lock.json'
$AssetLock = Join-Path $BenchDir 'assets.lock.json'
$BaselinePatches = Join-Path $BenchDir 'baseline-patches.json'
$FixtureRoot = Join-Path $BenchRoot 'fixtures/control-v1'
$RunRoot = Join-Path $BenchRoot 'runs'
$WorkRoot = Join-Path $BenchRoot 'work'
$ArtifactRoot = Join-Path $BenchRoot 'artifacts'
$CampaignId = 'sfbench-' + (Get-Date -Format 'yyyyMMddTHHmmss')

$FrozenInputs = @(
    '--benchmark-root'
    $BenchRoot
    '--fixture-root'
    $FixtureRoot
    '--cases'
    $Cases
    '--campaign'
    $Campaign
    '--corpus-lock'
    $CorpusLock
    '--asset-lock'
    $AssetLock
    '--baseline-patches'
    $BaselinePatches
    '--server'
    'symforge'
)
```

## 1. Resume or create the frozen corpus

The same command is safe after an interruption. `-Resume` verifies every
existing mirror's origin, commit, clean state, tracked blobs, and Git object
connectivity, then clones only missing mirrors. It fails closed on a partial or
drifted mirror.

```powershell
pwsh -NoProfile -File $SetupCorpus `
    -BenchRoot $BenchRoot `
    -ProjectRoot $ProjectRoot `
    -Resume

$env:GIT_CONFIG_NOSYSTEM = '1'
$env:GIT_CONFIG_GLOBAL = Join-Path $BenchRoot 'gitconfig.empty'
$env:GIT_TEMPLATE_DIR = Join-Path $BenchRoot 'git-template.empty'
$env:GIT_LFS_SKIP_SMUDGE = '1'
```

After this command succeeds, disable network access for the measurement
processes.

## 2. Generate and verify the controlled fixture

The generator accepts only a new directory. The wrapper below generates once
and otherwise leaves the existing fixture untouched. `validate` independently
checks the fixture repositories, dirty worktree, oracle hashes, corpus pins,
tokenizer locks, and SUT identity.

```powershell
if (-not (Test-Path -LiteralPath $FixtureRoot)) {
    uv run $FixtureGenerator $FixtureRoot
}

uv run $DirectRunner validate @FrozenInputs --require-asset-lock
```

Do not regenerate a fixture after measured runs begin. A validation failure
stops the campaign.

## 3. Reproduce the baseline patch freeze

Generate a fresh external candidate, scratch-apply every patch, and compare it
with the frozen file. Keep the candidate as evidence. Never overwrite a frozen
asset in place.

```powershell
$PatchCandidate = Join-Path $ArtifactRoot "$CampaignId-baseline-patches.candidate.json"
if (Test-Path -LiteralPath $PatchCandidate) {
    throw "Candidate already exists: $PatchCandidate"
}

uv run $DirectRunner freeze-baseline-patches @FrozenInputs `
    --require-asset-lock `
    --output $PatchCandidate

$FrozenPatchHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $BaselinePatches).Hash
$CandidatePatchHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $PatchCandidate).Hash
if ($CandidatePatchHash -cne $FrozenPatchHash) {
    throw 'Reproduced baseline patches differ from the frozen asset. Stop.'
}
```

## 4. Validate every asset lock entry

The first block checks all core and auxiliary assets plus the SUT. The runner
command then performs semantic validation. Neither command prints asset
contents.

```powershell
$Lock = Get-Content -Raw -LiteralPath $AssetLock | ConvertFrom-Json
$Entries = @($Lock.core_assets) + @($Lock.auxiliary_assets)
foreach ($Entry in $Entries) {
    $Path = switch -CaseSensitive ([string] $Entry.path) {
        'corpus-manifest.json' { Join-Path $BenchRoot 'corpus-manifest.json'; break }
        'oracle.json' { Join-Path $FixtureRoot 'oracle.json'; break }
        default { Join-Path $ProjectRoot ([string] $Entry.path) }
    }
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Locked asset is missing: $($Entry.path)"
    }
    $Actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
    if ($Actual -cne ([string] $Entry.sha256).ToLowerInvariant()) {
        throw "Locked asset hash mismatch: $($Entry.path)"
    }
}

$Sut = (Get-Command ([string] $Lock.system_under_test.path) `
    -CommandType Application -ErrorAction Stop).Source
$SutHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $Sut).Hash.ToLowerInvariant()
if ($SutHash -cne ([string] $Lock.system_under_test.sha256).ToLowerInvariant()) {
    throw 'System-under-test hash differs from assets.lock.json.'
}
Write-Host "assets.lock: PASS ($($Entries.Count) files plus SUT)"

uv run $Harness self-test
uv run $DirectRunner self-test
uv run $NonToolRunner self-test
uv run $ClaudeRunner self-test
uv run $DirectRunner validate @FrozenInputs --require-asset-lock
```

## 5. Resolve the dry-run plan

The frozen manifest has 115 cases: 37 happy, 38 adverse, 36 control, and 4
stateful. The current direct runner can execute 108 through stdio or a declared
stdio parity shadow. Seven require setup machinery that is not implemented.

```powershell
$PlanPath = Join-Path $ArtifactRoot "$CampaignId-dry-run.json"
if (Test-Path -LiteralPath $PlanPath) {
    throw "Dry-run artifact already exists: $PlanPath"
}
$PlanText = & uv run $DirectRunner dry-run @FrozenInputs `
    --require-asset-lock `
    --all `
    --allow-stdio-shadow
$PlanText | Set-Content -LiteralPath $PlanPath -Encoding utf8NoBOM
$Plan = ($PlanText -join [Environment]::NewLine) | ConvertFrom-Json

$ExpectedBlocked = @(
    'SF-checkpoint_now-002'
    'SF-checkpoint_now-003'
    'SF-health-003'
    'SF-index_folder-003'
    'SF-search_files-003'
    'SF-status-003'
    'SF-symforge_retrieve-002'
)
$ActualBlocked = @(
    $Plan.selected_cases |
        Where-Object { -not $_.executable } |
        ForEach-Object { [string] $_.id } |
        Sort-Object
)
if ($Plan.executable_count -ne 108 -or
    $Plan.unsupported_count -ne 7 -or
    (Compare-Object ($ExpectedBlocked | Sort-Object) $ActualBlocked)) {
    throw 'Dry-run support inventory differs from the frozen expectation.'
}
```

The seven unsupported setup actions are:

| Case | Missing setup action |
|---|---|
| `SF-checkpoint_now-002` | `runtime.snapshot.prepare_path_collision` |
| `SF-checkpoint_now-003` | `runtime.snapshot.prepare_corrupt_copy` |
| `SF-health-003` | `runtime.health.mutate_between_requests` |
| `SF-index_folder-003` | `runtime.daemon.start_with_home_project` |
| `SF-search_files-003` | `runtime.frecency.seed_fixture_paths` |
| `SF-status-003` | `runtime.calibration.seed_durable_samples` |
| `SF-symforge_retrieve-002` | `runtime.ccr.create_foreign_session_handle` |

HTTP is also blocked. `campaign.config.json` requires an official-SDK parity
runner, while `direct_case_runner.py` and `mcp_harness.py` implement stdio only.
There are 39 primary HTTP cases. `--allow-stdio-shadow` executes their declared
stdio shadows; it is not HTTP evidence and must not be reported as such. Without
that flag, the primary HTTP cases remain unsupported.

## 6. Run direct happy, adverse, control, and stateful cohorts

The helper selects only cases marked executable by the dry-run plan. This keeps
the seven setup blockers explicit without manufacturing `case_error` rows. It
uses the frozen per-case repetitions and the competent recipe baseline.

```powershell
function Invoke-DirectCohort {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [ValidateSet('happy', 'adverse', 'control', 'stateful')]
        [string] $Kind
    )

    $CohortPlanText = & uv run $DirectRunner dry-run @FrozenInputs `
        --require-asset-lock `
        --case-kind $Kind `
        --allow-stdio-shadow
    $CohortPlan = ($CohortPlanText -join [Environment]::NewLine) | ConvertFrom-Json
    $Selectors = @(
        foreach ($Case in $CohortPlan.selected_cases) {
            if ($Case.executable) {
                '--case-id'
                [string] $Case.id
            }
        }
    )
    if ($Selectors.Count -eq 0) {
        throw "No executable $Kind cases."
    }

    $RunId = "$CampaignId-direct-$Kind"
    $Output = Join-Path $RunRoot "$RunId.jsonl"
    $WorkParent = Join-Path $WorkRoot $RunId
    if ((Test-Path -LiteralPath $Output) -or
        (Test-Path -LiteralPath $WorkParent)) {
        throw "Choose a new campaign ID; $Kind output already exists."
    }
    $RunArguments = @('run') + $FrozenInputs + @(
        '--require-asset-lock'
        '--allow-stdio-shadow'
        '--run-id'
        $RunId
        '--output'
        $Output
        '--work-parent'
        $WorkParent
        '--with-baseline'
    ) + $Selectors
    & uv run $DirectRunner @RunArguments

    $Terminal = @(
        Get-Content -LiteralPath $Output |
            ForEach-Object { $_ | ConvertFrom-Json } |
            Where-Object { $_.record_type -eq 'campaign_complete' }
    )
    if ($Terminal.Count -ne 1 -or
        ($Terminal[0].completed_trials + $Terminal[0].failed) -ne
            $Terminal[0].scheduled_trials) {
        throw "$Kind cohort has an incomplete terminal count. Preserve evidence and stop."
    }
    if ($Terminal[0].failed -gt 0) {
        Write-Warning (
            "$Kind captured $($Terminal[0].failed) failed trial(s). " +
            'These are evidence, not permission to relabel the campaign incomplete.'
        )
    }
}

Invoke-DirectCohort -Kind happy
Invoke-DirectCohort -Kind adverse
Invoke-DirectCohort -Kind control
Invoke-DirectCohort -Kind stateful
```

The first three calls are the requested happy, adverse, and control cohorts.
The stateful call runs the one currently executable stateful case so the paired
gate can distinguish completed evidence from the three stateful setup blockers.

## 7. Smoke-test resources, templates, and prompts

Use a fresh independent fixture clone because starting SymForge may create
`.symforge/` state. Never point the smoke runner at the immutable fixture.

```powershell
function New-IndependentClone {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Source,
        [Parameter(Mandatory)] [string] $Destination,
        [Parameter(Mandatory)] [string] $Commit
    )
    if (Test-Path -LiteralPath $Destination) {
        throw "Clone destination already exists: $Destination"
    }
    git clone --local --no-hardlinks --no-checkout $Source $Destination
    git -C $Destination config core.autocrlf false
    git -C $Destination config core.eol lf
    git -C $Destination checkout --detach $Commit
    if (git -C $Destination status --porcelain=v1) {
        throw "Fresh clone is dirty: $Destination"
    }
}

$OraclePath = Join-Path $FixtureRoot 'oracle.json'
$Oracle = Get-Content -Raw -LiteralPath $OraclePath | ConvertFrom-Json
$SmokeSource = Join-Path $FixtureRoot ([string] $Oracle.paths.clean_repository)
$SmokeCommit = [string] $Oracle.repositories.clean.head
$SmokeSymbol = @($Oracle.symbols.Rust | Where-Object { $_.name -eq 'sfbench_leaf' })[0]
$SmokeRunId = "$CampaignId-non-tool-smoke"
$SmokeRepo = Join-Path $WorkRoot $SmokeRunId
$SmokeOutput = Join-Path $RunRoot "$SmokeRunId.jsonl"
if (Test-Path -LiteralPath $SmokeOutput) {
    throw "Smoke output already exists: $SmokeOutput"
}
New-IndependentClone -Source $SmokeSource -Destination $SmokeRepo -Commit $SmokeCommit

uv run $NonToolRunner run `
    --repo $SmokeRepo `
    --output $SmokeOutput `
    --fixture-path ([string] $SmokeSymbol.path) `
    --fixture-symbol ([string] $SmokeSymbol.name) `
    --fixture-kind ([string] $SmokeSymbol.kind) `
    --server symforge `
    --run-id $SmokeRunId `
    --case-id 'SF-non-tool-surface-001'
```

This covers all six static resources, four resource templates, and seven
prompts in one isolated stdio session. Keep it separate from tool economics.

## 8. Gate neutral and forced Claude trials

Paid paired trials are disabled during the direct phase. Run them only after:

1. all executable direct cohorts and the non-tool smoke complete;
2. the seven setup blockers and HTTP limitation are accepted as explicit
   limitations, not silently dropped;
3. an operator records the maximum approved campaign cost;
4. Claude Code reports exactly version `2.1.207`.

The natural baseline and natural SymForge arms use the same derived neutral
prompt and fresh independent clones. The forced prompt is SymForge-only and is
reported separately. Never pool forced results with the natural paired
headline.

```powershell
$MaxBudgetUsd = Read-Host 'Enter the approved maximum USD per Claude trial'
if ($MaxBudgetUsd -notmatch '^(?:0|[1-9][0-9]*)(?:\.[0-9]+)?$' -or
    [decimal]::Parse(
        $MaxBudgetUsd,
        [Globalization.CultureInfo]::InvariantCulture
    ) -le 0) {
    throw 'The approved maximum must be a positive decimal.'
}

$DirectRunFiles = @(
    'happy', 'adverse', 'control', 'stateful' |
        ForEach-Object { Join-Path $RunRoot "$CampaignId-direct-$_.jsonl" }
)
foreach ($Path in $DirectRunFiles) {
    $Complete = @(
        Get-Content -LiteralPath $Path |
            ForEach-Object { $_ | ConvertFrom-Json } |
            Where-Object { $_.record_type -eq 'campaign_complete' }
    )
    if ($Complete.Count -ne 1 -or $Complete[0].failed -ne 0) {
        throw "Direct gate failed: $Path"
    }
}
if (-not (Test-Path -LiteralPath $SmokeOutput -PathType Leaf)) {
    throw 'Non-tool smoke evidence is missing.'
}
if ((claude --version) -notmatch '2\.1\.207') {
    throw 'Claude Code version must be 2.1.207.'
}

$CaseId = 'SF-get_repo_map-001'
$Case = @(
    (Get-Content -Raw -LiteralPath $Cases | ConvertFrom-Json).cases |
        Where-Object { $_.id -eq $CaseId }
)[0]
$PairId = "$CampaignId-paired-get-repo-map"
$PairSource = Join-Path (Join-Path $BenchRoot 'sources') ([string] $Case.repo)
$BaselineRepo = Join-Path $WorkRoot "$PairId-neutral-baseline"
$SymForgeRepo = Join-Path $WorkRoot "$PairId-neutral-symforge"
$ForcedRepo = Join-Path $WorkRoot "$PairId-forced-symforge"
New-IndependentClone -Source $PairSource -Destination $BaselineRepo -Commit ([string] $Case.commit)
New-IndependentClone -Source $PairSource -Destination $SymForgeRepo -Commit ([string] $Case.commit)
New-IndependentClone -Source $PairSource -Destination $ForcedRepo -Commit ([string] $Case.commit)

$BaselineOutput = Join-Path $RunRoot "$PairId-neutral-baseline.jsonl"
$SymForgeOutput = Join-Path $RunRoot "$PairId-neutral-symforge.jsonl"
$ForcedOutput = Join-Path $RunRoot "$PairId-forced-symforge.jsonl"
$ClaudeCommon = @(
    'run'
    '--case'
    $Cases
    '--case-id'
    $CaseId
    '--campaign'
    $Campaign
    '--corpus-manifest'
    (Join-Path $BenchRoot 'corpus-manifest.json')
    '--benchmark-root'
    $BenchRoot
    '--claude'
    'claude'
    '--symforge'
    'symforge'
    '--max-budget-usd'
    $MaxBudgetUsd
)

# These three commands make no paid call and write no artifact.
uv run $ClaudeRunner @ClaudeCommon --arm baseline --prompt-mode neutral `
    --repo $BaselineRepo --output $BaselineOutput --dry-run
uv run $ClaudeRunner @ClaudeCommon --arm symforge --prompt-mode neutral `
    --repo $SymForgeRepo --output $SymForgeOutput --dry-run
uv run $ClaudeRunner @ClaudeCommon --arm symforge --prompt-mode forced `
    --repo $ForcedRepo --output $ForcedOutput --dry-run
```

Stop here until an operator approves the recorded cap. In the same PowerShell
session, use the explicit confirmation gate below. These are the only paid
commands in this runbook.

```powershell
$PaidApproval = Read-Host 'Type RUN PAID PAIRED TRIALS to approve the recorded cap'
if ($PaidApproval -cne 'RUN PAID PAIRED TRIALS') {
    throw 'Paid paired gate remains closed.'
}

# Paid calls begin here. Existing auth is inherited privately; never put it in arguments.
uv run $ClaudeRunner @ClaudeCommon --arm baseline --prompt-mode neutral `
    --repo $BaselineRepo --output $BaselineOutput
uv run $ClaudeRunner @ClaudeCommon --arm symforge --prompt-mode neutral `
    --repo $SymForgeRepo --output $SymForgeOutput
uv run $ClaudeRunner @ClaudeCommon --arm symforge --prompt-mode forced `
    --repo $ForcedRepo --output $ForcedOutput

function Get-ClaudeTrial {
    param([Parameter(Mandatory)] [string] $Path)
    $Trial = @(
        Get-Content -LiteralPath $Path |
            ForEach-Object { $_ | ConvertFrom-Json } |
            Where-Object { $_.record_type -eq 'claude_task_trial' }
    )
    if ($Trial.Count -ne 1) {
        throw "Expected one Claude trial: $Path"
    }
    return $Trial[0]
}

$BaselineTrial = Get-ClaudeTrial $BaselineOutput
$SymForgeTrial = Get-ClaudeTrial $SymForgeOutput
$ForcedTrial = Get-ClaudeTrial $ForcedOutput
if ($BaselineTrial.prompt_mode -ne 'neutral' -or
    $SymForgeTrial.prompt_mode -ne 'neutral' -or
    $BaselineTrial.policy.task_prompt_sha256 -cne
        $SymForgeTrial.policy.task_prompt_sha256 -or
    $BaselineTrial.policy.model -cne $SymForgeTrial.policy.model) {
    throw 'Natural paired-arm prompt/model parity failed.'
}
if ($ForcedTrial.prompt_mode -ne 'forced' -or
    $ForcedTrial.policy.task_prompt_sha256 -ceq
        $SymForgeTrial.policy.task_prompt_sha256) {
    throw 'Forced workflow was not recorded separately.'
}
```

Expand paired cases and seeds only after the representative gate passes and the
campaign-wide cost cap is recorded.

## 9. Build the adjudication packet and coding-agent report

`adjudicate_results.py` validates artifact identity and structure, associates
ordered repeated trial windows, computes token/latency views, and joins explicit
human decisions. It deliberately never treats `case_complete` as correctness.
The evaluator decision file is a JSON object using schema
`SFBENCH-adjudication-decisions-1`; every pass or fail needs evidenced checks.

The executed happy campaign uses `happy-v2-evaluator.json`. Reproduce its
correctness-adjusted summary with new external output paths:

```powershell
$HappyArtifact = Join-Path $RunRoot 'formal-happy-v2-20260712.jsonl'
$Evaluator = Join-Path $BenchDir 'happy-v2-evaluator.json'
$SummaryJson = Join-Path $ArtifactRoot "$CampaignId-adjudicated.json"
$SummaryMarkdown = Join-Path $ArtifactRoot "$CampaignId-adjudicated.md"
foreach ($Path in @($SummaryJson, $SummaryMarkdown)) {
    if (Test-Path -LiteralPath $Path) {
        throw "Choose a new summary path: $Path"
    }
}

uv run --script (Join-Path $BenchDir 'adjudicate_results.py') summarize `
    --artifact $HappyArtifact `
    --manifest (Join-Path $ArtifactRoot 'surface-final-full.jsonl') `
    --manifest (Join-Path $ArtifactRoot 'surface-final-compact.jsonl') `
    --manifest (Join-Path $ArtifactRoot 'surface-final-meta.jsonl') `
    --cases $Cases `
    --asset-lock $AssetLock `
    --evaluator $Evaluator `
    --output-json $SummaryJson `
    --output-markdown $SummaryMarkdown
```

The optional manual handoff below remains useful for a new campaign or an
independent evaluator. Its output shape is evaluator-owned and must not be
confused with the executable summary schema.

Create a hash-only evidence index and an exact evaluator handoff:

```powershell
$EvidenceIndexPath = Join-Path $ArtifactRoot "$CampaignId-evidence-index.json"
$HandoffPath = Join-Path $ArtifactRoot "$CampaignId-adjudication-request.md"
$AdjudicationOutput = Join-Path $ArtifactRoot "$CampaignId-adjudication.jsonl"
$ReportOutput = Join-Path $ArtifactRoot "$CampaignId-report.md"
foreach ($Path in @($EvidenceIndexPath, $HandoffPath, $AdjudicationOutput, $ReportOutput)) {
    if (Test-Path -LiteralPath $Path) {
        throw "Choose a new output path; evidence already exists: $Path"
    }
}

$RunFiles = @(Get-ChildItem -LiteralPath $RunRoot -Filter "$CampaignId*.jsonl" -File)
if ($RunFiles.Count -eq 0) {
    throw 'No campaign JSONL files found.'
}
$IndexRows = @(
    foreach ($File in ($RunFiles | Sort-Object FullName)) {
        $RowCount = 0
        foreach ($Line in Get-Content -LiteralPath $File.FullName) {
            if ([string]::IsNullOrWhiteSpace($Line)) { continue }
            $Record = $Line | ConvertFrom-Json
            if ($Record.protocol -ne 'SFBENCH-1.0') {
                throw "Protocol mismatch in $($File.Name)."
            }
            $RowCount += 1
        }
        [ordered]@{
            path = $File.FullName
            sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $File.FullName).Hash.ToLowerInvariant()
            utf8_bytes = $File.Length
            jsonl_rows = $RowCount
        }
    }
)
[ordered]@{
    protocol = 'SFBENCH-1.0'
    campaign_id = $CampaignId
    files = $IndexRows
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $EvidenceIndexPath -Encoding utf8NoBOM

@"
# SFBENCH adjudication request

Read these sources before judging any result:

- Protocol: $ProjectRoot\docs\dogfood\2026-07-12-symforge-8.14.0-full-surface-benchmark-protocol.md
- Adjudication contract: $ProjectRoot\research\full-surface-benchmark\adjudication.md
- Frozen cases: $Cases
- Asset lock: $AssetLock
- Fixture oracle: $OraclePath
- Evidence index: $EvidenceIndexPath

Write normalized adjudication JSONL to: $AdjudicationOutput
Write the coding-agent report to: $ReportOutput

Fail closed on frozen identity, ordering, parser, oracle, mutation, determinism,
or accounting gaps. An efficient wrong answer is INVALID_INCORRECT. Keep
capability-only baselines N/A. Report negative token deltas with a minus sign.
Include explicit blocked entries for the seven unsupported setup cases and the
39 unmeasured primary HTTP cases. Do not treat stdio shadows as HTTP evidence.

The report must contain these exact sections:
## Executive verdict
## Environment and frozen methodology
## Portfolio economics
## Per-tool scorecard
## Findings
## Enhancement backlog
## Acceptance-test checklist
## Sanitized artifact index and reproduction
## Limitations

Score all 36 full-surface tools plus the compact/meta-only symforge facade.
For every token-negative, failed, partial, slow, or noisy case, include the
measured cost breakdown, read-only formatter counterfactual, concrete change,
acceptance case, neighboring reruns, priority, and facts that must remain.
Use NO CHANGE RECOMMENDED where no measured improvement survives correctness.
Never include raw secrets or unsanitized output.
"@ | Set-Content -LiteralPath $HandoffPath -Encoding utf8NoBOM
```

After the evaluator writes both outputs, run this fail-closed shape check:

```powershell
if (-not (Test-Path -LiteralPath $AdjudicationOutput -PathType Leaf) -or
    -not (Test-Path -LiteralPath $ReportOutput -PathType Leaf)) {
    throw 'Adjudication JSONL and report are both required.'
}
$AdjudicationRows = @(
    Get-Content -LiteralPath $AdjudicationOutput |
        Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
        ForEach-Object { $_ | ConvertFrom-Json }
)
if ($AdjudicationRows.Count -eq 0) {
    throw 'Adjudication output is empty.'
}
$RequiredRecordFields = @(
    'identity', 'artifact', 'steps', 'checks', 'correctness', 'determinism',
    'mutation', 'latency', 'tokens', 'baseline', 'economics'
)
foreach ($Row in $AdjudicationRows) {
    foreach ($Field in $RequiredRecordFields) {
        if ($Field -notin $Row.PSObject.Properties.Name) {
            throw "Adjudication row is missing $Field."
        }
    }
}

$Report = Get-Content -Raw -LiteralPath $ReportOutput
$RequiredSections = @(
    '## Executive verdict'
    '## Environment and frozen methodology'
    '## Portfolio economics'
    '## Per-tool scorecard'
    '## Findings'
    '## Enhancement backlog'
    '## Acceptance-test checklist'
    '## Sanitized artifact index and reproduction'
    '## Limitations'
)
foreach ($Section in $RequiredSections) {
    if (-not $Report.Contains($Section, [StringComparison]::Ordinal)) {
        throw "Report is missing section: $Section"
    }
}
Write-Host "Adjudication/report shape: PASS ($($AdjudicationRows.Count) rows)"
```

Preserve every artifact. If any source mirror drifts, a write escapes a
disposable clone, a secret reaches output, frozen identity changes, or the same
harness failure repeats three times, stop the affected campaign and report the
blocker. Do not repair evidence in place.
