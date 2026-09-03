# REVIEW-FINDINGS — readiness hardening — 2026-09-03

**Spectrum:** Phase 3 (Hardening)
**Baseline:** `main` @ `6188c5af`
**Instruments:** SymForge MCP release binary, code audit of unsafe boundaries, process lifecycle tracing.

---

## 3.1 Unsafe audit (12 production sites across 4 files)

Crate denies `unsafe_code` globally (`Cargo.toml`). Exactly 12 production sites carry `#[allow(unsafe_code)]`:

1. **`src/cli/update.rs` (5 Win32 API sites):**
   - Line 935: `CloseHandle(self.0)` — release of process handle.
   - Line 945: `pid_image_full_path` — `OpenProcess` + `QueryFullProcessImageNameW` to confirm process image identity.
   - Line 979: `terminate_process` — `OpenProcess` + `TerminateProcess` after owner verification.
   - Line 1000: `enumerate_by_image_name` — `CreateToolhelp32Snapshot` process table traversal.
   - Line 1061: `move_file_replace_existing` — atomic binary replacement via `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`.
   - **Verdict:** All 5 have complete `SAFETY` comments with parameter validity and cleanup reasoning. Standard Windows self-update mechanics.

2. **`src/daemon.rs` (4 POSIX / Windows process signaling sites):**
   - Line 2940: `kill(pid, SIGKILL)` on Unix — called only after ownership safety gate passes.
   - Line 2985 / 3026: `kill(pid, 0)` on Unix — probes existence without sending a signal.
   - Line 3261: Linux `/proc/{pid}/status` UID inspection (`libc::geteuid()`).
   - Line 6338: `libc::kill(pid, SIGKILL)`.
   - **Verdict:** Clean POSIX wrappers with valid bounds.

3. **`src/sidecar/port_file.rs` (2 sites):**
   - Line 523 (Windows): `process_may_be_alive` (`OpenProcess` + `WaitForSingleObject`).
   - Line 552 (Unix): `kill(pid, 0)`.
   - **Verdict:** Correct read-only existence probes.

4. **`src/protocol/knowledge_curation.rs:2076` (1 site):**
   - `windows_write_through_replace`: `CreateFileW` with `FILE_FLAG_WRITE_THROUGH` to force durable filesystem commits for knowledge curation records.
   - **Verdict:** Sound Win32 durability pattern.

5. **`src/path_shadow.rs:549-565` — SPEC REFUTED:**
   - The diagnosis specification warned of *"path_shadow.rs mutating process-global PATH in production code paths"*.
   - **PROVEN FALSE**: The `PathGuard` and its unsafe `std::env::set_var("PATH", ...)` live strictly inside `#[cfg(test)] mod tests` (line 528). Production code contains zero unsafe environment mutations.

---

## 3.2 Cold-start deadline margin

- **Operator deadline:** `ADMIN_SERVE_START_DEADLINE = 60s`.
- **Measured on this workstation (Core Ultra 7 265, NVMe):**
  - Warm snapshot restore: **7.0s** (transition from `Empty` → `Loading` → `Ready`). Headroom: **53.0s (88%)**.
  - Cold discovery + full parse of 1146 files: **18.0s** (measured via `index_folder`).
  - Total cold startup estimate: **~25.0s**. Headroom: **35.0s (58%)**.
- **Risk assessment (PROVEN MARGIN CURVE):**
  - While this workstation has a 58% margin, prior test logs on loaded runners with phase0 corpora present (~1500 files) measured up to 49.5s. On slower machines or large monorepos (>5,000 files), a cold scan will easily breach the 60s deadline without the snapshot restore path (spec 026).

---

## 3.3 Error taxonomy at boundaries

1. **MCP Tools:**
   - All 39 tools catch internal errors and serialize them as MCP `ToolResult { isError: true, content: [...] }`. No panics escape to the transport layer.
2. **Hook Fail-Open (`src/cli/hook.rs:226`):**
   - `run_hook` enforces a 250ms deadline on reading stdin; any error emits `fail_open_json` (`{"hookSpecificOutput":{"additionalContext":""}}`) and exits with code 0.
   - **Safety Boundary Analysis:** The hook's `Edit` handling is strictly advisory (`PostToolUse` `/impact` hints) and has no ability to allow or block client edits. Actual edit enforcement lives in `src/protocol/edit.rs` (`guarded_atomic_write_file`) and `src/edit_safety/`, which fails CLOSED on path escape, symlink redirection, or scheme URIs.

---

## 3.4 Watcher robustness & repair counts

- **Telemetry finding (STRESS-TEST §P2-10: 2839 repairs vs 714 events):**
  - Inspected `src/watcher/mod.rs:439-485` and `:3961-3974`.
  - The counter `repairs_applied` increments ONLY when a file is actually re-indexed or removed from the store during a reconciliation pass.
  - `reconcile_repair_count_excludes_generation_mismatch_noops` explicitly verifies that rejected generation mismatch mutations do NOT increment the repair total.
  - **Verdict (PROVEN):** The repair count is not telemetry inflation. Periodic reconciliation sweeps perform true repairs for files modified during watcher debounce or missed events.

---

## 3.5 Process lifecycle & PID reuse

- Daemon start is serialized via `daemon.start.lock` file holding PID and start timestamp.
- **PID recycling defense:** `src/cli/update.rs:946` inspects the executable image path (`QueryFullProcessImageNameW`) of the target PID before issuing termination. If an OS PID is recycled to a process not named `symforge.exe`, termination is refused.

---

## 3.6 Capacity & backpressure

- **`SYMFORGE_MAX_INDEX_FILES`:** Confirmed to be applied per discovery pass, not globally across the daemon.
- **`MAX_INFLIGHT_BYTES_ENV`:** Default 512 MiB limit bounds peak memory allocation for uncommitted file buffers during parallel discovery.
- **`ProcessCapacityPool` (`src/index_lifecycle/capacity.rs`):** Shipped as part of Feature 020 to track ledger allocations per surface.
