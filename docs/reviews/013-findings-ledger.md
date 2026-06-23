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

**DECIDED (not a defect — evidence-based):** the durable write is lossy ONLY under
pathological multi-process write contention, by design. WAL + `busy_timeout=5000`
blocks a writer up to 5s for the lock, so a drop requires >5s of sustained
contention; the typical topology is a SINGLE writer per project (one stdio session)
where the lock is uncontended and a drop cannot occur. Calibration derives from a
median over >= `TUNING_MIN_CORPUS` (24) samples + held-out validation, so a rare
dropped sample cannot move a tuned constant. The drop is NOT silent — it logs at
WARN (the evidence channel); if that fires frequently in practice, THAT is the
trigger to add a bounded retry/queue. The "best-effort" hand-wave comment was
replaced with this reasoned decision in `ledger_store.rs::record` (and the
migration-orphan comment reworded to "re-derivable", not "best-effort").

| D7 | **api-key store has the SAME doubled-path bug** (`root/.symforge/.symforge/api-keys.db`, `serve.rs:370`+`api_keys.rs:103`) — a SECOND live instance of D1's class, shipping in 8.5.0, hidden by the same test-blind-spot. | Major | OPEN | YES → D1-ROOT (the class) |

## Investigation outcomes (2026-06-22, tech-researcher + 1 adversarial pass)

- **D3-ROOT — SYSTEMIC but SHALLOW. Attack: EXTRACT-UP seam.** `ledger_store.rs` + `types.rs` are ALREADY protocol-free; `calibration.rs`'s only protocol tie is 4 `u32` consts (`COMPACT_SCHEMA_TOKENS`/`COMPACT_INVOKE_TOKENS`/`STATIC_RESPONSE_FLOOR`/`STATIC_MANUAL_FLOOR`). The coupling is the PARENT `mod stel` gate + `stel/mod.rs`'s unconditional `pub use controller/...` (which drag in `crate::protocol`). Fix: lift `types`+`ledger_store`+`calibration`+the 4 consts into a new `stel_core` module gated `#[cfg(any(server,embed))]`; server-`stel` re-exports FROM it; `controller` re-imports the consts via a shim. Delivers FR-001 embed durability + decouples storage from transport. Effort medium, risk low-medium (semver: widens the `embed` public contract → needs embed contract tests). The one missing receipt: `cargo check --no-default-features --features embed` on the spike — RUN IT.
- **D1-ROOT — SYSTEMIC (2 live instances). Attack: shared `paths::symforge_db_path(root, name)` helper** (root.join(`.symforge`).join(bare-filename)) used by ALL db stores; fix the live api-keys bug (D7); migrate analytics/coupling/frecency/stel-ledger; add a PRODUCTION-call-path regression test (the missing test that hid both instances). Other file stores (port_file/onboarding/tee/persist) already use the resolve-dir→bare-filename convention correctly; version_registry uses a separate home-dir scheme (unaffected).
  **FIXED `2cacda1`:** `paths::symforge_db_path(root, bare_name)` helper added; ALL 5 db stores (api-keys, analytics, coupling, frecency, stel-ledger) migrated through it; the live api-keys doubled path (D7) fixed → `root/.symforge/api-keys.db`; the 4 already-correct stores behavior-preserving; the 5 `.symforge/`-prefixed consts deleted; production-convention regression tests added (open via ROOT, assert single-prefix exists / doubled absent). **D16 (Minor, found+fixed in the same pass):** a `tools.rs` test hand-rolled `.symforge/analytics.db` (a third instance of the construction-by-hand smell, dodged the const grep) — routed through the helper. The class is now closed: no db store hand-rolls a path.
- **D2-ROOT — SYSTEMIC, only 1/4 fixed. Attack: extend the overlay** to the remaining 3 proxy-owned `status` lines (`ledger_events`, `last_ledger_decision`/`route`, the `calibration` section), sourced from the proxy's own ledger/store — not a render-architecture rewrite (that would be over-engineering for a 4-line leak).
  **FIXED `def21a3`:** `overlay_proxy_status_lines` now rewrites ALL proxy-owned lines (events, last decision/route, durable_ledger, the full calibration section) from a single proxy-side `StelStatusContext` rendered through the shared `render_proxy_owned_lines` (same formatters as the worker — no drift); index lines stay the worker's. The operator-critical leak is closed: on daemon-backed stdio the calibration verdict (`tuned`/`accumulating`) is now visible, not a permanent worker-blind `deferred`. Honesty mirror tested (empty proxy → truthful `deferred`/`0`/`unavailable`).

