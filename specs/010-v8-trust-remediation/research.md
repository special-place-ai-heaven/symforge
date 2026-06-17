# Phase 0 Research: v8 Trust Remediation

Discovery is **already complete and verified** in
`docs/reviews/v8-trust-remediation-ledger.md` (two external skeptical reviews + two
advisory programs + a 5-agent code-verification panel, triple-confirmed against live
8.0.0 code @ 711ee68). This file records the **decisions** that resolve every design
fork; there are no open `NEEDS CLARIFICATION` markers.

Format per decision: **Decision / Rationale / Alternatives rejected**.

---

## D1 — `if_match` enforcement shape (US3 / TR-06)

**Decision**: Re-verify the guard **at the write**, inside the same critical section
that performs the splice and `atomic_write`. Thread an `if_match` field through
`ReplaceSymbolBodyInput` (`edit_tools.rs:570-676`) → the edit planner
(`edit_planner.rs:72`) → the write path (`edit_apply.rs:73-91`, `edit.rs:1160`). The
write path re-reads the on-disk bytes under the lock, compares against the supplied
guard, and rejects (no write) on divergence; the response only claims a guarded apply
when the guard was honored at the write.

**Rationale**: The TOCTOU window exists precisely because the only check today is a STEL
pre-flight (`edit_apply.rs:73`) with the guard never reaching the write. A second
pre-flight would leave the same window. Closing it requires the comparison to happen
against the *exact bytes about to be written*, under the write lock — the splice and the
verify cannot be separated by an async boundary.

**Alternatives rejected**:
- *Second pre-flight before write* — still a TOCTOU; a writer between pre-flight and
  splice clobbers silently. Rejected.
- *Advisory `if_match` (warn, write anyway)* — violates the honesty contract and US3's
  "guard actually guards". Rejected.
- *Tee-snapshot as the safety net* — the tee is best-effort recovery, "zero protection"
  against the clobber (ledger). It must NOT be presented as transactional rollback
  (FR-010).

**N-6 note**: batch executors have no `if_match` plumbing today; the single-edit fix
lands first, and the batch path carries a documented "no `if_match` (same TOCTOU if
extended)" marker rather than a silent false-safety control. `verify_index_matches_disk`
is also pre-flight-only and must not be advertised as a write-time guard.

---

## D2 — Economics grounding: ground now, reuse the existing estimator (US5 / TR-04)

**Decision**: Wire the existing byte-grounded estimator
(`format.rs:4925-5029`, `competent_manual_baseline_chars` /
`saved_tokens_vs_competent_manual`) into the STEL planner (`planner.rs:44-55`) so the
predicted figures derive from real request/result size instead of the hardcoded
`400/800`. This reopens the `degrade` / `bypass` / `mandatory_degrade` branches
(`controller.rs:40-135`) on real input. Confirmed in clarify (2026-06-17): grounding is
**in-scope for 010**, not deferred.

**Rationale**: The estimator already exists and runs on the response path; grounding is a
**wiring** effort, not new science. US5's acceptance scenarios (predictions vary by size;
a non-serve branch reachable) are binding. Until a given figure is grounded, the honest
heuristic label (FR-001, Phase A) is the floor — never a substitute.

**Alternatives rejected**:
- *Defer grounding, label-only* — explicitly declined in clarify. Rejected.
- *Build a new estimator* — duplicates `format.rs:4925-5029`; violates single-source.
  Rejected.
- *Keep the `400/800` constant but relabel `est_`* — that is Phase A's interim floor, not
  the US5 deliverable; leaving it there permanently fails US5's "predictions vary".
  Rejected as the end state.

---

## D3 — Status truth: proxy to the daemon + add a daemon `status` arm (US2 / TR-01)

**Decision**: `status_stel_tool` (`tools.rs:8529`) must read the **daemon** index facts
via the existing proxy channel, not the empty front-end `self.index`. The daemon has no
`status` arm today (`daemon.rs ~2435`) — add one that returns index readiness + counts +
ledger state, and route `status` through it like every other proxying reader
(`mod.rs:266`).

**Rationale**: Constitution Principle I — one authoritative index. The bug is that
`status` reads a *different* index than the one serving queries. Proxying makes the
readout reflect the served index. The compact surface currently has **no** honest
index-health tool (`health` is full-surface-only), so this also closes FR-007.

**Alternatives rejected**:
- *Read the front-end `self.index`* — it is structurally empty in the daemon-proxy
  topology. That is the bug. Rejected.
