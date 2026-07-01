# SymForge Binary Self-Update Procedure (Windows)

Audience: the SymForge agent / operator performing an in-place upgrade of the
SymForge binary that an MCP client is actively serving. Platform: Windows
(PowerShell 7+). The same logic maps to Unix with `pgrep`/`kill`.

## The core problem

The MCP client (Claude Code, etc.) SPAWNS `symforge.exe` and keeps it (plus a
sidecar daemon) running for the whole session. On Windows you **cannot overwrite
a running `.exe`** — the loader holds an exclusive image-section handle, so
`Copy-Item` over it fails with a sharing violation (`The process cannot access
the file because it is being used by another process`). The version drift the
daemon reports (`health`: serving 7.27.0, newer 8.0.0 exists) persists until the
holder is stopped, the binary is swapped, and the client respawns.

So the update is a four-beat dance: **discover -> stop the right holders ->
swap -> respawn + verify**. Never just `Copy-Item -Force` and hope.

## Invariants (do not violate)

1. **Scope kills by EXECUTABLE PATH, not by name.** There may be several
   `symforge.exe` processes (this project, another project, a standalone build).
   Only stop the PID(s) whose `ExecutablePath` equals the install path you are
   about to overwrite. Killing every `symforge.exe` by name is the offending
   mistake — it nukes unrelated sessions.
2. **Graceful before forced.** Try a clean stop first; `-Force`/`/F` only if it
   does not exit within a short grace window.
3. **The SymForge daemon is a cache/index, so a kill is data-safe** — it
   re-indexes on next start. But a graceful stop lets it checkpoint/flush its
   index (faster warm start, no half-written checkpoint). Prefer graceful.
4. **The MCP client owns the daemon lifecycle.** After you stop it, do NOT
   manually relaunch the binary — reconnect the MCP client (`/mcp`) and let it
   respawn. Manually starting a second daemon double-spawns and fights the
   client's instance.
5. **Verify by version, not by exit code.** A successful copy is not success;
   `health` reporting the new version with no drift is.

## Step 1 - Discover every holder (read-only)

Use CIM, not `Get-Process` — you need `ExecutablePath` and `CommandLine` to tell
the offending instance from the innocent ones:

```powershell
Get-CimInstance Win32_Process -Filter "name='symforge.exe'" |
  Select-Object ProcessId, ParentProcessId, ExecutablePath, CommandLine |
  Format-List
```

Note: SymForge runs a **sidecar** process (the daemon `health` reports
`Sidecar: pid=<N> port=<P>`). Capture BOTH the MCP-spawned `symforge.exe` and any
child/sidecar PID — both can hold a handle to the image or its port.

## Step 2 - Identify the OFFENDING PIDs

```powershell
$target = "$env:USERPROFILE\.npm-global\node_modules\symforge-windows-x64\bin\symforge.exe"
$holders = Get-CimInstance Win32_Process -Filter "name='symforge.exe'" |
  Where-Object { $_.ExecutablePath -ieq $target }
$holders | Select-Object ProcessId, ExecutablePath
```

`$holders` is the closed set you may stop. Anything outside it is off-limits.

## Step 3 - Amicable shutdown (graceful -> forced)

```powershell
foreach ($p in $holders) {
  $pid = $p.ProcessId
  # (a) Graceful: lets the console-ctrl / shutdown handler flush the index.
  taskkill /PID $pid 2>$null            # WM_CLOSE / Ctrl-style close, no /F
  # (b) Wait out a short grace window.
  $deadline = (Get-Date).AddSeconds(5)
  while ((Get-Process -Id $pid -ErrorAction SilentlyContinue) -and (Get-Date) -lt $deadline) {
    Start-Sleep -Milliseconds 200
  }
  # (c) Forced ONLY if it ignored the graceful stop.
  if (Get-Process -Id $pid -ErrorAction SilentlyContinue) {
    Stop-Process -Id $pid -Force
  }
}
```

If SymForge exposes a clean-shutdown verb (e.g. a `symforge shutdown` /
checkpoint command, or an IPC `shutdown`), call THAT first instead of `taskkill`
so the index is flushed deterministically; fall back to the kill ladder above.

Easiest of all (what happened here): **disconnect the MCP client first** (its
`/mcp` teardown stops the daemon it spawned). With no holder left, the file is
unlocked with zero force-kills — the cleanest path when the client is yours.

## Step 4 - Confirm the binary is unlocked (handle lag)

