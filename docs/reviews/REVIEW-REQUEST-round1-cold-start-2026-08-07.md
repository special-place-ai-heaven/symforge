# Review request — SymForge: "index reports Ready before it is rooted"

**What this is.** A proposed bug fix in a Rust MCP server. I want an independent
adversarial review before it merges. You do not need repository access — the complete
diff is inline below. If you *do* have the repo (`special-place-ai-heaven/symforge`,
branch `fix/cold-start-ready-before-rooted`, off `main`), use it.

**What I want from you.** Answers to the nine numbered questions at the end. Not a
summary of the diff, not praise, not style notes. If the fix is wrong, say so and say
why. If a question is unanswerable from what you have, say "cannot determine from the
diff" — that is a useful answer and I would rather have it than a guess.

**Ground rules, because these have burned this project in the last 48 hours:**

- **A comment is not behaviour.** A `///` or `//` line asserting X is not evidence that
  X happens. This diff is heavily commented; the comments are claims under review, not
  premises.
- **Existence is not invocation.** Finding a function that would do X does not establish
  that anything calls it.
- **Cite `file:line`.** A claim without a location is not actionable.
- Confidence labels please: **proven / likely / speculative**.

---

## 1. The bug

SymForge indexes a repository and answers code-navigation queries over MCP. Each answer
carries a "trust envelope" describing how much the answer can be relied on. The index
also publishes a *knowledge* layer keyed on a **source identity**, which is derived from
the indexed repository root.

**Observed symptom.** A code-context receipt printed `source=unknown` where a hex
identity hash belongs, together with `counts total=0` — the whole knowledge layer
published unbound. Intermittent: reproduced locally 3 times in 25 runs (~12%), and in
CI once.

**Root cause, established by reading the code that runs:**

1. On cold start with no persisted snapshot, the process publishes a **placeholder**
   empty index. That placeholder has `indexed_root: None`, `is_empty: true`, and
   `load_source: EmptyBootstrap`. The real load is dispatched detached, on another
   thread.
2. `get_file_context` calls a "freshen this exact path" helper **one line before** it
   reads the published generation, and **two lines before** the guard that refuses to
   answer while the index is loading.
3. The requested file is absent from the placeholder, so its recorded mtime is
   `u64::MAX`, which never equals the on-disk mtime. It therefore looks stale and gets
   indexed on the spot, into the placeholder.
4. That mutation path sets `is_empty = false` and **never sets `indexed_root`**.
5. Publication calls `capture_published_manifest`, which begins
   `let root = live.indexed_root.as_deref()?;` — a **silent** early return. No root, so
   no manifest, so no source identity, so the knowledge bridge publishes as default/empty.
6. `index_state()` only consults `is_empty`, the snapshot-verify state, and a circuit
   breaker. With `is_empty` now false, it returns **`Ready`** — so the guard in step 2
   does not fire on the *next* call either. The outline renders correctly from the
   freshened file; the knowledge block is empty and the receipt says `source=unknown`.

**In one sentence:** a file is admitted into the index before the index knows what
repository it is, which makes it report Ready while it has no identity.

**Independent measurement from the harness side** (recorded in the diff): on the CI test
fixture, **4 of 12 cold starts** reported `index_files>0` **and** `index_state=Ready`
**and** `load_source=EmptyBootstrap` **and** `generation=0` at the very first poll.

---

## 2. The fix

Three parts.

**(a) State guard.** `index_state()` returns `Loading` when `load_source` is
`EmptyBootstrap`. Deliberately gated on `load_source`, **not** on
`indexed_root.is_none()`, because a separate "local-ref" lane builds an index that is
legitimately rootless but *does* publish its own source identity — a root-based guard
would pin that lane at `Loading` forever.

**(b) Diagnostics.** The two silent `?` early-returns in `capture_published_manifest`
now `warn!`. Their silence is why this took two days to find.

**(c) Harness gate.** The CI tool-correctness harness previously opened its readiness
gate on `index_files > 0`. After (a) that is the wrong signal — file count is reported
regardless of state — so it now requires a published generation that is `Ready` with
`load_source != EmptyBootstrap`. Plus three evidence-preservation changes: stderr was
being discarded, `RUST_LOG` was `off`, and diff output showed only the first differing
line.

(a) and (c) **must** ship together: (a) alone would make CI deterministically red.

---

## 3. The complete diff

### `src/live_index/health_view.rs`

