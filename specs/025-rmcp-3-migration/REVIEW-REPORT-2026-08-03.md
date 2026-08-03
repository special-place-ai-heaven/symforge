# Review Report: specs/025-rmcp-3-migration

**Date**: 2026-08-03  
**Reviewer**: Cursor agent (Grok)  
**Scope**: `spec.md`, `research.md`, `VERIFICATION-REPORT-owner-agent-2026-08-03.md`  
**Tree checked against**: branch `feat/knowledge-llm-sift`  
**API ground truth**: docs.rs/rmcp/3.1.0  

**Companion canvas**: `C:\Users\rakovnik\.cursor\projects\e-project-symforge\canvases\rmcp3-spec-review.canvas.tsx`

---

## Verdict

**Almost ready for `/speckit-plan`.** Unusually strong migration pack — blast radius, frozen invariants, silent-failure focus, and two-PR delivery are right. Spot-checks against the live tree and docs.rs mostly hold.

Do **not** start Phase 0 until the wire-contract contradiction (H1) and the adopted owner-test 15 conflict (H2) are resolved in the docs.

| Severity | Count |
|----------|------:|
| Blocker  | 1 |
| High     | 2 |
| Medium   | 4 |
| Low      | 2 |

---

## Findings

### H1 — BLOCKER: FR-305 / SC-302 vs FR-A6 disagree on `resultType`

**Area**: Wire contract  

Base FR-305 pins `result_type: Some(ResultType::Complete)` and a always-present wire key `"resultType": "complete"`. SC-302 says golden/conformance batteries get that key. Addendum FR-A6 (explicitly wins on conflict) requires version-aware fixtures: modern peers emit the key; legacy peers get it **stripped by the SDK**.

SC-302 and the FR-305 Assumptions paragraph were not rewritten to match FR-A6.

**Action**: Rewrite SC-302 + FR-305 Assumptions under FR-A6. Distinguish:

1. Rust struct construction (`Some(Complete)` on manual list/read results)
2. Direct `serde_json` of `CallToolResult` (current golden/conformance path)
3. Transport wire after negotiated-version SDK serialization (owner tests 1–2)

---

### H2 — HIGH: Owner acceptance test 15 conflicts with FR-309 / SC-314

**Area**: Adopted tests  

`research.md` reconciliation adopts owner-report acceptance test 15 (“missing or mismatched protocol metadata/header is rejected”). FR-309 / SC-314 may set `with_stateless_protocol_metadata_required(false)` so header-less legacy requests keep today’s HTTP-200 JSON-RPC error semantics (never a transport-level rejection).

**Action**: Narrow adopted test 15 to the modern / metadata-required path. Keep FR-309 legacy HTTP-200 semantics explicit and authoritative for this repo.

---

### H3 — HIGH: research.md Phase 3 still describes rejected lazy-bind

**Area**: research.md drift  

`research.md` §Migration order Phase 3 still says:

> Modern-lifecycle workspace-binding trigger per decision 3; `_meta` capability fallback in `bind_workspace_from_client_roots`

Owner decision 2 / FR-313–314 chose the shipped fallback chain only (`SYMFORGE_WORKSPACE_ROOT` > `index_folder` > CWD walk) plus FR-319 evidence disclosure. Spec addendum is correct; research Phase 3 will mislead implementers.

**Action**: Patch research Phase 3 (and any remaining decision-3a language) to match owner decision 2.

---

### M1 — MEDIUM: FR-316 misses stale `on_initialized` transport claim

**Area**: Docs FR  

`src/protocol/mod.rs:1573–1575` still claims `on_initialized` is shared by both transports. `src/server/mcp_http.rs:16–18` proves `serve_directly` skips handshake enforcement, so the hook never fires over `/mcp`. US2-AS2 correctly scopes roots binding to stdio; FR-316’s doc list should also rewrite the mod.rs comment.

**Action**: Extend FR-316 to include `mod.rs` lifecycle comment cleanup.

---

### M2 — MEDIUM: Public `tools/list` assumes homogeneous surface

**Area**: Cache policy  

FR-311 justifies `CacheScope::Public` via per-process fixed surface + INV-2 dispatch gate. A shared HTTP cache keyed only by URL could serve a full-surface `tools/list` to a compact deployment (schema disclosure across surface configs).

**Action**: Document single-surface-per-origin assumption, or require cache key to include `SYMFORGE_SURFACE`.

---

### M3 — MEDIUM: SC-302 overclaims what golden tests pin

**Area**: SC wording  

`tests/stel_golden_replay.rs:101–102` and `tests/conformance.rs:754–773` serialize `CallToolResult` via `serde_json::to_value` and do **not** assert `resultType` today. Key-plucking tolerates additive keys; these tests do not prove `ListToolsResult` wire shape or SDK stripping by negotiated version.

**Action**: Split SC-302 wording: existing batteries remain regression gates for `_meta` / `structuredContent`; version-aware `resultType` belongs in new transport fixtures (owner tests 1–2 / FR-A6).

