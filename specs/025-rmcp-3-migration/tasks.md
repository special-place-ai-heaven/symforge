# Tasks: rmcp 3.x Migration & MCP 2026-07-28 Service

Derived from `plan.md` (order binding) and `spec.md` (FR/SC authority; Addendum A wins).
Two PRs, hard fault boundary. Every task cites its FR/SC.

## Standing rules for every task

- Worktree `E:\project\symforge-rmcp3`, branch `feat/rmcp-3-migration` (PR-A) then a follow-on
  branch off merged main for PR-B. Base gate first (T200).
- Builds/tests through Terminal Commander `run_and_watch`; never `| tail`, never sleep-polling.
- No behavior change lands in PR-A; anything behavioral discovered mid-chase STOPS the task and
  gets recorded, not smuggled.
- Frozen invariants INV-1…INV-4 are non-negotiable; a task that would touch one is a spec bug —
  stop and surface.

## Phase A0: Preconditions (PR-A)

- [ ] **T200** Base gate: `git merge-base --is-ancestor 114b793 HEAD` (FR-321). STOP on failure.
- [ ] **T201** Bump `Cargo.toml` to `rmcp = { version = "3.1", ... }` (features UNCHANGED —
  FR-A5 resolved); `cargo update -p rmcp`; record `cargo tree -p rmcp` showing exactly 3.1.0.
- [ ] **T202** Mixed-major assertion (FR-A4, D4): `cargo metadata` parse asserting distinct
  majors of `rmcp`+`rmcp-macros` == {3}; add the same check as a CI step in this PR.

## Phase A1: Mechanical chase (PR-A; compiler-driven, zero behavior change)

- [ ] **T210** `Meta` → `MetaObject`: `src/protocol/result_status.rs:1`, `:142`,
  `src/protocol/edit_tools.rs:271` (FR-303/meta-split).
- [ ] **T211** `with_stateful_mode(false)` → `with_legacy_session_mode(false)` at
  `src/server/mcp_http.rs:119` + module-doc touch-up of that line only (stateful-rename).
- [ ] **T212** `ListToolsResult` literal `mod.rs:1557`: explicit
  `result_type: Some(ResultType::Complete)`, cache-hint fields via `..Default::default()`
  (stay `None` in PR-A) (FR-305 struct layer only).
- [ ] **T213** `call_tool` signature → `Result<CallToolResponse, ErrorData>`; body UNCHANGED
  (router already returns the enum — source-verified). `read_resource` →
  `Result<ReadResourceResponse, ErrorData>` with `.map(ReadResourceResponse::Complete)`;
  admission-gated delegation untouched (FR-304, INV-1, INV-3).
- [ ] **T214** `StreamableHttpService` constructor chase (`mcp_http.rs:42/:110/:126-129`):
  `LocalSessionManager` RETAINED; `with_json_response(true)` re-verified; bearer-auth layering
  byte-identical (FR-306, SC-308).
- [ ] **T215** Closure greps (FR-315): tasks-extension, http-headers-2243, event-store,
  sep2260-association each zero-hit; FR-A2 zero-match baseline for the four response enums.
  Record outputs in the PR body.
- [ ] **T216** `CLAUDE.md` "39-tool" → 40 with counting rule (ledger M4 deferred item).

## Phase A2: PR-A gates

- [ ] **T220** `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`.
- [ ] **T221** Full serial suite green — pure regression gate (SC-301…SC-309 batteries incl.
  SC-303 `_meta` byte shape, SC-304 compact rejection, `serve_http_attach`, byte budgets
  FR-317 rule on any budget failure). `cargo build --release`.
- [ ] **T222** PR-A: conventional title, body carries T201/T202/T215 evidence; squash-merge on
  green CI; cleanup (target, worktree keeps for PR-B or re-cut).

## Phase B0: PR-B branch

- [ ] **T230** New branch off merged main (contains PR-A); base gate again.

## Phase B1: Protocol policy (PR-B)

- [ ] **T231** Pin `.with_stateless_protocol_metadata_required(false)` (FR-309).
- [ ] **T232** `supported_protocol_versions()` override = explicit frozen allow-list (today's
  `KNOWN_VERSIONS` incl. `V_2026_07_28`) (FR-307 as corrected round 3).
- [ ] **T233** Cache hints (FR-310/311/312, INV-4): builder style on the four list surfaces
  (`ttl_ms 3_600_000`, `Public`) + manual `list_prompts` override; `read_resource` `ttl_ms 0`,
  `Private`; deterministic tools/list ordering.
- [ ] **T234** FR-319 central evidence attachment per plan D1: post-router `_meta` merge in
  `call_tool` (single-writer, statused path byte-identical) + same on `read_resource`; explicit
  unbound marker; `_meta` parity exception documented (D2).

## Phase B2: New test battery (PR-B; each with its feature)

- [ ] **T240** SC-310 negotiation (2026-07-28 and 2025-06-18 both negotiate).
- [ ] **T241** SC-311 discover-FIRST full surface: authenticated version-headered
  `server/discover` as literally the first request; then tools/list, tools/call, prompts/list,
  resources/list, resources/read; explicit asserts: no initialize, no initialized,
  on_initialized never ran.
- [ ] **T242** SC-312 modern binding per fallback-chain rung; never solicits roots outside
  on_initialized; never blocks.
- [ ] **T243** SC-313 cache-hint conformance on all five surfaces.
- [ ] **T244** SC-314 strict-metadata battery incl. the four modern NEGATIVE cases (behind
  bearer auth).
- [ ] **T245** SC-315 compact gate via REAL dispatch (HTTP harness + in-process call_tool).
- [ ] **T246** SC-316 evidence disclosure five cases (statused, plain-String `health`,
  resources/read, foreign-binding negative, unbound marker).
- [ ] **T247** SC-316b legacy stdio roots interop end-to-end.
- [ ] **T248** FR-A6 version-aware wire fixtures (owner tests 1-2): modern emits `resultType`,
  legacy stripped.

## Phase B3: Docs + closure (PR-B)

- [ ] **T250** FR-316 doc list: `mcp_http.rs:11-28`, `serve_http_attach.rs:23-30` comment,
  `main.rs:236`, `mod.rs:1570-1576` on_initialized scoping.
- [ ] **T251** Full gates (fmt/clippy/serial suite/release) + SC-317/SC-318 closure.
- [ ] **T252** PR-B: squash-merge on green; final cleanup (targets, worktrees, branches both
  ends); update task board + agentmemory.