```rust
-use super::store::{IndexState, IndexedFile, LiveIndex, ParseStatus, SnapshotVerifyState};
+use super::store::{
+    IndexLoadSource, IndexState, IndexedFile, LiveIndex, ParseStatus, SnapshotVerifyState,
+};

     /// `true` when the index has been loaded and the circuit breaker has NOT tripped.
+    ///
+    /// Delegates to [`Self::index_state`] so the two never disagree — an index the
+    /// tool guards refuse must not report itself ready in `status`.
     pub fn is_ready(&self) -> bool {
-        if self.is_empty {
-            return false;
-        }
-        if matches!(
-            self.snapshot_verify_state,
-            SnapshotVerifyState::Pending | SnapshotVerifyState::Running
-        ) {
-            return false;
-        }
-        !self.cb_state.is_tripped()
+        matches!(self.index_state(), IndexState::Ready)
     }

     /// Returns the current index state.
     pub fn index_state(&self) -> IndexState {
         if self.is_empty {
             return IndexState::Empty;
         }
+        // A bootstrap placeholder that acquired files — targeted-retrieval
+        // freshening admits one before the detached initial load binds a root —
+        // has no `indexed_root`, so `capture_published_manifest` returns None,
+        // publication captures no source identity, and knowledge publishes
+        // unbound (`source=unknown`). It is not Ready; it is still loading.
+        //
+        // Gate on `load_source`, NOT on `indexed_root.is_none()`: the P1
+        // local-ref lane (`LiveIndex::from_source_files`, store.rs:4290) is
+        // legitimately rootless and publishes its own source identity
+        // unconditionally (store.rs:1568), so a root-based guard would pin that
+        // lane at Loading forever.
+        if self.load_source == IndexLoadSource::EmptyBootstrap {
+            return IndexState::Loading;
+        }
         if matches!(
             self.snapshot_verify_state,
             SnapshotVerifyState::Pending | SnapshotVerifyState::Running
```

### `src/live_index/store.rs` — diagnostics

```rust
 fn capture_published_manifest(
     live: &LiveIndex,
     scout_plan: Option<&discovery::ScoutPlan>,
 ) -> Option<Arc<RepositoryManifest>> {
-    let root = live.indexed_root.as_deref()?;
-    let canonical_root = dunce::canonicalize(root).ok()?;
+    let Some(root) = live.indexed_root.as_deref() else {
+        // An empty bootstrap index legitimately has no root yet. A POPULATED one
+        // publishing without a root is the unbound-knowledge defect: the manifest,
+        // source identity, source version and knowledge bridge are all silently
+        // dropped downstream, and the receipt prints `source=unknown`.
+        if !live.is_empty {
+            warn!(
+                files = live.files.len(),
+                "publishing a non-empty index with no indexed_root; source identity unbound"
+            );
+        }
+        return None;
+    };
+    let canonical_root = match dunce::canonicalize(root) {
+        Ok(canonical_root) => canonical_root,
+        Err(error) => {
+            warn!(
+                root = %root.display(),
+                %error,
+                "failed to canonicalize the indexed root; publishing without source identity"
+            );
+            return None;
+        }
+    };
```

### `src/live_index/store.rs` — new regression test

```rust
+    /// Regression: a file admitted into the empty bootstrap placeholder before the
+    /// initial load bound a root must NOT publish as Ready. Such an index has no
+    /// `indexed_root`, so `capture_published_manifest` returns None and knowledge
+    /// publishes unbound with `source=unknown`, while the tool guards wave the
+    /// request through.
+    #[test]
+    fn bootstrap_placeholder_that_admitted_a_file_is_loading_not_ready() {
+        let shared = LiveIndex::empty();
+        shared.update_file(
+            "src/admitted_before_load.rs".to_string(),
+            make_indexed_file_for_mutation("src/admitted_before_load.rs"),
+        );
+        assert_eq!(shared.read().index_state(), IndexState::Loading);
+    }
```

### `src/live_index/store.rs` — four existing assertions flipped

All four are in tests that build `LiveIndex::empty()` and then mutate it.

```rust
-        assert_eq!(after_add.status, PublishedIndexStatus::Ready);
+        // Still an EmptyBootstrap index with no bound root: mutating it does not
+        // make it Ready, it makes it a placeholder that is still loading.
+        assert_eq!(after_add.status, PublishedIndexStatus::Loading);

-        assert_eq!(after_remove.status, PublishedIndexStatus::Ready);
+        // `remove_file` never restores `is_empty`, so the index stays a
+        // non-empty-flagged, still-unbound bootstrap.
+        assert_eq!(after_remove.status, PublishedIndexStatus::Loading);

-        assert_eq!(after_add.status, PublishedIndexStatus::Ready);
+        // Write-guard drop publishes, but an unbound EmptyBootstrap index that
+        // gained a file is Loading, not Ready (see index_state's guard).
+        assert_eq!(after_add.status, PublishedIndexStatus::Loading);

-        assert_eq!(after_remove.status, PublishedIndexStatus::Ready);
+        assert_eq!(after_remove.status, PublishedIndexStatus::Loading);
```

