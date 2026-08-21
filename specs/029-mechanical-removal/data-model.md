# Phase 1 Data Model — Feature 020 Slice 5 (Mechanical Removal)

This slice adds no runtime types. Its entities are the artifacts that make a
removal auditable. They are modelled here so the tasks phase has exact field
names to produce and the evidence document has exact fields to compare.

The design rule throughout is Constitution Principle V: prefer states that
cannot be spelled wrongly over states that are checked after the fact. Applied
to documents rather than Rust, that means **every field carries the command
that produced it**, so a field cannot be present without provenance.

---

## NeutralityBaseline

The recorded pre-removal state. Captured once as T074, re-captured unchanged as
T077, and compared field by field.

| Field | Type | Meaning |
|---|---|---|
| `captured_at_ref` | git ref | The tree the capture describes. Recorded, never hand-typed into prose elsewhere. |
| `lifecycle_phase` | `preactivation` \| `postactivation` | Which frozen set the derived public atom set matched (research R1). |
| `public_atom_count` | integer | Size of the derived public atom set. |
| `public_atom_digest` | digest | Order-independent digest of that set, so a swap of two atoms is not mistaken for no change. |
| `activation_result` | text | The observed terminal activation mode and the lane-registration outcome. |
| `writer_reachability_verdict` | pass \| fail + case name | The executed ingress-authority case and its result. |
| `gate_outcomes` | list of (gate, result, duration) | Every gate from research R6. |
| `source_pins` | list of (pin name, digest, file count, byte count) | The two whole-source pins (research R2). |
| `commands` | map field → command | The exact command that produced each field above. **Required for every field**; a field without one is not a captured field, it is a claim. |

**Validation rules**

- Every field in the record has an entry in `commands`. A record failing this
  is invalid and cannot serve as a baseline.
- `public_atom_digest` is order-independent; `public_atom_count` alone is
  insufficient because a same-size substitution would be invisible.
- `lifecycle_phase` is recorded as observed, never as expected.

---

## NeutralityComparison

The diff of two `NeutralityBaseline` records, plus the evidence that the diff
can detect a real change.

| Field | Type | Meaning |
|---|---|---|
| `before` / `after` | baseline refs | The two records compared. |
| `differing_fields` | list of field names | Empty for a neutral removal. Named, not counted, when non-empty. |
| `control_result` | detected(field) \| **not-detected** | Outcome of the deliberate control edit (research R5). |
| `control_description` | text | What the control changed, so a reader can judge whether it was a real change. |

**Validation rules**

- A comparison whose `control_result` is `not-detected` is **void**. It may not
  be cited as evidence of anything, and no removal may proceed on it.
- `differing_fields` names fields; a comparison reporting only a count is
  incomplete.
- The control edit is discarded before any removal lands; it never appears in
  a shipped diff.

**State transitions**

```
captured(before) → control-run → { void | armed }
armed → removal applied → captured(after) → compared → { neutral | differences named }
```

`armed` is the only state from which a removal may proceed. There is no edge
from `void` to `armed` except by fixing the comparison and re-running the
control — a bracket that failed its control is not repaired by re-reading it.

---

## RemovalCandidate

One named item considered for deletion.

| Field | Type | Meaning |
|---|---|---|
| `item` | qualified path | Exact identifier, quoted as the contract spells it (Principle III). |
| `visibility` | public \| non-public | Public candidates are refused outright (research R1). |
| `unreachability_evidence` | citation | The executed Slice 4 reachability case or inventory disposition. Task wording and "looks unused" are not admissible. |
| `is_frozen_v11_seam` | bool | `true` refuses the candidate (research R3, spec FR-005). |
| `dependent_tests` | list | Tests whose only subject is this item; they go in the same change. |
| `disposition` | removed \| retained | Outcome. |
| `retained_reason` | text | Required when `retained`. |

**Validation rules**

- `visibility = public` ⇒ `disposition = retained`. Not a warning — the
  postactivation set requires every surviving public atom to exist.
- `is_frozen_v11_seam = true` ⇒ `disposition = retained`.
- `unreachability_evidence` empty ⇒ `disposition = retained`. The absence of
  evidence is itself the reason, and it is recorded rather than resolved by
  looking harder for a justification.
- A test may be listed in `dependent_tests` only if the candidate is its sole
  subject.

---

## DischargedExpectation

A removal the frozen roster predicts, which the tree turns out not to need.

| Field | Type | Meaning |
|---|---|---|
| `predicted` | text | The roster's own words for what should be removed. |
| `observed` | text | What the tree actually contains. |
| `evidence` | citation | The command and output establishing `observed`. |
| `discharge` | already-removed \| never-existed | Which of the two it was. |

**Validation rules**

- A `DischargedExpectation` is not satisfied by removing something adjacent.
  Spec FR-011 exists because that substitution is the tempting move when a
  slice looks empty.
- `evidence` must be an observation, not an inference from file size or name.

---

## Relationships

```
NeutralityBaseline ──(before)──┐
                               ├──> NeutralityComparison ──gates──> RemovalCandidate*
NeutralityBaseline ──(after)───┘

RemovalCandidate.disposition = retained ──when roster-predicted──> DischargedExpectation
```

Read as one sentence: **a comparison that has survived its own control is the
only thing that authorizes a candidate to become a removal, and any predicted
removal that does not happen must leave a discharge record behind.**
