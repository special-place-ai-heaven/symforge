# Feature 020 Slice 5 — Mechanical Removal Evidence (v11)

**Task**: frozen 020:T075–T077 (Phase 7, mechanical removal). **Scope**: delete-only
pass on non-public code with Slice 4 unreachability evidence; neutrality re-run.

**Branch**: `cursor/feature-020-slice5-mechanical-removal` (draft PR #685).

**Binding**: Martin formal GATE — GREEN-LIGHT empty/thin C-7 at T074 HEAD
`4edd8fdf26ea`. No production deletes invented.

**Baseline**: `docs/reviews/FEATURE-020-SLICE5-BASELINE-v11.md` (T074).

**Plain outcome**: **no production code was removed in this slice.** T075 closed
with an honest empty C-7 pass. T076 formally discharged the already-removed V10
embed surface without editing `src/embed.rs`.

---

## 1. Neutrality bracket (C-1)

**Before ref**: `4edd8fdf26ea` (T074 baseline).

**Control procedure** (quickstart Step 3, contract C-1): one deliberate edit to a
field the baseline covers; re-capture; comparison must name the moved field;
discard before any removal.

**Control edit** (transient, not shipped):

```
echo '// C-1 neutrality-bracket control (transient — discard before T075 close)' >> src/lib.rs
```

**Re-capture command**:

```
cargo test --test preventive_runtime_dark_v11 full_source_set_matches_reviewed_darkness_baseline -- --exact
```

**Observed stdout** (2026-09-07):

```
test full_source_set_matches_reviewed_darkness_baseline ... FAILED
assertion `left == right` failed: an in-tree source candidate changed. Re-review the complete src diff for a new direct, aliased, macro-generated, trait, inherent-method, registration, or re-export bridge before updating this pin
  left: ("4aed5fcc9f8a96d83147a41914fff9328748376485e047996f3efb73326b31aa", 197, 9514680)
 right: ("33c72c24874c63a51a7a2604445a1ce2598df5ed9058a7b514b2887a6ec084d1", 197, 9514604)
```

**Comparison**:

| Field | Value |
|---|---|
| `control_result` | `detected(source_pins)` |
| `control_description` | One-line append to `src/lib.rs` changed `FULL_SOURCE_PIN_V1` digest and byte count (+76 bytes); file count unchanged (197). |
| `differing_fields` (control only) | `source_pins` |

**Discard command**:

```
git checkout -- src/lib.rs
```

**Post-discard verification**: `git status` clean before T075 close recorded.

**Bracket state**: **armed**. Zero-delete T075 authorized under C-7 once C-6
discharges are recorded below.

---

## 2. T075 — delete pass (C-7 empty)

**Martin gate**: §5.3 honest empty conforming. Thin deletes only when
evidence-gated (stale comments / obsolete §5.2-only tests / non-public helper
with dark-seal or oracle citation).

**File-scoped pass** (2026-09-07):

| Category | Method | Result |
|---|---|---|
| Stale compatibility comments naming retired constructs | `rg` over `src/` for `RETIRED`, `atomic_replace`, live alias prose | No comment-only deletion warranted: the sole `atomic_replace` mention (`src/gitignore_hygiene.rs:154`) documents a C2b mid-cut behavioral residual, not a stale route name |
| Obsolete tests whose sole subject is §5.2-discharged | `rg 'fn.*atomic_replace\|test.*atomic_replace'`; `rg V10.*embed` in `tests/` | No test's sole subject is a discharged member — `activation_cut_v11` inventory rows cite retired paths as oracle documentation, not as executable subjects |
| Non-public helpers proven unreachable (dark seal / campaign oracle) | Baseline §5.3 + Martin hard rejects §5.1 | No still-present non-public helper meets both admissible Slice 4 citation and absence from hard-retain set |

**RemovalCandidate table**:

| Item | Visibility | Evidence | Disposition |
|---|---|---|---|
| *(none)* | — | Baseline §5.3 verdict upheld after file-scoped pass | **no production code removed** |

**Hard RETAIN unchanged** (Martin rejects, not deletion candidates):

- `ActivationMode::LegacyOpen` / `LegacyClosing`
- `ProjectPublicationRoot` / Track A publication seams
- `live_index::persist::serialize_index` (PHASE 4b — see §6)
- `IndexLoadSource::EmptyBootstrap`, `IndexState::Loading` / circuit-breaker live paths
- Live compatibility alias routes (`trace_symbol`, `detect_changes`)

**Pin refresh**: not required — no `src/` deletions landed.

---

## 3. C-6 — DischargedExpectation records (§5.2 roster-predicted)

Each prediction from baseline §5.2: command + verbatim output establishing
`observed`.

### 3.1 `src/gitignore_hygiene.rs::atomic_replace`

**Predicted**: delete retired byte writer (C2b execution map).

**Command**:

```
rg -n '^(pub )?fn atomic_replace' src/gitignore_hygiene.rs; rg -n 'atomic_replace' src/gitignore_hygiene.rs
```

**Observed stdout**:

```
---
154:    // `atomic_replace` did; nothing pins that behavior.
---
```

**Discharge**: `already-removed` — function absent; comment-only residual at `:154`.

### 3.2 V10 raw `pub mod` crate-root surface

**Predicted**: remove raw `pub mod` declarations from `src/lib.rs` (C5 exposure flip).

**Command**:

```
rg -n '^pub mod ' src/lib.rs
```

**Observed stdout**:

```
77:pub mod server_api;
81:pub mod embed;
```

**Discharge**: `already-removed` — census holds exactly `embed` + `server_api`; no raw V10 modules.

### 3.3 V10 raw embed update/remove exports

**Predicted**: retire V10 raw embed re-exports; V11 facade only (C5, T067 map).

**Command**:

```
rg -n 'pub (use|mod|fn).*(update_embedded|remove_embedded|LiveIndex|SharedIndex)' src/embed.rs; wc -l src/embed.rs
```

**Observed stdout**:

```
---
163 src/embed.rs
```

**Discharge**: `already-removed` — 163-line V11 facade + `contract` tripwire only; no V10 raw exports.

### 3.4 Bare `index: SharedIndex` / `project_indexes` root-holder fields

**Predicted**: C4a/C4b census green — no bare roots in frozen holder structs.

**Command**:

```
cargo test --test preventive_runtime_dark_v11 root_holders_store_no_bare_shared_index -- --exact
```

**Observed stdout**:

```
test root_holders_store_no_bare_shared_index ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out
```

**Discharge**: `already-removed` — dark-seal census green at T077 re-run.

### 3.5 `proxy_reset_calibration_receipt` dead `#[cfg(not(feature = "server"))]` twin

**Predicted**: T038 round 2 cfg-lens — embed twin absent.

**Command**:

```
rg -n 'proxy_reset_calibration_receipt' src/ --glob '*.rs'; rg -n '#\[cfg\(not\(feature = "server"\)\)\]' src/protocol/tools.rs | rg -i proxy || echo '(no server-gated proxy twin)'
```

**Observed stdout**:

```
src/protocol/tools.rs:11949:                Some(self.proxy_reset_calibration_receipt())
src/protocol/tools.rs:12175:    pub(crate) fn proxy_reset_calibration_receipt(&self) -> String {
src/protocol/tools.rs:13338:        let receipt = proxy.proxy_reset_calibration_receipt();
src/protocol/tools.rs:13438:            bare.proxy_reset_calibration_receipt(),
---
(no server-gated proxy twin)
```

**Discharge**: `already-removed` — single server-gated implementation; no embed cfg twin.

---

## 4. T076 — embed surface (`src/embed.rs`)

**Gate-first** (contract C-2b, quickstart Step 5):

**Command**:

```
python3 execution/refreeze_v11.py verify-internal --target-ref HEAD
```

**Observed stdout** (2026-09-07, before and after T075):

```
Feature 020 V11 internal refreeze verification passed.
```

**Verdict**: allowlist negative suite **pass** — all 64 `introduced_v11_atoms`
resolve; no remaining V10 embed surface is nameable through the refreeze door.

**Disposition**: **formal discharge (already-removed)**. `src/embed.rs` was **not
edited**. Slice 4 C5 retired the V10 raw re-exports; the file is the V11 facade
only (baseline §5.4). Deletion would remove live contracted atoms — refused.

---

## 5. Four-axis neutrality comparison (T077)

**Before**: T074 baseline at `4edd8fdf26ea`.
**After**: T077 re-run at this commit (same tree — zero `src/` removals).

| Axis | Before | After | Verdict |
|---|---|---|---|
| Runtime authority | `all_ingress_uses_exact_typed_authority_branch` 1/1 | 1/1 | **unchanged** |
| Public behavior | 34 atoms, postactivation | 34 atoms, postactivation | **unchanged** |
| Writer reachability | pass | pass | **unchanged** |
| Activation mode | `preventive_v1_is_the_only_live_mode` 1/1 | 1/1 | **unchanged** |

**NeutralityComparison**:

| Field | Value |
|---|---|
| `before` | `4edd8fdf26ea` (T074) |
| `after` | *(this commit)* |
| `differing_fields` | **[]** (empty — no unexplained field moved) |
| `control_result` | `detected(source_pins)` (§1, pre-removal) |

**Lifecycle / public surface trio** (C-2):

| Gate | Command | Result |
|---|---|---|
| traceability | `node scripts/validate-lifecycle-oracle-traceability.cjs` | OK — 78 requirements, 24 acceptance oracles, 13 retirement categories |
| phase probe | `node scripts/lifecycle-phase-probe.cjs` | `PHASE: postactivation` — `actual: 34  pre: 83  post: 34` |
| refreeze allowlist | `python3 execution/refreeze_v11.py verify-internal --target-ref HEAD` | pass |

**Public atom digest**: unchanged from T074
(`38c84c8541351d7de3b7fe5f5b1bc3f3d7514e79729d3a0aa067d3bdff7d40cc`) — no public
atom added or removed.

**Source pins** (unchanged — no removal intersected pin file sets):

| Pin | SHA-256 digest | Files | Bytes |
|---|---|---:|---:|
| `FULL_SOURCE_PIN_V1` | `33c72c24874c63a51a7a2604445a1ce2598df5ed9058a7b514b2887a6ec084d1` | 197 | 9_514_604 |
| `EXCLUDED_RUNTIME_SOURCE_PIN_V1` | `3dc255e1ff1060b63ad67b18b0ec2c11f28d92f274448f8b1decf07a0b88c202` | 20 | 404_003 |

**Dark seal**: `cargo test --test preventive_runtime_dark_v11 -- --test-threads=1`
— **10 passed; 0 failed**.

**Slice 0 oracle**: `node scripts/slice0-oracle-artifact.cjs` — 12 controls
preserved (7 red, 5 resolved-green).

---

## 6. PHASE 4b — `serialize_index` retention (House item)

**Required**: explicitly record `serialize_index` still present for mint-door feed.

| Field | Value |
|---|---|
| Path | `src/live_index/persist.rs` |
| Line | **278** (`pub fn serialize_index`) |
| Disposition | **RETAIN** — PHASE 4b House item; excluded from Slice 5 deletion set per Martin reject and baseline §5.1 |

---

## 7. Gate battery (T077 re-run)

Every gate from baseline §4 re-run 2026-09-07:

| Gate | Command | Result |
|---|---|---|
| fmt | `cargo fmt --check` | exit 0 |
| clippy | `cargo clippy --all-targets -- -D warnings` | exit 0 |
| lib serial | `cargo test --lib -- --test-threads=1` | 3339 passed, 4 ignored, exit 0 |
| lib+bins+tests serial | `cargo test --lib --bins --tests -- --test-threads=1` | exit 0 |
| embed cell | `cargo test --no-default-features --features embed --lib -- --test-threads=1` | 1362 passed, 4 ignored, exit 0 |
| release build | `cargo build --release` | exit 0 |
| tool correctness | `node scripts/verify-tools.cjs --bin target/release/symforge` | 7 PASS, 1 REVIEW, 0 FAIL |
| lifecycle traceability | `node scripts/validate-lifecycle-oracle-traceability.cjs` | exit 0 |
| lifecycle phase | `node scripts/lifecycle-phase-probe.cjs` | exit 0 — postactivation |
| refreeze allowlist | `python3 execution/refreeze_v11.py verify-internal --target-ref HEAD` | exit 0 |
| slice 0 oracle | `node scripts/slice0-oracle-artifact.cjs` | exit 0 |
| npm | `cd npm && npm test` | 31 passed |
| writer reachability | `cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact` | 1/1 |
| activation mode | `cargo test --test activation_cut_v11 preventive_v1_is_the_only_live_mode -- --exact` | 1/1 |
| dark seal | `cargo test --test preventive_runtime_dark_v11 -- --test-threads=1` | 10/10 |

**Frozen spec tree**: `git diff specs/020-repository-knowledge-index/` — **0 bytes**
(unchanged).

---

## 8. Post-slice adversarial disposition

**Scope**: empty C-7 mechanical removal — no `src/` diff to cfg-lens beyond the
transient C-1 control (discarded).

**Cfg-lens sweep** (mandatory, Constitution VI): reviewed every hard-retain item
and every §5.2 discharge command output. No `#[cfg(...)]` twin, no server/embed
split artifact, and no dark-seal pin regression surfaced that would authorize a
T075 deletion without violating Martin rejects.

**Findings**:

| ID | Severity | Finding | Disposition |
|---|---|---|---|
| — | — | No deletion candidates survived evidence gating | **N/A — empty pass** |
| — | — | No structural reshape attempted | **conforming** |
| — | — | C-1 bracket armed before T075 close | **conforming** |
| — | — | All five §5.2 predictions discharged with command transcripts | **conforming** |

**Verdict**: Slice 5 closes as an **honest empty C-7** mechanical-removal pass.
Track A, Ada-B, and `serialize_index` deletion were not attempted. Independent
adversarial review of a zero-removal diff reduces to bracket + discharge audit;
no RED findings remain open.

---

## 9. References

- `docs/reviews/FEATURE-020-SLICE5-BASELINE-v11.md` (T074)
- `docs/reviews/FEATURE-020-SLICE4-ACTIVATION-EVIDENCE-v11.md`
- `specs/029-mechanical-removal/contracts/neutrality-bracket-v1.md` (C-1, C-6, C-7)
- `specs/020-repository-knowledge-index/tasks.md` Phase 7 (T074–T077)
