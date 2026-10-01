# Research: Agent-Driven Secret Remediation (034)

**Date**: 2026-10-01  
**Branch**: `feat/034-secret-remediation`  
**Authority**: `HANDOVER.md` §3 (settled design) + live seam survey on post-D1 `main` tip `d8ee3d0c` lineage.

## Decisions

### R1 — Actionable refusal via `_meta["symforge/withheld"]`

**Decision**: Keep today's human refusal renderers; add a structured `_meta` carrier for findings.  
**Rationale**: Existing clients already surface refusal text; `_meta` is how Feature 032 delivered machine-readable notices without corrupting primary content.  
**Alternatives rejected**: Replacing refusal text with JSON-only (harnesses often show models text only); embedding remediation URLs/tokens in the text (self-service bypass class, previously rejected).

### R2 — One remediation tool, preview default true

**Decision**: Single tool `secret_remediate` with `preview` defaulting to true; apply is the write path.  
**Rationale**: HANDOVER §3; mirrors edit-lane dry-run/apply discipline and keeps harness permission prompts meaningful.  
**Alternatives rejected**: Separate preview/apply tools (doubles registry surface); auto-apply on read (no human gate).

### R3 — Externalize idioms + root `.env`

**Decision**: v1 idioms JS/TS, Python, Rust, `${X}` config; target repo-root `.env` + `.gitignore` ensure.  
**Rationale**: Covers SymForge's primary agent languages; root `.env` is the least-surprising default.  
**Alternatives rejected**: Language-plugin architecture in v1 (scope); writing secrets into committed dotenv variants (defeats the point).

### R4 — Encrypt = SOPS/age public recipient only

**Decision**: Delegate to `sops`; public key from `.sops.yaml` or `SOPS_AGE_RECIPIENTS`; spawn via `hidden_command`.  
**Rationale**: No private key in SymForge; no custom crypto; matches HANDOVER never-list.  
**Alternatives rejected**: Embed age crates directly (custom path + key handling risk); agent-supplied keys as tool args (forbidden).

### R5 — Dismiss bound to line content digest

**Decision**: Committed dismissal file; bind path + content digest of the line; size cap; no symlink follow; health must not echo unscanned text.  
**Rationale**: Directly counters the five rejected allowlist failure modes in HANDOVER §3.  
**Alternatives rejected**: Path+rule allowlist; agent-writable uncommitted dismiss; health echoing file text.

### R6 — `refuse_by_policy` stays syscall-free

**Decision**: Metadata for withheld findings is attached on lanes that already read admission state / live index findings — not by adding disk I/O inside `refuse_by_policy`.  
**Rationale**: Existence-oracle contract (HANDOVER §3 past lessons).  
**Verified anchor**: `src/protocol/read_gate.rs` — `refuse_by_policy` at line ~249; `recorded_refusal_naming_lines` already derives line ranges from live index state.

### R7 — Reuse edit-lane apply + idempotency

**Decision**: Remediation apply follows `src/protocol/edit_tools.rs` atomic write / dry-run / idempotency patterns rather than inventing a second mutation stack.  
**Rationale**: Proven rollback and replay semantics; keeps reporting honest (Constitution I).  
**Alternatives rejected**: Ad-hoc `fs::write` without rollback; best-effort multi-file writes without transaction narrative.

## Verified anchors (re-check before implement)

| Concern | Location (as of 2026-10-01 survey) |
|---|---|
| Admission refusal text | `format::content_withheld_by_admission` / `content_withheld_by_path_rule` in `src/protocol/format.rs` (~3855+) |
| Line ranges for naming | `recorded_refusal_naming_lines` in `src/protocol/read_gate.rs` (~330) |
| Syscall-free policy refuse | `refuse_by_policy` in `src/protocol/read_gate.rs` (~249) |
| Path resolution | `discovery::resolve_repo_path` in `src/discovery/mod.rs` (~540) |
| Edit apply / idempotency | `src/protocol/edit_tools.rs` |
| `_meta` key peers | `RESULT_STATUS_META_KEY`, `PROJECT_EVIDENCE_META_KEY`, `REPEAT_NOTICE_META_KEY` in `src/protocol/result_status.rs` |
| Hidden spawn guard | `process_util::hidden_command` + `test_no_raw_command_spawns_outside_hidden_command` |
| Secret policy version | `SECRET_POLICY_VERSION` in `src/knowledge/mod.rs` (7 after D1) |

Note: `content_withheld_by_admission` docs currently say the surface "offers no remedy". Implementation MUST update that comment when remediation ships — the remedy is a separate write tool, not a self-service override inside the refusal.

## Clarifications closed

| Topic | Resolution |
|---|---|
| Detector quoted-key work | Done on main (#738); out of scope here |
| Decrypt for agents | Never |
| Approval mechanism | Harness write permission on apply |
| Tool naming | `secret_remediate` working name; pins updated in implement |
| Path-rule / unscanned | Honest unavailable remediation, no invented content findings |

## Open items for implement (not blockers for spec PR)

- Exact dismissal file path/schema (propose in implement task; contract stub pins digest bind + constraints).
- Whether path-rule refusals ever gain a remediation action in v1 beyond "unavailable".
- Final env var naming for externalize when multiple findings share a key stem.
