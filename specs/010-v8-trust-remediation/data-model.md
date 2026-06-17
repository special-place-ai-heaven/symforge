# Phase 1 Data Model: v8 Trust Remediation

The "data" of this feature is the set of **LLM-facing surfaces** and the honest **proof
state** each must carry. These are not new persistent entities — they are the state
machines that govern how existing fields are labeled and reported. Validation rules trace
to the spec's Functional Requirements.

---

## E1 — LLM-facing surface (string)

Any string an agent reads on a call: status banner field, economics-envelope field,
error/recovery text, tool description, public doc line.

| Field | Type | Rule |
|-------|------|------|
| `text` | string | The literal surfaced to the agent. |
| `proof_state` | enum `Measured \| Heuristic \| Observational \| Deferred` | Every figure/claim carries one (FR-001, FR-003). |
| `label_matches_code` | invariant | A field named `saved`/`net`/`active`/`validated` MUST satisfy the word in code, else it is renamed or qualified (FR-001/002/003). |

**Invariant (honesty contract)**: `proof_state == Measured` ⟹ the value is derived from a
real measurement at call time; otherwise the surface is explicitly qualified.

---

## E2 — Economics figure

| Field | Type | Rule |
|-------|------|------|
| `value` | integer (estimated tokens) | Derived from real bytes once grounded (FR-014); chars/4 is labeled "estimated tokens (chars/4)" (N-4). |
| `source` | enum `Measured \| Heuristic` | A `400/800` constant is `Heuristic`; a byte-grounded estimate is `Measured`-from-size (still an estimate, labeled as such). |
| `name` | string | Names what it is. A monotonic running total is `session_tokens_served`, never `session_net_vs_manual` (FR-002, TR-05). |
| `sign_semantics` | rule | A figure that can only grow MUST NOT be printed as `+net` implying savings (TR-05, TR-11). |

**State transition (US5)**: `Heuristic(constant 400/800)` → *[Phase E wiring]* →
`Measured-from-size(format.rs estimator)`. Until transitioned, stays `Heuristic` and the
adaptive behavior is described as not-yet-active (FR-014).

---

## E3 — Index health readout

| Field | Type | Rule |
|-------|------|------|
| `index_state` | enum `Ready \| Empty \| Loading \| Unavailable` | Reflects the index that **serves queries** (FR-006). |
| `symbol_count` / `file_count` | integer | Non-zero and matching the served index after a successful query (SC-002). |
| `source` | enum `Daemon \| FrontEnd` | MUST be `Daemon` in the proxy topology (TR-01); a `FrontEnd` empty read while serving is the bug. |
| `empty_index_reason` | enum `NoRoot \| NotIndexedYet \| Disabled \| n/a` | Distinguishes why empty, drives recovery (US4). |

**Invariant (US2)**: a successful query ⟹ `index_state == Ready` ∧ counts > 0 on the
next status read. No "Empty/not-ready while serving."

---

## E4 — Subsystem state (status banner: ledger / handlers / layers)

Replaces the unconditional `active`/`pending` literals (TR-10, N-1, N-3).

| State | Meaning |
|-------|---------|
| `InMemory` | Layer is the always-on in-memory implementation (true today for l4 cache). |
| `Durable` | Durable store is open and writable (serve mode, ledger). |
| `Disabled(reason)` | Configured off, or open failed — **with the reason** (N-3, TR-17). |
| `Unavailable` | Not wired in this build/surface. |

**Invariant (FR-008)**: `Disabled(open_failed)` and `Unavailable` are **distinct** — a
wired-but-failing store never reports identically to a never-configured one. `summary()`
must not swallow the DB error into `None` (N-3).

---

## E5 — Guard condition (`if_match`)

| Field | Type | Rule |
|-------|------|------|
| `expected_body` | string \| hash | The pre-edit body the apply is conditioned on. |
| `enforced_at` | enum `Write` (only valid value) | The guard is verified against the bytes actually written, in the splice's critical section (FR-009, D1). |
| `on_divergence` | action `Reject` | No write; divergent on-disk content left intact (US3 AC-1). |
| `claim_rule` | invariant | The response claims a guarded apply only when `enforced_at == Write` succeeded (FR-010, US3 AC-3). |

**State machine**: `Requested` → re-read on-disk under lock → `Match` → splice +
`atomic_write` → `AppliedGuarded`; or `Diverged` → `RejectedNoWrite`. No path reports
`AppliedGuarded` without passing through write-time `Match`.

---

## E6 — Assumption record entry (`docs/stel-assumptions.md`)

| Field | Type | Rule |
|-------|------|------|
| `id` | `A-0NN` | Stable identifier. |
| `proof_state` | enum `OPEN \| PARTIAL \| VALIDATED` | Exactly one, single source of truth per id (TR-16; A-005, A-016 deduped). |
| `artifact_ref` | path/link \| none | `VALIDATED` REQUIRES a backing artifact; a verdict with no artifact is demoted (FR-004, TR-12 A-009→PARTIAL, TR-13 A-028 demoted). |
| `claimed_by` | list of surfaces | Surfaces that assert this capability; the CI gate cross-checks (FR-018). |

**Invariant (relabel ≠ validate)**: a label change never moves `proof_state` to
`VALIDATED`. A-011/A-015/A-016/A-028 stay OPEN/PARTIAL until reproduced with an artifact.

---

## E7 — Capability matrix entry (`docs/v8-capability-matrix.md`, NEW)

| Field | Type | Rule |
|-------|------|------|
| `feature` | string | A user-visible capability. |
| `proof_state` | enum `Implemented \| Heuristic \| Observational \| Deferred` | Maps the feature to what the code delivers. |
| `assumption_id` | `A-0NN` | The backing assumption; one source of truth (FR-017). |
| `surface_claim` | string \| none | What the shipped surface says — must not exceed `proof_state` (FR-018, the CI gate's input). |

**Invariant (FR-018)**: `surface_claim` asserts a validated capability ∧
`assumption.proof_state == OPEN` ⟹ **CI fails**.

---

## Relationships

```text
LLM-facing surface (E1) ──carries──▶ proof_state
   economics field (E2) ──is-a──▶ E1
   index readout  (E3) ──is-a──▶ E1, ──reads──▶ Daemon index (Principle I)
   subsystem state(E4) ──is-a──▶ E1, ──derived-from──▶ ledger_store.summary()
   recovery text       ──is-a──▶ E1, ──computed-from──▶ active surface profile
Guard condition (E5) ──enforced-at──▶ Write critical section
Assumption (E6) ──backs──▶ Capability matrix entry (E7) ──claimed-by──▶ surfaces (E1)
                          └──enforced-by──▶ honesty CI gate (FR-018)
```

Every entity reduces to one rule: **the surface never asserts more than the code
delivers** (SC-008, keystone).
