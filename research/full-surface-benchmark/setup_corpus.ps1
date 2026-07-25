[CmdletBinding()]
param(
    [string] $BenchRoot = 'C:\AI_STUFF\BENCHMARKS\symforge-8.14.0-surface',
    [string] $ProjectRoot = 'C:\AI_STUFF\PROGRAMMING\symforge',
    [switch] $Resume
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Assert-ValidAlias {
    param([Parameter(Mandatory)] [string] $Alias)

    $isReservedDevice = $Alias -match '^(?i:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?$'
    if ($Alias.Length -gt 64 -or
        $Alias -notmatch '^[a-z0-9][a-z0-9._-]*$' -or
        $Alias.EndsWith('.') -or
        $Alias -in @('.', '..') -or
        $isReservedDevice) {
        throw 'Corpus lock contains an invalid repository alias.'
    }
}

function Get-SafeGitHubUrl {
    param(
        [Parameter(Mandatory)] [string] $Url,
        [Parameter(Mandatory)] [string] $Alias
    )

    $invalid = "Repository '$Alias' has an invalid source URL."
    if ($Url -match '[\x00-\x20\x7f]' -or $Url.Contains('\') -or $Url.Contains('%')) {
        throw $invalid
    }

    [Uri] $uri = $null
    if (-not [Uri]::TryCreate($Url, [UriKind]::Absolute, [ref] $uri) -or
        $uri.Scheme -ne 'https' -or
        $uri.IdnHost -ne 'github.com' -or
        $uri.Port -ne 443 -or
        $uri.UserInfo -or
        $uri.Query -or
        $uri.Fragment) {
        throw $invalid
    }

    $segments = @($uri.AbsolutePath.Trim('/') -split '/')
    if ($segments.Count -ne 2 -or
        $segments[0] -notmatch '^[A-Za-z0-9_.-]+$' -or
        $segments[1] -notmatch '^[A-Za-z0-9_.-]+$' -or
        $segments[0] -in @('.', '..') -or
        $segments[1] -in @('.', '..')) {
        throw $invalid
    }

    return "https://github.com/$($segments[0])/$($segments[1])"
}

function Assert-PlainDirectory {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $Description
    )

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "$Description must be an ordinary directory."
    }
    $item = Get-Item -Force -LiteralPath $Path
    if (-not $item.PSIsContainer -or
        ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "$Description must be an ordinary directory."
    }
}

function Get-ContainedSourceDestination {
    param(
        [Parameter(Mandatory)] [string] $SourcesRoot,
        [Parameter(Mandatory)] [string] $Alias
    )

    Assert-ValidAlias $Alias
    $source = [IO.Path]::GetFullPath($SourcesRoot).TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar
    )
    $destination = [IO.Path]::GetFullPath((Join-Path $source $Alias))
    $parent = [IO.Path]::GetDirectoryName($destination)
    $prefix = $source + [IO.Path]::DirectorySeparatorChar
    if (-not $parent.Equals($source, [StringComparison]::OrdinalIgnoreCase) -or
        -not $destination.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Repository '$Alias' resolves outside the sources directory."
    }

    if (Test-Path -LiteralPath $destination) {
        Assert-PlainDirectory $destination "Repository '$Alias' destination"
    }
    return $destination
}

