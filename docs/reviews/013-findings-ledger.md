# 013 Predictor Calibration — Findings Ledger

Every defect found during 013 is logged here ON THE SPOT, named as a defect (never
an "honest gap"/"caveat"/"best-effort"), fixed ASAP, and assessed immediately for
whether it is a SYMPTOM of a systemic root. Systemic roots get a proper
investigation + an immediate attack on the root — not a per-instance patch.

Status: OPEN | FIXED | INVESTIGATING | ROOT-ATTACK

| ID | Defect | Severity | Status | Systemic root? |
|----|--------|----------|--------|----------------|
| D1 | Durable db written to a DOUBLED path `root/.symforge/.symforge/stel-ledger.db` (const carries the `.symforge/` prefix AND callers pass `ensure_symforge_dir(root)`). Pre-existing since serve (010). | Major | FIXED (`44cbea3`) | **YES → D1-ROOT** |
| D2 | Daemon-default `status detail:full` reported `durable_ledger: unavailable` while events DID accumulate (worker renders status; store lives on the proxy). | Major | FIXED (`44cbea3` overlay) | **YES → D2-ROOT** |
| D3 | FR-001 embed durability NOT delivered: the durable store cannot reach the `embed` facade. | Major | INVESTIGATING → ROOT-ATTACK | **YES → D3-ROOT** |
| D4 | Crash-durability test was a clean-close (drop checkpoints the WAL), proving nothing about crash durability. | Minor | FIXED (`87305da` real process-abort) | No (test defect) |
| D5 | `record()` silently drops a single event if `busy_timeout` is exceeded under contention. | Minor | OPEN | TBD (see D5 below) |
| D6 | The calibration feature itself is unimplemented: the predictor does not tune (US2 auto-tune) and the honest state machine is not surfaced (US3). This is the feature, not a gap. | Major | OPEN (in progress) | No (scoped work) |

## Systemic roots (proper investigation + immediate attack)

### D1-ROOT — no enforced db-path construction convention
D1 was not a typo; it is a symptom. There is no single helper that builds a
`.symforge/<name>.db` path, so every store hand-rolls it and the ledger diverged
(double-prefixed) while analytics/coupling/frecency join the prefixed const against
the ROOT. **Attack:** a single `symforge_db_path(root, name)` helper used by ALL
stores so no store can double-prefix again; audit the other stores to confirm none
share the defect. (Per-instance fix D1 already shipped; the root fix prevents the class.)

### D2-ROOT — proxy-owned state invisible to worker-rendered responses
D2 is a symptom of a structural split: the proxy owns some state (the durable store)
while the daemon worker renders some responses (`status`), so any proxy-side state a
worker-rendered surface depends on reads as absent. The overlay fixed `status`'s
durable line. **Attack/investigate:** are there OTHER proxy-owned states that a
worker-rendered response silently misrepresents? If yes, the overlay pattern (or a
proxy-render-owns-its-state rule) must cover them, not just this one line.

### D3-ROOT — storage/calibration COUPLED to the protocol/transport layer
This is the real architecture defect. `src/lib.rs` gates the whole `stel` and
`protocol` modules behind `#[cfg(feature="server")]`, and `stel::{controller,
executor,planner,edit_apply}` hard-import `crate::protocol::*` (rmcp/axum). So the
ledger + calibration — which are STORAGE + pure math, transport-agnostic by nature —
cannot exist without the server/transport stack. That coupling is why embed cannot
get durability, and it is a smell beyond embed (storage entangled with transport).
**Attack:** extract a protocol-free ledger + calibration seam (the pure store, the
`derive/validate/CalibrationVerdict` math, the POD types) compilable under
`any(server, embed)`, leaving the protocol-dependent parts server-only. This both
delivers FR-001 embed durability AND decouples the architecture. Investigation in
progress to scope the minimal clean seam before the attack.

### D5 — decide the durability guarantee (not "best-effort")
"Best-effort" is a non-decision. Either `record()` must guarantee the write
(bounded retry / WAL-append is already durable so the real question is the lock
acquisition under contention), or the contract is explicitly "calibration sampling
is lossy under heavy multi-writer contention, by design, because N missed samples
out of thousands do not move a calibration." Decide it on evidence (how often does
the busy_timeout actually trip?), record the decision here, and implement to match —
no hand-wave comment.

| D7 | **api-key store has the SAME doubled-path bug** (`root/.symforge/.symforge/api-keys.db`, `serve.rs:370`+`api_keys.rs:103`) — a SECOND live instance of D1's class, shipping in 8.5.0, hidden by the same test-blind-spot. | Major | OPEN | YES → D1-ROOT (the class) |

## Investigation outcomes (2026-06-22, tech-researcher + 1 adversarial pass)

- **D3-ROOT — SYSTEMIC but SHALLOW. Attack: EXTRACT-UP seam.** `ledger_store.rs` + `types.rs` are ALREADY protocol-free; `calibration.rs`'s only protocol tie is 4 `u32` consts (`COMPACT_SCHEMA_TOKENS`/`COMPACT_INVOKE_TOKENS`/`STATIC_RESPONSE_FLOOR`/`STATIC_MANUAL_FLOOR`). The coupling is the PARENT `mod stel` gate + `stel/mod.rs`'s unconditional `pub use controller/...` (which drag in `crate::protocol`). Fix: lift `types`+`ledger_store`+`calibration`+the 4 consts into a new `stel_core` module gated `#[cfg(any(server,embed))]`; server-`stel` re-exports FROM it; `controller` re-imports the consts via a shim. Delivers FR-001 embed durability + decouples storage from transport. Effort medium, risk low-medium (semver: widens the `embed` public contract → needs embed contract tests). The one missing receipt: `cargo check --no-default-features --features embed` on the spike — RUN IT.
- **D1-ROOT — SYSTEMIC (2 live instances). Attack: shared `paths::symforge_db_path(root, name)` helper** (root.join(`.symforge`).join(bare-filename)) used by ALL db stores; fix the live api-keys bug (D7); migrate analytics/coupling/frecency/stel-ledger; add a PRODUCTION-call-path regression test (the missing test that hid both instances). Other file stores (port_file/onboarding/tee/persist) already use the resolve-dir→bare-filename convention correctly; version_registry uses a separate home-dir scheme (unaffected).
- **D2-ROOT — SYSTEMIC, only 1/4 fixed. Attack: extend the overlay** to the remaining 3 proxy-owned `status` lines (`ledger_events`, `last_ledger_decision`/`route`, the `calibration` section), sourced from the proxy's own ledger/store — not a render-architecture rewrite (that would be over-engineering for a 4-line leak).

**Attack order by leverage:** D3 (foundation, unblocks calibration's clean home) → D1+D7 (a live shipping bug) → US2 auto-tune (lands in `stel_core`) → US3 surfacing → D2 overlay → D5 decision.