- *Hide the index fields on compact* — "one lie for another" (ledger Do-Not #6); leaves
  the agent with no health signal. Rejected.
- *Expose `health` on compact* — would widen the wire surface; the targeted `status`
  proxy is sufficient and respects compact-3. Rejected.

---

## D4 — Recovery: one surface-aware `empty_index_recovery_hint(profile)` (US4 / TR-02)

**Decision**: Replace the 4 distinct compact-reachable dead-end strings (and the message
fanned through 37 `loading_guard!` sites) with a single surface-aware
`empty_index_recovery_hint(profile)` that **never names a gated tool**. On the compact
surface it names only callable recovery (re-launch from project root, or the documented
opt-out); on the full surface it may name `index_folder`.

**Rationale**: The compact gate itself is correct (pure `(profile,name)->bool`, single
chokepoint `mod.rs:935`); the dead-end is a *message* defect, not a gate defect. The
message must be computed from the **active** surface, never a fixed string (FR-012).

**Alternatives rejected**:
- *Per-site string edits* — 37 sites + 4 strings drift; centralization is the only
  durable fix. Rejected.
- *Allow `index_folder` on compact to make the message true* — reverts compact-3 / widens
  the surface (ledger Do-Not #1). Rejected.

---

## D5 — Cold start populates the index (US4 / TR-03)

**Decision**: Fix the default Desktop wrapper so it does **not** `cd /d "%USERPROFILE%"`
(`init.rs:837`) before launch — the CWD must allow `find_project_root()` (`main.rs:217-248`)
to discover the project workspace. Write a proven init `env` (root / `SYMFORGE_SURFACE` /
auto-index hint) instead of `env:{}` (`init.rs:761`).

**Rationale**: Root cause (panel-corrected) is the wrapper CWD → `find_project_root()`
returns None → no root → no daemon → empty index → the agent then hits TR-02. Fixing CWD +
init env makes cold start actually index. Overlaps the parked 009 setup-wizard but is
self-contained here.

**Alternatives rejected**:
- *Auto-index `%USERPROFILE%`* — indexes the wrong (home) tree; expensive and useless.
  Rejected.
- *Only fix the error message (D4) without the CWD* — leaves cold start permanently empty;
  D4 alone is recovery, not a populated start. Both are needed. Rejected as sufficient.

---

## D6 — Honest labels are enumerated, behavior-preserving (US1 / Phase A)

**Decision**: Phase A is a single **zero-behavior** pass. Concretely:
`session_net_vs_manual` → `session_tokens_served` (TR-05, drop the `+`-only framing);
envelope figures gain `est_`/`heuristic` vs `measured` qualifiers (TR-11);
`calibration: "pending"` → `calibration: deferred` and `CalibrationState` relabeled
`deferred`/`observational` (do **not** delete the seam — ledger Do-Not #7, N-1); status
`l1..l4`/`handler_*` literals become enumerated states
(`l4_ledger: in_memory|durable|unavailable`, `index_state`, `empty_index_reason`);
drop the stale `ledger_persistence` from `deferred:`; bytes/4 figures labeled
"estimated tokens (chars/4)" (N-4). Docs demote A-009 → PARTIAL and A-028 (TR-12/13),
single-source A-005/A-016 (TR-16).

**Rationale**: Cheap-honest beats dishonest-confident; relabel is a valid, shippable fix.
**Relabel ≠ validate**: no OPEN assumption (A-011/A-015/A-016/A-028) is promoted to
VALIDATED by a label change.

**Alternatives rejected**:
- *Mark figures VALIDATED now that they're labeled* — violates relabel≠validate. Rejected.
- *Delete `CalibrationState` dead seam* — ledger Do-Not #7; relabel instead. Rejected.

---

## D7 — Enforced honesty CI (US6 / FR-018)

**Decision**: Add a CI honesty gate that (a) parses the assumptions register and **fails
the build** when a shipped surface claims a capability whose assumption is OPEN, and
(b) enforces one-source-of-truth per number (no figure with two divergent definitions).
Publish `docs/v8-capability-matrix.md` mapping feature → assumption ID → proof state
(Implemented / Heuristic / Observational / Deferred).

**Rationale**: Closes the loop so the honesty work cannot silently rot; frames v8 as a
*bet under test* (A-017/A-011 stay OPEN, never a proven-win claim).

**Alternatives rejected**:
- *Manual review only* — regresses over time; the spec requires automation (FR-018).
  Rejected.
- *Block honest OPEN-labeling changes* — the gate must allow correctly labeling a
  still-OPEN assumption; it only fails on *claimed validation* of an OPEN one (spec edge
  case). Rejected as written; the gate keys on "claim ⇒ proof", not on the word OPEN.

---

## Cross-cutting decisions

- **Per-phase verification** (FR-019): after each phase run `cargo fmt --check`,
  `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, and
  `cargo check --no-default-features --features embed`. Not only at the end.
- **Regression determinism**: the TR-06 test uses an injected interleave point (a test
  hook that mutates the file between guard-read and write), **not** a timing sleep.
- **Harness note**: the dev MCP binary may be any version (currently downgraded to
  7.27.0 for full-surface comfort); 010 correctness is proven by `cargo` against the
  on-disk 8.0.0 source. Live STEL-surface dogfood uses the **locally-built** 8.0.0 binary
  at verify time.
- **No new dependency, no new feature flag, no new index** — Constitution I/VI preserved.
