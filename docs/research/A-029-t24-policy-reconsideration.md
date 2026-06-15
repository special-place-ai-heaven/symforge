# A-029 — T2.4 P-T2 Policy Reconsideration (Proposal)

**Program:** 8.1 index-recall · T2.4 policy reconsideration (proposal only)
**Baseline:** `main` @ `5bbde13` (post-#319 TX-04 merge)
**Authoritative replay:** [`A-029-t2-replay.json`](./A-029-t2-replay.json)
**Status:** **PROPOSAL** — no golden or `eligible_h6` changes in this packet

## Executive summary

Post-TX-04 replay achieves **2/4** T2 equivalence on tokio+django external reference tasks. The machine **A-029** threshold (≥2/4) is **met**. P-T2 reconsideration is therefore **warranted**, but restoration is **row-level only** — not a blanket lift of all four T2 rows.

This document records the policy reconsideration. It does **not** apply restoration. Independent reviewer sign-off is required before any golden-row or `eligible_h6` edits (see [`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md)).

## Threshold assessment

| Criterion | Required | Observed | Met? |
|-----------|----------|----------|------|
| T2 equivalence count | ≥2 / 4 | **2 / 4** | **Yes** |
| Judge method | A-004 stratified rules | `scripts/a029-t2-spike.cjs` + `src/stel/a029.rs` | Yes |
| Pinned corpora | tokio + django SHAs stable | See replay JSON `pinned_shas` | Yes |
| Golden / `eligible_h6` | Unchanged until sign-off | Unchanged on `main` | Yes |

**Program verdict:** `VALIDATED` (per [`index-recall-evidence-contract.md`](../../specs/003-81-index-recall/contracts/index-recall-evidence-contract.md)).

**Phase 2 baseline (unchanged artifact):** 0/4 PIVOT — [`a029-t2-results.json`](./a029-t2-results.json).

## Remediation trajectory

| Tranche | Commit / PR | Equiv after | Notes |
|---------|-------------|-------------|-------|
| Phase 2 baseline | `061583c` | 0/4 | P-T2 registered |
| TX-01 (FM-CAP) | `830ad8f` | 1/4 | Tokio cap-bound lift |
| TX-02 (xref) | #317 / `c955fc1` | 1/4 | Django structured refs |
| TX-04 (tests) | #319 / `ed0fbc6` | **2/4** | Test-path ordering + xref |

Evidence per tranche: [`A-029-tx01-cap-evidence.md`](./A-029-tx01-cap-evidence.md), [`A-029-tx02-xref-evidence.md`](./A-029-tx02-xref-evidence.md), [`A-029-tx04-tests-evidence.md`](./A-029-tx04-tests-evidence.md).

## Row-level restoration posture (proposed)

Restoration is **not blanket**. Only rows that independently meet equivalence on replay are proposed for `eligible_h6=true` restoration.

### Proposed for restoration (equivalent on replay)

| Task ID | Recall | Equiv | Proposed `expected_decision` | Proposed `eligible_h6` |
|---------|--------|-------|------------------------------|------------------------|
| `tokio/t2_block_on` | 70.9% | **EQUIVALENT** | `serve` | `true` |
| `django/t2_model` | 28.2% | **EQUIVALENT** | `serve` | `true` |

### Remain bypass-only under P-T2 (non-equivalent on replay)

| Task ID | Recall | Equiv | Proposed `expected_decision` | Proposed `eligible_h6` |
|---------|--------|-------|------------------------------|------------------------|
| `tokio/t2_spawn` | 34.5% (need 35%) | SYMFORGE-LESS | `bypass` | `false` |
| `django/t2_queryset` | 26.8% (need 35%) | SYMFORGE-LESS | `bypass` | `false` |

**Rationale for non-restored rows:** Both miss their per-task `min_baseline_recall` threshold by A-004 judge rules. Partial program success does **not** authorize partial equiv rows to inherit serve posture by association.

## Golden corpus context

External A-029 T2 tasks are defined in [`tests/fixtures/a029-t2/tasks.jsonl`](../../tests/fixtures/a029-t2/tasks.jsonl). They are **not yet** present in [`docs/fixtures/routes.golden.jsonl`](../../docs/fixtures/routes.golden.jsonl) (in-repo sf-bench golden has 36 rows; P-FF bypass only).

P-T2 was registered at Phase 2 exit ([`A-029-t2-spike.md`](./A-029-t2-spike.md)) but golden enforcement was deferred to T2.4. A follow-up **restoration commit** (after sign-off) would add or update four T2 rows with the row-level posture above.

In-repo golden `*/t4_refs` rows are **out of scope** for this reconsideration (they pass on small corpora per [`A-029-t2-task-crosswalk.md`](./A-029-t2-task-crosswalk.md)).

## Explicit non-claims

This reconsideration packet makes **no** claim of:

- **H6**, **H7**, or **H8** gate pass
- A-029 **PASS** in the sense of full T2 reference parity (2/4 ≠ 4/4)
- Automatic P-T2 revocation for non-equivalent rows
- TX-03 bench remediation (deferred)
- Runtime, MCP surface, persistence, or B-RESULTS changes

## Next step

1. Independent reviewer completes checklist in [`A-029-t24-restoration-signoff.md`](./A-029-t24-restoration-signoff.md).
2. On **GO**, separate restoration commit edits golden rows / assumption register per sign-off packet.
3. On **NO-GO**, P-T2 remains fully in force; program exit records VALIDATED measurement with zero row restoration.
