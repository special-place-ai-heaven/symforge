# Data Model: Agent-Driven Secret Remediation

Phase 1 design entities. Wire shapes are normative in `contracts/`; this file defines conceptual fields and invariants.

## WithheldFinding

In-memory / wire description of one withheld secret instance.

| Field | Notes |
|---|---|
| `id` | Stable finding id for remediation targeting (opaque string; minted by SymForge per scan identity) |
| `rule_id` | Detector / path rule id already used in refusal text |
| `line_start`, `line_end` | 1-based inclusive line range (same basis as `recorded_refusal_naming_lines`) |
| `shape` | Value shape descriptor — NEVER raw bytes (e.g. `40-char base62 after api_key =`) |
| `confidence` | Discrete level (`high` / `medium` / `low`) or detector-native mapping documented at implement |
| `actions` | List of `{ name, available, reason? }` for `externalize` / `encrypt` / `dismiss` |

Invariant: serializing a `WithheldFinding` MUST NOT embed secret bytes.

## WithheldMeta

`_meta["symforge/withheld"]` payload.

| Field | Notes |
|---|---|
| `contract_version` | Integer; bump on shape or claim changes |
| `path` | Repo-relative path already named in refusal |
| `findings` | Array of `WithheldFinding` |
| `policy_version` | `SECRET_POLICY_VERSION` observed at scan |

Absence of the key means no withheld-content claim on that result (successful reads omit it).

## RemediationRequest

Inputs to `secret_remediate`.

| Field | Notes |
|---|---|
| `scope` | One path, list of paths, or whole-repo sentinel — each path through `resolve_repo_path` |
| `finding_ids` | Subset to act on; **required non-empty** — empty array is rejected (contract `secret-remediate.md`; no implicit "all in scope") |
| `action` | `externalize` \| `encrypt` \| `dismiss` |
| `preview` | Bool; **default true** |
| `idempotency_key` | Optional; reuse edit-lane semantics on apply |

## RemediationPreview

| Field | Notes |
|---|---|
| `masked_diff` | Unified diff with secrets replaced by `«secret:N»` |
| `files_touched` | Paths that would be created/modified |
| `actions_resolved` | Per-finding availability after scope resolution |

No worktree mutation.

## RemediationApplyResult

| Field | Notes |
|---|---|
| `written` | Paths actually written (observed) |
| `rescan` | Per-path fresh scan summary (observed) |
| `history_note` | `since commit <sha>` or honest working-tree/uncommitted note |
| `rollback` | Present only if a failure triggered rollback — success MUST NOT claim writes that rolled back |

## DismissalRecord

Committed false-positive marker.

| Field | Notes |
|---|---|
| `path` | Repo-relative path |
| `line_digest` | Content digest of the dismissed line bytes |
| `rule_id` | Rule that fired |
| `created_at` / `note` | Operator-facing metadata (no secret bytes) |

Invariants (bind the five rejected allowlist modes):

1. Bind includes **content digest**, not path+rule alone.
2. Refusal text does **not** teach a self-service bypass.
3. Revocation is observable (delete/amend record → finding can return).
4. Loader: no symlink escape; explicit size cap.
5. Health / status MUST NOT echo unscanned dismissal file text.

## ShapeDescriptor

Opaque string safe for agents. Produced by classifier over matched value form (length, charset class, key context). MUST be identical for equal forms and MUST NOT include substrings of the secret value.