### `tests/fixtures/sidecar_contract/health.json`

```diff
 {
   "file_count": 2,
-  "index_state": "Ready",
+  "index_state": "Loading",
   "symbol_count": 2,
   "uptime_secs": 0
 }
```

### `scripts/verify-tools.cjs` — the CI harness

```javascript
   const proc = spawn(BIN, [], {
     cwd: FIXTURE,
-    stdio: ["pipe", "pipe", "ignore"],
+    // stderr INHERITED, not dropped: the daemon's own warnings are the evidence that
+    // explains a readiness/cold-start failure. Safe — tracing writes to stderr
+    // (src/observability.rs), and the JSON-RPC stream this harness parses is stdout-only.
+    stdio: ["pipe", "pipe", "inherit"],
     env: {
       ...process.env,
-      RUST_LOG: "off",
+      // "warn", not "off": a silenced index-publish warning is precisely what made the
+      // source-binding bug cost two days. Measured cost on a healthy run: one line.
+      RUST_LOG: "warn",
```

```javascript
-  // Compact auto-indexes SYMFORGE_WORKSPACE_ROOT on startup (async). Poll until the
-  // index reports a NON-ZERO file count AND a known symbol is actually queryable.
-  // ponytail: fixed 30-poll x 250ms ceiling (~7.5s)
+  // ... the PLACEHOLDER index it serves meanwhile can ALREADY report files: a targeted
+  // read admits files into the EmptyBootstrap index before the initial load has bound a
+  // source root. So `index_files > 0` opens the gate on an index with no source
+  // identity — measured on this fixture, 4 of 12 cold starts reported
+  // index_files>0 / index_state=Ready / load_source=EmptyBootstrap / generation=0 at
+  // poll 0, with the search probe hitting (the targeted read admitted the very file
+  // it needed). Gate on the per-call trust evidence in `_meta` instead.
+  // Do NOT gate on the status body's `index_ready:` — that renders LiveIndex::is_ready().
+  // ponytail: fixed 80-poll x 250ms ceiling (~20s of sleep)
+  const EVIDENCE_KEY = "symforge/project_evidence";
   let ready = false;
-  for (let i = 0; i < 30 && !ready; i++) {
-    const status = (await callTool("status", {})).text;
-    const files = (status.match(/index_files"?\s*:?\s*(\d+)/) || [])[1];
-    if (files && Number(files) > 0) {
+  let evidence = null;
+  for (let i = 0; i < 80 && !ready; i++) {
+    const status = await callTool("status", {});
+    evidence = (status.result && status.result._meta && status.result._meta[EVIDENCE_KEY]) || null;
+    const sourceBound =
+      !!evidence &&
+      evidence.index_state === "Ready" &&
+      evidence.load_source !== "EmptyBootstrap" &&
+      Number(evidence.index_files) > 0;
+    if (sourceBound) {
       if (!READINESS_PROBE) {
         ready = true;
       } else {
         const probe = await callTool("search_symbols", { query: READINESS_PROBE });
         ...
       }
     }
     if (!ready) await new Promise((r) => setTimeout(r, 250));
   }
-  if (!ready) throw new Error("index never became queryable (cold-start timeout)");
+  if (!ready) {
+    // LOUD and specific. Never fall through to a snapshot comparison against a
+    // half-loaded index. Exit 2 = the harness could not run; exit 1 stays reserved
+    // for a REAL regression.
+    console.error(
+      "\n  HARNESS ABORT — the index never reached a source-bound Ready generation " +
+        "(80 polls x 250ms, ~20s of sleep).\n" +
+        `  last ${EVIDENCE_KEY}: ${JSON.stringify(evidence)}\n` + ...
+    );
+    proc.kill();
+    process.exit(2);
+  }
```

```javascript
+const DIFF_LINES = 5;
 function firstDiff(a, b) {
   // was: return on the FIRST differing line
   // now: collect up to DIFF_LINES differing lines, plus "N of M differing" count
 }
```

---

## 4. Questions

Answer as many as you can. **1, 3 and 4 are the ones I most need.**

**Q1 — Can the fix deadlock the index at `Loading` forever?**
This is the catastrophic case and it is worse than the bug being fixed. The guard says:
`load_source == EmptyBootstrap` ⟹ `Loading`. So the index escapes `Loading` only if
something sets `load_source` to another value once the real load completes. If the
detached load *mutates the placeholder in place* rather than replacing it, and does not
update `load_source`, the process serves `Loading` forever and every tool refuses. What
would you need to verify to rule this out, and can you rule it out from the diff alone?
(The claim I was given, unverified by me: the real load replaces the index via a path
that sets `FreshLoad`.)

