# Hook latency: unbounded liveness probing of never-collected sidecar descriptors

**Status:** root cause confirmed by controlled experiment
**Reported:** 2026-07-28
**Binary under test:** `symforge 8.16.6` (npm `symforge-windows-x64`)
**Source tree for line refs:** `E:\project\symforge` @ `9cbe6a9`, `Cargo.toml version = 8.16.5` (see §10 Version skew)
**Platform:** Windows 11 Pro 26200, `SYMFORGE_HOME=C:\Users\rakovnik\.symforge`

---

## 1. Verdict

`symforge hook <any-subcommand>` took **5.0–5.3 s** on every invocation. The hook module's own
stated design budget is **100 ms** (`src/cli/hook.rs:1-9`, HOOK-10). It was over budget by **50×**.

The cost is **not** in any hook workflow. It is in the shared prologue: resolving the sidecar
endpoint walks *every* descriptor file in `$SYMFORGE_HOME/sidecar/sessions/` and performs a
**blocking TCP liveness probe per descriptor**, with no cap, no total deadline, no early exit,
and no parallelism. Descriptors from dead processes are **never garbage-collected on this path**,
so the scan cost grows monotonically for the lifetime of the install.

On the reporting machine 40 descriptors had accumulated, 27 of them for dead PIDs. Quarantining
only the dead-PID descriptors dropped the hook from **5086 ms → 162 ms** (31×) with no code change.

This is a self-inflicted user-visible failure: `symforge init` writes `"timeout": 5` into the user's
`settings.json` (`src/cli/init.rs:627`), and the hook then exceeds that timeout, so Claude Code
prints `UserPromptSubmit hook timed out after 5s — output discarded` and **discards the hook output
on every prompt**. SymForge shipped a config its own hook cannot satisfy.

---

## 2. Impact

| Surface | Configured timeout (written by `symforge init`) | Measured | Result |
|---|---|---|---|
| `UserPromptSubmit` → `hook prompt-submit` | 5 s | 5075–5128 ms | **Times out ~always.** Output discarded, user sees an error banner on every prompt |
| `PostToolUse` → `hook` (Read/Edit/Write/Grep) | 5 s | 5122 ms | **Times out silently.** 5 s added to every matching tool call |
| `SessionStart` → `hook session-start` | 5 s | 5064 ms | **Times out silently.** Repo map never injected |
| `PreToolUse` → `hook pre-tool` (Grep/Read/Glob/Edit) | 2 s | 5059–5286 ms | **Killed at 2 s, always.** 2 s tax on every file tool call, output never once delivered |

The `PreToolUse` row is the worst: it fires on the highest-frequency tools in the agent loop, is
killed before it can ever produce a suggestion, and still burns its full 2 s wall-clock every time.

Only `UserPromptSubmit` surfaces an error banner. The other three fail **silently** — the user
believes SymForge context injection is working when it has never once delivered a payload.

---

## 3. Reproduction

```bash
# 1. Accumulate descriptors: start and kill N sidecar-backed sessions.
#    Each session writes $SYMFORGE_HOME/sidecar/sessions/sidecar.<pid>.<os>.json
#    and does not remove it on exit.
ls ~/.symforge/sidecar/sessions/ | wc -l     # 40 here

# 2. Time any hook subcommand.
echo '{"session_id":"t","cwd":"x","tool_name":"Read","tool_input":{"file_path":"x.rs"}}' \
  | symforge hook pre-tool
# -> ~5.1 s
```

Latency scales with the count of **dead-PID** descriptors present, not with repo size, not with cwd,
not with which subcommand runs.

---

## 4. Measurements

### 4.1 Before (40 descriptors: 13 live, 27 dead)

```
hook prompt-submit   5118 / 5075 / 5077 ms   (3 runs)
hook pre-tool        5086 / 5083 / 5286 ms   (3 runs, 3 different cwds)
hook session-start   5064 ms
hook (post-tool)     5122 ms
```

Baselines on the same binary, same machine — these exit inside clap, before `run_hook`:

```
symforge --version     291 ms
symforge --help        245 ms
symforge hook --help   143 ms
```

Process startup is ~150–290 ms. The remaining ~4.8 s is inside `run_hook`.

### 4.2 Phase breakdown, `SYMFORGE_HOOK_VERBOSE=1`, before

Timestamps are ms from process spawn, captured per stderr line:

```
 5052ms ERR: read port file: port=63307
 5052ms ERR: HTTP GET 127.0.0.1:63307/prompt-context?text=hello
 9996ms ERR: HTTP request failed — sidecar liveness=alive, attempting daemon fallback
10509ms ERR: daemon fallback unavailable — outcome=NoSidecar reason=sidecar_port_stale
10512ms OUT: {"hookSpecificOutput":{...}}
10516ms <exit>
```

Three phases:

- **A — 5052 ms before the first log line.** Everything prior to `read port file`: stdin parse,
  control-state placement, `read_sidecar_endpoint`. This is the descriptor scan.
- **B — 4944 ms for one HTTP GET** whose configured timeout is `HTTP_TIMEOUT = 50 ms`. This is
  **phase A happening a second time**: the verbose-only liveness diagnostic at
  `src/cli/hook.rs:433-455` calls `read_sidecar_status`, which re-scans and re-probes every
  descriptor. It is inside `if verbose`, which is exactly why non-verbose runs cost ~5 s and
  verbose runs cost ~10 s. The 50 ms HTTP timeout itself is **not** violated.
- **C — 513 ms daemon fallback**, matching `DAEMON_FALLBACK_DEADLINE = 500 ms`. **This budget works
  correctly** and is the model the descriptor scan should follow.

### 4.3 After quarantining 27 dead-PID descriptors (13 remain, all live)

No code change, no restart, nothing else touched:

```
hook pre-tool        162 / 167 / 181 ms     (was 5086)   -> 31x faster
hook prompt-submit   796 / 692 ms           (was 5118)   ->  7x faster
```

Verbose phase breakdown after:

```
   43ms ERR: read port file: port=63307
   44ms ERR: HTTP GET 127.0.0.1:63307/prompt-context?text=hello
   56ms ERR: HTTP request failed — sidecar liveness=alive, attempting daemon fallback
   56ms ERR: daemon fallback unavailable — outcome=NoSidecar reason=sidecar_port_stale
   58ms OUT: {...}
   65ms <exit>
```

**10516 ms → 65 ms.** Phase A: 5052 → 43 ms. Phase B: 4944 → 12 ms. Every phase collapses once the
dead descriptors are gone. This is the proof: the descriptor scan is the entire cost.

---

## 5. Root cause

### 5.1 The unbounded probe loop

`src/sidecar/port_file.rs:274` — `select_descriptor_status()`:

```rust
for descriptor in read_descriptors_at(dir) {
    if let (Some(declared), Some(expected)) =
        (descriptor.project_root.as_deref(), expected_root.as_deref())
        && !same_root_identity(declared, expected)
    {
        rejected += 1;
        continue;
    }
    let alive = sidecar_port_is_alive(bind_host, descriptor.port).unwrap_or(false);   // :290
    candidates.push((descriptor, alive));
}
```

`src/sidecar/port_file.rs:409`:

```rust
fn sidecar_port_is_alive(bind_host: &str, port: u16) -> io::Result<bool> {
    let sock_addr = sidecar_socket_addr(bind_host, port)?;
    Ok(TcpStream::connect_timeout(&sock_addr, Duration::from_millis(200)).is_ok())
}
```

Defects, in order of severity:

1. **No total deadline.** Worst case is `descriptor_count × 200 ms`, unbounded. Compare
   `DAEMON_FALLBACK_DEADLINE` (`src/cli/hook.rs:49`), which correctly bounds an entire multi-step
   sequence at 500 ms. The descriptor scan has no equivalent.
2. **No early exit.** The loop probes *all* candidates before sorting `alive`-first
   (`:306-311`) and taking `candidates[0]`. The moment one live descriptor for the expected root is
   found, the remaining probes are dead weight.