Windows releases the image handle slightly AFTER the process disappears from the
process list, and AV/indexers can briefly re-lock it. Treat the copy as
retryable rather than assuming the handle is gone:

```powershell
function Test-Locked($path) {
  try { $fs = [IO.File]::Open($path,'Open','ReadWrite','None'); $fs.Close(); $false }
  catch { $true }
}
$tries = 0
while ((Test-Locked $target) -and $tries -lt 25) { Start-Sleep -Milliseconds 200; $tries++ }
```

## Step 5 - Install (overwrite) with a rename fallback

```powershell
$new = "E:\project\symforge\target\release\symforge.exe"   # the freshly built binary
# Long-path safe form if needed: prefix paths with \\?\
try {
  Copy-Item -LiteralPath $new -Destination $target -Force
} catch {
  # Fallback when a handle still lingers: a locked image CAN usually be RENAMED
  # aside (rename only needs DELETE access, not exclusive write), then the new
  # binary dropped in; sweep the stale one next run.
  Move-Item -LiteralPath $target -Destination "$target.old-$(Get-Random)" -Force
  Copy-Item -LiteralPath $new -Destination $target -Force
}
```

(The `.old-*` leftovers are deletable once nothing maps them; schedule a sweep.)

## Step 6 - Respawn + verify (the only proof that counts)

1. Reconnect the MCP client: `/mcp` (it respawns `symforge.exe` from the new
   binary). Do not hand-launch the daemon.
2. Verify with SymForge `health`:
   - version is the NEW version,
   - the "version drift" warning is GONE,
   - `uptime` reset (fresh process), `Sidecar: state=alive`,
   - index re-loaded (`Files: N indexed`, `Watcher: active`).

If `health` still shows the old version, a holder survived (a missed sidecar or
an out-of-scope PID re-spawned it) — return to Step 1.

## One-shot script (discover -> graceful-stop -> swap -> verify-prep)

```powershell
$target = "$env:USERPROFILE\.npm-global\node_modules\symforge-windows-x64\bin\symforge.exe"
$new    = "E:\project\symforge\target\release\symforge.exe"

$holders = Get-CimInstance Win32_Process -Filter "name='symforge.exe'" |
           Where-Object { $_.ExecutablePath -ieq $target }
"Holders of ${target}:"; $holders | Select-Object ProcessId, ExecutablePath | Format-Table

foreach ($p in $holders) {
  taskkill /PID $p.ProcessId 2>$null
  $d=(Get-Date).AddSeconds(5)
  while ((Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue) -and (Get-Date) -lt $d){Start-Sleep -m 200}
  if (Get-Process -Id $p.ProcessId -ErrorAction SilentlyContinue){ Stop-Process -Id $p.ProcessId -Force }
}
function Test-Locked($x){ try{$f=[IO.File]::Open($x,'Open','ReadWrite','None');$f.Close();$false}catch{$true} }
$t=0; while((Test-Locked $target) -and $t -lt 25){Start-Sleep -m 200;$t++}
try { Copy-Item -LiteralPath $new -Destination $target -Force }
catch { Move-Item -LiteralPath $target -Destination "$target.old-$(Get-Random)" -Force; Copy-Item -LiteralPath $new -Destination $target -Force }
"Swapped. Now reconnect the MCP client (/mcp) and check symforge health for the new version + no drift."
```

## Failure modes and the fix

| Symptom | Cause | Fix |
|---|---|---|
| `Copy-Item` sharing violation | A holder still maps the image | Step 3 (kill all in-scope holders incl. sidecar), Step 4 retry, or the Step 5 rename fallback |
| `health` still old version after swap | A holder survived or the client re-spawned the old image before swap | Disconnect the MCP client FIRST, then swap, then reconnect |
| Killed an unrelated session | Stopped by name, not by `ExecutablePath` | Always filter to `$target` (Invariant 1) |
| Index cold / slow after update | Forced-killed mid-write, no checkpoint | Prefer graceful stop / a `shutdown` verb so the index flushes |
| Repeated drift after every build | Build output path != the served install path | Either build directly to the served path, or make the swap part of the build's post-step |

## Why "disconnect-first" is the amicable default

The cleanest update needs **zero force-kills**: tear down the MCP client's
connection (it owns and stops the daemon it spawned), confirm no `symforge.exe`
remains for `$target`, `Copy-Item`, reconnect. This is what happened in practice
here — the binary was unlocked because the client had already released the
daemon, so the swap was a plain copy with no PID hunting at all. Engineer FOR
that window when you can; the kill ladder above is the fallback for when a holder
won't let go.
