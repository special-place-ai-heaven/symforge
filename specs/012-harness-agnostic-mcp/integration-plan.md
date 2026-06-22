# 012 Serve-Path Integration Plan (vetted: design + adversarial critique applied)

Source: design workflow `wf_db5af05d` (3 parallel investigators → synthesis → adversarial critique). Critique verdict: **REVISE → PROCEED after must-fixes**; no fatal flaw. The additive side-route thesis (zero single-project regression *by construction*) survived attack. This doc folds in all 7 must-fixes.

## Target & precondition
- **Integration target = this worktree** `E:/project/symforge-012` (branch `feat/012-harness-agnostic-mcp`), where the engine primitive (`src/live_index/view.rs`: `IndexBase`/`Overlay`/`IndexView`/`WorkingSet`/`Targets`/`ProjectHit`) ALREADY exists. (The critique's "view.rs not in main" / "wrong tree" notes assume main is the target — it is NOT; we integrate on this branch. Engine is present.)
- **Re-anchor every line number by READING the actual file.** The design's net-change file:line map is approximate and partly wrong (critique #1): e.g. `DaemonState` ~`daemon.rs:112-125` (not 562-570), `execute_tool_call` *defined* ~`daemon.rs:2287` (2054 is a call site), the 3 search handlers are `SymForgeServer` methods in `tools.rs` (~4275/4476/6793), `IndexFolderInput` today is `{path, idempotency_key}` (~mod.rs:880-883). Grep/read to confirm before editing.

## Core thesis (keep — survived adversarial review)
Cross-project query is an **additive side route** that reads the session's `WorkingSet` directly and **never touches** the cached per-project `SymForgeServer`. The dominant single-project path is therefore byte-identical (same code, not "equivalent" code). All new params default to today's behavior. The 64 `self.index.read()` sites are **NOT** flipped for US1.

## Ownership (two-tier)
- **Per-daemon base intern table:** `DaemonState { bases: RwLock<HashMap<BaseKey, Arc<IndexBase>>> }`. Enforces SC-002 (equal `(root,commit)` → one `Arc<IndexBase>` shared via `Arc::clone`). Payload is the SAME `Arc<LiveIndex>` the project's `SharedIndex` already yields (`store.rs` `load_full`) — a shared handle, **no second store, no LiveIndex change** (Principle I compliant).
- **Per-session:** `SessionRecord { active_project_id: String /* was project_id */, working_set: Arc<RwLock<WorkingSet>> }`. `Arc<RwLock>` is mandatory — `SessionRecord` is cloned on every `session_runtime` call and `WorkingSet: Clone` deep-clones overlays; the `Arc` keeps that O(1) and overlay state singular.

## Lock order (critique #4 — the #1 destabilizer)
New order: **`bases` → `projects` → `sessions`** (extends the existing documented rule ~`daemon.rs:867-869/931-933`; `session_runtime` is on the hot path of every tool call). Never acquire upward. `index_folder_for_session`'s mid-function lock juggling must be re-checked against this.

## Phases (each gated by full `cargo test --all-targets -- --test-threads=1` green + the stress test)

**Phase 0 — base intern table (no behavior change).** Add `DaemonState.bases`; add `ProjectInstance::base()` (wraps `index.read()` into `BaseKey{canonical_root, CommitId::Sha(crate::git::head_sha) | Dirtyless}` + a `base_generation`); intern on project load/activate. Nothing reads `bases` yet.
- Verify: unit test — two `open_project_session` on same `(root,commit)` → `Arc::ptr_eq` bases. Full suite green.

**Phase 1 — per-session WorkingSet, single-entry, INERT.** Rename `project_id → active_project_id`; add `working_set` seeded with one entry (active project + interned base, **empty overlay**). `session_runtime` resolves via `active_project_id`, otherwise unchanged. No route reads the working set yet.
- Verify: **full suite green = the no-regression gate** (byte-identical single-project path).
- **CRITICAL (critique #4): add a multi-threaded concurrent stress test** — parallel `open_project_session` / `index_folder` / cross-route / `close_session` across threads — because `--test-threads=1` cannot surface the deadlock risk from the new locks on the hot path. This test gates Phase 0+1.

**Phase 2 — `index_folder(add:true)` opens additively.** `IndexFolderInput { + add: Option<bool> }` (default false/omitted = existing retarget verbatim, FR-006). `add:true` → intern base + `working_set.add(project_id, Arc::clone(base))` + join `session_ids` additively (NO evict; a NEW code path, not the destructive `needs_reassign` move). New daemon methods `add_project_to_session` / `set_active_project` (+ `remove_project_from_session`).
- Verify: open A, `index_folder(B, add:true)` → working set {A,B}, A still active/bound; `shares_base_with` if B reopened elsewhere. stdio embed (no daemon) → honest "multi-project requires the daemon" (Principle VII honesty), NOT a fake.

**Phase 3 — cross-project query route → US1 DELIVERED.** Add `project: Option<String>` + `projects: Option<Vec<String>>` to the 3 cross-project reads (`search_symbols`, `search_text`, `find_references`) and to `StelRequest`. Map → `Targets::One/Subset/All` (`["*"]`→All). **Both omitted → `Targets::One(active_project_id)` → today's exact bytes.** Mutually exclusive; empty `projects:[]` → InvalidRequest. Target keys on **project_id/alias, NEVER path** (path in `project=` → corrective InvalidRequest pointing at `index_folder(add:true)`). New dispatch branch in `execute_tool_call`: single-active → existing path untouched; else read `working_set`, call `WorkingSet::{...}`, format `ProjectHit` with `── project: <id> ──` headers (flat when one).
- Verify: **live end-to-end dogfood** — one MCP connection: `index_folder(A)`, `index_folder(B, add:true)`, query with `projects:["*"]` → attributed hits from BOTH repos **on the daemon path**; single-project query with no params → byte-identical to pre-change. stdio test asserts single-project parity + honest multi-project refusal (cross-project stdio cannot exist — daemon-only; state this so the implementer doesn't chase it).

## INVARIANTS (must hold for US1)
- **No US1 code path writes an `Overlay`.** Overlays exist but stay EMPTY until the deferred cross-project-write track. This keeps Principle I airtight and makes the (deferred) rebase hook a documented no-op until writes exist. (critique #6)
- All **mutation** (edit/`symforge_edit`/`analyze_file_impact`) stays **single-project** (`active_project_id`). Cross-project writes are deferred (no consent/which-project design exists).

## DEFERRED (do NOT build for US1)
- **Phase 4 — republish→rebase on HEAD advance + status working-set visibility (US2).** This is a FOLLOW-UP, NOT the US1 gate (critique #5). Ship US1 at Phase 3 with the documented limitation: *"a repo that commits mid-session may drop that project's hits until reopened."* Then do Phase 4 with a dedicated HEAD-advance test (`Overlay::rebase`, view.rs:171; guard the `StaleOverlay` skip at view.rs:949).
- **Phase 5 — flip the single-project read path to `IndexView`.** The "64 sites" count is wrong: it's 64 `self.index.read()` PLUS the `published_state`/`published_repo_outline` snapshot path (`tools.rs:3520/3588/5673`) PLUS `edit_tools.rs` (~13) (critique #3). Blocked on porting the ~20 `capture_*` methods. Only build if single-project sessions must see their own overlay edits in ordinary reads (beyond US1).
- **Base ref-count pruning** — YAGNI at "one harness, a few projects" scale; add only if a session-churn test shows unbounded `bases` growth (critique #7).
- Durable/rehydrated working set; per-tenant multi-harness isolation; similarity ranking; a dedicated working-set management tool.

## Principle compliance (verified in critique)
- **I:** one authoritative in-memory index per project; `bases` shares its `Arc<LiveIndex>`, no second store. ✔ (gated by the no-overlay-writes invariant)
- **VI:** engine types pure in-memory; `head_sha` via local `crate::git`; daemon-resident state stays in `daemon.rs`, never migrates into `view.rs`. ✔
- **VII:** params in shared input structs (identical both transports); working set keyed by `session_id` (the shared abstraction); stdio honestly refuses multi-project. ✔

## Implementation note
Phases are strictly sequential (same files, gate between). Suggested execution: Agent A = Phase 0+1 (foundation + lock order + concurrent stress test — the riskiest, verify hardest); Agent B = Phase 2+3 (additive open + cross-project route → US1) after A is full-suite-green. Then adversarial code-review + live dogfood before declaring US1 done.
