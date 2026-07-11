# SymForge — Outstanding Work, Deferred Items & Backlog

**Compiled**: 2026-07-10 · **Resolved**: 2026-07-11 (018 hardening campaign,
branch `018-dogfood-surface-hardening`) · Status tags: 🔴 blocking/now ·
🟠 next · 🟡 deferred · 🔵 investigation · ⚪ housekeeping · ✅ resolved
(with evidence pointer).

Every item below now carries an explicit resolution: **implemented**,
**superseded**, **environment-only**, **operator-gated**, or **deferred**
(with rationale). Evidence pointers are commits on this branch, tests, or
live verification.

---

## 1. In-flight — needs closing now

| # | Item | Status | Resolution |
|---|------|--------|-----------|
| 1.1 | **018 verification gate** | ✅ superseded | The original 018-story gate was absorbed into the hardening campaign's final gate (plan Task 13). Current branch state: `fmt`/`check`/`clippy --all-targets -D warnings`/full `--all-targets` suite green at every slice commit (`d0623f5`, `f40352d`, `7656699`; 110 test binaries, 3459 passed, 0 failed on the latest full run). Release build + embed + npm gates run in Task 13. |
| 1.2 | **018 merge** | 🔴 operator-gated | Unchanged: merge to main requires explicit user approval, with the release-please double-count guard (`gh pr merge N --merge --delete-branch --body "PR #N"`). |
| 1.3 | **018 `tasks.md` status** | ✅ implemented | `specs/018-dogfood-surface-hardening/tasks.md` statuses updated as part of Task 12 (this commit). |
| 1.4 | **8.13.9 publish verification** | ✅ verified | `npm view symforge version` → `8.13.9` (2026-07-11); installed binary reports `symforge 8.13.9`. |
| 1.5 | **`cargo clean`** | ⚪ operator-gated | After merge+push, per CLAUDE.md cleanup discipline. |

---

## 2. Major deferred feature — the big one

### 2.1 Feature 019: Multi-index router / per-session index isolation — ✅ implemented (this branch)

The regression investigation concluded and the capability was implemented as
the 018 hardening campaign rather than a separate spec:

- **Immutable home + additive `index_folder`** — a session's home project can
  never be retargeted; additional projects open additively into the session's
  working set with a durable idempotency ledger (`ea342f8`).
- **Explicit project routing** — 15 read/guidance tools + 7 structural edit
  verbs accept `project`; set-valued discovery (`search_symbols`,
  `search_text`, `find_references`, `search_files`) accepts `projects`,
  merged under one deterministic cap/budget (`d651ba5`, `3d5a209`, `489c285`,
  `f40352d`). The compact `symforge` facade routes a single `project` through
  every planned step (`f40352d`).
- **Trust evidence** — `status(detail="projects")` session inventory with
  per-project ID/root/home/counts/generation/snapshot plus session
  `last_seen`/`ttl_secs`; typed `ProjectEvidence` on every routed tool result
  (`7be810e`, `ed143c4`, `d0623f5`).
