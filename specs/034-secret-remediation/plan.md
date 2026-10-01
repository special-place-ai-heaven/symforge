# Implementation Plan: Agent-Driven Secret Remediation

**Branch**: `feat/034-secret-remediation` | **Date**: 2026-10-01 | **Spec**: [spec.md](./spec.md)

**Input**: Feature specification from `specs/034-secret-remediation/spec.md`

**Grounding**: Design settled in [HANDOVER.md](./HANDOVER.md) §3; seams verified in [research.md](./research.md). This plan is for the **implementation** follow-on. The current PR is **docs/spec only** and MUST NOT land behavior changes.

## Summary

When SymForge withholds secrets, agents receive structured `_meta["symforge/withheld"]` findings (shape, never bytes) and can call one write tool — `secret_remediate` — to preview and apply `externalize`, `encrypt` (SOPS/age public recipient), or `dismiss` (content-digest-bound). Apply is harness-approved, atomic, idempotent, and reports only observed outcomes including a fresh re-scan and git-history honesty.

Deliverable 1 (quoted credential keys) is already on `main` (#738). This feature does not reopen detector matching except for post-apply re-scan.

## Technical Context

**Language/Version**: Rust 2021, toolchain as pinned by CI `rust` job

**Primary Dependencies**: Existing rmcp / serde / edit-lane stack; external binary `sops` (optional at runtime for encrypt). **No new crypto crates** for encrypt (delegate to SOPS). Possible small helper modules under `src/protocol/` and/or `src/knowledge/`.

**Storage**: Committed dismissal file (schema in implement); gitignored `.env` for externalize; no private keys stored by SymForge.

**Testing**: Repo gates from HANDOVER §4 / Constitution IV — `cargo fmt --check`; `clippy -D warnings`; serial `cargo test`; embed gate; `preventive_runtime_dark_v11` if `src/` changes; `python execution/conventional_commits.py check-range origin/main..HEAD`. Synthetic secrets only. RED-before-GREEN for each security oracle.

**Target Platform**: Windows / Linux / macOS server builds. Protocol code is server-gated; embed cell must keep compiling (cfg discipline).

**Project Type**: Single Rust crate; adds one MCP tool + `_meta` key; updates `verify-tools.cjs` / tool-list pins.

**Performance Goals**: Withheld meta attachment is O(findings already known to admission); remediation is explicit operator/agent action, not on the hot read path beyond meta serialization.

**Constraints**: Never return secret bytes or private keys; `refuse_by_policy` stays syscall-free; all paths through `resolve_repo_path`; spawns only via `hidden_command`; no AGENTS.md edits; `CLAUDE_PROJECT_DIR=` for cargo.

**Scale/Scope**: Spec track = documentation under `specs/034-secret-remediation/` only. Implementation track = new tool module, meta attachment at refusal seams, contracts tests, tool registry pins — sized in tasks.md.

## Constitution Check

*GATE: evaluated against SymForge Constitution v1.0.0 for this design.*

| Principle | Compliance | Status |
|---|---|---|
| **I. Reporting Invariant** | Apply success requires observed writes + observed re-scan; unavailable encrypt/dismiss reported as unavailable; git history note distinguishes commit vs working tree | PASS |
| **II. RED-First Evidence** | quickstart names oracles that must fail before machinery; positives paired with negatives | PASS (obligation) |
| **III. Frozen Contracts Win** | No frozen Feature 020 tree edits; new contract ids introduced under `contracts/` | PASS |
| **IV. Verification Gates** | Full gate list in quickstart; serial cargo; embed regression | PASS (obligation) |
| **V. Unrepresentable Over Checked** | Finding ids minted by SymForge; apply preview token / idempotency reuse; shape type cannot carry raw bytes in API structs | PASS (design intent) |
| **VI. Independent Review Before Merge** | Spec PR and later implementation PR each need LINUS (+HOLMES if contested); Grove/others merge — implementers do not self-merge unless HANDOVER §1 fully satisfied | PASS (process) |

**Post-Phase-1 re-check**: No constitution violations requiring Complexity Tracking rows.

## Project Structure

### Documentation (this feature)

```text
specs/034-secret-remediation/
├── HANDOVER.md              # Owner brief of record (D1 + D2)
├── spec.md                  # Feature specification (clarify-complete)
├── plan.md                  # This file
├── research.md              # Phase 0 decisions + anchors
├── data-model.md            # Phase 1 entities
├── quickstart.md            # Gates + validation scenarios
├── contracts/
│   ├── withheld-meta.md     # _meta["symforge/withheld"] shape
│   └── secret-remediate.md  # Tool request/response contract
├── checklists/requirements.md
├── tasks.md                 # Phase 2 implementation tasks
└── analyze.md               # Cross-artifact analysis (spec track exit)
```

### Source Code (implementation follow-on — not in spec PR)

```text
src/protocol/
├── result_status.rs      # WITHHELD_META_KEY constant + attach helper
├── format.rs             # Comment/doc updates; refusal text stays
├── read_gate.rs          # Attach meta from known findings; keep refuse_by_policy syscall-free
├── secret_remediate.rs   # NEW — tool handler, preview/apply, actions
├── edit_tools.rs         # Reuse atomic apply / idempotency patterns
└── tools.rs              # Register tool; list/pin updates

src/knowledge/            # Re-scan helpers; optional dismissal evaluation hook
src/discovery/            # resolve_repo_path only (no API change expected)
src/process_util.rs       # hidden_command for sops

scripts/verify-tools.cjs  # Advertise new tool
tests/                    # Acceptance oracles per quickstart
```

**Structure Decision**: Extend existing server protocol + knowledge admission; no new crate.

## Complexity Tracking

> None — no constitution violations to justify.
