# Review request — SymForge cold-start fix, round 2 (post-remediation)

**What this is.** A Rust MCP server bug fix that has already been through one full
adversarial review round (three independent reviewers) and a remediation pass. This
asks you to review the **remediated** result, including code that no external
reviewer has seen.

**Where the code is.** Repo `special-place-ai-heaven/symforge`, branch
`fix/cold-start-ready-before-rooted`, off `main`. It is pushed — clone or fetch it.

```
git fetch origin
git diff origin/main...origin/fix/cold-start-ready-before-rooted
```

**What I want.** Answers to the seven questions in §4. Not a summary. If the fix is
wrong, say so. "Cannot determine from the diff" is a useful answer.

**Ground rules — these have each produced a wrong finding on this project in the
last 72 hours:**

- **A comment is not behaviour.** This diff is heavily commented; the comments are
  claims under review, not premises.
- **Existence is not invocation.** A function that *would* do X is not evidence that
  anything calls it.
- **"I checked them all" needs evidence.** A verification agent here reported four
  sites safe having opened three; an external reviewer opened the fourth and found
  the hole. If you sweep, say how many you opened.
- Cite `file:line`. Label confidence **proven / likely / speculative**.

---

## 1. The original bug (fixed, verified — context only)

On cold start with no snapshot, the process publishes a placeholder empty index
(`indexed_root: None`, `is_empty: true`, `load_source: EmptyBootstrap`) and
dispatches the real load detached. `get_file_context` freshens the requested path
**before** consulting the loading guard. The file is absent from the placeholder so
it looks stale and gets indexed on the spot — and that path sets `is_empty = false`
while never setting `indexed_root`.

`index_state()` consulted only `is_empty`, so it returned `Ready`.
`capture_published_manifest` hit its silent `?` on `indexed_root`, so no manifest, no
source identity, and knowledge published unbound. The receipt printed
`source=unknown` beside `counts total=0`.

**A file was admitted into the index before the index knew what repository it was.**

Reproduced at ~12% (3 of 25 cold starts).

## 2. What round 1 produced

Three reviewers (Composer, Grok, Kimi) plus an internal verification pass. All three
review documents are in the branch under `docs/reviews/`. Read them — they contain
findings you should not need to re-derive.

Round 1's headline: **the first fix introduced a second defect of the same class.**
Making `index_state()` return `Loading` for `EmptyBootstrap` broke the **no-root
lane** — the `else` arm in `main` that calls `set_local_empty_reason`, which spawns
neither watcher nor reload. That lane could never leave `EmptyBootstrap`, so it went
from `Empty → Ready` (dishonest) to `Empty → Loading forever`, hiding
`format::empty_index_recovery_hint`, the only message naming the real recovery.

"Reports Ready when it cannot answer" became "reports Loading when it will never
load."

The remediation added `local_empty_reason` as the discriminator: set exactly in that
lane, cleared by `apply_reload_data`. That lane now reports `Empty`.

## 3. What the remediation changed beyond that

- **A guard sweep of 14 sites.** One hole found and fixed (`validate_file_syntax`,
  which both *caused* the bug — calling it first could flip the server to Ready — and
  was its last *victim*). **Three HTTP sidecar sites were found to have no readiness
  check at all and were deliberately NOT fixed** (separate surface).
- **An embed API break, fixed additively.** `LiveIndex::empty()` + `add_file` is the
  documented public route for embedders and now yields a permanently-`Loading` index.
  New `LiveIndex::from_indexed_files(root, files)` requires a root so the broken
  shape cannot be built by accident. Contract-pinned in `src/embed.rs`.
- **Log gating**, because the new `warn!` fired ~10x per healthy cold start.
- **Harness readiness gate** now requires a source-bound Ready generation instead of
  `index_files > 0`, plus evidence preservation (stderr was being discarded,
  `RUST_LOG` was `off`, diffs showed one line).
- **Citations converted from line numbers to symbol names**, after the original
  line citations were found 50+ lines stale — and the corrections rotted again
  within one edit.

## 4. Questions

**Q1 — Does the two-condition guard leave a fourth wrong state?**
Every fix in this saga traded one dishonest state for another. Enumerate every
production-reachable combination of `(is_empty, load_source, indexed_root,
local_empty_reason)` and give `index_state()`'s verdict for each. Name any
combination where the verdict is wrong. **This is the most important question.**

**Q2 — Is the P1 local-ref lane still `Ready`?**
`LiveIndex::from_source_files` is legitimately rootless and builds its own source
identity via `build_ref_source_generation`. The guard deliberately keys on
`load_source` rather than `indexed_root` to avoid pinning it. Verify that holds after
the remediation. A regression there is worse than the original bug.

**Q3 — Is the embed fix actually sufficient?**
Does `from_indexed_files` give a genuine public path to a Ready in-memory index? Is
it pinned by a test that would fail if the path regressed? And is there any remaining
public route that still produces the permanently-`Loading` shape? Note a downstream
consumer builds indexes in-process from parsed files.

**Q4 — Can the new tests fail?**
For each test added, reason through the reverted case. A test that still passes with
the fix removed is worthless. Say which ones you checked.

**Q5 — The guard sweep claims 14 sites, one hole.**
Independently sweep for handlers that freshen a path. Do you find 14? Do you find the
same single unguarded one? The three HTTP sidecar sites are known and out of scope —
is that scoping defensible, or does it leave the original bug reachable in production
through that surface?

**Q6 — Is the log gating hiding a case where the warning is wanted?**
`capture_published_manifest` now debugs rather than warns for `EmptyBootstrap`. That
covers a cold start whose background reload **failed** — permanently stuck, and now
silent at `debug`. Is that acceptable given the failure is logged elsewhere, or does
it bury a real defect?

**Q7 — Anything else actually wrong.**
Not style, not naming. Something that produces an incorrect result, hangs, hides a
failure, or breaks a consumer.

## 5. Evidence already gathered — do not re-derive

- Full gate green: `fmt`, `clippy --all-targets -D warnings`,
  `cargo test --all-targets -- --test-threads=1` (exit 0), `cargo build --release`.
- Race probe **0 hits in 200 cold starts**, with a **working positive control**: the
  defect condition still fires ~8.3 times per cold start, so the zero means the guard
  works rather than that the window closed.
- The no-root lane was **driven live** and returned the recovery hint.
- Honest bound: 200 clean runs on one timing profile puts the residual near 1.5%, not
  zero.
- The daemon lane was **never exercised** — all runs used `SYMFORGE_NO_DAEMON=1`.
  That is a real gap, already known.

## 6. Known-open, deliberately not fixed

- Three HTTP sidecar sites with no readiness check.
- `daemon.rs` keeps an `EmptyBootstrap` placeholder when catalog capacity refuses a
  cold load; if it ever admits a file it pins the same way.
- A failed cold-start reload could surface as `Degraded`-with-reason rather than
  perpetual `Loading`.

Confirming these are correctly scoped out — or arguing one of them belongs in this
PR — is a legitimate answer to Q7.