---

### M4 — MEDIUM: 35 vs 39/40 tool surface wording

**Area**: Counts  

Tree spot-check: ~40 `#[tool(` sites (`tools.rs` 33 + `edit_tools.rs` 7); `SYMFORGE_TOOL_NAMES` lists 40 names. US3 says “39-tool” surface; US6 says “35 tools”; research says “35 `#[tool]` handlers”.

**Action**: Pick one counting rule and use it consistently (handlers vs advertised surface vs init allow-list).

---

### L1 — LOW: Minor line drift on `with_server_info`

**Area**: Anchors  

Cited as `mod.rs:1467`; actual `with_server_info` is `:1474` (`get_info` starts `:1462`). Other anchors verified:

| Citation | Status |
|----------|--------|
| `call_tool` `:1529` | OK |
| `read_resource` `:1505` | OK |
| `ListToolsResult` `:1557` | OK |
| `on_initialized` `:1576` | OK |
| `dispatch_tool_result_for_tests` `:1386` | OK |
| `with_project_evidence_scope` `:1540–1544` | OK |
| Meta sites `result_status.rs:1,:142` / `edit_tools.rs:271` | OK |
| `with_stateful_mode` `mcp_http.rs:119` | OK |
| `LocalSessionManager` `mcp_http.rs:42,:110,:128` | OK |
| `surface_default.rs:139,:158,:178` | OK |

---

### L2 — LOW: Verification report citation ledger uses `/latest/` URLs

**Area**: VERIFICATION-REPORT  

Report claims 3.1.0 ground truth but links `docs.rs/rmcp/latest/...`. Prefer pinned `/3.1.0/` paths (as in the spec header).

---

## What holds up

1. Blast radius correctly concentrated (~4 files; daemon/IPC/`#[tool]` handlers off the changed trait surface).
2. Frozen INV-1…INV-4 are the right security/correctness locks.
3. Silent failure modes (lifecycle-discover + version advertisement) correctly elevated above compile breaks.
4. Prior review dispositions folded cleanly (SC-315, FR-319/SC-316, INV-3/FR-321, INV-4).
5. Two-PR delivery (PR-A Phases 0–1 mechanical; PR-B Phases 2–4 protocol/lifecycle) matches risk.
6. Owner verification report reconciled with repo-specific carve-outs (zero `ServerResult` matches; MRTR/Tasks out of scope).

---

## Ground-truth spot checks

| Claim | Status | Note |
|-------|--------|------|
| Branch lacks `114b793` / `read_gate.rs` | Confirmed | `feat/knowledge-llm-sift`; `merge-base --is-ancestor` → false; FR-321 correct |
| `114b793` on `main` | Confirmed | `fix(023): gate raw-disk and working-tree content disclosure (#485)` |
| `rmcp` still `2.0` | Confirmed | features `transport-io`, `transport-streamable-http-server` — FR-A5 needed |
| `CallToolResponse` 3 variants + `non_exhaustive` | Confirmed | docs.rs/rmcp/3.1.0 — FR-A1 correct |
| `surface_default` calls gate directly | Confirmed | `:139/:158/:178` — SC-315 justified |
| HTTP skips initialize enforcement | Confirmed | `serve_directly` — US2-AS2 scope correct |
| Project evidence already on dispatch | Confirmed | `with_project_evidence_scope` at `call_tool` — FR-319 feasible |
| Zero `ProtocolVersion` refs in `src` | Confirmed | Silent `LATEST` pin risk is real |

---

## Doc roles

| Document | Role | Review note |
|----------|------|-------------|
| `spec.md` | Authority for scope, posture, acceptance | FULL scope + Addendum A; ready for plan after H1/H2 edits |
| `research.md` | Authority for change-to-site mapping | Map still excellent; Phase 3 narrative stale (H3) |
| `VERIFICATION-REPORT-owner-agent-2026-08-03.md` | Upstream 3.1.0 API ledger | Keep as citation appendix, not implementation script; most P1 MRTR/Tasks rows correctly carved out |

---

## Before `/speckit-plan`

1. **(Blocker)** Rewrite SC-302 + FR-305 Assumptions under FR-A6: version-parameterized expectations; distinguish serde tests from transport wire fixtures.
2. **(High)** Narrow adopted owner-test 15 to “strict when metadata required / modern path”; keep FR-309 legacy HTTP-200 semantics explicit.
3. **(High)** Patch `research.md` Phase 3 to match owner decision 2 (fallback chain + FR-319 disclosure; no lazy-bind / `RequestMetaObject`).
4. **(Medium)** Extend FR-316 to fix `mod.rs` `on_initialized` “both transports” comment; note Public-cache homogeneous-surface assumption; normalize tool counts.

---

## Out of scope for this review

- Implementation / code changes
- Running the full cargo gate under rmcp 3.1
- Writing `plan.md` / `tasks.md` (status correctly says ready for `/speckit-plan` sign-off; those artifacts are not yet present)