function Invoke-Git {
    param(
        [AllowNull()] [string] $Repository,
        [Parameter(Mandatory)] [string[]] $Arguments,
        [int[]] $AllowedExitCodes = @(0),
        [switch] $Raw
    )

    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = 'git'
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $start.StandardOutputEncoding = [Text.Encoding]::UTF8
    $start.StandardErrorEncoding = [Text.Encoding]::UTF8
    $start.Environment.Clear()

    foreach ($name in @('PATH', 'Path', 'PATHEXT', 'SystemRoot', 'WINDIR', 'ComSpec', 'TEMP', 'TMP')) {
        $value = [Environment]::GetEnvironmentVariable($name, 'Process')
        if ($value -and -not $start.Environment.ContainsKey($name)) {
            $start.Environment[$name] = $value
        }
    }
    $start.Environment['GIT_CONFIG_NOSYSTEM'] = '1'
    $start.Environment['GIT_CONFIG_SYSTEM'] = $script:IsolatedGitConfig
    $start.Environment['GIT_CONFIG_GLOBAL'] = $script:IsolatedGitConfig
    $start.Environment['GIT_TEMPLATE_DIR'] = $script:EmptyGitTemplate
    $start.Environment['GIT_TERMINAL_PROMPT'] = '0'
    $start.Environment['GCM_INTERACTIVE'] = 'Never'
    $start.Environment['GIT_LFS_SKIP_SMUDGE'] = '1'
    $start.Environment['GIT_PROTOCOL_FROM_USER'] = '0'

    foreach ($argument in @(
        '-c', 'credential.helper=',
        '-c', 'credential.interactive=never',
        '-c', 'core.askPass=',
        '-c', "core.hooksPath=$script:EmptyGitHooks",
        '-c', 'core.fsmonitor=false'
    )) {
        $start.ArgumentList.Add($argument)
    }
    if ($Repository) {
        $start.ArgumentList.Add('-c')
        $start.ArgumentList.Add("core.worktree=$Repository")
        $start.ArgumentList.Add('-c')
        $start.ArgumentList.Add('core.bare=false')
        $start.ArgumentList.Add('-C')
        $start.ArgumentList.Add($Repository)
    }
    foreach ($argument in $Arguments) {
        $start.ArgumentList.Add($argument)
    }

    $process = [Diagnostics.Process]::new()
    $process.StartInfo = $start
    try {
        if (-not $process.Start()) {
            throw 'Unable to start Git.'
        }
        $stdoutTask = $process.StandardOutput.ReadToEndAsync()
        $stderrTask = $process.StandardError.ReadToEndAsync()
        $process.WaitForExit()
        $stdout = $stdoutTask.GetAwaiter().GetResult()
        $null = $stderrTask.GetAwaiter().GetResult()
        if ($AllowedExitCodes -notcontains $process.ExitCode) {
            throw "Git operation failed with exit code $($process.ExitCode)."
        }
    }
    finally {
        $process.Dispose()
    }

    if ($Raw) {
        return $stdout
    }
    if (-not $stdout) {
        return @()
    }
    return @($stdout.TrimEnd([char] 13, [char] 10) -split '\r?\n')
}

