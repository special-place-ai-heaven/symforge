# Implementation Plan: rmcp 3.x Migration & MCP 2026-07-28 Service

**Feature**: `025-rmcp-3-migration` · **Branch base (binding)**: main containing `114b793`
(verified: `git merge-base --is-ancestor 114b793 HEAD`) · **Authority**: `spec.md` (Addendum A
binding; three external review rounds folded; every named API signature source-verified against
docs.rs/rmcp/3.1.0) · **Change-to-site map**: `research.md` (incl. round-3 source corrections)

## Summary

Move symforge from rmcp 2.2 to **3.1.0 exactly** (manifest `"3.1"`, lockfile pinned and recorded)
and serve MCP **2026-07-28** alongside every currently supported legacy revision. Blast radius is
concentrated in four files (`src/protocol/mod.rs`, `src/protocol/result_status.rs`,
`src/protocol/edit_tools.rs`, `src/server/mcp_http.rs`); the daemon proxy, all 40 `#[tool]`
handlers, and the IPC layer are untouched. Delivery is **two PRs** with a hard fault boundary:

- **PR-A — mechanical chase at protocol parity.** Compile against 3.1.0 with ZERO intended
  behavior change; the full existing suite is a pure regression gate.
- **PR-B — protocol enablement.** Allow-list freeze, cache hints, discover-first lifecycle,
  FR-319 central evidence attachment, the SC-310…SC-318 test battery, docs.

## Technical Context

- Toolchain: `rust-toolchain.toml` pins 1.96.0 (> rmcp MSRV 1.88); CI installs from the pin.
- Dependency: `rmcp = { version = "3.1", features = ["transport-io",
  "transport-streamable-http-server"], optional = true }` — feature names verified to exist on
  3.1.0 (round 3); `server` is in rmcp's default set. Lockfile must resolve **exactly 3.1.0**
  (`cargo tree -p rmcp` recorded in the PR).
- Source-verified API facts the plan RELIES on (no open questions remain):
  `ToolRouter::call → Result<CallToolResponse, ErrorData>` (no conversion in `call_tool`);
  `read_resource_uri()` stays on internal `ReadResourceResult` → one
  `.map(ReadResourceResponse::Complete)` at the trait boundary;
  `StreamableHttpService::new(factory, Arc<M>, config)` still requires the session manager
  (`LocalSessionManager` retained); `with_stateless_protocol_metadata_required` defaults `false`
  (pinned explicitly anyway); default `supported_protocol_versions()` = `KNOWN_VERSIONS`
  **including** `V_2026_07_28` (override = allow-list freeze, not activation).
- Gates (repo CLAUDE.md): `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -D
  warnings`, full serial test suite, `cargo build --release`; plus the spec's named batteries.

## Constitution Check

No `.specify` constitution exists; the binding constraints are: the frozen invariants
**INV-1…INV-4** (response enums stop at the trait boundary; compact-surface dispatch gate
survives the 3.x macro; the 023 admission gates stay load-bearing; resource reads never
shared-cacheable), the owner decisions in the spec header, and the repo verification gates
above. Any plan step that would touch these is non-conforming by definition.

## Project Structure

### Documentation (this feature)
`specs/025-rmcp-3-migration/`: `spec.md` · `research.md` · `plan.md` (this) · `tasks.md` ·
`REVIEW-DISPOSITIONS.md` · `REVIEW-REPORT-2026-08-03.md` ·
`VERIFICATION-REPORT-owner-agent-2026-08-03.md`. No data-model.md (the feature owns no data
entities — every type is upstream rmcp's) and no contracts/ dir (the wire contract IS the spec's
FR set; house precedent 021 likewise has none).

### Source code touched
- **PR-A**: `Cargo.toml`, `Cargo.lock`, `src/protocol/mod.rs` (`:1529` call_tool signature,
  `:1505` read_resource signature+map, `:1557` ListToolsResult literal),
  `src/protocol/result_status.rs` (`:1`, `:142` Meta→MetaObject),
  `src/protocol/edit_tools.rs` (`:271` same), `src/server/mcp_http.rs` (`:119` legacy-session
  rename; constructor chase `:42/:110/:126-129`), `CLAUDE.md` (stale "39-tool" → 40; deferred
  M4 sub-item), CI workflow (mixed-major `cargo metadata` assertion).
- **PR-B**: `src/protocol/mod.rs` (supported_protocol_versions override; manual `list_prompts`
  override for cache hints; central evidence attachment seam), `src/protocol/resources.rs`
  (`:142` read hints), `src/protocol/result_status.rs` (evidence helper),
  `src/server/mcp_http.rs` (metadata knob pin, module docs), `src/main.rs:236` +
  `tests/serve_http_attach.rs:23-30` docs, `src/protocol/mod.rs:1570-1576` on_initialized
  comment, new tests (see tasks).

## Implementation order and rationale

**PR-A (strict order — each step keeps `cargo check` progress monotonic):**
1. Base gate + mixed-major assertion + Cargo bump (`"3.1"`), lock resolves 3.1.0.
2. Compiler-driven renames: `Meta`→`MetaObject` (3 sites), `with_stateful_mode`→
   `with_legacy_session_mode` (1 site).
