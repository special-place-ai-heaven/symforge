# Latent hazards found during the optimization campaign (2026-08-06)

Observations from working inside the code during the #530 / #532 / #534 stretch.
None of these caused the bugs being fixed; all were noticed on the way past and
would otherwise be lost when the session ends.

Each is marked **VERIFIED** (read from source, file:line given) or **OPINION**
(a design judgement, argued but not a defect).

---

## 1. Watcher handles do not stop their watcher on drop — VERIFIED

```639:642:src/watcher/mod.rs
pub struct WatcherTaskHandle {
    pub task: tokio::task::JoinHandle<()>,
    pub stop_token: Arc<AtomicBool>,
}
```

There is no `impl Drop for WatcherTaskHandle`. Dropping the handle neither sets
`stop_token` nor aborts `task`, so the watcher keeps running with no owner.

Stopping one correctly takes four steps, which every caller must remember:

```rust
let watcher = server.watcher_handle.lock().take().expect("...");
watcher.stop_token.store(true, Ordering::Release);
let mut task = watcher.task;
if tokio::time::timeout(Duration::from_secs(2), &mut task).await.is_err() {
    task.abort();
}
```

**Consequence in tests.** A test that does not run that dance leaves a live
watcher for the remainder of the process. In the lib test binary that is the rest
of the suite — ~3,100 tests sharing one process under `--test-threads=1`. Several
`index_folder` tests create watchers; not all tear them down.

This is not the cause of the #534 flake (that was a git-temporal publication
race), but it is the same category of shared-state pressure, and leaked watchers
plausibly contribute to the load that made #534 reproducible only in a full run.

**Suggestion:** give `WatcherTaskHandle` a `Drop` that sets `stop_token` and
aborts the task. It makes the correct behaviour the default and turns the
four-step dance into an explicit opt-in for callers that need to await a graceful
stop. Production is unaffected — the server's watcher lives for the process.

---

## 2. `EnvVarGuard` is used without `env_lock` in `protocol/tools.rs` tests — VERIFIED

`daemon.rs` defines both a process-global env guard and a serializing lock:

```5666:5668:src/daemon.rs
    async fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().await
    }
```

Its own tests pair them (`let _env_lock = env_lock().await;` before
`EnvVarGuard::set(..)`). `EnvVarGuard` itself is candid about the assumption:

> SAFETY: called only in single-threaded test context; no concurrent env readers.

`src/protocol/tools.rs` uses `EnvVarGuard` but contains **zero** references to
`env_lock` — grep returns nothing. Its tests mutate process-global env
(`SYMFORGE_MAX_INDEX_FILES`, and others) with no serialization.

`--test-threads=1` makes this mostly safe today, because tests do not overlap.
The assumption it actually rests on is stronger and unstated: that no *background
task* reads env concurrently. Given item 1 above — leaked watchers outliving
their tests — that assumption is not obviously true.

**Suggestion:** either take `env_lock` in `tools.rs` tests that mutate env, or
lift `EnvVarGuard` somewhere shared and make acquiring the lock part of
constructing it, so it cannot be used without serialization.

---

## 3. Display reasons and typed reasons round-trip lossily by construction — OPINION

`#530` fixed the symptom: `SkipReason` (the display projection) no longer claims
a security demotion is a language problem. The underlying shape remains awkward.

`compatibility_admission_decision` maps `MetadataOnlyReason` (typed, persisted,
carries `rule_id` / `rule_ids` / `finding_count`) onto `SkipReason` (display,
never persisted). The reverse maps — `disposition_from_admission` in `store.rs`
and the scout helper in `discovery/mod.rs` — must then accept `SkipReason`
values that the admission pipeline never produces, and have nowhere honest to
send them. #530 added a `debug_assert` there precisely because every available
target is a false statement.

The types permit an unrepresentable state: a display-only reason arriving where a
typed reason is required. That is why the arm exists and why it can only lie.

**Suggestion (not urgent):** split the display-only variants into their own enum
so the round-trip is impossible to express, rather than possible-but-asserted-
against. The `debug_assert` is the right guard for today; the type split is the
fix that removes the guard's reason to exist.

---

## 4. Cold-start timing has ~27% run-to-run spread — VERIFIED (measured)

Measuring #532 required three cold samples per side. The baseline spread:

```
serve: runtime built  ->  3.6496s / 3.9576s / 3.1662s   (mean 3.591s)
```

3.11s to 3.96s across three runs of identical code on an idle machine. A single
sample supports almost any conclusion in that band.

This bit the campaign twice before it was noticed: a CI figure was quoted at
−11.0% from the fastest of one run, corrected to −8.3%, then to −6.5%.

**Suggestion:** record the rule where it will be read — `CLAUDE.md`'s
verification section. Something like: *perf claims quote the mean over ≥3 samples
per side, measured in the same session on the same fixture; single-sample
numbers are not evidence.* The rule already exists in practice; it is not written
anywhere a future session will see it.

---

## 5. `MetadataOnlyReason::PlatformPathCollision` has no production mint site — VERIFIED

```888:src/domain/index.rs
    PlatformPathCollision,
```

Repo-wide, this variant appears only in its own definition and in the match arms
that must cover it exhaustively. Nothing constructs it.

Three independent reviewers of #530 flagged this separately, and a direct grep
confirms it.

Windows/macOS path-case collisions are a real condition and currently go
unhandled, so this reads as a capability the code claims but does not have.

**Suggestion:** either wire up the detection it was meant to represent, or delete
the variant. Deleting touches the serde-persisted `MetadataOnlyReason`, so unlike
adding `SkipReason` variants it needs a snapshot-compatibility check. Tracked as
a low-priority board item; recorded here so the reasoning is not lost.