function Get-GitBlobObjectId {
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $ExpectedObjectId,
        [switch] $Symlink
    )

    $algorithm = switch ($ExpectedObjectId.Length) {
        40 { [Security.Cryptography.HashAlgorithmName]::SHA1; break }
        64 { [Security.Cryptography.HashAlgorithmName]::SHA256; break }
        default { throw 'Pinned commit uses an unsupported Git object format.' }
    }
    if (-not (Test-Path -LiteralPath $Path)) {
        throw 'Tracked worktree blob verification failed.'
    }
    $item = Get-Item -Force -LiteralPath $Path
    [byte[]] $linkBytes = $null
    if ($Symlink -and ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        if (-not $item.LinkTarget) {
            throw 'Tracked worktree blob verification failed.'
        }
        $linkBytes = [Text.Encoding]::UTF8.GetBytes($item.LinkTarget.Replace('\', '/'))
        [long] $length = $linkBytes.Length
    }
    else {
        if (-not (Test-Path -LiteralPath $Path -PathType Leaf) -or
            ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Tracked worktree blob verification failed.'
        }
        [long] $length = $item.Length
    }

    $hasher = [Security.Cryptography.IncrementalHash]::CreateHash($algorithm)
    try {
        $header = [Text.Encoding]::ASCII.GetBytes("blob $length$([char] 0)")
        $hasher.AppendData($header)
        if ($null -ne $linkBytes) {
            $hasher.AppendData($linkBytes)
        }
        else {
            $buffer = [byte[]]::new(1MB)
            $stream = [IO.File]::Open($Path, 'Open', 'Read', 'Read')
            try {
                while (($count = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
                    $hasher.AppendData($buffer, 0, $count)
                }
            }
            finally {
                $stream.Dispose()
            }
        }
        return [Convert]::ToHexString($hasher.GetHashAndReset()).ToLowerInvariant()
    }
    finally {
        $hasher.Dispose()
    }
}

function Assert-PinnedWorktreeBlobs {
    param(
        [Parameter(Mandatory)] [string] $Repository,
        [Parameter(Mandatory)] [string] $Commit,
        [Parameter(Mandatory)] [string] $Alias
    )

    $rawTree = Invoke-Git -Repository $Repository -Arguments @(
        'ls-tree', '-r', '-z', '--full-tree', $Commit
    ) -Raw
    $prefix = [IO.Path]::GetFullPath($Repository).TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar
    ) + [IO.Path]::DirectorySeparatorChar
    [int] $verified = 0
    foreach ($record in $rawTree.Split([char] 0, [StringSplitOptions]::RemoveEmptyEntries)) {
        $match = [regex]::Match(
            $record,
            '\A([0-9]{6}) (blob|commit) ([0-9a-f]{40}|[0-9a-f]{64})\t([\s\S]+)\z'
        )
        if (-not $match.Success) {
            throw "Repository '$Alias' has an invalid pinned tree entry."
        }
        if ($match.Groups[2].Value -eq 'commit') {
            continue
        }

        $relative = $match.Groups[4].Value
        $path = [IO.Path]::GetFullPath((Join-Path $Repository $relative))
        if (-not $path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Repository '$Alias' has a tracked path outside its worktree."
        }
        $blobParameters = @{
            Path = $path
            ExpectedObjectId = $match.Groups[3].Value
            Symlink = ($match.Groups[1].Value -eq '120000')
        }
        $actual = Get-GitBlobObjectId @blobParameters
        if ($actual -ne $match.Groups[3].Value) {
            throw "Repository '$Alias' failed pinned worktree blob verification."
        }
        $verified += 1
    }
    if ($verified -eq 0) {
        throw "Repository '$Alias' pinned tree contained no blobs."
    }
}

function Get-Sha256Text {
    param([Parameter(Mandatory)] [string] $Text)

    $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
    $hash = [Security.Cryptography.SHA256]::HashData($bytes)
    return [Convert]::ToHexString($hash).ToLowerInvariant()
}

function Test-BinaryPrefix {
    param([Parameter(Mandatory)] [string] $Path)

    $buffer = [byte[]]::new(8192)
    $stream = [IO.File]::Open($Path, 'Open', 'Read', 'ReadWrite')
    try {
        $count = $stream.Read($buffer, 0, $buffer.Length)
        for ($index = 0; $index -lt $count; $index += 1) {
            if ($buffer[$index] -eq 0) {
                return $true
            }
        }
        return $false
    }
    finally {
        $stream.Dispose()
    }
}

$project = [IO.Path]::GetFullPath($ProjectRoot)
$root = [IO.Path]::GetFullPath($BenchRoot)
$separator = [IO.Path]::DirectorySeparatorChar

if ($root.Equals($project, [StringComparison]::OrdinalIgnoreCase) -or
    $root.StartsWith($project + $separator, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Benchmark root must be outside the SymForge checkout.'
}

if ((Test-Path -LiteralPath $root) -and -not $Resume) {
    throw "Refusing to reuse an existing benchmark root: $root"
}

$lockPath = Join-Path $PSScriptRoot 'corpus.lock.json'
$lock = Get-Content -Raw -LiteralPath $lockPath | ConvertFrom-Json
$aliases = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
$safeUrls = @{}
foreach ($repo in $lock.repositories) {
    $alias = [string] $repo.alias
    Assert-ValidAlias $alias
    if (-not $aliases.Add($alias)) {
        throw 'Corpus lock contains duplicate repository aliases.'
    }
    $safeUrls[$alias] = Get-SafeGitHubUrl -Url ([string] $repo.url) -Alias $alias
    if ([string] $repo.commit -notmatch '^[0-9a-f]{40}$') {
        throw "Repository '$alias' has an invalid pinned commit."
    }
    if ([int] $repo.history_depth -lt 1) {
        throw "Repository '$alias' has an invalid history depth."
    }
}

if (-not (Test-Path -LiteralPath $root)) {
    New-Item -ItemType Directory -Path $root | Out-Null
}
Assert-PlainDirectory $root 'Benchmark root'
$sourceRoot = New-Item -ItemType Directory -Path (Join-Path $root 'sources') -Force
Assert-PlainDirectory $sourceRoot.FullName 'Sources root'
New-Item -ItemType Directory -Path (Join-Path $root 'runs') -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $root 'work') -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $root 'artifacts') -Force | Out-Null

