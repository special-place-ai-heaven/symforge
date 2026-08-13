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
# This used to match `id_*` only, so a key named anything else -- vps_ai_ed25519,
# say -- was invisible and had to be typed by hand. A private key is identified
# by having a `.pub` sibling, not by its name.
$candidates = @()
if (Test-Path "$HOME\.ssh") {
    $candidates = Get-ChildItem "$HOME\.ssh" -File -ErrorAction SilentlyContinue |
        Where-Object {
            $_.Extension -ne '.pub' -and
            $_.Name -notin @('known_hosts', 'known_hosts.old', 'config', 'authorized_keys') -and
            (Test-Path "$($_.FullName).pub")
        } |
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
    # ssh-keygen reads the signed message from stdin. `< $approval` is a PARSE
    # error in PowerShell -- '<' is reserved -- so the earlier spelling meant this
    # whole script failed to parse and never ran once. Piping would also be wrong:
    # it would re-encode the bytes the signature covers. Start-Process feeds the
    # file itself.
    $verification = Start-Process -FilePath 'ssh-keygen' -NoNewWindow -Wait -PassThru `
        -ArgumentList @('-Y', 'verify', '-f', $allowed, '-I', $identity, '-n', $Namespace, '-s', $signature) `
        -RedirectStandardInput $approval
    if ($verification.ExitCode -eq 0) {
        Write-Host "VERIFIED." -ForegroundColor Green
    } else {
        Write-Host "VERIFICATION FAILED (exit $($verification.ExitCode))." -ForegroundColor Red
        Write-Host "The signature exists but does not verify. Tell the agent before proceeding."
        exit 1
    }
} else {
    Write-Host "No allowed_signers at $allowed, so the signature was not verified here." -ForegroundColor Yellow
    Write-Host "CI will verify it. Reporting this rather than claiming a check that did not run."
}

Write-Host ""
Write-Host "Done. Tell the agent the signature is in place." -ForegroundColor Green
