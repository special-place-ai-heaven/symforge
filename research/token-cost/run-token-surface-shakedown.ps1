[CmdletBinding()]
param(
    [ValidateSet('SelfTest', 'Prepare', 'WiringCheck', 'MaterializeBaseline', 'Run', 'Grade', 'All', 'Cleanup')]
    [string]$Action = 'SelfTest',
    [ValidateRange(1, 20)]
    [int]$RunId = 1,
    [switch]$AllowRerunIncomplete,
    [ValidateSet('Pass', 'Fail')]
    [string]$OracleVerdict,
    [string[]]$OracleFailures = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:RepositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$script:FixtureRoot = 'C:\Users\rakovnik\AppData\Local\Temp\symforge-token-shakedown-a10ff102'
$script:FixtureCommit = 'a10ff102546241f1ffd49852ba4d3088c0bb8029'
$script:GoldenRoot = 'C:\Users\rakovnik\AppData\Local\Temp\symforge-token-shakedown-golden-a10ff102-a019'
$script:RawEvidenceRoot = 'C:\Users\rakovnik\AppData\Local\Temp\symforge-token-shakedown-evidence-a10ff102-a019'
$script:EvidenceRoot = Join-Path $PSScriptRoot 'evidence\token-surface-shakedown-a019'
$script:SemanticBaselinePath = Join-Path $script:EvidenceRoot 'semantic-baseline.json'
$script:SymForgeExe = 'C:\Users\rakovnik\.codex\tools\symforge-token-trust-8.14.1-a019\symforge.exe'
$script:SymForgeSha256 = '6C4176E03299B768793ACB64012FDD95783476B6AE59662FC4AD7B8C310FFC3B'
$script:CodexExe = (Get-Command node -ErrorAction Stop).Source
$script:CodexScript = 'C:\Users\rakovnik\.npm-global\node_modules\@openai\codex\bin\codex.js'
$script:CodexAuthSource = Join-Path $HOME '.codex\auth.json'
$script:CodexVersion = '0.144.2'
$script:SymForgeVersion = '8.14.1'
$script:Model = 'gpt-5.6-sol'
$script:ReasoningLevel = 'high'
$script:RunTimeoutSeconds = 1200

function Assert-ExactTempPath {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Expected,
        [Parameter(Mandatory)][string]$Purpose
    )

    $resolved = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
    $exact = [System.IO.Path]::GetFullPath($Expected).TrimEnd('\')
    $repo = $script:RepositoryRoot.TrimEnd('\')
    if (-not $resolved.Equals($exact, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing $Purpose outside the declared path: $resolved"
    }
    if ($resolved.StartsWith($repo + '\', [System.StringComparison]::OrdinalIgnoreCase) -or
        $resolved.Equals($repo, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing $Purpose inside the development repository: $resolved"
    }
    $resolved
}

function Assert-IsolatedCodexHomePath {
    param([Parameter(Mandatory)][string]$Path)

    $root = [System.IO.Path]::GetFullPath($script:RawEvidenceRoot).TrimEnd('\')
    $resolved = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
    if (-not $resolved.StartsWith($root + '\.codex-home-', [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing isolated Codex-home operation outside the declared evidence root: $resolved"
    }
    $resolved
}

function New-IsolatedCodexHome {
    param([Parameter(Mandatory)][ValidatePattern('^[a-z0-9-]+$')][string]$TraceStem)

    if (-not (Test-Path -LiteralPath $script:CodexAuthSource)) {
        throw 'Codex authentication is unavailable for the isolated benchmark home.'
    }
    $path = Assert-IsolatedCodexHomePath -Path (Join-Path $script:RawEvidenceRoot ".codex-home-$TraceStem")
    if (Test-Path -LiteralPath $path) { throw "Isolated Codex home already exists: $path" }
    [void](New-Item -ItemType Directory -Path $path)
    Copy-Item -LiteralPath $script:CodexAuthSource -Destination (Join-Path $path 'auth.json')
    $path
}

function Remove-IsolatedCodexHome {
    param([Parameter(Mandatory)][string]$Path)

    $resolved = Assert-IsolatedCodexHomePath -Path $Path
    if (Test-Path -LiteralPath $resolved) {
        Remove-Item -LiteralPath $resolved -Recurse -Force
    }
}

function Assert-FreeSpace {
    $cFree = (Get-PSDrive -Name C).Free
    $eFree = (Get-PSDrive -Name E).Free
    if ($cFree -lt 10GB) { throw ('C: has only {0:N2} GB free.' -f ($cFree / 1GB)) }
    if ($eFree -lt 5GB) { throw ('E: has only {0:N2} GB free.' -f ($eFree / 1GB)) }
}

function Assert-CandidateBinary {
    if (-not (Test-Path -LiteralPath $script:SymForgeExe)) {
        throw "Pinned SymForge candidate is missing: $script:SymForgeExe"
    }
    $actualHash = (Get-FileHash -LiteralPath $script:SymForgeExe -Algorithm SHA256).Hash
    Assert-Equal $actualHash $script:SymForgeSha256 'pinned SymForge candidate hash'
}

function Invoke-Git {
    param([Parameter(Mandatory)][string[]]$Arguments)

    $output = & git @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git $($Arguments -join ' ') failed: $output" }
    $output
}

function Test-RegisteredFixture {
    $expected = [System.IO.Path]::GetFullPath($script:FixtureRoot).TrimEnd('\')
    $listed = Invoke-Git -Arguments @('-C', $script:RepositoryRoot, 'worktree', 'list', '--porcelain')
    foreach ($line in $listed) {
        if ($line -notlike 'worktree *') { continue }
        $candidate = [System.IO.Path]::GetFullPath($line.Substring(9)).TrimEnd('\')
        if ($candidate.Equals($expected, [System.StringComparison]::OrdinalIgnoreCase)) { return $true }
    }
    $false
}

function New-Fixture {
    [void](Assert-FixturePath -Path $script:FixtureRoot)
    if ((Test-Path -LiteralPath $script:FixtureRoot) -or (Test-RegisteredFixture)) {
        throw "Fixture already exists or remains registered: $script:FixtureRoot"
    }
    [void](Invoke-Git -Arguments @(
        '-C', $script:RepositoryRoot, 'worktree', 'add', '--detach',
        $script:FixtureRoot, $script:FixtureCommit
    ))
}

function Remove-Fixture {
    [void](Assert-FixturePath -Path $script:FixtureRoot)
    if (Test-RegisteredFixture) {
        [void](Invoke-Git -Arguments @(
            '-C', $script:RepositoryRoot, 'worktree', 'remove', '--force', $script:FixtureRoot
        ))
        [void](Invoke-Git -Arguments @('-C', $script:RepositoryRoot, 'worktree', 'prune'))
    }
    elseif (Test-Path -LiteralPath $script:FixtureRoot) {
        throw "Refusing raw deletion of an unregistered fixture path: $script:FixtureRoot"
    }
}

function Get-UnexpectedFixtureChanges {
    $lines = @(Invoke-Git -Arguments @(
        '-C', $script:FixtureRoot, 'status', '--porcelain', '--untracked-files=all'
    ))
    @($lines | Where-Object {
        $path = if ($_.Length -gt 3) { $_.Substring(3).Trim('"') } else { '' }
        $path -and -not ($path -eq '.symforge' -or $path.StartsWith('.symforge/'))
    })
}

function Assert-GoldenPath {
    Assert-ExactTempPath -Path $script:GoldenRoot -Expected $script:GoldenRoot -Purpose 'golden-state mutation'
}

function Copy-GoldenStateToFixture {
    [void](Assert-GoldenPath)
    $source = Join-Path $script:GoldenRoot '.symforge'
    $destination = Join-Path $script:FixtureRoot '.symforge'
    if (-not (Test-Path -LiteralPath (Join-Path $source 'index.bin'))) {
        throw "Golden index is missing: $source"
    }
    if (Test-Path -LiteralPath $destination) {
        throw "Fixture state already exists before golden restore: $destination"
    }
    Copy-Item -LiteralPath $source -Destination $destination -Recurse
    $expectedHash = (Get-Content -LiteralPath (Join-Path $script:GoldenRoot 'index.sha256') -Raw).Trim()
    $actualHash = (Get-FileHash -LiteralPath (Join-Path $destination 'index.bin') -Algorithm SHA256).Hash
    Assert-Equal $actualHash $expectedHash 'golden index hash'
}

function Send-SymForgeMcpNotification {
    param(
        [Parameter(Mandatory)]$Connection,
        [Parameter(Mandatory)][string]$Method,
        [Parameter(Mandatory)]$Params
    )

    $Connection.Process.StandardInput.WriteLine((@{
        jsonrpc = '2.0'; method = $Method; params = $Params
    } | ConvertTo-Json -Depth 50 -Compress))
    $Connection.Process.StandardInput.Flush()
}

function Invoke-SymForgeMcpRequest {
    param(
        [Parameter(Mandatory)]$Connection,
        [Parameter(Mandatory)][string]$Method,
        [Parameter(Mandatory)]$Params
    )

    $requestId = [int]$Connection.NextRequestId
    $Connection.NextRequestId = $requestId + 1
    $Connection.Process.StandardInput.WriteLine((@{
        jsonrpc = '2.0'; id = $requestId; method = $Method; params = $Params
    } | ConvertTo-Json -Depth 50 -Compress))
    $Connection.Process.StandardInput.Flush()

    while ($true) {
        $line = $Connection.Process.StandardOutput.ReadLine()
        if ($null -eq $line) { throw "SymForge stdout closed before response $requestId" }
        $message = $line | ConvertFrom-Json
        if ($message.id -ne $requestId) { continue }
        if ($message.PSObject.Properties.Name -contains 'error') {
            throw ($message.error | ConvertTo-Json -Depth 20 -Compress)
        }
        return $message.result
    }
}

function Start-SymForgeMcpConnection {
    param([Parameter(Mandatory)][ValidateSet('full', 'compact')][string]$Surface)

    Assert-CandidateBinary
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $script:SymForgeExe
    $psi.WorkingDirectory = $script:FixtureRoot
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.Environment['RUST_LOG'] = 'off'
    $psi.Environment['SYMFORGE_WORKSPACE_ROOT'] = $script:FixtureRoot
    $psi.Environment['SYMFORGE_SURFACE'] = $Surface
    $psi.Environment['SYMFORGE_NO_DAEMON'] = '1'
    $process = [Diagnostics.Process]::Start($psi)
    $connection = [pscustomobject]@{
        Process = $process
        StderrTask = $process.StandardError.ReadToEndAsync()
        NextRequestId = 1
        Closed = $false
    }
    try {
        [void](Invoke-SymForgeMcpRequest -Connection $connection -Method 'initialize' -Params @{
            protocolVersion = '2025-03-26'; capabilities = @{}
            clientInfo = @{ name = 'token-surface-shakedown'; version = '1.0.0' }
        })
        Send-SymForgeMcpNotification -Connection $connection -Method 'notifications/initialized' -Params @{}
        $connection
    }
    catch {
        try { $process.Kill($true) } catch {}
        $process.WaitForExit()
        throw
    }
}

function Close-SymForgeMcpConnection {
    param([Parameter(Mandatory)]$Connection)

    if ($Connection.Closed) { return }
    $Connection.Closed = $true
    $forcedStop = $false
    $Connection.Process.StandardInput.Close()
    if (-not $Connection.Process.WaitForExit(5000)) {
        $forcedStop = $true
        $Connection.Process.Kill($true)
    }
    $Connection.Process.WaitForExit()
    $stderr = $Connection.StderrTask.GetAwaiter().GetResult()
    if ($Connection.Process.ExitCode -ne 0 -and -not $forcedStop) {
        throw "SymForge exited $($Connection.Process.ExitCode): $stderr"
    }
}

function Assert-SymForgeToolResult {
    param(
        [Parameter(Mandatory)]$Result,
        [Parameter(Mandatory)][string]$Label
    )

    if (($Result.PSObject.Properties.Name -contains 'isError') -and $Result.isError) {
        throw "SymForge $Label returned isError=true: $($Result | ConvertTo-Json -Depth 20 -Compress)"
    }
}

function Invoke-SymForgeMcp {
    param(
        [Parameter(Mandatory)][ValidateSet('full', 'compact')][string]$Surface,
        [Parameter(Mandatory)][object[]]$Calls
    )

    $connection = Start-SymForgeMcpConnection -Surface $Surface
    $results = [ordered]@{}
    try {
        foreach ($call in $Calls) {
            $result = Invoke-SymForgeMcpRequest -Connection $connection -Method $call.Method -Params $call.Params
            Assert-SymForgeToolResult -Result $result -Label $call.Label
            $results[$call.Label] = $result
        }
    }
    finally {
        Close-SymForgeMcpConnection -Connection $connection
    }
    [pscustomobject]$results
}

function Invoke-Prepare {
    Assert-FreeSpace
    [void](Assert-GoldenPath)
    if (Test-Path -LiteralPath $script:GoldenRoot) {
        throw "Golden state already exists; run Cleanup before replacing it: $script:GoldenRoot"
    }
    New-Fixture
    try {
        $calls = @(
            @{ Label = 'index_folder'; Method = 'tools/call'; Params = @{ name = 'index_folder'; arguments = @{
                path = $script:FixtureRoot; idempotency_key = 'token-shakedown-prep-a10ff102'
            } } }
            @{ Label = 'checkpoint_now'; Method = 'tools/call'; Params = @{ name = 'checkpoint_now'; arguments = @{
                verify_after_write = $true
            } } }
            @{ Label = 'health'; Method = 'tools/call'; Params = @{ name = 'health_compact'; arguments = @{} } }
        )
        $receipt = Invoke-SymForgeMcp -Surface full -Calls $calls
        $state = Join-Path $script:FixtureRoot '.symforge'
        $index = Join-Path $state 'index.bin'
        if (-not (Test-Path -LiteralPath $index)) { throw "Prepared index is missing: $index" }
        $goldenParent = Split-Path -Parent $script:GoldenRoot
        [void](New-Item -ItemType Directory -Path $goldenParent -Force)
        [void](New-Item -ItemType Directory -Path $script:GoldenRoot)
        Copy-Item -LiteralPath $state -Destination $script:GoldenRoot -Recurse
        $hash = (Get-FileHash -LiteralPath $index -Algorithm SHA256).Hash
        [IO.File]::WriteAllText((Join-Path $script:GoldenRoot 'index.sha256'), $hash)
        [void](New-Item -ItemType Directory -Path $script:EvidenceRoot -Force)
        $prepareReceipt = [ordered]@{
            fixture_commit = $script:FixtureCommit
            symforge_version = $script:SymForgeVersion
            symforge_binary = $script:SymForgeExe
            symforge_binary_sha256 = $script:SymForgeSha256
            golden_index_sha256 = $hash
            mcp = $receipt
        }
        [IO.File]::WriteAllText(
            (Join-Path $script:EvidenceRoot 'prepare-receipt.json'),
            ($prepareReceipt | ConvertTo-Json -Depth 30)
        )
    }
    finally {
        Remove-Fixture
    }
    'PASS: prepared golden SymForge state and removed fixture'
}

function Get-TaskPrompt {
    param([Parameter(Mandatory)][ValidateSet('S1', 'S2')][string]$Task)

    if ($Task -eq 'S1') {
        return 'Investigate this repository and answer four questions: (1) which environment variable controls the MCP tool-surface profile, (2) which profile is selected by default, (3) what are the exact three tool names in the compact profile, and (4) which functions read the environment, choose the profile-specific tool list, and construct the compact list. Cite file and line evidence. Do not change files or run builds or tests.'
    }
    'Trace how oversized code-discovery results are limited and later recovered. Report: (1) every eligible tool and its default token budget, (2) when the complete result is returned versus stored, (3) how the continuation is exposed, (4) how continuation identifiers are validated and retrieved, and (5) what usage accounting changes on retrieval. Cite source files and symbols. Do not change files or run builds or tests.'
}

function Get-SurfaceForArm {
    param([Parameter(Mandatory)][ValidateSet('A-full', 'C-compact')][string]$Arm)
    if ($Arm -eq 'A-full') { 'full' } else { 'compact' }
}

function New-RunFixture {
    New-Fixture
    try {
        Copy-GoldenStateToFixture
        $unexpected = @(Get-UnexpectedFixtureChanges)
        if ($unexpected.Count -ne 0) {
            throw "Fresh fixture is unexpectedly dirty: $($unexpected -join '; ')"
        }
    }
    catch {
        Remove-Fixture
        throw
    }
}

function Get-SymForgeProcessIds {
    @(
        Get-CimInstance Win32_Process -Filter "Name = 'symforge.exe'" -ErrorAction SilentlyContinue |
            Where-Object {
                $_.ExecutablePath -and
                ([System.IO.Path]::GetFullPath($_.ExecutablePath)).Equals(
                    [System.IO.Path]::GetFullPath($script:SymForgeExe),
                    [System.StringComparison]::OrdinalIgnoreCase
                )
            } |
            ForEach-Object { [int]$_.ProcessId }
    )
}

function Add-CodexArgument {
    param(
        [Parameter(Mandatory)][Diagnostics.ProcessStartInfo]$ProcessStartInfo,
        [Parameter(Mandatory)][string]$Value
    )
    [void]$ProcessStartInfo.ArgumentList.Add($Value)
}

function Invoke-CodexProcess {
    param(
        [Parameter(Mandatory)][ValidateSet('full', 'compact')][string]$Surface,
        [Parameter(Mandatory)][string]$Prompt,
        [Parameter(Mandatory)][string]$TraceStem
    )

    Assert-FreeSpace
    Assert-CandidateBinary
    [void](Assert-ExactTempPath -Path $script:RawEvidenceRoot -Expected $script:RawEvidenceRoot -Purpose 'raw-evidence mutation')
    [void](New-Item -ItemType Directory -Path $script:RawEvidenceRoot -Force)
    $stdoutPath = Join-Path $script:RawEvidenceRoot "$TraceStem.events.jsonl"
    $stderrPath = Join-Path $script:RawEvidenceRoot "$TraceStem.stderr.log"
    $answerPath = Join-Path $script:RawEvidenceRoot "$TraceStem.answer.md"
    foreach ($path in @($stdoutPath, $stderrPath, $answerPath)) {
        if (Test-Path -LiteralPath $path) { throw "Refusing to overwrite trace artifact: $path" }
    }

    $isolatedCodexHome = New-IsolatedCodexHome -TraceStem $TraceStem
    $beforePids = @(Get-SymForgeProcessIds)
    $psi = [Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $script:CodexExe
    $psi.WorkingDirectory = $script:FixtureRoot
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.Environment['CODEX_HOME'] = $isolatedCodexHome
    Add-CodexArgument -ProcessStartInfo $psi -Value $script:CodexScript

    foreach ($argument in @(
        'exec', '--json', '--ephemeral', '--ignore-user-config', '--strict-config',
        '--color', 'never', '--model', $script:Model, '--sandbox', 'read-only',
        '--cd', $script:FixtureRoot, '--output-last-message', $answerPath,
        '-c', "model_reasoning_effort='$script:ReasoningLevel'",
        '-c', "approval_policy='never'",
        '-c', "mcp_servers.symforge.command='$script:SymForgeExe'",
        '-c', "mcp_servers.symforge.cwd='$script:FixtureRoot'",
        '-c', "mcp_servers.symforge.env={ SYMFORGE_SURFACE = '$Surface', SYMFORGE_WORKSPACE_ROOT = '$script:FixtureRoot', SYMFORGE_NO_DAEMON = '1', RUST_LOG = 'off' }",
        '-c', 'mcp_servers.symforge.startup_timeout_sec=60',
        '-c', 'mcp_servers.symforge.tool_timeout_sec=600',
        '-'
    )) {
        Add-CodexArgument -ProcessStartInfo $psi -Value $argument
    }

    $startedAt = [DateTimeOffset]::UtcNow
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $timedLines = [Collections.Generic.List[object]]::new()
    $stdoutBuilder = [Text.StringBuilder]::new()
    $process = $null
    $timedOut = $false
    try {
        $process = [Diagnostics.Process]::Start($psi)
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $process.StandardInput.Write($Prompt)
        $process.StandardInput.Close()

        while ($true) {
            $remaining = ($script:RunTimeoutSeconds * 1000) - [int]$stopwatch.ElapsedMilliseconds
            if ($remaining -le 0) {
                $timedOut = $true
                $process.Kill($true)
                break
            }
            $lineTask = $process.StandardOutput.ReadLineAsync()
            if (-not $lineTask.Wait($remaining)) {
                $timedOut = $true
                $process.Kill($true)
                break
            }
            $line = $lineTask.GetAwaiter().GetResult()
            if ($null -eq $line) { break }
            $observedAt = [DateTimeOffset]::UtcNow.ToString('O')
            [void]$timedLines.Add([pscustomobject]@{ ObservedAt = $observedAt; Line = $line })
            [void]$stdoutBuilder.AppendLine($line)
        }

        if (-not $process.HasExited) {
            $remaining = ($script:RunTimeoutSeconds * 1000) - [int]$stopwatch.ElapsedMilliseconds
            if ($remaining -le 0 -or -not $process.WaitForExit($remaining)) {
                $timedOut = $true
                $process.Kill($true)
            }
        }
        $process.WaitForExit()
        $stderr = $stderrTask.GetAwaiter().GetResult()
        $stdout = $stdoutBuilder.ToString()
        $diagnostics = Get-TraceDiagnostics -Stdout $stdout -Stderr $stderr
        if ($diagnostics.PotentialSecretLineCount -ne 0) {
            if (Test-Path -LiteralPath $answerPath) { Remove-Item -LiteralPath $answerPath -Force }
            throw "Trace rejected before persistence: $($diagnostics.PotentialSecretLineCount) potential sensitive-data line(s)."
        }
        if ($diagnostics.ConfigurationDiagnosticCount -ne 0) {
            if (Test-Path -LiteralPath $answerPath) { Remove-Item -LiteralPath $answerPath -Force }
            throw "Trace rejected before persistence: $($diagnostics.ConfigurationDiagnosticCount) user-configuration diagnostic line(s)."
        }
        [IO.File]::WriteAllText($stdoutPath, $stdout)
        [IO.File]::WriteAllText($stderrPath, $stderr)
    }
    finally {
        if ($null -ne $process -and -not $process.HasExited) {
            $process.Kill($true)
            $process.WaitForExit()
        }
        $stopwatch.Stop()
        Remove-IsolatedCodexHome -Path $isolatedCodexHome
    }

    Start-Sleep -Milliseconds 750
    $afterPids = @(Get-SymForgeProcessIds)
    $newPids = @($afterPids | Where-Object { $_ -notin $beforePids })
    if ($newPids.Count -ne 0) {
        throw "Owned SymForge process did not exit after Codex PID $($process.Id): $($newPids -join ',')"
    }

    [pscustomobject]@{
        StartedAt = $startedAt.ToString('O')
        FinishedAt = [DateTimeOffset]::UtcNow.ToString('O')
        WallMilliseconds = $stopwatch.ElapsedMilliseconds
        CodexPid = $process.Id
        ExitCode = $process.ExitCode
        TimedOut = $timedOut
        Stdout = $stdout
        Stderr = $stderr
        StdoutPath = $stdoutPath
        StderrPath = $stderrPath
        AnswerPath = $answerPath
        Answer = if (Test-Path -LiteralPath $answerPath) { Get-Content -LiteralPath $answerPath -Raw } else { '' }
        TimedLines = @($timedLines)
        TraceDiagnostics = $diagnostics
    }
}

function Get-CodexToolEvents {
    param([Parameter(Mandatory)][object[]]$Records)

    @(
        foreach ($record in $Records) {
            $line = if ($record -is [string]) { $record } else { [string]$record.Line }
            $observedAt = if ($record -is [string]) { $null } else { [string]$record.ObservedAt }
            if ([string]::IsNullOrWhiteSpace($line)) { continue }
            try { $event = $line | ConvertFrom-Json -ErrorAction Stop } catch { continue }
            if ($event.PSObject.Properties.Name -notcontains 'item' -or $null -eq $event.item) { continue }
            $item = $event.item
            $itemType = if ($item.PSObject.Properties.Name -contains 'type') { [string]$item.type } else { '' }
            if ($itemType -notmatch 'tool|command|file') { continue }
            [pscustomobject]@{
                ObservedAt = $observedAt
                EventType = [string]$event.type
                ItemType = $itemType
                Name = if ($item.PSObject.Properties.Name -contains 'tool') { [string]$item.tool } elseif ($item.PSObject.Properties.Name -contains 'name') { [string]$item.name } elseif ($item.PSObject.Properties.Name -contains 'tool_name') { [string]$item.tool_name } else { '' }
                Server = if ($item.PSObject.Properties.Name -contains 'server') { [string]$item.server } else { '' }
                Status = if ($item.PSObject.Properties.Name -contains 'status') { [string]$item.status } else { '' }
                Channel = if ($itemType -eq 'mcp_tool_call') { 'mcp' } else { 'native' }
                OutcomeClass = if ($event.type -eq 'item.completed' -and $item.status -eq 'completed') { 'success' } elseif ($event.type -eq 'item.completed') { 'error' } else { 'in_progress' }
            }
        }
    )
}

function Test-CompletedMcpCall {
    param(
        [Parameter(Mandatory)][object[]]$ToolEvents,
        [Parameter(Mandatory)][string]$Name
    )

    @($ToolEvents | Where-Object {
        $_.EventType -eq 'item.completed' -and
        $_.ItemType -eq 'mcp_tool_call' -and
        $_.Server -eq 'symforge' -and
        $_.Name -eq $Name -and
        $_.Status -eq 'completed' -and
        $_.OutcomeClass -eq 'success'
    }).Count -eq 1
}

function Get-ToolAnnotationHint {
    param(
        [Parameter(Mandatory)]$Tool,
        [Parameter(Mandatory)][ValidateSet('readOnlyHint', 'openWorldHint')][string]$Name
    )

    $toolProperties = @($Tool.PSObject.Properties | ForEach-Object Name)
    if ($toolProperties -notcontains 'annotations' -or $null -eq $Tool.annotations) {
        return $null
    }
    $annotationProperties = @($Tool.annotations.PSObject.Properties | ForEach-Object Name)
    if ($annotationProperties -notcontains $Name) { return $null }
    [bool]$Tool.annotations.$Name
}

function Get-TraceDiagnostics {
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string]$Stdout,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Stderr
    )

    $lines = @(($Stdout + [Environment]::NewLine + $Stderr) -split "`r?`n")
    $configurationPatterns = @(
        'malformed agent role',
        'skills context budget',
        '[\\/].codex[\\/]agents[\\/]'
    )
    $sensitivePatterns = @(
        'sk-[A-Za-z0-9_-]{16,}',
        'gh[pousr]_[A-Za-z0-9_]{16,}',
        '(?i)(api[_-]?key|access[_-]?token|bearer|password|client[_-]?secret)\s*[:=]\s*["''][^"'']+["'']',
        '-----BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY-----'
    )
    $configurationLines = @($lines | Where-Object {
        $line = $_
        @($configurationPatterns | Where-Object { $line -match $_ }).Count -ne 0
    })
    $sensitiveLines = @($lines | Where-Object {
        $line = $_
        @($sensitivePatterns | Where-Object { $line -match $_ }).Count -ne 0
    })
    [pscustomobject]@{
        ConfigurationDiagnosticCount = $configurationLines.Count
        PotentialSecretLineCount = $sensitiveLines.Count
    }
}

function Get-RedactedGraderAnswer {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Answer)

    [regex]::Replace(
        $Answer,
        '(?i)(?:[A-Z]:[\\/])?Users[\\/][^\\/]+[\\/]AppData[\\/]Local[\\/]Temp[\\/]',
        '<temp>/'
    )
}

function Get-Utf8Sha256 {
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Text)

    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
        ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace('-', '')
    }
    finally {
        $sha.Dispose()
    }
}

function Get-ManifestFingerprintFromRecords {
    param([Parameter(Mandatory)][object[]]$Records)

    $canonical = @(
        $Records |
            Sort-Object -Property @{ Expression = { [string]$_.Path }; Ascending = $true } -CaseSensitive |
            ForEach-Object {
                $encodedPath = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes([string]$_.Path))
                '{0}|{1}|{2}' -f $encodedPath, ([string]$_.Mode), ([string]$_.Sha256).ToUpperInvariant()
            }
    )
    [pscustomobject]@{
        Hash = Get-Utf8Sha256 -Text ($canonical -join "`n")
        FileCount = $canonical.Count
    }
}

function Test-DeadlineExpired {
    param(
        [Parameter(Mandatory)][DateTimeOffset]$StartedAt,
        [Parameter(Mandatory)][ValidateRange(1, [int]::MaxValue)][int]$TimeoutMs,
        [Parameter(Mandatory)][DateTimeOffset]$Now
    )

    ($Now - $StartedAt).TotalMilliseconds -ge $TimeoutMs
}

function Get-SnapshotVerificationReceipt {
    param([Parameter(Mandatory)][string]$Text)

    $status = [regex]::Match(
        $Text,
        '(?m)^Status:\s*Ready\s*\|\s*Files:\s*(\d+)\s+indexed\s*\((\d+)\s+parsed,\s*(\d+)\s+partial,\s*(\d+)\s+failed\)\s*\|\s*Symbols:\s*(\d+)'
    )
    if (-not $status.Success) {
        throw 'SymForge readiness response was not Ready or omitted index counts.'
    }

    $snapshot = [regex]::Match(
        $Text,
        '(?m)^Snapshot:\s+load_source=(\S+)\s+verify=(not_needed|pending|running|completed)(?:\s+mismatches=(\d+))?'
    )
    if (-not $snapshot.Success) {
        throw 'SymForge readiness response omitted snapshot verification evidence.'
    }
    if ($snapshot.Groups[1].Value -ne 'snapshot_restore') {
        throw "SymForge readiness used unexpected load source: $($snapshot.Groups[1].Value)"
    }
    $state = $snapshot.Groups[2].Value
    if ($state -eq 'completed' -and -not $snapshot.Groups[3].Success) {
        throw 'Completed snapshot verification omitted its mismatch count.'
    }

    [pscustomobject]@{
        State = $state
        IsComplete = $state -eq 'completed'
        Mismatches = if ($snapshot.Groups[3].Success) { [int]$snapshot.Groups[3].Value } else { $null }
        Files = [int]$status.Groups[1].Value
        Parsed = [int]$status.Groups[2].Value
        Partial = [int]$status.Groups[3].Value
        Failed = [int]$status.Groups[4].Value
        Symbols = [int]$status.Groups[5].Value
        LoadSource = $snapshot.Groups[1].Value
    }
}

function Get-McpContentText {
    param([Parameter(Mandatory)]$Result)

    [string]::Join([Environment]::NewLine, @(
        $Result.content | Where-Object type -eq 'text' | ForEach-Object { [string]$_.text }
    ))
}

function Get-TrackedSourceFingerprint {
    $head = [string](@(Invoke-Git -Arguments @('-C', $script:FixtureRoot, 'rev-parse', 'HEAD'))[0])
    Assert-Equal $head.Trim() $script:FixtureCommit 'fixture commit'
    $tree = [string](@(Invoke-Git -Arguments @('-C', $script:FixtureRoot, 'rev-parse', 'HEAD^{tree}'))[0])
    $stageLines = @(Invoke-Git -Arguments @(
        '-C', $script:FixtureRoot, '-c', 'core.quotePath=false', 'ls-files', '--stage'
    ))
    $records = @(
        foreach ($line in $stageLines) {
            $match = [regex]::Match([string]$line, '^(\d{6})\s+([0-9a-f]+)\s+(\d+)\t(.+)$')
            if (-not $match.Success) { throw "Could not parse git index entry: $line" }
            if ($match.Groups[3].Value -ne '0') { throw "Fixture contains an unmerged git index entry: $line" }
            $mode = $match.Groups[1].Value
            $path = $match.Groups[4].Value
            $fullPath = Join-Path $script:FixtureRoot $path
            $hash = if ($mode -eq '160000') {
                Get-Utf8Sha256 -Text ("gitlink:" + $match.Groups[2].Value)
            }
            else {
                if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) {
                    throw "Tracked fixture file is missing: $path"
                }
                (Get-FileHash -LiteralPath $fullPath -Algorithm SHA256).Hash
            }
            [pscustomobject]@{ Path = $path; Mode = $mode; Sha256 = $hash }
        }
    )
    $manifest = Get-ManifestFingerprintFromRecords -Records $records
    [pscustomobject]@{
        Commit = $head.Trim()
        GitTreeOid = $tree.Trim()
        Hash = $manifest.Hash
        FileCount = $manifest.FileCount
    }
}

function Invoke-SnapshotMaterialization {
    param(
        [ValidateRange(1000, 600000)][int]$TimeoutMs = 120000,
        [ValidateRange(10, 5000)][int]$PollIntervalMs = 100
    )

    $connection = Start-SymForgeMcpConnection -Surface full
    $startedAt = [DateTimeOffset]::UtcNow
    $stopwatch = [Diagnostics.Stopwatch]::StartNew()
    $observedStates = [Collections.Generic.List[string]]::new()
    $verification = $null
    $healthText = $null
    $outlineText = $null
    try {
        while ($true) {
            $health = Invoke-SymForgeMcpRequest -Connection $connection -Method 'tools/call' -Params @{
                name = 'health_compact'; arguments = @{}
            }
            Assert-SymForgeToolResult -Result $health -Label 'materialization health_compact'
            $healthText = Get-McpContentText -Result $health
            $verification = Get-SnapshotVerificationReceipt -Text $healthText
            if ($verification.State -notin $observedStates) {
                [void]$observedStates.Add($verification.State)
            }
            if ($verification.IsComplete) {
                Assert-Equal $verification.Mismatches 0 'snapshot verification mismatch count'
                break
            }
            if (Test-DeadlineExpired -StartedAt $startedAt -TimeoutMs $TimeoutMs -Now ([DateTimeOffset]::UtcNow)) {
                throw "Snapshot verification did not complete within $TimeoutMs ms; observed=$($observedStates -join ',')"
            }
            Start-Sleep -Milliseconds $PollIntervalMs
        }

        $outline = Invoke-SymForgeMcpRequest -Connection $connection -Method 'tools/call' -Params @{
            name = 'get_repo_map'; arguments = @{
                detail = 'full'; max_files = 5000; max_tokens = 1000000
            }
        }
        Assert-SymForgeToolResult -Result $outline -Label 'semantic get_repo_map'
        $outlineText = Get-McpContentText -Result $outline
        $outlineHeader = [regex]::Match($outlineText, '(?m)^.+\s+\((\d+) files, (\d+) symbols\)$')
        if (-not $outlineHeader.Success) { throw 'Semantic repo outline omitted its complete header.' }
        Assert-Equal ([int]$outlineHeader.Groups[1].Value) $verification.Files 'semantic outline file count'
        Assert-Equal ([int]$outlineHeader.Groups[2].Value) $verification.Symbols 'semantic outline symbol count'
    }
    finally {
        $stopwatch.Stop()
        Close-SymForgeMcpConnection -Connection $connection
    }

    $version = [regex]::Match($healthText, '(?m)^Runtime:.*?\bversion=([^\s|]+)')
    if (-not $version.Success) { throw 'SymForge readiness response omitted its version.' }
    Assert-Equal $version.Groups[1].Value $script:SymForgeVersion 'SymForge version'
    $indexPath = Join-Path $script:FixtureRoot '.symforge\index.bin'
    if (-not (Test-Path -LiteralPath $indexPath)) { throw 'Materialized snapshot is missing.' }

    [pscustomobject]@{
        Milliseconds = $stopwatch.ElapsedMilliseconds
        ObservedStates = @($observedStates)
        SnapshotHash = (Get-FileHash -LiteralPath $indexPath -Algorithm SHA256).Hash
        RepoOutlineSha256 = Get-Utf8Sha256 -Text $outlineText
        RepoOutlineBytes = [Text.Encoding]::UTF8.GetByteCount($outlineText)
        LoadSource = $verification.LoadSource
        Files = $verification.Files
        Parsed = $verification.Parsed
        Partial = $verification.Partial
        Failed = $verification.Failed
        Symbols = $verification.Symbols
        Mismatches = $verification.Mismatches
        SymForgeVersion = $version.Groups[1].Value
    }
}

function Assert-SemanticBaselineMatch {
    param(
        [Parameter(Mandatory)]$Baseline,
        [Parameter(Mandatory)]$Receipt
    )

    Assert-Equal $Receipt.FixtureCommit $Baseline.fixture_commit 'semantic fixture commit'
    Assert-Equal $Receipt.GitTreeOid $Baseline.git_tree_oid 'semantic git tree'
    Assert-Equal $Receipt.SourceManifestSha256 $Baseline.source_manifest_sha256 'semantic source manifest'
    Assert-Equal $Receipt.SourceFileCount $Baseline.source_file_count 'semantic source file count'
    Assert-Equal $Receipt.GoldenInputHash $Baseline.golden_input_sha256 'semantic golden input snapshot'
    Assert-Equal $Receipt.RepoOutlineSha256 $Baseline.repo_outline_sha256 'semantic repo outline'
    Assert-Equal $Receipt.Files $Baseline.index_files 'semantic index file count'
    Assert-Equal $Receipt.Parsed $Baseline.parsed_files 'semantic parsed file count'
    Assert-Equal $Receipt.Partial $Baseline.partial_files 'semantic partial file count'
    Assert-Equal $Receipt.Failed $Baseline.failed_files 'semantic failed file count'
    Assert-Equal $Receipt.Symbols $Baseline.symbols 'semantic symbol count'
    Assert-Equal $Receipt.SymForgeBinarySha256 $Baseline.symforge_binary_sha256 'semantic candidate binary'
    Assert-Equal $Receipt.SymForgeVersion $Baseline.symforge_version 'semantic candidate version'
}

function Get-ReadinessReceipt {
    param([switch]$SkipBaselineComparison)

    $expectedHash = (Get-Content -LiteralPath (Join-Path $script:GoldenRoot 'index.sha256') -Raw).Trim()
    $indexPath = Join-Path $script:FixtureRoot '.symforge\index.bin'
    $beforeHash = (Get-FileHash -LiteralPath $indexPath -Algorithm SHA256).Hash
    Assert-Equal $beforeHash $expectedHash 'preflight golden index hash'
    $source = Get-TrackedSourceFingerprint
    $materialized = Invoke-SnapshotMaterialization
    $receipt = [pscustomobject]@{
        Verdict = 'ready'
        FixtureCommit = $source.Commit
        GitTreeOid = $source.GitTreeOid
        SourceManifestSha256 = $source.Hash
        SourceFileCount = $source.FileCount
        GoldenInputHash = $expectedHash
        SnapshotHash = $materialized.SnapshotHash
        RepoOutlineSha256 = $materialized.RepoOutlineSha256
        RepoOutlineBytes = $materialized.RepoOutlineBytes
        Milliseconds = $materialized.Milliseconds
        ObservedStates = $materialized.ObservedStates
        LoadSource = $materialized.LoadSource
        Files = $materialized.Files
        Parsed = $materialized.Parsed
        Partial = $materialized.Partial
        Failed = $materialized.Failed
        Symbols = $materialized.Symbols
        Mismatches = $materialized.Mismatches
        SymForgeVersion = $materialized.SymForgeVersion
        SymForgeBinarySha256 = $script:SymForgeSha256
    }
    if (-not $SkipBaselineComparison) {
        if (-not (Test-Path -LiteralPath $script:SemanticBaselinePath)) {
            throw 'Semantic baseline is missing; run -Action MaterializeBaseline first.'
        }
        $baseline = Get-Content -LiteralPath $script:SemanticBaselinePath -Raw | ConvertFrom-Json
        Assert-SemanticBaselineMatch -Baseline $baseline -Receipt $receipt
    }
    $receipt
}

function Invoke-MaterializeBaseline {
    Assert-FreeSpace
    if (Test-Path -LiteralPath $script:SemanticBaselinePath) {
        throw "Refusing to overwrite semantic baseline: $script:SemanticBaselinePath"
    }

    $first = $null
    $probe = $null
    New-RunFixture
    try {
        $first = Get-ReadinessReceipt -SkipBaselineComparison
        $probeInputHash = (Get-FileHash -LiteralPath (Join-Path $script:FixtureRoot '.symforge\index.bin') -Algorithm SHA256).Hash
        Assert-Equal $probeInputHash $first.SnapshotHash 'semantic probe input snapshot'
        $probe = Invoke-SnapshotMaterialization
        Assert-Equal $probe.RepoOutlineSha256 $first.RepoOutlineSha256 'semantic probe repo outline'
        Assert-Equal $probe.Files $first.Files 'semantic probe index files'
        Assert-Equal $probe.Symbols $first.Symbols 'semantic probe symbols'
        $unexpected = @(Get-UnexpectedFixtureChanges)
        if ($unexpected.Count -ne 0) { throw "Semantic baseline changed the fixture: $($unexpected -join '; ')" }
    }
    finally {
        Remove-Fixture
    }

    [void](New-Item -ItemType Directory -Path $script:EvidenceRoot -Force)
    $baseline = [ordered]@{
        created_at = [DateTimeOffset]::UtcNow.ToString('O')
        fixture_commit = $first.FixtureCommit
        git_tree_oid = $first.GitTreeOid
        source_manifest_sha256 = $first.SourceManifestSha256
        source_file_count = $first.SourceFileCount
        golden_input_sha256 = $first.GoldenInputHash
        first_materialized_snapshot_sha256 = $first.SnapshotHash
        probe_input_snapshot_sha256 = $probeInputHash
        probe_output_snapshot_sha256 = $probe.SnapshotHash
        repo_outline_sha256 = $first.RepoOutlineSha256
        repo_outline_bytes = $first.RepoOutlineBytes
        index_files = $first.Files
        parsed_files = $first.Parsed
        partial_files = $first.Partial
        failed_files = $first.Failed
        symbols = $first.Symbols
        snapshot_mismatches = $first.Mismatches
        first_observed_verify_states = $first.ObservedStates
        probe_observed_verify_states = $probe.ObservedStates
        first_materialization_milliseconds = $first.Milliseconds
        probe_verification_milliseconds = $probe.Milliseconds
        symforge_binary_sha256 = $first.SymForgeBinarySha256
        symforge_version = $first.SymForgeVersion
        residual_speed_confound = 'Measured startup still performs stat-all plus 10% spot verification; matched mtimes remove full reparse but not this symmetric product behavior.'
    }
    [IO.File]::WriteAllText($script:SemanticBaselinePath, ($baseline | ConvertTo-Json -Depth 20))
    'PASS: materialized and probed semantic baseline'
}

function Get-CompactRunArtifactPaths {
    param(
        [Parameter(Mandatory)][ValidateRange(1, 20)][int]$Id,
        [string]$Root = $script:EvidenceRoot
    )

    $stem = 'run-{0:d2}' -f $Id
    @(
        Join-Path $Root "$stem-answer.md"
        Join-Path $Root "$stem-grader.md"
    ) | Where-Object { Test-Path -LiteralPath $_ }
}

function Initialize-RunEvidence {
    param(
        [Parameter(Mandatory)][ValidateRange(1, 20)][int]$Id,
        [Parameter(Mandatory)][bool]$AllowIncompleteRerun
    )

    [void](New-Item -ItemType Directory -Path $script:EvidenceRoot -Force)
    [void](New-Item -ItemType Directory -Path $script:RawEvidenceRoot -Force)
    $resultsPath = Join-Path $script:EvidenceRoot 'shakedown-results.jsonl'
    $lines = if (Test-Path -LiteralPath $resultsPath) {
        @(Get-Content -LiteralPath $resultsPath | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    }
    else { @() }
    $matches = @($lines | Where-Object { ($_ | ConvertFrom-Json).run_id -eq $Id })
    $stem = 'run-{0:d2}' -f $Id
    $rawArtifacts = @(Get-ChildItem -LiteralPath $script:RawEvidenceRoot -File -Filter "$stem.*" -ErrorAction SilentlyContinue)
    $compactArtifacts = @(Get-CompactRunArtifactPaths -Id $Id)

    if ($matches.Count -eq 0 -and $rawArtifacts.Count -eq 0 -and $compactArtifacts.Count -eq 0) { return }
    if (-not $AllowIncompleteRerun) {
        throw "Run $Id evidence already exists; use -AllowRerunIncomplete only after verifying it is not a graded result."
    }
    if ($matches.Count -gt 1) { throw "Run $Id has duplicate active records; refusing automatic quarantine." }
    if ($matches.Count -eq 1) {
        $existing = $matches[0] | ConvertFrom-Json
        $status = if ($existing.PSObject.Properties.Name -contains 'record_status') { [string]$existing.record_status } else { '' }
        $graded = $existing.PSObject.Properties.Name -contains 'oracle_pass' -and $null -ne $existing.oracle_pass
        if ($graded -or $status -eq 'graded') {
            throw "Run $Id is graded and cannot be rerun."
        }
    }

    $stamp = [DateTimeOffset]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
    $compactQuarantine = Join-Path $script:EvidenceRoot "quarantine\$stem-$stamp"
    $rawQuarantine = Join-Path $script:RawEvidenceRoot "quarantine\$stem-$stamp"
    [void](New-Item -ItemType Directory -Path $compactQuarantine -Force)
    [void](New-Item -ItemType Directory -Path $rawQuarantine -Force)
    if ($matches.Count -eq 1) {
        [IO.File]::WriteAllText((Join-Path $compactQuarantine 'record.jsonl'), $matches[0] + [Environment]::NewLine)
        $remaining = @($lines | Where-Object { ($_ | ConvertFrom-Json).run_id -ne $Id })
        $remainingText = if ($remaining.Count -eq 0) { '' } else { ($remaining -join [Environment]::NewLine) + [Environment]::NewLine }
        [IO.File]::WriteAllText($resultsPath, $remainingText)
    }
    foreach ($artifact in $compactArtifacts) {
        Move-Item -LiteralPath $artifact -Destination (Join-Path $compactQuarantine ([IO.Path]::GetFileName($artifact)))
    }
    foreach ($artifact in $rawArtifacts) {
        Move-Item -LiteralPath $artifact.FullName -Destination (Join-Path $rawQuarantine $artifact.Name)
    }
}

function Set-OracleGrade {
    param(
        [Parameter(Mandatory)][ValidateRange(1, 20)][int]$Id,
        [Parameter(Mandatory)][ValidateSet('Pass', 'Fail')][string]$Verdict,
        [string[]]$Failures = @()
    )

    $resultsPath = Join-Path $script:EvidenceRoot 'shakedown-results.jsonl'
    if (-not (Test-Path -LiteralPath $resultsPath)) { throw 'No shakedown results exist to grade.' }
    $records = @(Get-Content -LiteralPath $resultsPath | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json })
    $matches = @($records | Where-Object run_id -eq $Id)
    if ($matches.Count -ne 1) { throw "Expected exactly one active record for run $Id." }
    $record = $matches[0]
    if ($record.record_status -ne 'captured_ungraded') { throw "Run $Id is not awaiting its first grade." }
    if ($Verdict -eq 'Pass' -and $Failures.Count -ne 0) { throw 'A passing grade cannot include failed criteria.' }
    if ($Verdict -eq 'Fail' -and $Failures.Count -eq 0) { throw 'A failing grade must name at least one criterion.' }
    $record.oracle_pass = $Verdict -eq 'Pass'
    $record.oracle_failures = @($Failures)
    $record.record_status = 'graded'
    $record.oracle_grade_count = 1
    $record | Add-Member -NotePropertyName graded_at -NotePropertyValue ([DateTimeOffset]::UtcNow.ToString('O')) -Force
    $serialized = @($records | ForEach-Object { $_ | ConvertTo-Json -Depth 30 -Compress })
    [IO.File]::WriteAllText($resultsPath, ($serialized -join [Environment]::NewLine) + [Environment]::NewLine)
    "PASS: graded run $Id as $Verdict"
}

function Invoke-WiringCheck {
    Assert-FreeSpace
    $receipts = [Collections.Generic.List[object]]::new()
    foreach ($arm in @('A-full', 'C-compact')) {
        $surface = Get-SurfaceForArm -Arm $arm
        New-RunFixture
        try {
            $listed = Invoke-SymForgeMcp -Surface $surface -Calls @(
                @{ Label = 'tools_list'; Method = 'tools/list'; Params = @{} }
            )
            $names = @($listed.tools_list.tools | ForEach-Object { [string]$_.name })
            if ($surface -eq 'full') {
                if ($names.Count -ne 36 -or 'health_compact' -notin $names) {
                    throw "Full surface tools/list mismatch: count=$($names.Count)"
                }
                $tool = 'health_compact'
            }
            else {
                $missing = @(@('symforge', 'symforge_edit', 'status') | Where-Object { $_ -notin $names })
                if ($names.Count -ne 3 -or $missing.Count -ne 0) {
                    throw "Compact surface tools/list mismatch: $($names -join ',')"
                }
                $symforgeTool = $listed.tools_list.tools | Where-Object name -eq 'symforge'
                if ((Get-ToolAnnotationHint -Tool $symforgeTool -Name 'readOnlyHint') -ne $true -or
                    (Get-ToolAnnotationHint -Tool $symforgeTool -Name 'openWorldHint') -ne $false) {
                    throw 'Compact symforge annotations do not match the amended read-only closed-world contract.'
                }
                foreach ($unsafeName in @('symforge_edit', 'status')) {
                    $unsafeTool = $listed.tools_list.tools | Where-Object name -eq $unsafeName
                    if ((Get-ToolAnnotationHint -Tool $unsafeTool -Name 'readOnlyHint') -eq $true) {
                        throw "Compact $unsafeName is incorrectly advertised as read-only."
                    }
                }
                $tool = 'symforge'
            }
            $prompt = if ($tool -eq 'symforge') {
                'Unmeasured wiring check. Call the SymForge symforge tool exactly once with query "repository overview", then report only whether it succeeded.'
            }
            else {
                "Unmeasured wiring check. Call the SymForge $tool tool exactly once, then report only whether it succeeded."
            }
            $run = Invoke-CodexProcess -Surface $surface -Prompt $prompt -TraceStem "wiring-$surface"
            if ($run.ExitCode -ne 0 -or $run.TimedOut) {
                throw "Codex wiring check failed for $surface (exit=$($run.ExitCode), timeout=$($run.TimedOut))."
            }
            $toolEvents = @(Get-CodexToolEvents -Records $run.TimedLines)
            if (-not (Test-CompletedMcpCall -ToolEvents $toolEvents -Name $tool)) {
                throw "Codex trace did not prove exactly one completed $tool call for $surface."
            }
            $compactHints = if ($surface -eq 'compact') {
                [ordered]@{
                    symforge_read_only = Get-ToolAnnotationHint -Tool ($listed.tools_list.tools | Where-Object name -eq 'symforge') -Name 'readOnlyHint'
                    symforge_open_world = Get-ToolAnnotationHint -Tool ($listed.tools_list.tools | Where-Object name -eq 'symforge') -Name 'openWorldHint'
                    symforge_edit_read_only = Get-ToolAnnotationHint -Tool ($listed.tools_list.tools | Where-Object name -eq 'symforge_edit') -Name 'readOnlyHint'
                    status_read_only = Get-ToolAnnotationHint -Tool ($listed.tools_list.tools | Where-Object name -eq 'status') -Name 'readOnlyHint'
                }
            }
            else { $null }
            [void]$receipts.Add([ordered]@{
                arm = $arm
                surface = $surface
                tool_count = $names.Count
                tool_names = $names
                compact_annotations = $compactHints
                codex_completed_tool = $tool
            })
        }
        finally {
            Remove-Fixture
        }
    }
    $receiptPath = Join-Path $script:EvidenceRoot 'wiring-receipt.json'
    if (Test-Path -LiteralPath $receiptPath) { throw "Refusing to overwrite wiring receipt: $receiptPath" }
    [IO.File]::WriteAllText(
        $receiptPath,
        ([ordered]@{
            symforge_binary = $script:SymForgeExe
            symforge_binary_sha256 = $script:SymForgeSha256
            arms = $receipts
        } | ConvertTo-Json -Depth 20)
    )
    'PASS: full and compact MCP wiring checks'
}

function Invoke-OneRun {
    param(
        [Parameter(Mandatory)][ValidateRange(1, 20)][int]$Id,
        [Parameter(Mandatory)][bool]$AllowIncompleteRerun
    )

    $schedule = @(Get-RunSchedule)
    $entry = $schedule | Where-Object RunId -eq $Id
    if ($null -eq $entry) { throw "Unknown run ID: $Id" }
    Initialize-RunEvidence -Id $Id -AllowIncompleteRerun $AllowIncompleteRerun
    $resultsPath = Join-Path $script:EvidenceRoot 'shakedown-results.jsonl'

    $surface = Get-SurfaceForArm -Arm $entry.Arm
    New-RunFixture
    $unexpected = @()
    try {
        $readiness = Get-ReadinessReceipt
        $run = Invoke-CodexProcess -Surface $surface -Prompt (Get-TaskPrompt -Task $entry.Task) -TraceStem ('run-{0:d2}' -f $Id)
        $lines = @($run.TimedLines | ForEach-Object { $_.Line })
        $incremental = $null
        $cumulative = $null
        try { $incremental = Get-CodexUsage -Lines $lines -Mode Incremental } catch {}
        try { $cumulative = Get-CodexUsage -Lines $lines -Mode Cumulative } catch {}
        $toolEvents = @(Get-CodexToolEvents -Records $run.TimedLines)
        $completedSymForge = @($toolEvents | Where-Object {
            $_.EventType -eq 'item.completed' -and $_.ItemType -eq 'mcp_tool_call' -and
            $_.Server -eq 'symforge'
        })
        $completedNative = @($toolEvents | Where-Object {
            $_.EventType -eq 'item.completed' -and $_.Channel -eq 'native'
        })
        $firstSymForge = $completedSymForge | Select-Object -First 1
        $firstSubstantive = $completedSymForge | Where-Object { $_.Name -notin @('health', 'health_compact', 'status') } | Select-Object -First 1
        $graderOut = Join-Path $script:EvidenceRoot ('run-{0:d2}-grader.md' -f $Id)
        [IO.File]::WriteAllText($graderOut, (Get-RedactedGraderAnswer -Answer $run.Answer))
        $unexpected = @(Get-UnexpectedFixtureChanges)
        $usageReviewRequired = $incremental -and $incremental.EventCount -gt 1
        $record = [ordered]@{
            schema_version = 2
            record_status = 'captured_ungraded'
            run_id = $Id
            block = $entry.Block
            task = $entry.Task
            arm = $entry.Arm
            surface = $surface
            fixture_commit = $script:FixtureCommit
            host = 'windows-local'
            model = $script:Model
            reasoning_level = $script:ReasoningLevel
            codex_version = $script:CodexVersion
            symforge_version = $readiness.SymForgeVersion
            symforge_binary_sha256 = $script:SymForgeSha256
            snapshot_hash = $readiness.SnapshotHash
            snapshot_hash_semantics = 'per-run worktree-materialized input bytes; informational, not a cross-run semantic equality key'
            golden_input_snapshot_hash = $readiness.GoldenInputHash
            snapshot_load_source = $readiness.LoadSource
            snapshot_verify_states = $readiness.ObservedStates
            snapshot_verify_mismatches = $readiness.Mismatches
            source_git_tree_oid = $readiness.GitTreeOid
            source_manifest_sha256 = $readiness.SourceManifestSha256
            source_file_count = $readiness.SourceFileCount
            repo_outline_sha256 = $readiness.RepoOutlineSha256
            repo_outline_bytes = $readiness.RepoOutlineBytes
            index_files = $readiness.Files
            parsed_files = $readiness.Parsed
            partial_files = $readiness.Partial
            failed_files = $readiness.Failed
            index_symbols = $readiness.Symbols
            residual_speed_confound = 'Measured startup still performs stat-all plus 10% spot verification; matched mtimes remove full reparse but not this symmetric product behavior.'
            readiness_verdict = $readiness.Verdict
            readiness_milliseconds = $readiness.Milliseconds
            started_at = $run.StartedAt
            finished_at = $run.FinishedAt
            wall_milliseconds = $run.WallMilliseconds
            exit_code = $run.ExitCode
            timed_out = $run.TimedOut
            usage_event_count = if ($incremental) { $incremental.EventCount } else { 0 }
            canonical_total_tokens = if ($cumulative) { $cumulative.TotalTokens } else { $null }
            canonical_token_rule = 'final turn.completed input_tokens + output_tokens; review required if event count exceeds one'
            usage_semantics_review_required = [bool]$usageReviewRequired
            incremental_total_tokens = if ($incremental) { $incremental.TotalTokens } else { $null }
            cumulative_total_tokens = if ($cumulative) { $cumulative.TotalTokens } else { $null }
            input_tokens = if ($cumulative) { $cumulative.InputTokens } else { $null }
            cached_input_tokens_informational = if ($incremental) { $incremental.CachedInputTokens } else { $null }
            output_tokens = if ($cumulative) { $cumulative.OutputTokens } else { $null }
            reasoning_output_tokens = if ($cumulative) { $cumulative.ReasoningOutputTokens } else { $null }
            tool_event_count = $toolEvents.Count
            tool_events = $toolEvents
            symforge_call_count = $completedSymForge.Count
            symforge_success_count = @($completedSymForge | Where-Object OutcomeClass -eq 'success').Count
            symforge_error_count = @($completedSymForge | Where-Object OutcomeClass -eq 'error').Count
            first_symforge_tool = if ($firstSymForge) { $firstSymForge.Name } else { $null }
            first_substantive_symforge_tool = if ($firstSubstantive) { $firstSubstantive.Name } else { $null }
            native_tool_count = $completedNative.Count
            raw_trace = $run.StdoutPath
            stderr_trace = $run.StderrPath
            raw_answer = $run.AnswerPath
            grader_answer = $graderOut
            configuration_diagnostic_count = $run.TraceDiagnostics.ConfigurationDiagnosticCount
            potential_secret_line_count = $run.TraceDiagnostics.PotentialSecretLineCount
            repository_clean = $unexpected.Count -eq 0
            unexpected_changes = $unexpected
            oracle_pass = $null
            oracle_failures = @()
            oracle_grade_count = 0
            exclusion = $null
        }
        [IO.File]::AppendAllText($resultsPath, (($record | ConvertTo-Json -Depth 20 -Compress) + [Environment]::NewLine))
        if ($run.TimedOut -or $run.ExitCode -ne 0 -or -not $run.Answer -or $null -eq $incremental) {
            throw "Run $Id trace is incomplete; result was preserved and execution is halted."
        }
        if ($usageReviewRequired) {
            throw "Run $Id emitted multiple usage events; explicit token-semantics review is required before continuing."
        }
    }
    finally {
        Remove-Fixture
    }
    if ($unexpected.Count -ne 0) { throw "Run $Id changed the fixture; see the compact result record." }
    "PASS: captured run $Id; awaiting blind oracle grade and checkpoint review"
}

function Get-RunSchedule {
    $rows = @(
        @{ RunId = 1; Block = 1; Task = 'S1'; Arm = 'A-full' }
        @{ RunId = 2; Block = 1; Task = 'S2'; Arm = 'C-compact' }
        @{ RunId = 3; Block = 1; Task = 'S1'; Arm = 'C-compact' }
        @{ RunId = 4; Block = 1; Task = 'S2'; Arm = 'A-full' }
        @{ RunId = 5; Block = 2; Task = 'S2'; Arm = 'A-full' }
        @{ RunId = 6; Block = 2; Task = 'S1'; Arm = 'C-compact' }
        @{ RunId = 7; Block = 2; Task = 'S2'; Arm = 'C-compact' }
        @{ RunId = 8; Block = 2; Task = 'S1'; Arm = 'A-full' }
        @{ RunId = 9; Block = 3; Task = 'S1'; Arm = 'C-compact' }
        @{ RunId = 10; Block = 3; Task = 'S2'; Arm = 'A-full' }
        @{ RunId = 11; Block = 3; Task = 'S1'; Arm = 'A-full' }
        @{ RunId = 12; Block = 3; Task = 'S2'; Arm = 'C-compact' }
        @{ RunId = 13; Block = 4; Task = 'S2'; Arm = 'C-compact' }
        @{ RunId = 14; Block = 4; Task = 'S1'; Arm = 'A-full' }
        @{ RunId = 15; Block = 4; Task = 'S2'; Arm = 'A-full' }
        @{ RunId = 16; Block = 4; Task = 'S1'; Arm = 'C-compact' }
        @{ RunId = 17; Block = 5; Task = 'S1'; Arm = 'A-full' }
        @{ RunId = 18; Block = 5; Task = 'S2'; Arm = 'C-compact' }
        @{ RunId = 19; Block = 5; Task = 'S1'; Arm = 'C-compact' }
        @{ RunId = 20; Block = 5; Task = 'S2'; Arm = 'A-full' }
    )

    foreach ($row in $rows) {
        [pscustomobject]$row
    }
}

function Get-CodexUsage {
    param(
        [Parameter(Mandatory)][string[]]$Lines,
        [Parameter(Mandatory)][ValidateSet('Incremental', 'Cumulative')][string]$Mode
    )

    $usages = @(
        foreach ($line in $Lines) {
            if ([string]::IsNullOrWhiteSpace($line)) { continue }
            try { $event = $line | ConvertFrom-Json -ErrorAction Stop } catch { continue }
            if ($event.type -eq 'turn.completed' -and $null -ne $event.usage) {
                [pscustomobject]@{
                    Input = [int64]$event.usage.input_tokens
                    CachedInput = [int64]$event.usage.cached_input_tokens
                    Output = [int64]$event.usage.output_tokens
                    ReasoningOutput = if ($event.usage.PSObject.Properties.Name -contains 'reasoning_output_tokens') { [int64]$event.usage.reasoning_output_tokens } else { 0 }
                }
            }
        }
    )

    if ($usages.Count -eq 0) { throw 'No turn.completed usage events found.' }
    if ($Mode -eq 'Cumulative') {
        $inputTokens = $usages[-1].Input
        $cachedInputTokens = $usages[-1].CachedInput
        $outputTokens = $usages[-1].Output
        $reasoningOutputTokens = $usages[-1].ReasoningOutput
    }
    else {
        $inputTokens = [int64](($usages | Measure-Object -Property Input -Sum).Sum)
        $cachedInputTokens = [int64](($usages | Measure-Object -Property CachedInput -Sum).Sum)
        $outputTokens = [int64](($usages | Measure-Object -Property Output -Sum).Sum)
        $reasoningOutputTokens = [int64](($usages | Measure-Object -Property ReasoningOutput -Sum).Sum)
    }

    [pscustomobject]@{
        Mode = $Mode
        EventCount = $usages.Count
        InputTokens = $inputTokens
        CachedInputTokens = $cachedInputTokens
        OutputTokens = $outputTokens
        ReasoningOutputTokens = $reasoningOutputTokens
        TotalTokens = $inputTokens + $outputTokens
    }
}

function Assert-FixturePath {
    param([Parameter(Mandatory)][string]$Path)

    $resolved = [System.IO.Path]::GetFullPath($Path).TrimEnd('\')
    $expected = [System.IO.Path]::GetFullPath($script:FixtureRoot).TrimEnd('\')
    $repo = $script:RepositoryRoot.TrimEnd('\')
    if (-not $resolved.Equals($expected, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing fixture operation outside the declared path: $resolved"
    }
    if ($resolved.StartsWith($repo + '\', [System.StringComparison]::OrdinalIgnoreCase) -or
        $resolved.Equals($repo, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing fixture operation inside the development repository: $resolved"
    }
    $resolved
}

function Assert-Equal {
    param(
        [Parameter(Mandatory)]$Actual,
        [Parameter(Mandatory)]$Expected,
        [Parameter(Mandatory)][string]$Message
    )

    if ($Actual -ne $Expected) {
        throw "$Message (expected=$Expected actual=$Actual)"
    }
}

function Invoke-SelfTest {
    $schedule = @(Get-RunSchedule)
    Assert-Equal $schedule.Count 20 'schedule length'

    foreach ($task in @('S1', 'S2')) {
        foreach ($arm in @('A-full', 'C-compact')) {
            $count = @($schedule | Where-Object { $_.Task -eq $task -and $_.Arm -eq $arm }).Count
            Assert-Equal $count 5 "schedule balance for $task/$arm"
        }
    }

    $events = @(
        '{"type":"turn.completed","usage":{"input_tokens":10,"cached_input_tokens":3,"output_tokens":2,"reasoning_output_tokens":1}}',
        '{"type":"turn.completed","usage":{"input_tokens":20,"cached_input_tokens":8,"output_tokens":5,"reasoning_output_tokens":4}}'
    )
    $incremental = Get-CodexUsage -Lines $events -Mode Incremental
    Assert-Equal $incremental.TotalTokens 37 'incremental token total'
    Assert-Equal $incremental.ReasoningOutputTokens 5 'incremental reasoning token total'
    $cumulative = Get-CodexUsage -Lines $events -Mode Cumulative
    Assert-Equal $cumulative.TotalTokens 25 'cumulative token total'
    Assert-Equal $cumulative.ReasoningOutputTokens 4 'cumulative reasoning token total'

    $failedTool = @([pscustomobject]@{
        EventType = 'item.completed'; ItemType = 'mcp_tool_call'; Name = 'status'
        Server = 'symforge'; Status = 'failed'; OutcomeClass = 'error'
    })
    Assert-Equal (Test-CompletedMcpCall -ToolEvents $failedTool -Name 'status') $false 'failed MCP call rejection'
    $completedTool = @([pscustomobject]@{
        EventType = 'item.completed'; ItemType = 'mcp_tool_call'; Name = 'status'
        Server = 'symforge'; Status = 'completed'; OutcomeClass = 'success'
    })
    Assert-Equal (Test-CompletedMcpCall -ToolEvents $completedTool -Name 'status') $true 'completed MCP call acceptance'

    $annotatedTool = '{"annotations":{"readOnlyHint":true,"openWorldHint":false}}' | ConvertFrom-Json
    Assert-Equal (Get-ToolAnnotationHint -Tool $annotatedTool -Name 'readOnlyHint') $true 'read-only annotation extraction'
    Assert-Equal (Get-ToolAnnotationHint -Tool $annotatedTool -Name 'openWorldHint') $false 'closed-world annotation extraction'
    $unannotatedTool = '{}' | ConvertFrom-Json
    Assert-Equal ($null -eq (Get-ToolAnnotationHint -Tool $unannotatedTool -Name 'readOnlyHint')) $true 'missing annotation extraction'

    $cleanTrace = Get-TraceDiagnostics -Stdout '{"type":"turn.completed"}' -Stderr ''
    Assert-Equal $cleanTrace.ConfigurationDiagnosticCount 0 'clean trace configuration diagnostics'
    $dirtyTrace = Get-TraceDiagnostics -Stdout 'malformed agent role in user configuration' -Stderr ''
    Assert-Equal $dirtyTrace.ConfigurationDiagnosticCount 1 'configuration diagnostic detection'

    $grader = Get-RedactedGraderAnswer -Answer '[x](C:/Users/example/AppData/Local/Temp/symforge-token-token-shakedown-a10ff102/src/lib.rs:1)'
    Assert-Equal $grader.Contains('C:/Users/', [System.StringComparison]::OrdinalIgnoreCase) $false 'grader home-path redaction'
    Assert-Equal $grader.Contains('symforge-token-token-shakedown-a10ff102') $true 'grader preserves fabricated fixture name'
    $missingArtifactRoot = Join-Path ([IO.Path]::GetTempPath()) ("symforge-selftest-missing-artifacts-{0}" -f [Guid]::NewGuid().ToString('N'))
    Assert-Equal @(Get-CompactRunArtifactPaths -Id 20 -Root $missingArtifactRoot).Count 0 'missing compact artifacts stay an empty array'

    $pendingSnapshot = Get-SnapshotVerificationReceipt -Text @'
Status: Ready | Files: 10 indexed (10 parsed, 0 partial, 0 failed) | Symbols: 20 | Loaded: 5ms
Snapshot: load_source=snapshot_restore verify=pending
'@
    Assert-Equal $pendingSnapshot.State 'pending' 'pending snapshot state parsing'
    Assert-Equal $pendingSnapshot.IsComplete $false 'pending snapshot completion parsing'

    $runningSnapshot = Get-SnapshotVerificationReceipt -Text @'
Status: Ready | Files: 10 indexed (10 parsed, 0 partial, 0 failed) | Symbols: 20 | Loaded: 5ms
Snapshot: load_source=snapshot_restore verify=running
'@
    Assert-Equal $runningSnapshot.State 'running' 'running snapshot state parsing'

    $completedSnapshot = Get-SnapshotVerificationReceipt -Text @'
Status: Ready | Files: 10 indexed (10 parsed, 0 partial, 0 failed) | Symbols: 20 | Loaded: 5ms
Snapshot: load_source=snapshot_restore verify=completed mismatches=0
'@
    Assert-Equal $completedSnapshot.State 'completed' 'completed snapshot state parsing'
    Assert-Equal $completedSnapshot.IsComplete $true 'completed snapshot completion parsing'
    Assert-Equal $completedSnapshot.Mismatches 0 'completed snapshot mismatch parsing'
    Assert-Equal $completedSnapshot.Files 10 'snapshot file count parsing'
    Assert-Equal $completedSnapshot.Symbols 20 'snapshot symbol count parsing'

    $mismatchedSnapshot = Get-SnapshotVerificationReceipt -Text @'
Status: Ready | Files: 10 indexed (9 parsed, 1 partial, 0 failed) | Symbols: 20 | Loaded: 5ms
Snapshot: load_source=snapshot_restore verify=completed mismatches=2 showing=2 omitted=0 paths=a.rs, b.rs
'@
    Assert-Equal $mismatchedSnapshot.Mismatches 2 'snapshot mismatch count is separate from state'

    Assert-Equal (Get-Utf8Sha256 -Text 'abc') 'BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD' 'stable UTF-8 SHA-256'
    $manifestForward = Get-ManifestFingerprintFromRecords -Records @(
        [pscustomobject]@{ Path = 'src/b.rs'; Mode = '100644'; Sha256 = ('B' * 64) }
        [pscustomobject]@{ Path = 'src/a.rs'; Mode = '100755'; Sha256 = ('A' * 64) }
    )
    $manifestReverse = Get-ManifestFingerprintFromRecords -Records @(
        [pscustomobject]@{ Path = 'src/a.rs'; Mode = '100755'; Sha256 = ('A' * 64) }
        [pscustomobject]@{ Path = 'src/b.rs'; Mode = '100644'; Sha256 = ('B' * 64) }
    )
    Assert-Equal $manifestForward.Hash $manifestReverse.Hash 'source manifest is order independent'
    Assert-Equal $manifestForward.FileCount 2 'source manifest file count'

    $deadlineStart = [DateTimeOffset]::Parse('2026-07-13T00:00:00Z')
    Assert-Equal (Test-DeadlineExpired -StartedAt $deadlineStart -TimeoutMs 1000 -Now $deadlineStart.AddMilliseconds(999)) $false 'snapshot polling before deadline'
    Assert-Equal (Test-DeadlineExpired -StartedAt $deadlineStart -TimeoutMs 1000 -Now $deadlineStart.AddMilliseconds(1000)) $true 'snapshot polling at deadline'

    $semanticBaseline = [pscustomobject]@{
        fixture_commit = 'commit'; git_tree_oid = 'tree'; source_manifest_sha256 = 'manifest'
        source_file_count = 2; golden_input_sha256 = 'golden'; repo_outline_sha256 = 'outline'; index_files = 10
        parsed_files = 9; partial_files = 1; failed_files = 0; symbols = 20
        symforge_binary_sha256 = 'binary'; symforge_version = '8.14.1'
    }
    $semanticReceipt = [pscustomobject]@{
        FixtureCommit = 'commit'; GitTreeOid = 'tree'; SourceManifestSha256 = 'manifest'
        SourceFileCount = 2; GoldenInputHash = 'golden'; RepoOutlineSha256 = 'outline'; Files = 10
        Parsed = 9; Partial = 1; Failed = 0; Symbols = 20
        SymForgeBinarySha256 = 'binary'; SymForgeVersion = '8.14.1'
    }
    Assert-SemanticBaselineMatch -Baseline $semanticBaseline -Receipt $semanticReceipt
    $semanticReceipt.RepoOutlineSha256 = 'changed'
    $semanticMismatchRejected = $false
    try { Assert-SemanticBaselineMatch -Baseline $semanticBaseline -Receipt $semanticReceipt }
    catch { $semanticMismatchRejected = $true }
    Assert-Equal $semanticMismatchRejected $true 'semantic baseline mismatch rejection'

    [void](Assert-FixturePath -Path $script:FixtureRoot)
    [void](Assert-IsolatedCodexHomePath -Path (Join-Path $script:RawEvidenceRoot '.codex-home-self-test'))
    $refused = $false
    try {
        [void](Assert-FixturePath -Path $script:RepositoryRoot)
    }
    catch {
        $refused = $true
    }
    Assert-Equal $refused $true 'repository path cleanup refusal'

    'PASS: token surface shakedown self-test'
}

switch ($Action) {
    'SelfTest' { Invoke-SelfTest }
    'Prepare' { Invoke-Prepare }
    'WiringCheck' { Invoke-WiringCheck }
    'MaterializeBaseline' { Invoke-MaterializeBaseline }
    'Run' { Invoke-OneRun -Id $RunId -AllowIncompleteRerun ([bool]$AllowRerunIncomplete) }
    'Grade' {
        if (-not $OracleVerdict) { throw 'Grade requires -OracleVerdict Pass or Fail.' }
        Set-OracleGrade -Id $RunId -Verdict $OracleVerdict -Failures $OracleFailures
    }
    'Cleanup' { Remove-Fixture }
    'All' { throw 'All is gated until run 01 receives Claude Opus approval.' }
    default { throw "Action '$Action' is not implemented yet." }
}