$isolatedGitConfig = Join-Path $root 'gitconfig.empty'
if (-not (Test-Path -LiteralPath $isolatedGitConfig)) {
    New-Item -ItemType File -Path $isolatedGitConfig | Out-Null
}
else {
    $configItem = Get-Item -Force -LiteralPath $isolatedGitConfig
    if ($configItem.PSIsContainer -or
        ($configItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -or
        $configItem.Length -ne 0) {
        throw 'Isolated Git configuration must remain an empty regular file.'
    }
}
$emptyGitTemplate = Join-Path $root 'git-template.empty'
$emptyGitHooks = Join-Path $root 'git-hooks.empty'
foreach ($emptyDirectory in @($emptyGitTemplate, $emptyGitHooks)) {
    if (-not (Test-Path -LiteralPath $emptyDirectory)) {
        New-Item -ItemType Directory -Path $emptyDirectory | Out-Null
    }
    Assert-PlainDirectory $emptyDirectory 'Git isolation directory'
    if (@(Get-ChildItem -Force -LiteralPath $emptyDirectory).Count -ne 0) {
        throw 'Git isolation directories must remain empty.'
    }
}
$script:IsolatedGitConfig = $isolatedGitConfig
$script:EmptyGitTemplate = $emptyGitTemplate
$script:EmptyGitHooks = $emptyGitHooks

$rows = [Collections.Generic.List[object]]::new()

foreach ($repo in $lock.repositories) {
    $alias = [string] $repo.alias
    $destination = Get-ContainedSourceDestination -SourcesRoot $sourceRoot.FullName -Alias $alias
    if (-not (Test-Path -LiteralPath $destination)) {
        Invoke-Git -Repository $null -Arguments @(
            'init', '--quiet', "--template=$emptyGitTemplate", $destination
        ) | Out-Null

        Invoke-Git $destination @('config', 'core.autocrlf', 'false') | Out-Null
        Invoke-Git $destination @('config', 'core.eol', 'lf') | Out-Null
        Invoke-Git $destination @('config', 'core.filemode', 'false') | Out-Null
        Invoke-Git $destination @('config', 'core.longpaths', 'true') | Out-Null
        Invoke-Git $destination @('config', 'core.quotepath', 'false') | Out-Null
        Invoke-Git $destination @('remote', 'add', 'origin', $safeUrls[$alias]) | Out-Null
        Invoke-Git $destination @(
            'fetch', '--quiet', '--no-tags', "--depth=$($repo.history_depth)",
            'origin', $repo.commit
        ) | Out-Null
        Invoke-Git $destination @('checkout', '--quiet', '--detach', 'FETCH_HEAD') | Out-Null
    }
    elseif (-not $Resume) {
        throw "Source destination already exists: $destination"
    }

    Assert-PlainDirectory (Join-Path $destination '.git') "Repository '$alias' Git metadata"
    $origin = @(Invoke-Git $destination @('remote', 'get-url', 'origin'))
    if ($origin.Count -ne 1 -or $origin[0].Trim() -cne $safeUrls[$alias]) {
        throw "Repository '$alias' has an unexpected origin configuration."
    }

    $head = (Invoke-Git $destination @('rev-parse', 'HEAD') | Select-Object -First 1).Trim()
    if ($head -ne $repo.commit) {
        throw "$($repo.alias) checked out $head instead of $($repo.commit)"
    }

    $status = @(Invoke-Git $destination @('status', '--porcelain=v1'))
    if ($status.Count -gt 0) {
        throw "$($repo.alias) is dirty immediately after checkout"
    }
    if ($Resume) {
        Assert-PinnedWorktreeBlobs -Repository $destination -Commit $repo.commit -Alias $alias
    }

    Invoke-Git $destination @('fsck', '--connectivity-only', '--no-dangling') | Out-Null

    $partialCloneConfig = @(Invoke-Git -Repository $destination -Arguments @(
        'config', '--get-regexp', '^remote\.origin\.(promisor|partialclonefilter)$'
    ) -AllowedExitCodes @(0, 1))
    if ($partialCloneConfig.Count -gt 0) {
        throw "$($repo.alias) unexpectedly has promisor/partial-clone configuration"
    }

    $missing = @(
        Invoke-Git $destination @('rev-list', '--objects', '--missing=print', 'HEAD') |
            Where-Object { $_.StartsWith('?') }
    )
    if ($missing.Count -gt 0) {
        throw "$($repo.alias) has missing Git objects"
    }

    $tracked = @(Invoke-Git $destination @('ls-files'))
    [long] $trackedBytes = 0
    [int] $binaryFiles = 0
    $extensionCounts = @{}
    foreach ($relative in $tracked) {
        $path = Join-Path $destination $relative
        if (Test-Path -LiteralPath $path -PathType Leaf) {
            $trackedBytes += (Get-Item -LiteralPath $path).Length
            $extension = [IO.Path]::GetExtension($relative).ToLowerInvariant()
            if (-not $extension) {
                $extension = '<none>'
            }
            $extensionCounts[$extension] = 1 + [int]($extensionCounts[$extension] ?? 0)

            if (Test-BinaryPrefix $path) {
                $binaryFiles += 1
            }
        }
    }

    $indexRows = (Invoke-Git $destination @('ls-files', '--stage')) -join "`n"
    $showRefs = (Invoke-Git $destination @('show-ref', '--head')) -join "`n"
    $tree = (Invoke-Git $destination @('rev-parse', 'HEAD^{tree}') | Select-Object -First 1).Trim()
    $commitCount = [int](Invoke-Git $destination @('rev-list', '--count', 'HEAD') | Select-Object -First 1)
    $isShallow = ((Invoke-Git $destination @('rev-parse', '--is-shallow-repository') | Select-Object -First 1).Trim() -eq 'true')

    $row = [ordered]@{
        alias = $repo.alias
        source_url_sha256 = Get-Sha256Text $safeUrls[$alias]
        commit = $head
        tree = $tree
        history_depth_requested = [int]$repo.history_depth
        commit_count_present = $commitCount
        is_shallow = $isShallow
        stratum = $repo.stratum
        primary_language = $repo.primary_language
        overlay_language = $repo.overlay_language
        tracked_files = $tracked.Count
        tracked_worktree_bytes = $trackedBytes
        binary_files = $binaryFiles
        extension_counts = [ordered]@{}
        index_manifest_sha256 = Get-Sha256Text $indexRows
        show_ref_sha256 = Get-Sha256Text $showRefs
    }
    foreach ($extension in ($extensionCounts.Keys | Sort-Object)) {
        $row.extension_counts[$extension] = $extensionCounts[$extension]
    }
    $rows.Add([pscustomobject]$row)
    Write-Host "$($repo.alias): $($tracked.Count) tracked files, $trackedBytes bytes"
}

$manifest = [ordered]@{
    version = 2
    corpus_lock_sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $lockPath).Hash.ToLowerInvariant()
    git_version = (Invoke-Git -Repository $null -Arguments @('--version') | Select-Object -First 1)
    root = $root
    repositories = $rows
}

$manifestPath = Join-Path $root 'corpus-manifest.json'
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM
Write-Host "Corpus manifest: $manifestPath"
