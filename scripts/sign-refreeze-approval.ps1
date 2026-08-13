# Sign the Feature 020 V11 refreeze approval.
#
# Run this and answer one prompt. It never prints, copies, or logs key material —
# it passes the key path straight to ssh-keygen and reports only public digests.
#
#   pwsh scripts/sign-refreeze-approval.ps1
#
# What you are signing is printed before the prompt, so you can read it first.

param(
    [string]$ApprovalDir = "$HOME\symforge-approval",
    [string]$Namespace   = "symforge-feature-020-refreeze-v11"
)

$ErrorActionPreference = 'Stop'

$approval = Join-Path $ApprovalDir 'approval.json'
$signature = "$approval.sig"

if (-not (Test-Path $approval)) {
    Write-Host "No approval record at $approval." -ForegroundColor Red
    Write-Host "The agent generates it after the digest chain is final. Nothing to sign yet."
    exit 1
}

Write-Host ""
Write-Host "===== WHAT YOU ARE SIGNING =====" -ForegroundColor Cyan
Get-Content $approval | Write-Host
Write-Host "================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "Read the above. It names the exact manifest and attestation digests this"
Write-Host "signature binds. If they do not match what the agent told you, stop."
Write-Host ""

# Offer the keys that actually exist, so there is nothing to remember.
$candidates = @()
if (Test-Path "$HOME\.ssh") {
    $candidates = Get-ChildItem "$HOME\.ssh" -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^id_' -and $_.Name -notmatch '\.pub$' } |
        Select-Object -ExpandProperty FullName
}

if ($candidates.Count -gt 0) {
    Write-Host "Signing keys found:" -ForegroundColor Green
    for ($i = 0; $i -lt $candidates.Count; $i++) {
        Write-Host ("  [{0}] {1}" -f ($i + 1), $candidates[$i])
    }
    Write-Host "  [p]  type a different path"
    Write-Host ""
    $choice = Read-Host "Which key? (number, or p)"
    if ($choice -match '^\d+$' -and [int]$choice -ge 1 -and [int]$choice -le $candidates.Count) {
        $KeyPath = $candidates[[int]$choice - 1]
    } else {
        $KeyPath = Read-Host "Full path to your private signing key"
    }
} else {
    $KeyPath = Read-Host "Full path to your private signing key"
}

if (-not (Test-Path $KeyPath)) {
    Write-Host "No key at $KeyPath" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Signing $approval with namespace $Namespace ..." -ForegroundColor Cyan

# ssh-keygen reads the key itself; the passphrase prompt (if any) is its own.
& ssh-keygen -Y sign -f $KeyPath -n $Namespace $approval
if ($LASTEXITCODE -ne 0) {
    Write-Host "Signing failed (exit $LASTEXITCODE). Nothing was written." -ForegroundColor Red
    exit $LASTEXITCODE
}

if (-not (Test-Path $signature)) {
    Write-Host "ssh-keygen reported success but produced no signature file." -ForegroundColor Red
    Write-Host "Refusing to report success for something that did not happen."
    exit 1
}

Write-Host ""
Write-Host "Signature written: $signature" -ForegroundColor Green

# Verify immediately. A signature nobody checked is not evidence.
$allowed = Join-Path $ApprovalDir 'allowed_signers'
if (Test-Path $allowed) {
    $identity = (Get-Content $allowed -First 1).Split(' ')[0]
    Write-Host "Verifying against $allowed as $identity ..." -ForegroundColor Cyan
    Get-Content $approval -Raw -Encoding Byte | Out-Null
    & ssh-keygen -Y verify -f $allowed -I $identity -n $Namespace -s $signature < $approval
    if ($LASTEXITCODE -eq 0) {
        Write-Host "VERIFIED." -ForegroundColor Green
    } else {
        Write-Host "VERIFICATION FAILED (exit $LASTEXITCODE)." -ForegroundColor Red
        Write-Host "The signature exists but does not verify. Tell the agent before proceeding."
        exit 1
    }
} else {
    Write-Host "No allowed_signers at $allowed, so the signature was not verified here." -ForegroundColor Yellow
    Write-Host "CI will verify it. Reporting this rather than claiming a check that did not run."
}

Write-Host ""
Write-Host "Done. Tell the agent the signature is in place." -ForegroundColor Green