**Q2 — Is `EmptyBootstrap` sufficient to cover the bug?**
The bug is "non-empty index with no `indexed_root`". The guard proxies that with
"`load_source == EmptyBootstrap`". Is there any *other* route to a populated, rootless
index — a different constructor, a reload path, a deserialization path, a rebind — that
this guard would miss? If yes, the bug survives in a rarer form and we would have
declared it fixed. Note the diff itself contains a hint: a comment says `remove_file`
never restores `is_empty`.

**Q3 — Is flipping `tests/fixtures/sidecar_contract/health.json` legitimate?**
This is a *contract* fixture — it appears to describe what an external consumer should
expect from a sidecar health response. It was changed from `"index_state": "Ready"` to
`"Loading"` with `file_count: 2, symbol_count: 2`. Two possibilities: (i) the fixture
models a bootstrap placeholder, and flipping it is correct; (ii) it models a normal
healthy index, and flipping it means the published contract now advertises `Loading` for
a healthy index — a real regression, hidden as a test update. Which is it, and what
would settle it? Nobody predicted this file would change.

**Q4 — Is the positive case still tested?**
Four existing assertions moved from `Ready` to `Loading`. Is there still *any* test
asserting that a properly-rooted, fully-loaded index publishes `Ready` after a mutation?
If all coverage of "mutation → Ready" was converted to "mutation → Loading", then a
future change that pins everything at `Loading` would pass the suite. A test suite that
cannot fail in the positive direction is a hole.

**Q5 — Does `is_ready()` delegating to `index_state()` change behaviour beyond the fix?**
This was not in the original scope. Previously `is_ready()` checked `is_empty`,
snapshot-verify state, and the circuit breaker. It now returns
`index_state() == Ready`, which additionally makes it false during `EmptyBootstrap`.
Is `is_ready()` called anywhere — a startup gate, a health endpoint, a readiness probe,
a loop — where newly returning `false` during bootstrap could hang or change an
externally visible contract?

**Q6 — Is the 20-second harness ceiling enough?**
The poll ceiling went from 30×250 ms (~7.5 s) to 80×250 ms (~20 s), and now requires the
*real* load to have published, not merely a non-zero file count. Cold-load timings
reported elsewhere for this project are around 10 s on a developer machine for the full
repository. The CI job containing this harness takes 26–45 minutes wall-clock, which
suggests a heavily loaded runner. If the real load exceeds 20 s under CI contention,
**every** run aborts. Is 20 s defensible, and what would you set it to?

**Q7 — Is `process.exit(2)` handled correctly by the caller?**
The harness now exits 2 on abort instead of throwing. Does that reliably fail a CI step
(GitHub Actions `run:` treats any non-zero as failure — but is this harness invoked
directly, or wrapped)? And is there any path where the abort branch is reached but the
child process is left running?

**Q8 — Is the `_meta` gate contract-safe?**
The new gate reads `result._meta["symforge/project_evidence"]` and depends on the fields
`index_state`, `load_source`, `index_files`. Is that a stable published surface or an
internal detail that could change without anyone noticing the harness broke? Note the
abort message does distinguish "absent evidence" from "still loading", which is a point
in its favour — but does that distinction actually hold?

**Q9 — Anything else that is actually wrong.**
Not style. Not naming. Something that would produce an incorrect result, hang, hide a
failure, or break a consumer.

---

## 5. What I already know — do not spend effort re-reporting

- The comments in this diff are verbose. That is deliberate house style; the project's
  binding rule is that a component may not report success for work it did not observe,
  and the comments record *why* each guard exists. Not a finding.
- `RUST_LOG=warn` plus inherited stderr makes CI logs noisier. That is the point, and it
  was measured at roughly one line on a healthy run.
- The fix does not address the *caller-side* ordering problem — `get_file_context`
  freshens a path before it checks the loading guard. The guard makes the resulting state
  honest rather than reordering the calls. If you think reordering is the better fix,
  say so, but note that the freshen path is shared with the file watcher and the edit
  path, so the guard is the single choke point.
- The published version at time of writing is 10.0.0; this fix is not yet released.

## 6. Status of this diff

Applied but **not yet committed**. The full gate (`cargo fmt --check`,
`cargo clippy --all-targets -D warnings`, `cargo test --all-targets --test-threads=1`,
`cargo build --release`) was still running when this packet was written, so the diff may
shift if a test failure forces a change. Review the logic; do not assume it compiles
clean.