3. **Serial.** N probes run sequentially; they are independent and could be concurrent or,
   better, skipped entirely (see #4).
4. **PID liveness is never consulted.** The descriptor carries `pid`. Checking whether that PID is
   alive is a cheap local syscall and needs no socket at all. The code reaches for the network
   before checking the free local signal it already holds.
5. **No cap on descriptor count.** Nothing bounds directory growth.

### 5.2 Why stale descriptors are never collected

`cleanup_stale_descriptors` exists, and has exactly two callers:

- `src/cli/update.rs:113` — only during `symforge update`
- `src/sidecar/server.rs:46` — only on sidecar startup, and only for the current root

Neither runs on the hook path. A sidecar that dies without a clean shutdown leaves its descriptor
behind forever. Every Claude Code session that starts a sidecar adds one. Growth is monotonic and
the hook pays for all of it, forever.

Observed here: **40 descriptors, oldest 2026-07-27 12:21, newest 2026-07-28 11:49** — 40 accumulated
in roughly 24 hours of normal use. 27 belonged to dead PIDs.

### 5.3 Secondary: null `project_root` defeats the rejection guard

The root-mismatch fast path at `:283-289` only fires when **both** the descriptor's `project_root`
and the expected root are `Some`. Descriptors are being written with `project_root: null`:

```json
{ "session_id": null, "project_root": null, "pid": 15060, "port": 50165,
  "updated_at_unix_secs": 1785219093 }
```

**24 of the 27 stale descriptors had `project_root: null`** (and `session_id: null`). A null-root
descriptor is never rejected, so it is probed for **every project on the machine**. The cheap filter
that should have made the scan nearly free is bypassed by the majority of records.

Two questions for the owner: which write path emits `project_root: null`, and should a descriptor
without a root be treated as unusable (skip, don't probe) rather than as a universal candidate?

### 5.4 Secondary: port collision across descriptors

Four descriptors (pids 16840, 39964, 54872, 58044) all claim **port 56651**, and all four probe as
`alive`. Ports are recycled by the OS after a process dies, so a dead descriptor can probe alive
against an unrelated process. `sidecar_port_is_alive` returning `true` therefore does **not** prove
the sidecar is alive — a successful connect is not identity. This is a correctness bug independent
of the latency bug: the hook can select a descriptor that points at a stranger's socket, which is
consistent with the observed `liveness=alive` immediately followed by
`reason=sidecar_port_stale`.

---

## 6. Hypotheses tested and disproven

Recorded so the next investigator does not re-tread them.

| Hypothesis | Test | Result |
|---|---|---|
| Process startup / AV scanning | `--version`, `--help` on same binary | **Disproven.** 143–291 ms |
| Fixed `sleep`/timeout constant in the hook path | grep `from_secs(5)`, `from_millis(5000)` across `src/` | **Disproven.** Only hits are `daemon.rs` (not on hook path), `frecency.rs`/`ledger_store.rs`/`api_keys.rs` SQLite `busy_timeout` |
| SQLite lock contention (5 s `busy_timeout`) | `find $SYMFORGE_HOME -name '*.db' -o -name '*-wal'` | **Disproven.** No DB files in `$SYMFORGE_HOME`; hook run touched zero files (`find -newermt '-20 seconds'` empty) |
| Filesystem discovery walking from cwd | Ran from empty temp dir, `$HOME`, and repo root | **Disproven.** 5086 / 5083 / 5286 ms — cwd-insensitive |
| Cost is at process teardown, not in the work | Timestamped stdout vs exit | **Disproven.** stdout at 5500 ms, exit at 5506 ms — the work blocks |
| Sidecar HTTP GET violates its 50 ms timeout | Verbose phase timing before/after | **Disproven.** 4944 ms was a second descriptor scan (verbose-only path `hook.rs:433-455`); post-fix the same GET is 12 ms |
| `TcpStream::connect_timeout` slow to dead local ports | Independent async probe of all 40 ports | **Partially disproven — see §11.** All 40 resolved in 20 ms total, every dead one `ECONNREFUSED` in ≤3 ms |

---

## 7. Proposed fixes

Ordered by value. #1 and #2 alone restore the 100 ms budget.

### Fix 1 — Bound the scan with a total deadline (required)

Mirror the pattern already used correctly by `DAEMON_FALLBACK_DEADLINE`. Give
`select_descriptor_status` a wall-clock budget (suggest **150 ms**, well inside HOOK-10's 100 ms
normal path since the common case is a single live descriptor). On expiry, return the best candidate
found so far. Never let descriptor count set hook latency.

### Fix 2 — Check PID before touching the network (required)

`sidecar_port_is_alive` should be gated behind a local PID-liveness check using the `pid` already in
the descriptor. A dead PID is skipped with zero syscalls of network cost. On Windows,
`OpenProcess`/`GetExitCodeProcess`; on Unix, `kill(pid, 0)`. This alone would have made the reported
case free, since all 27 offenders had dead PIDs.

### Fix 3 — Early exit on first live match for the expected root (required)

Once a descriptor matching the expected root probes alive, stop. Only continue scanning when no
match has been found. Removes the "probe 40 to use 1" waste.

### Fix 4 — Garbage-collect on the read path (required)

When a descriptor is found to have a dead PID, unlink it opportunistically (best-effort, ignore
errors, never fail the hook). The directory then self-heals instead of growing forever. Today
cleanup only happens in `symforge update` and sidecar startup, which is why 40 accumulated in a day.

### Fix 5 — Verify identity, not just reachability (correctness)

A successful connect must not be read as "this is my sidecar" (§5.4: four descriptors share port
56651). Confirm identity — session id or pid echoed on a cheap handshake endpoint — before treating
a descriptor as alive.

### Fix 6 — Do not write `project_root: null` (correctness)

Find the write path emitting null roots (§5.3) and populate the root. Additionally, treat a
null-root descriptor as non-selectable rather than universally-selectable.

### Fix 7 — Raise the timeout `symforge init` writes (defensive)

`src/cli/init.rs:627` writes `"timeout": 5`, and the pre-tool entry writes `2`. A hook whose budget
is 100 ms should not be configured at a limit it can hit. After fixes 1–4, `10` gives honest
headroom without masking regressions. **Do not treat this as the fix** — it only hides the symptom.

### Fix 8 — Regression test (required)

The bug is invisible to unit tests because it needs N stale descriptors on disk. Add an integration
test: write 200 descriptors with dead PIDs into a temp control-state dir, assert
`select_descriptor_status` returns within the deadline. Without this, the defect returns.

---

## 8. Acceptance criteria

1. With **200** dead-PID descriptors on disk, `symforge hook pre-tool` completes in **< 300 ms**.
2. With 200 dead descriptors + 1 live descriptor for the expected root, the live one is still
   selected, and latency stays < 300 ms.
3. After one hook invocation over a directory of dead-PID descriptors, the dead entries are gone
   (Fix 4).
4. A descriptor whose port is reachable but whose identity does not match is **not** selected (Fix 5).
5. `SYMFORGE_HOOK_VERBOSE=1` does not more than double total runtime (the verbose-only second scan
   at `hook.rs:433-455` should reuse the first scan's result, not repeat it).
6. Existing hook integration tests still pass.

---

## 9. Reporter-side mitigation already applied

For the record, so the owner knows the reporting machine's state is no longer virgin:

- 27 dead-PID descriptors moved out of `~/.symforge/sidecar/sessions/` to a quarantine dir.
  Full 40-descriptor backup retained. 13 live descriptors untouched.
- User's `~/.claude/settings.json` `UserPromptSubmit` timeout raised `5 → 15` as a stopgap before
  the root cause was known. Post-fix the hook runs in ~65 ms, so this is now pure headroom.
- No symforge source, binary, or config was modified.

---

## 10. Version skew — verify before trusting line numbers

The installed binary reports **8.16.6**. The source tree used for every line reference in this
report is `E:\project\symforge` @ `9cbe6a9`, whose `Cargo.toml` says **version = "8.16.5"**.

All *measurements* come from the 8.16.6 binary; all *line numbers* come from the 8.16.5 tree.
Confirm the cited code is unchanged in 8.16.6 before acting on exact line numbers. The behavioural
evidence (§4) stands regardless.

---

## 11. Open question for the owner

**Why does each dead descriptor cost ~185 ms?**

The arithmetic is clean — 27 dead descriptors × ~185 ms ≈ 5.0 s, and 185 ms is suspiciously close to
the 200 ms `connect_timeout` — but an independent async probe of all 40 ports from outside the
process returned `ECONNREFUSED` in **≤3 ms each, 20 ms total** (§6, last row). A refused connect on
Windows loopback is instant, so a naive reading says `connect_timeout` should *not* be hit.

Candidate explanations, needing symforge-side instrumentation to separate:

- The per-descriptor cost is not the connect but something else in the loop body
  (`read_descriptors_at` file I/O, JSON parse, path canonicalization in `same_root_identity`).
- Blocking `TcpStream::connect_timeout` on Windows behaves differently from an async connect for
  ports in `TIME_WAIT` or held by a recycled owner (recall four descriptors share port 56651).
- Some descriptors resolve to a host that blackholes rather than refuses.

The fix set in §7 is correct regardless of which it turns out to be — Fixes 1–4 remove the loop's
ability to cost anything at all. But the answer should be nailed down so the deadline in Fix 1 is
chosen from data rather than guessed. **Recommend adding per-descriptor timing behind
`SYMFORGE_HOOK_VERBOSE` as the first commit**, so this is measurable from the field instead of
inferred.
