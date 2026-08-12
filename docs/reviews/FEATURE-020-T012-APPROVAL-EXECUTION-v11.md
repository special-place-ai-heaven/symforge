# Feature 020 — T012 external approval execution record (V11)

Status: **T012 complete.** The HARD STOP in
`specs/020-repository-knowledge-index/tasks.md` is cleared; Slice 0 (T013) may begin.

Per that same block, no checkbox in `tasks.md` was touched and no frozen file was
edited. This document is the execution status and evidence it directs here.

Executed 2026-08-12 against `main` at `26846ab27ae8396e66f737aa7760daf6845cda21`.

## What was approved

| Field | Value |
|---|---|
| `target_commit` | `bbbe2a94d27ce884c942bfcc0e6248dc1b849d54` |
| `target_tree` | `fb64e2a1823135734c97766537e61f98019b9fbe` |
| attestation | `docs/reviews/FEATURE-020-REFREEZE-ATTESTATION-v11.md` @ `e7f0064997f5567667e8582600e881989150e6ab0f17e7bb70de29b53e10a0c2` |
| `sequence` / `predecessor_digest` | `1` / `null` — chain origin, so no approval history is required |
| `signature_namespace` | `symforge-feature-020-refreeze-v11` |
| `release_identity` | `symforge-release@special-place-ai-heaven` |

The record and its detached SSHSIG signature are held outside the repository by the
operator. Neither was copied into this tree. Their digests, which are not secret and
do not reconstruct either file:

| Artifact | SHA-256 |
|---|---|
| `approval.json` | `5d6ab7cc0e604e4e31d30cf9a10294f077f69e25c37cfd5990da41f439972f8c` |
| `approval.json.sig` | `5079dc55359ce370ff3b5b6fa9a492a376d807648d470366e8cf6fd96ffe2769` |
| `allowed_signers` | `ad06cfce0c3e42a0893461e4d0f6dd3fe37a7e4675b2529648d659af1465623f` |

## Gates rerun at HEAD

```
python execution/refreeze_v11.py verify-internal --target-ref HEAD
  -> Feature 020 V11 internal refreeze verification passed.
node scripts/validate-lifecycle-oracle-traceability.cjs
  -> OK (78 requirements, 24 acceptance oracles, 13 retirement categories)
```

The approved commit is still an ancestor of HEAD and
`git diff bbbe2a94 HEAD -- <FROZEN_PATHS>` is empty, so the approval binds the
current tree without re-freezing.

## Proof that the record accepts only its target

`verify-approval` was run with the real signing material. One positive case and four
independent negative cases, each rejecting at a different check:

| Case | Result |
|---|---|
| unmodified record, target `bbbe2a94` | **passed** |
| same record, target `26846ab2` (HEAD) | `APPROVAL_COMMIT_MISMATCH` |
| record with `target_commit` rewritten to another commit | `APPROVAL_COMMIT_MISMATCH` |
| one hex character flipped in the attestation digest | `APPROVAL_ATTESTATION_HASH_MISMATCH` |
| `approved_at` moved by one second, nothing else | `APPROVAL_SIGNATURE_INVALID` |
| unknown key in `allowed_signers` | `APPROVAL_SIGNATURE_INVALID` |

The one-second case is the load-bearing one: it alters a field nothing else
cross-checks, so only the signature can catch it, and it does. A coordinated in-tree
rewrite that keeps this record produces a different commit, which lands on the first
row's failure.

## CI wiring

The three approval inputs and the release identity are scoped to the
`feature-020-v11-release-approval` GitHub Environment — not to the repository — so
only `feature-020-v11-gate`, the one job that declares that environment, can read
them. The environment carries a custom deployment branch policy admitting `main`
only, which stops a `workflow_dispatch` from another branch from reaching them.

`FEATURE_020_V11_APPROVAL_HISTORY_ZIP_B64` is deliberately unset: the workflow
decodes it with `required=False` and `_verify_approval_history` returns early at
`sequence == 1`.

## Still outstanding, unrelated to this gate

`main` is `protected: false` with zero rulesets, so `.github/CODEOWNERS` requests
review but blocks nothing. Until that changes, the release-evidence phase flag can be
flipped by a direct push.