**Attack order by leverage:** D3 (foundation, unblocks calibration's clean home) → D1+D7 (a live shipping bug) → US2 auto-tune (lands in `stel_core`) → US3 surfacing → D2 overlay → D5 decision.

## US2 auto-tune review (2010cfe) — CRITICAL: the core math is unsound

| ID | Defect | Severity | Status | Root |
|----|--------|----------|--------|------|
| D8 | Auto-tune corrects the static **400 floor** and validates held-out error by predicting that floor — but the LIVE predictor is byte-grounded (`grounded_step_tokens`, `index_refs` present) and bypasses the floor for most served reads. The validated "improvement" does not reach the live path; `tuned` claims a win the predictor never receives. Worse than honest `deferred`. | **CRITICAL** | OPEN → ROOT-ATTACK | **D8-ROOT** |
| D9 | Scales fixed `schema`(45)/`invoke`(80) overheads by the response-bias factor, UNVALIDATED (validate only scores `response_floor`); corruption flows into `predicted_net` → serve/degrade/bypass. Test asserts the corruption as correct. | Major | OPEN | D8-ROOT (one factor, 4 constants, 1 validated) |
| D10 | `TUNING_MIN_SAMPLES`(12) re-applied to the 6-elem train slice after the parity split → true tune threshold is 23 not 12; surface renders absurd `accumulating (18/12)` (n>min). No test covers n∈[12,22]. | Major | OPEN | — |
| D11 | Train/held-out split is index-parity (not out-of-time) → train+test see the same distribution, hides estimator/codebase drift → optimistic by construction. | Major | OPEN | — |
| D12 | Embed open-and-record BEHAVIORAL proof test missing — D3 proved "compiles", not "works"; commit subject "embed durability now real" overstates. | Minor | OPEN | — |
| D13 | `RETUNE_HYSTERESIS_MARGIN` defined+documented but is a no-op (validate only re-applies the 20% bar). | Minor | FIXED (`6eeaf96`, deleted; genuine in-force-anchored hysteresis) | — |
| D14 | `derive_tuning_candidate` is `pub` + a foot-gun: a direct caller passing a full corpus derives a factor from the WHOLE corpus, not the train half — the leakage-free split lives only in `compute_calibration_verdict`. No live leak (all callers route through it), but the public API invites misuse. | Minor | OPEN | — |

| D15 | **Re-tune unit inconsistency** (introduced by `6eeaf96`): the live path RECORDS the corrected prediction (`raw·f₀`) into the ledger, but `derive` then learns a DELTA (`median(actual/(raw·f₀)) ≈ f_true/f₀`) while the live application applies that single factor to RAW → every re-tune (in_force≠1.0) under-corrects; the validate baseline double-applies `f₀` → inflated `error_before` → false `tuned` re-accept on a fabricated artifact. Same class as D8, displaced to tune #2+. Uncaught: NO test exercises a second tuning (all pass factor=1.0). | **Blocker** | OPEN → ROOT-ATTACK | D8-ROOT (one meaning for the recorded prediction) |

### D15 attack — store the RAW prediction; correct only at display/decision
The defect is that `predicted_response_tokens` has THREE meanings (record=corrected, derive/live=raw, validate=raw-again). Pick ONE: the ledger stores the RAW (pre-factor) prediction; the correction factor is applied ONLY to the served/decision economics (net + envelope display), never to the recorded sample. Then `derive` yields the ABSOLUTE `f_true`, the live path is `apply_factor(raw, f_true)`, and `held_out_mae(held_out, in_force_factor)` reconstructs the true live residual under any active tuning — all four links agree regardless of the in-force factor. Add a re-tune test (record 24+ events with a factor active, persist twice, assert the live prediction CONVERGES to actual and the second artifact is honest) — the missing coverage that hid this.

> **D8–D15 ALL FIXED + independently gate-verified** (`6eeaf96` D8–D13, `8963193` D14/D15). The calibration math is sound: learns an absolute `response_correction_factor`, validated against the REAL `|predicted·f − actual|` residual, applied to both live paths; records RAW so re-tunes converge (`second_tune_with_factor_in_force_stays_consistent` proves it, red→green); schema/invoke/manual untouched; `tuned` only with a real held-out win. Calibration suite 17/0, embed 1034/0, golden-replay byte-identical.

> D8/D9/D10/D11/D12 fixed in `6eeaf96` (calibrate the predictor OUTPUT via a single `response_correction_factor` validated against the REAL `|predicted·f − actual|` residual; schema/invoke/manual untouched; corpus gated at `2·MIN` with true-threshold display; out-of-time ts-ordered split; embed open-record proof). Pending independent re-review + gate.

### D8-ROOT — calibration must correct the predictor's OUTPUT against the REAL residual, not a floor the live path bypasses
**Attack (redesign, not patch):** calibration learns a single multiplicative `response_correction_factor` on the predictor's PREDICTED RESPONSE output (whatever sub-model produced it — byte-grounded OR floor), validated against the real `|predicted·factor − actual|` residual (the SAME quantity the live predictor errs on). Apply the factor to the live `predicted_response` AFTER grounding-or-floor, so BOTH paths are corrected. Leave `schema`/`invoke` FIXED (D9). This is self-honest: if the dominant byte-grounded path is already ~calibrated (ratio≈1.0), the factor≈1.0, the held-out gain is <margin, and it stays `Accumulating` — no false `tuned`; it only tunes when there is real systematic bias to correct, proven on the real residual. Also: gate the full corpus at `2·MIN` + render the true threshold (D10); time-ordered split (D11); add the embed open-record test (D12); remove/implement hysteresis (D13). D3 seam + honesty surfacing + the `Tuned{artifact}` type are SOUND — fix forward, do not revert.