3. `ListToolsResult` literal at `mod.rs:1557`: explicit `result_type:
   Some(ResultType::Complete)` + `..Default::default()` for the two new cache-hint fields
   (values stay `None` in PR-A — policy is PR-B).
4. MRTR signatures: `call_tool → Result<CallToolResponse, ErrorData>` (body unchanged — router
   already returns the enum); `read_resource → Result<ReadResourceResponse, ErrorData>` with
   `.map(ReadResourceResponse::Complete)`.
5. `StreamableHttpService` constructor chase, `LocalSessionManager` retained,
   `with_json_response(true)` re-verified.
6. Phase-1 closure greps (FR-315: the four unadjudicated change-ids; FR-A2 zero-match baseline).
7. `CLAUDE.md` tool-count fix (40).
8. Full gates + full serial suite green ⇒ PR-A opens; squash-merge on green CI.

**PR-B (builds on merged PR-A):**
1. Pin `.with_stateless_protocol_metadata_required(false)` (FR-309).
2. `supported_protocol_versions()` allow-list freeze (FR-307) + negotiation/discover tests
   (SC-310, SC-311 discover-FIRST full-surface, SC-305 legacy pins).
3. Cache hints via builder style (FR-310/311/312, INV-4; manual `list_prompts` override;
   deterministic tools/list order test; SC-313).
4. FR-319 central evidence attachment (design D1 below) + SC-316 five-case battery + SC-316b
   legacy roots interop.
5. SC-314 strict-metadata positive/negative battery (behind bearer auth).
6. Docs (FR-316 list) + version-aware wire fixtures (FR-A6; owner tests 1-2).
7. Full gates ⇒ PR-B opens; squash-merge on green.

**Why this order**: PR-A steps 2-5 are compiler-forced and independently verifiable; nothing in
PR-A consults a policy decision, so a red suite in PR-A can only mean a mechanical mistake or an
upstream behavior surprise — exactly what a pure regression gate can adjudicate. PR-B's steps
order tests-with-their-features so every policy lands with its tripwire in the same commit.

## Design decisions

### D1 — Central evidence attachment site (FR-319)
Attach at the **trait-boundary seam, post-router**: in `call_tool`, after
`self.tool_router.call(tcc)` resolves, merge project evidence into the result's `_meta`
(creating `_meta` when absent; never overwriting an evidence key already written by
`ResultStatus::into_call_tool_result` — single-writer wins, seam is fallback). Same pattern in
`read_resource` on the `ReadResourceResult` before wrapping `Complete`. Rationale: one seam
covers all 40 handlers including plain-`String` returns, zero per-handler edits, and the
existing statused path stays byte-identical (its evidence is already attached before the seam
sees it). The unbound case writes an explicit `{"bound": false}`-shaped marker (exact shape in
tasks) rather than omitting the key — absence must remain distinguishable from unbound.

### D2 — `_meta` parity exception, stated not silent
Previously meta-less results (plain-`String` tools) gain a `_meta` carrying only project
evidence. SC-303's battery key-plucks and tolerates this; the spec's parity exception paragraph
is the documentation of record. Any golden fixture that asserts full `_meta` equality must be
updated in the same commit with the evidence key included.

### D3 — Version-aware fixtures
New transport tests parameterize expected JSON by negotiated version (modern: `resultType`
present; legacy: stripped). Existing serde-level batteries are NOT converted — they remain
regression gates for `_meta`/`structuredContent` only (FR-305 three-layer contract).

### D4 — CI mixed-major assertion shape
A small script step: `cargo metadata --format-version 1` parsed (python one-liner) asserting
the set of distinct major versions for packages named `rmcp`/`rmcp-macros` == {3}. Lands in
PR-A alongside the bump so the gate exists from the first migrated commit.

## Risks and mitigations

- **3.x `#[tool_handler]` macro stops deferring to the manual `call_tool` override** → the
  compact-surface gate silently dies. Mitigation: SC-304 (direct) + SC-315 (real-dispatch
  tripwire) run in PR-A's regression pass; if the macro contract changed, PR-A goes red before
  any protocol work starts.
- **Tool serde grows fields → A-025 ≤1500 B / H1 ≤5000 B byte budgets fail with no code
  change.** Mitigation: FR-317 decision rule — escalate to owner as a budget decision; never
  silently weaken.
- **`serve_http_attach` fixtures drift under 3.1.0's stricter validation.** Mitigation: it runs
  in PR-A's regression gate at parity (legacy semantics unchanged); strictness posture is pinned
  explicitly only in PR-B.
- **Evidence seam double-writes or breaks SFB09 `_meta` wire shape.** Mitigation: single-writer
  merge rule (D1) + SC-303 byte-level battery in the same PR.

## Out of scope for this plan

MRTR emission, Tasks extension, subscriptions/listen, sampling/logging capabilities, modern
roots re-trigger machinery, client-side cache behavior (documented, not implemented), startup
performance (024 backlog).

## Constitution re-check (post-design)

D1-D4 touch no frozen invariant: the response enums still stop at the trait boundary (D1
operates on `CallToolResult`/`ReadResourceResult` BEFORE wrapping), the compact gate ordering is
unchanged (`enforce_compact_surface` stays first in `call_tool`), the 023 admission gates are
not on any touched path, and INV-4's read-hint scope is exactly FR-312. PASS.
