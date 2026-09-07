# Feature 020 Slice 5 — Neutrality Baseline (v11)

**Task**: frozen 020:T074 (Phase 7, mechanical removal). **Scope**: inventory and
baseline capture only — no product deletions in this commit.

**Binding acceptance chain** (Ike / Grove — `GOAL_FEATURE_020_SLICE5.md`):

1. T074 — this document
2. T075 — delete-only, Slice-4-proven-unreachable non-public code
3. T076 — `src/embed.rs` only after the allowlist negative suite proves V10
   surface unnameable
4. T077 — `FEATURE-020-SLICE5-EVIDENCE-v11.md` re-run proving four-axis
   neutrality (runtime authority, public behavior, writer reachability,
   activation mode)

**Captured-at ref**: `44e3230c8be86a79c979b693f41925d81b6511cf` (origin/main
after #683 squash). Branch: `cursor/feature-020-slice5-mechanical-removal`.

**Evidence rule**: every field below names the command that produced it
(`specs/029-mechanical-removal/data-model.md`, contract C-1).

---

## 1. Tip SHA and lifecycle phase

| Field | Value |
|---|---|
| `captured_at_ref` | `44e3230c8be86a79c979b693f41925d81b6511cf` |
| `lifecycle_phase` | `postactivation` |

**Command** (`lifecycle_phase`):

```
node scripts/lifecycle-phase-probe.cjs
```

**Observed stdout** (2026-09-07):

```
PHASE: postactivation
actual: 34  pre: 83  post: 34
scannedModules: [server_api]
actualOnly: []
postOnly: []
introduced atoms invisible to this set (>3 segments): 34 of 64 — covered by execution/refreeze_v11.py, not by the lifecycle checker
```

**Consequence** (research R1, `specs/029-mechanical-removal/research.md`): no
3-segment public atom is removable on this tree. The slice removal surface is
strictly non-public code with admissible Slice 4 unreachability evidence.

---

## 2. Public API / lifecycle atom baseline

| Field | Value |
|---|---|
| `public_atom_count` | 34 |
| `public_atom_digest` | `38c84c8541351d7de3b7fe5f5b1bc3f3d7514e79729d3a0aa067d3bdff7d40cc` |
| `traceability_checker` | OK — 78 requirements, 24 acceptance oracles, 13 retirement categories |
| `refreeze_allowlist` | passed (`verify-internal --target-ref HEAD`) |

**Commands**:

| Field | Command |
|---|---|
| `public_atom_count`, `public_atom_digest` | Replicate `derivePublicApiAtoms` + `directPublicAtoms` per `scripts/lifecycle-phase-probe.cjs`, then `sha256(JSON.stringify(sorted_atoms))` |
| `traceability_checker` | `node scripts/validate-lifecycle-oracle-traceability.cjs` |
| `refreeze_allowlist` | `python3 execution/refreeze_v11.py verify-internal --target-ref HEAD` |

**Observed 3-segment atom set** (34):

```
symforge
symforge::embed
symforge::embed::AtomicAuthority
symforge::embed::Claim
symforge::embed::ClaimProvenance
symforge::embed::EmbeddedSourceHandle
symforge::embed::EmbeddedSourceSpec
symforge::embed::EngineInfo
symforge::embed::EvaluationProvenance
symforge::embed::OperationKind
symforge::embed::OperationReceipt
symforge::embed::ProcessIndexRuntime
symforge::embed::ReceiptWaitError
symforge::embed::RefreshTicket
symforge::embed::RetryAdvice
symforge::embed::ShutdownReceipt
symforge::embed::ShutdownReport
symforge::embed::SourceCloseReceipt
symforge::embed::SourceCloseReport
symforge::embed::SourceRefusal
symforge::embed::SourceRefusalKind
symforge::embed::SourceRuntimePhase
symforge::embed::SourceRuntimeView
symforge::embed::SymbolMatch
symforge::embed::SymbolSearchRequest
symforge::embed::SymbolSearchResult
symforge::embed::TextMatch
symforge::embed::TextSearchRequest
symforge::embed::TextSearchResult
symforge::embed::engine_info
symforge::server_api
symforge::server_api::ServerBootstrapError
symforge::server_api::ServerExit
symforge::server_api::run
```

The remaining 34 of 64 `introduced_v11_atoms` (4-segment associated methods) are
owned by `refreeze_v11.py`, not the lifecycle checker alone (contract C-2).

---

## 3. Authority-reachability snapshot

| Field | Value |
|---|---|
| `writer_reachability_verdict` | **pass** — `all_ingress_uses_exact_typed_authority_branch` |
| `activation_result` | **pass** — `preventive_v1_is_the_only_live_mode` |
| `dark_seal` | 10/10 (`preventive_runtime_dark_v11`) |
| `slice0_oracle` | 12 controls preserved (7 red, 5 resolved-green) |

**Commands**:

| Field | Command |
|---|---|
| `writer_reachability_verdict` | `cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact` |
| `activation_result` | `cargo test --test activation_cut_v11 preventive_v1_is_the_only_live_mode -- --exact` |
| `dark_seal` | `cargo test --test preventive_runtime_dark_v11 -- --test-threads=1` |
| `slice0_oracle` | `node scripts/slice0-oracle-artifact.cjs` |

**Observed**:

- Campaign oracle (Slice 4 §2, `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md`):
  `1 passed; 0 failed` (exit 0).
- Activation mode oracle: `1 passed; 0 failed` (exit 0).
- Dark seal: `10 passed; 0 failed` (exit 0).
- Slice 0 contract written to
  `target/ci/lifecycle-v11/slice-0-oracle-contract.json`.

**Source pins** (from `tests/preventive_runtime_dark_v11.rs` after dark seal):

| Pin | SHA-256 digest | Files | Bytes |
|---|---|---:|---:|
| `FULL_SOURCE_PIN_V1` | `33c72c24874c63a51a7a2604445a1ce2598df5ed9058a7b514b2887a6ec084d1` | 197 | 9_514_604 |
| `EXCLUDED_RUNTIME_SOURCE_PIN_V1` | `3dc255e1ff1060b63ad67b18b0ec2c11f28d92f274448f8b1decf07a0b88c202` | 20 | 404_003 |

**Command** (`source_pins`): same as `dark_seal` — pins are constants asserted
by `full_source_set_matches_reviewed_darkness_baseline` and
`excluded_runtime_source_set_matches_reviewed_baseline`.

**Martin hard rejects** (binding for T075/T076 inventory — not deletion
candidates):

| Reject | Rationale |
|---|---|
| `ActivationMode::LegacyOpen` / `LegacyClosing` | Live bootstrap states on every process start (`src/index_lifecycle/activation.rs`; Slice 5 plan non-target, `specs/029-mechanical-removal/plan.md`) |
| `ProjectPublicationRoot` / Track A publication seams | Live V11 production seams; deletion would orphan same-tree receipts (contract C-3) |
| `serialize_index` | PHASE 4b House item — **still present** at `src/live_index/persist.rs:278`; excluded from Slice 5 deletion set |

---

## 4. Behavior / activation baseline (T077 re-run set)

Every gate below must be re-run unchanged in T077 except source pins whose file
sets an actual removal intersects (contract C-5).

| Gate | Command | Result | Notes |
|---|---|---|---|
| fmt | `cargo fmt --check` | exit 0 | clean |
| clippy | `cargo clippy --all-targets -- -D warnings` | exit 0 | ~102 s cold |
| lib+bins+tests serial | `cargo test --lib --bins --tests -- --test-threads=1` | exit 0 | lib cell 3339 passed, 4 ignored; full harness exit 0 |
| embed cell | `cargo test --no-default-features --features embed --lib -- --test-threads=1` | exit 0 | 1362 passed, 4 ignored |
| release build | `cargo build --release` | exit 0 | |
| tool correctness | `node scripts/verify-tools.cjs --bin target/release/symforge` | exit 0 | 7 PASS, 1 REVIEW, 0 FAIL |
| lifecycle traceability | `node scripts/validate-lifecycle-oracle-traceability.cjs` | exit 0 | postactivation branch |
| lifecycle phase | `node scripts/lifecycle-phase-probe.cjs` | exit 0 | `PHASE: postactivation` |
| refreeze allowlist | `python3 execution/refreeze_v11.py verify-internal --target-ref HEAD` | exit 0 | |
| slice 0 oracle | `node scripts/slice0-oracle-artifact.cjs` | exit 0 | 7 red, 5 resolved-green |
| npm | `cd npm && npm test` | exit 0 | 31 passed |
| writer reachability | `cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact` | exit 0 | 1/1 |
| activation mode | `cargo test --test activation_cut_v11 preventive_v1_is_the_only_live_mode -- --exact` | exit 0 | 1/1 |
| dark seal | `cargo test --test preventive_runtime_dark_v11 -- --test-threads=1` | exit 0 | 10/10 |

**Public-surface trio** (contract C-2 — run after every removal landing, not
only embed):

```
node scripts/validate-lifecycle-oracle-traceability.cjs
node scripts/lifecycle-phase-probe.cjs
python3 execution/refreeze_v11.py verify-internal --target-ref HEAD
```

**Neutrality bracket control** (T074/T077, contract C-1): not armed in this
commit. T075 must run a deliberate control edit before any deletion, record
`control_result = detected(<field>)`, then discard the edit.

---

## 5. Proposed T075 / T076 deletion inventory

**Inventory rules** (Martin execution rejects + contract C-4):

- No candidate without executed Slice 4 unreachability evidence.
- No structural reshape; delete-only.
- Task wording, legacy-sounding names, and roster prose are not evidence.
- Track A seam fixes and Ada-B / mint-gate work are out of scope.

### 5.1 Hard excludes (never list for deletion)

| Item | Disposition | Evidence |
|---|---|---|
| `ActivationMode::LegacyOpen`, `LegacyClosing` | **RETAIN** | Live bootstrap path; Martin reject #1 |
| `ProjectPublicationRoot`, publication trunk seams | **RETAIN** | Live V11 seams; Martin reject #2 |
| `live_index::persist::serialize_index` | **RETAIN (House)** | Present `src/live_index/persist.rs:278`; PHASE 4b |
| `IndexLoadSource::EmptyBootstrap` / bootstrap placeholder storage | **RETAIN** | Live admission path (`src/live_index/store.rs`); not proven unreachable |
| `IndexState::Loading` / `CircuitBreakerTripped` / `loading_guard!` | **RETAIN** | Live ingress refusal path (`src/protocol/tools.rs`, `edit_tools.rs`) |
| Compatibility alias routes (`trace_symbol`, `detect_changes`) | **RETAIN** | Live daemon dispatch with typed authority (`tests/activation_cut_v11.rs` inventory) |

### 5.2 Discharged in Slice 4 (roster-predicted, already absent)

These satisfy contract C-6; T075 must not re-delete adjacent code to appear
productive.

| Predicted removal | Observed at baseline | Slice 4 evidence |
|---|---|---|
| `src/gitignore_hygiene.rs::atomic_replace` | **absent** — comment only at `:154` | C2b execution map (`specs/028-preventive-activation-cut/activation-cut-execution-map.md`); retirement inventory anchor retired |
| V10 raw `pub mod` crate-root surface | **absent** from public census | C5 exposure flip (`4823ad6a`); `src/lib.rs`, `src/internals.rs` |
| V10 raw embed update/remove exports | **absent** — V11 facade only | C5 (`embed.rs` module header); T067 map record |
| Bare `index: SharedIndex` / `project_indexes` root-holder fields | **absent** — census green | C4a/C4b; `preventive_runtime_dark_v11::root_holders_store_no_bare_shared_index` |
| `proxy_reset_calibration_receipt` dead `#[cfg(not(feature = "server"))]` twin | **absent** | T038 round 2 cfg-lens (`docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md` §7b) |

### 5.3 T075 candidates (Slice-4-proven-unreachable, still to enumerate)

**Baseline verdict**: no production item meets **both** (a) admissible Slice 4
unreachability citation and (b) still-present non-public code at
`44e3230c8be86a79c979b693f41925d81b6511cf`.

T075 opens with a file-scoped pass for:

- stale compatibility **comments** naming retired constructs (not live routes);
- obsolete tests whose sole subject is a member already discharged in §5.2;
- any non-public helper the dark seal or campaign oracle proves unreachable but
  which survived the Slice 4 activation commits.

Each candidate T075 lands must cite a specific executed reachability case or
retirement-inventory disposition **and** pass the Martin rejects in §5.1.
An empty T075 outcome is conforming (contract C-7).

### 5.4 T076 — embed (`src/embed.rs`)

| Field | Baseline observation |
|---|---|
| File shape | 163 lines; V11 facade + `EngineInfo` + `contract` tripwire tests only |
| V10 raw surface | **already retired** in Slice 4 C5 (module header, lines 13–19) |
| Gate before touch | `python3 execution/refreeze_v11.py verify-internal --target-ref HEAD` must prove any remaining V10 embed surface **unnameable** |
| Baseline disposition | **likely discharge (already-removed)** — pending allowlist verdict in T076; do not edit `src/embed.rs` until the suite speaks |

---

## 6. Pending work (not in this commit)

| Task | Deliverable |
|---|---|
| T075 | Enumerate and delete only §5.3 candidates with per-item evidence; refresh pins after `fmt` |
| T076 | Allowlist negative suite, then embed deletion or formal discharge |
| T077 | Re-run §4 gates into `FEATURE-020-SLICE5-EVIDENCE-v11.md`; arm neutrality bracket; adversarial review |

---

## 7. References

- `GOAL_FEATURE_020_SLICE5.md` (Ike / Grove binding acceptance summary)
- `docs/reviews/FEATURE-020-POST-V11-LEDGER.md` Track C (T074–T077)
- `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md`
- `specs/029-mechanical-removal/` (neutrality bracket, quickstart, data model)
- `specs/020-repository-knowledge-index/tasks.md` Phase 7 (958–966)