- **Daemon uniqueness + lifecycle** — guarded-start singleton seam (a
  foreground `symforge daemon` or auto-spawn race can never overwrite a live
  daemon's runtime record), owner-checked shutdown cleanup, session reaper
  with TTL, per-adapter session descriptors so sibling adapters cannot
  clobber each other's records (`671b281`, `bc96594`, `c0e6307`, `d0623f5`).
- **Root cause of the observed hijack** (daemon re-rooted to another project,
  5 uncoordinated daemons): retarget removal + guarded start + owner-checked
  cleanup + descriptor identity validation. The mis-rooted-daemon symptom
  observed live on 2026-07-11 came from the *installed 8.13.9* binary, which
  predates these fixes; it ships with the next release.

**Known residual (documented, deferred):** interned cross-project bases are
snapshots — a long-lived cross-project session does not track watcher reloads
of a sibling project until re-open (the Phase-4 "live rebase" note in
`src/live_index/view.rs`). Single-project sessions are unaffected.

---

## 3. Deferred residuals inside shipped/in-progress work

| # | From | Item | Status |
|---|------|------|--------|
| 3.1 | 018 US1 | **Admission-tier demotion of pure-data directories** (the "JSON-ratio" content heuristic proposal). | ✅ closed as superseded — the shipped generated-output demotion (path-shape + git tracked-prefix evidence at discovery/watcher, `f4e972c`) covers the acute case (untracked data/output dirs never become first-class symbols) without a content-ratio heuristic. A ratio heuristic on *tracked* data dirs remains 🟡 deferred: it changes global ranking/search behavior and needs its own spec + corpus evidence. |
| 3.2 | 018 US4 | **CCR-expansion proposal**: retrieval footers for single-target reads (`get_symbol`, `get_file_context`, …). | ✅ closed as rejected for now — single-target reads are not large ranked payloads; they now carry explicit truncation footers with corrective hints, and (2026-07-11) any post-assembly cut downgrades a stamped `Completeness: full` claim honestly (`enforce_token_budget_flagged` + envelope downgrade). CCR stays scoped to ranked search surfaces. Revisit only with evidence that agents re-fetch cut single-target reads at material token cost. |

---

## 4. Grok dogfood report backlog (`docs/grok_report.md`)

| # | Finding | Status | Resolution |
|---|---------|--------|-----------|
| 4.1 | **#2 — Multi-project retrieval incomplete** | ✅ implemented | Read/guidance tools accept `project`; `search_files` gained `project` + set-valued `projects` with attributed fan-out (`d651ba5`, `3d5a209`, `f40352d`). Pinned by `daemon::tests::test_project_routing_parity_table` and `test_search_files_projects_fan_out`. |
| 4.2 | **#6 — data files as first-class symbols** | ✅ acute fixed / 🟡 residual = §3.1 | Defaulting fix shipped in 018 US1; untracked generated-output demotion shipped (`f4e972c`). Tracked-data-dir ratio heuristic deferred with §3.1. |
| 4.3 | **#3 — large core file demoted to Tier 2** | ✅ implemented (root cause corrected) | The report attributed it to size; the live 2026-07-11 reason was the binary sniff misreading an 8 KB window cut mid-multibyte char (verified against the real bytes of `src/protocol/tools.rs`, byte 8190). Fixed in `7656699`; code files already had the 4 MB threshold. |

---

## 5. Environment / infrastructure issues

| # | Item | Status | Resolution |
|---|------|--------|-----------|
| 5.1 | **Daemon mis-rooted / multi-daemon sprawl** | ✅ implemented (code) / ⚪ env | Code fixes per §2.1. Installed daemons keep the old behavior until the next release ships; restart harness sessions after updating. |
| 5.2 | **codex-subagents server PATH** | ⚪ environment-only | Other-machine configuration; not SymForge implementation work. |
| 5.3 | **PATH strategy decision** | ⚪ environment-only | User decision (snapshot vs. inherit-live); recommendation on file: inherit-live. |
| 5.4 | **Terminal Commander daemon down** | ✅ environment resolved | TC updated and reconnected 2026-07-11 (facade surface). Product feedback for TC lives in the design/final handoff, not this ledger. |
| 5.5 | **C: disk under floor** | ⚪ environment-only | Pre-existing; SymForge builds target E:. |

---

## 6. Housekeeping

| # | Item | Status | Resolution |
|---|------|--------|-----------|
| 6.1 | `docs/grok_report.md` | ✅ | Tracked in the repository (valuable dogfood record). |
| 6.2 | `docs/grok-dogfood-prompt.md` | ✅ implemented | Canonical v1 written 2026-07-11: mandatory artifact self-disclosure, start/end version + counts/tiers, clean-checkout deltas, pre-existence marking, cleanup proof, trust-envelope audits. |
| 6.3 | Stale build dirs | ✅ | Removed earlier (`target-wtfix*`). |
| 6.4 | Grok CLI `init` support | 🟡 deferred feature | `InitClient::Grok` + `register_grok_mcp_server` (clone the Codex TOML path). Feature-sized backlog item; not part of the hardening scope. |

---

## Remaining before this branch is merge-ready (plan Task 13)

1. Final gate battery: `cargo fmt --check`, `git diff --check`, `cargo check`,
   `cargo clippy --all-targets -- -D warnings`,
   `cargo test --all-targets -- --test-threads=1`, `cargo build --release`,
   `cargo check --no-default-features --features embed`, `npm test`.
2. Release-binary multi-project dogfood with isolated `SYMFORGE_HOME`.
3. Fill `docs/reviews/2026-07-10-tool-substitution-scorecard.md` from
   controlled runs.
4. Adversarial review pass + Review section in `tasks/todo.md`.
5. Operator gates: merge approval, publish, `cargo clean`.
