<!-- Author verification note (wf_e25efee6-cb0):
All signatures verified against docs.rs/rmcp/3.1.0: `ServerHandler::call_tool → Result<CallToolResponse, ErrorData>` (rmcp aliases the error type as `McpError`; this spec uses `ErrorData` throughout — same type), `read_resource → Result<ReadResourceResponse, ErrorData>`, `supported_protocol_versions() → Cow<'static, [ProtocolVersion]>` with `V_2026_07_28` present and `LATEST = V_2025_11_25`, `ListToolsResult { tools, result_type: Option<ResultType>, meta: Option<MetaObject>, next_cursor, ttl_ms: Option<u64>, cache_scope: Option<CacheScope> }` (upstream constructors default `result_type` to `Some(ResultType::COMPLETE)`), `StreamableHttpServerConfig::with_legacy_session_mode(self, bool) -> Self` with `with_json_response` retained, `with_stateful_mode` gone, and `with_stateless_protocol_metadata_required(bool)` present, `ServerHandler::discover` and overridable `list_prompts` both real, and `model::MetaObject` / `model::RequestMetaObject` / `model::CacheScope { Public, Private }`.
-->

# Feature Specification: rmcp 3.x Migration & MCP 2026-07-28 Service

**Feature Branch**: `feat/knowledge-llm-sift` (drafted on) → implement on a dedicated branch<br>
**Implementation base (binding)**: branch from a base containing commit `114b793` (spec-023 raw-read admission gate, post-8.16.9 main). The drafting branch `feat/knowledge-llm-sift` does **not** contain it (`git merge-base --is-ancestor` verified) — see FR-321.<br>
**Created**: 2026-08-03<br>
**Status**: Draft — ready for `/speckit-plan` sign-off<br>
**Slice IDs**: `RM3-01` … `RM3-06`

**Requirements source (read-only)**:
`specs/025-rmcp-3-migration/research.md` — the adjudicated migration map (21 agents, 15 per-change
adversarial adjudications against the real code). That document is the authority for *which*
upstream changes land here and *where*; this spec is the authority for scope, posture, and
acceptance. API-signature ground truth: <https://docs.rs/rmcp/3.1.0> (all signatures cited below
were verified against it on 2026-08-03).

**Owner decisions (2026-08-03, binding)**:
1. **Scope = FULL** — land rmcp 3.1.0 *and* advertise/serve MCP 2026-07-28 in the same feature
   (research.md open question 1 resolved: in scope).
2. **Roots posture = fallback chain** — legacy clients keep roots-based binding for free via
   `on_initialized`; modern discover-lifecycle clients use the already-shipped chain
   (`SYMFORGE_WORKSPACE_ROOT` > `index_folder` > CWD walk). No new lazy-bind machinery
   (research.md open question 2 resolved: option b).
3. **Target `rmcp = "3.1"`** (3.1.0+), not 3.0.x — negotiation fix (#1093), stateless-metadata
   validation (#1089/#1091), MRTR decode fix (#1097). MSRV already satisfied
   (`rust-toolchain.toml` pins 1.96.0).
4. **Cache hints (SEP-2549)** — static list surfaces long TTL + `CacheScope::Public`;
   `read_resource` zero TTL + `CacheScope::Private`. Exact `ttl_ms` values below are the spec
   author's proposal, stated as FRs; the read-surface *scope* is frozen as INV-4.
5. **MRTR invariant** — the server never emits `InputRequired`; the response-enum types stop at
   the manual `ServerHandler` trait methods. Frozen invariant below.
6. **Compact-surface gate survival** — the manual `call_tool` override must keep suppressing the
   3.x `#[tool_handler]` macro. Frozen security invariant below.

---

## Why this feature exists

symforge is pinned to rmcp 2.x. Upstream rmcp 3.1.0 carries 19 breaking changes and the MCP
2026-07-28 protocol revision. Per research.md's adjudication, **~10 of the 19 land here**, almost
all confined to four files (`src/protocol/mod.rs`, `src/protocol/result_status.rs`,
`src/protocol/edit_tools.rs`, `src/server/mcp_http.rs`), and most are mechanical signature/rename
chases. The daemon proxy, all 40 `#[tool]` handlers (counting rule: `#[tool(` attribute sites — 33 in `tools.rs` + 7 in `edit_tools.rs` — equal to the 40 advertised names in `SYMFORGE_TOOL_NAMES`), and the IPC layer are untouched — they
dispatch on tool-name strings and `Parameters(...)` directly, bypassing the trait surfaces that
changed (`src/daemon.rs:5229-5375`, `src/server/mod.rs:156`).

Staying on 2.x means: no 2026-07-28 clients (specs/024's backlog already targets 2026-07-28
sessionless HTTP), no negotiation fix (#1093), no strict stateless-metadata validation
(#1089/#1091), and a growing gap under every future upstream fix.

## The one silent failure mode

Almost everything in this migration fails loudly at compile time. One thing does not:
**lifecycle-discover**. The entire workspace-binding-from-client-roots feature hangs off
`on_initialized` (`src/protocol/mod.rs:1576`) and `peer_info()` (`mod.rs:1252`), both structurally
dead for 2026-07-28 discover-lifecycle clients — a modern client never sends
`notifications/initialized`, so the hook never fires, and the server silently stays wrong-repo
bound (the repo's own "#1 field bug", not a compile error). Likewise, `supported_protocol_versions` has **zero** `ProtocolVersion` references in src —
which cuts the other way (round-3 source correction): rmcp 3.1.0's default already advertises
ALL of `KNOWN_VERSIONS` including `V_2026_07_28`, so without an explicit allow-list freeze a
future 3.x dependency bump could silently start advertising an untested protocol revision
(`mod.rs:1461`, `:1467`; FR-307). Both are resolved here:
modern clients bind through the shipped fallback chain (no roots re-trigger, owner decision 2),
with per-dispatch project evidence in result `_meta` making any residual wrong binding **loud
rather than silent** (FR-319 — the stale-CWD retarget of 012 D4-A fires only from
`on_initialized` and cannot help modern clients, so disclosure is the modern-lifecycle
safeguard); and `V_2026_07_28` is added *additively* to the advertised set.

### Implementation Phasing (informative — not FRs)

Per research.md §Migration order: **Phase 0** — `rmcp = "3.1"` bump (no code). **Phase 1** —
mechanical compile chase (renames, literals, MRTR signatures, `StreamableHttpService` constructor;
`cargo check` green, full suite as regression gate — no behavior change) plus the FR-315 closure
greps. **Phase 2** — conformance/policy (cache hints, `supported_protocol_versions` override).
**Phase 3** — lifecycle and security tripwires (fallback-chain binding for modern clients, the new
negotiation/discover/binding/evidence tests, and the gate tripwires SC-315/SC-316; the only phase
with design content). **Phase 4** — docs (`mcp_http.rs:11-28`, `tests/serve_http_attach.rs:23-30`
comment, `src/main.rs:236`).

---

## User Scenarios & Testing *(mandatory)*

### User Story 1 — A modern 2026-07-28 client attaches and gets full service (Priority: P1) — `RM3-01`

An MCP client speaking protocol revision 2026-07-28 (discover lifecycle, version-headered
stateless HTTP) attaches to the bearer-authed `/mcp` endpoint and gets identity, the full tool
surface, tool calls, prompts, and resources — with no `initialize`/`initialized` handshake
required and no session state.

**Why this priority**: this is the deliverable that makes the migration FULL scope rather than a
dependency bump. specs/024's backlog already targets 2026-07-28 sessionless HTTP; landing 3.1.0
without serving 2026-07-28 would leave the advertisement silently pinned at 2025-11-25 (zero
`ProtocolVersion` refs in src — nothing upgrades it for free).

**Independent Test**: an authenticated, version-headered client posts `server/discover` as the
**FIRST** `/mcp` request — no prior `initialize`, no `notifications/initialized` — then
`tools/list` + `tools/call`, all against a live `/mcp` bind. (A separate scenario may ALSO
exercise `initialize` with `protocolVersion: 2026-07-28` for negotiation coverage, but the
primary discover-lifecycle test never initializes — Codex F2.)

**Acceptance Scenarios**:

1. **Given** the `/mcp` endpoint under bearer auth, **When** a client initializes with
   `protocolVersion: 2026-07-28`, **Then** the negotiated version is 2026-07-28.
2. **Given** a discover-lifecycle client, **When** it calls `server/discover`, **Then** the
   response carries the symforge identity already set via `with_server_info`
   (`src/protocol/mod.rs:1474`) and lists `V_2026_07_28` among supported versions.
3. **Given** 3.1.0's strict stateless-metadata validation (#1089/#1091), **When** a
   version-headered 2026-07-28 request is posted to the stateless `/mcp` path, **Then** it is
   accepted and serviced.
4. **Given** `tools/list` and `tools/call` over stateless HTTP, **When** compared against direct
   dispatch, **Then** results are in parity (the `tests/serve_http_attach.rs` contract).

---

### User Story 2 — A legacy client keeps exactly today's behavior (Priority: P1) — `RM3-02`

A 2025-06-18 or 2025-11-25 client negotiates its version, gets roots-based workspace binding via
the `initialize` → `notifications/initialized` handshake on stdio, and observes identical
(sessionless) session semantics — what it gets today on rmcp 2.x.

**Why this priority**: two shipped test batteries pin 2025-06-18 negotiation
(`tests/graceful_degradation.rs:57`, `tests/watcher_b_p0_1_regression.rs:61`); every current
consumer is a legacy-lifecycle client. Regression here is a user-visible break on day one.

**Independent Test**: the two legacy-handshake batteries run unchanged; a roots-advertising legacy
client on stdio binds its workspace exactly as today.

**Acceptance Scenarios**:

1. **Given** a client pinning `protocolVersion: 2025-06-18`, **When** it initializes under
   3.1.0's fixed negotiation (#1093), **Then** it negotiates 2025-06-18 — the
   `graceful_degradation.rs:57` and `watcher_b_p0_1_regression.rs:61` pins stay green.
2. **Given** a legacy client **on the stdio transport** that advertises the roots capability,
   **When** `notifications/initialized` arrives, **Then** `on_initialized`
   (`src/protocol/mod.rs:1576`) still runs `bind_workspace_from_client_roots`
   (`mod.rs:1211-1263`) and binds the workspace as today. (Scoped to stdio deliberately: the
   stateless HTTP path uses `serve_directly`, which skips handshake enforcement, so
   `on_initialized` has never fired over `/mcp` — `src/server/mcp_http.rs:16-18`. There is no
   HTTP roots-binding behavior to preserve, and Phase 3 tests must not chase one.)
3. **Given** the `with_stateful_mode` → `with_legacy_session_mode` rename with the value `false`
   (`src/server/mcp_http.rs:119`), **When** any client of any generation attaches, **Then**
   session posture is unchanged — sessionless was already the intent and remains the posture for
   every protocol generation, with SC-301's HTTP-vs-dispatch parity as the measurable proxy for
   "unchanged".
4. **Given** a header-less legacy HTTP request, **When** it is posted to `/mcp`, **Then** it keeps
   today's HTTP-200 JSON-RPC error semantics — never a transport-level rejection (see FR-309 for
   the config knob that pins this).

---

### User Story 3 — The compact-surface gate still refuses off-surface calls (Priority: P1) — `RM3-03`

An operator running `SYMFORGE_SURFACE=compact` gets exactly the compact-3 surface; any call to an
off-surface tool is refused, exactly as today — under the 3.x `#[tool_handler]` macro.

**Why this priority**: this is a security gate (FR-008 heritage). The gate lives in the manual
`call_tool` override; the migration swaps the macro underneath it. If the 3.x macro stopped
deferring to the manual override, the gate would silently vanish while every test that doesn't set
`SYMFORGE_SURFACE=compact` stayed green.

**Independent Test**: the existing `SYMFORGE_SURFACE=compact` rejection tests, run unchanged,
**plus** the new dispatch-path tripwire (SC-315). The existing battery calls
`enforce_compact_surface()` directly as a function (`tests/surface_default.rs:139`, `:158`,
`:178` — its own header says it avoids binding a network transport) and therefore cannot, on its
own, detect the macro ceasing to route `tools/call` through the manual override.

**Acceptance Scenarios**:

1. **Given** `SYMFORGE_SURFACE=compact`, **When** an off-surface tool is called, **Then** the
   rejection is identical to today's (the existing rejection battery passes unchanged).
2. **Given** the 3.x `#[tool_handler]` macro, **When** the crate compiles, **Then** the manual
   `call_tool` override (`src/protocol/mod.rs:1529`) is the implementation that dispatches — the
   documented contract at `mod.rs:1515-1519` still holds; a macro-generated bypass is a spec
   violation even if all other tests pass.
3. **Given** the default (full, 40-tool) surface, **When** `tools/list` is called, **Then** the
   tool list is unchanged and deterministically ordered.
4. **Given** `SYMFORGE_SURFACE=compact` and the **real rmcp dispatch path** (the
   `tests/serve_http_attach.rs` HTTP harness and/or an in-process `ServerHandler::call_tool`
   invocation), **When** an off-surface `tools/call` arrives, **Then** it is rejected with the
   compact-surface `InvalidRequest` error — and an on-surface compact tool succeeds on the same
   path (FR-320 / SC-315, the binding INV-2 tripwire).

---

### User Story 4 — Modern clients get correct workspace binding via the fallback chain (Priority: P2) — `RM3-04`

A discover-lifecycle client — which never sends `notifications/initialized` and may never expose
roots — still ends up bound to the correct workspace, through the already-shipped chain:
`SYMFORGE_WORKSPACE_ROOT` env override, else an explicit `index_folder` call, else the CWD walk.
It is never **silently** bound to the wrong repository: any residual wrong binding (e.g. a stale
launch CWD with env unset and no `index_folder` — the 012 D4-A stale-CWD retarget at
`src/protocol/mod.rs:1223-1243` fires only from `on_initialized` and cannot help here) is
disclosed in every result's `_meta` project evidence (FR-319), so it is loud, not silent.

**Why this priority**: this is the resolution of the migration's one silent failure mode. It is P2
only because the mechanism already exists and ships today — the work is proving the chain covers
the modern lifecycle and making the residual case loud, not building new binding machinery (owner
decision 2: no new lazy-bind machinery).

**Independent Test**: a client that performs no `initialize`/`initialized` handshake issues tool
calls; binding is asserted for each rung of the chain in isolation (SC-312), and the `_meta`
project evidence discloses the bound root, including in the deliberate-mismatch negative case
(SC-316).

**Acceptance Scenarios**:

1. **Given** `SYMFORGE_WORKSPACE_ROOT` set, **When** a modern client attaches and calls a tool
   without ever sending `notifications/initialized`, **Then** the server is bound to that root.
2. **Given** no env override, **When** the client calls `index_folder` on a path, **Then** the
   server is bound to that folder.
3. **Given** neither env nor `index_folder`, **When** the first tool call arrives, **Then** the
   CWD-walk resolution applies — and the bound root is disclosed in the result's `_meta` project
   evidence, so a stale or foreign binding is visible to the client, never silent.
4. **Given** a modern client that never exposes roots, **When** it operates normally, **Then** the
   server never solicits `list_roots` outside `on_initialized` and never blocks or errors waiting
   for roots.
5. **Given** a server bound to repository A and a client expecting repository B, **When** any tool
   call returns, **Then** the `_meta` project evidence discloses the A binding — results never
   pass silently as if bound to B (FR-319 negative test).

---

### User Story 5 — Cache hints: static surfaces cacheable, live reads never (Priority: P2) — `RM3-05`

A hint-honoring 2026-07-28 client may cache the static list surfaces (tools, prompts, resources,
resource templates) for a long TTL in a shared cache, but is told — in band, per SEP-2549 — that
resource *reads* are private, uncacheable-by-default, per-workspace state (frozen as INV-4).

**Why this priority**: the list surfaces are fixed per-process (surface selection is env-driven at
`src/protocol/mod.rs:1552`), so public caching is free efficiency. The read surface is live
repository state served behind bearer auth (`src/protocol/resources.rs:142`); a shared cache entry
there is a cross-workspace information leak — and any nonzero read TTL would keep serving content
the spec-023 admission gate has since refused.

**Independent Test**: one conformance test asserting `ttl_ms`/`cache_scope` on all five list/read
results, with `Private` on reads (SC-313).

**Acceptance Scenarios**:

1. **Given** any of the four static list surfaces, **When** a result is returned, **Then** it
   carries `ttl_ms = 3600000` and `cache_scope = Public` (proposal — see FR-311).
2. **Given** `resources/read`, **When** a result is returned, **Then** it carries `ttl_ms = 0` and
   `cache_scope = Private` (INV-4).
3. **Given** repeated `tools/list` calls in one process, **When** results are compared, **Then**
   ordering is deterministic and content identical — a cacheable surface must not shuffle.
4. **Given** a client honoring hints across users or workspaces, **When** it processes a resource
   read, **Then** the `Private` scope prevents that read from entering any shared cache.

---

### User Story 6 — The daemon and stdio paths are behaviorally unchanged (Priority: P1) — `RM3-06`

An agent using symforge through the daemon proxy or plain stdio observes zero behavioral change:
same tool results, same `_meta` result-status wire shape, same golden-replay transcripts.

**Why this priority**: these are the highest-traffic paths, and by research.md's adjudication they
are structurally untouched — the daemon dispatches on `&str` tool names, not rmcp enums
(`src/daemon.rs:5229-5375`), and results cross the IPC wire as plain strings. Any observed change
on these paths is a defect in the migration, not an accepted consequence of it.

**Independent Test**: the golden replay, conformance, and `_meta` contract batteries run
unchanged.

**Acceptance Scenarios**:

1. **Given** the daemon proxy, **When** any of the 40 tools is called, **Then** results are
   identical to pre-migration; no response-enum type crosses the IPC wire.
2. **Given** `tests/stel_golden_replay.rs:101-102` and `tests/conformance.rs:754-773` run
   unchanged, **When** the migrated server serves them, **Then** they pass — the key-plucking
   assertion style tolerates the pinned `resultType: "complete"` key (FR-305), and
   `structuredContent` remains absent (`conformance.rs:757`).
3. **Given** the SFB09 `_meta` contract battery (`src/protocol/tools.rs:13189-13224`), **When**
   the `Meta` → `MetaObject` rename lands, **Then** the serialized `_meta` payload is
   byte-identical (`MetaObject` is a transparent JSON-map wrapper).

---

### Edge Cases

- **RM3-01/RM3-06**: the `ListToolsResult` literal at `src/protocol/mod.rs:1557` is exhaustive and
  will not compile under 3.1.0 until it accounts for `result_type`, `ttl_ms`, and `cache_scope` —
  one edit, not three. The edit MUST set `result_type: Some(ResultType::Complete)` explicitly per
  FR-305's pinned wire shape: bare `..Default::default()` (as the neighbors at
  `mod.rs:1486`/`:1499` use today) would likely yield `result_type: None` — no `resultType` key on
  the wire — leaving golden-replay expectations ambiguous, since upstream *constructors* default
  it to `Some(ResultType::COMPLETE)`. No code may start *reading* `result_type`.
- **RM3-02**: the `supported_protocol_versions` override MUST extend, never replace, the
  advertised set. Replacing it would break the 2025-06-18 pins with no compiler guidance — the
  invisible failure mode research.md flags for protocol-latest-default.
- **RM3-03**: `get_prompt` is macro-owned (`#[prompt_handler]`, `mod.rs:1460`) and absorbs the
  MRTR change without edits. Do not add a manual `get_prompt` override; the only new manual
  override in this feature is `list_prompts`, and only to set cache hints.
- **RM3-04**: the `peer_info()` roots-capability check inside `bind_workspace_from_client_roots`
  (`mod.rs:1252`) executes only on the legacy path. Under owner decision 2 it MUST NOT be
  "helpfully" extended with a per-request `_meta`/`RequestMetaObject` fallback — that is the
  rejected lazy-bind machinery.
- **RM3-04**: Roots is on rmcp's removal clock (upgrade path documented in-code at
  `mod.rs:1204-1210`). When rmcp removes `Peer::list_roots`, `bind_workspace_from_client_roots`
  (`mod.rs:1211-1263`) is **deleted**, not ported — see FR-313.
- **RM3-05/RM3-06**: FR-304 (the `read_resource` MRTR widening) and FR-312 (cache hints at
  `resources.rs:142`) edit exactly the seam through which the MCP resource surface inherits the
  spec-023 raw-read admission gate — the `read_resource_uri` → `get_file_content` delegation
  (`src/protocol/resources.rs:131-145`, `:205-219`). That delegation MUST survive both edits
  intact (INV-3); neither edit may restructure it into a direct disk read.
- **RM3-01**: `LocalSessionManager` exists only to satisfy the 2.x constructor arg
  (`src/server/mcp_http.rs:128`). Drop it if the 3.x `StreamableHttpService::new` signature
  allows; do not introduce an event store to keep it alive (no distributed replay requirement,
  single instance).
- **RM3-06**: the byte-budget tests (H1 ≤ 5000 B, A-025 ≤ 1500 B on serialized `Tool` entries,
  `src/stel/surface_list.rs`) can fail with **no code change** if 3.x `Tool` serde emits new
  fields. That failure is a budget decision (FR-317), not a bug to silently patch.

## Requirements *(mandatory)*

### Frozen invariants — stated first, not reopened by any FR

> **INV-1 (MRTR containment)**: the server never emits `InputRequired`. The 3.x response-enum
> types (`CallToolResponse`, `ReadResourceResponse`) appear **only** at the two manual
> `ServerHandler` trait methods (`src/protocol/mod.rs:1529`, `mod.rs:1505`), wrapping
> Complete-only results via `.into()`. The daemon proxy (`src/daemon.rs:5229-5375`),
> `dispatch_tool_result_for_tests` (`mod.rs:1386`), `ServerRuntime::dispatch_tool_call`
> (`src/server/mod.rs:156`), and all 40 tool handlers keep returning plain
> `CallToolResult`/`String`. No `RequestStateCodec` is introduced. Nobody threads the response
> enum through the daemon.

> **INV-2 (compact-surface security gate)**: the FR-008-heritage gate lives in the manual
> `call_tool` override, which the `#[tool_handler]` macro must keep suppressing (contract
> documented at `src/protocol/mod.rs:1515-1519`). This MUST survive the 3.x macro and MUST be
> re-verified by the existing `SYMFORGE_SURFACE=compact` rejection tests **and** the new
> dispatch-path tripwire (FR-320 / SC-315). The existing battery
> (`tests/surface_default.rs:139`, `:158`, `:178`) calls `enforce_compact_surface()` directly
> without binding a transport and therefore cannot detect a macro-generated bypass on its own —
> **SC-315 is the binding tripwire**. A migration in which the batteries pass only because the
> gate was never exercised through real dispatch is non-conforming.

> **INV-3 (raw-read admission gate, spec-023 heritage)**: every raw-disk content lane routes
> through `read_gate::admit_disk_read`/`admit_worktree_text` (`src/protocol/read_gate.rs`, on
> main as of commit `114b793`). The MCP resource surface inherits the gate **only** via the
> `read_resource_uri` → `get_file_content` delegation
> (`src/protocol/resources.rs:131-145`, `:205-219`); resource and resource-template reads keep
> delegating to the admission-gated tool handlers, and no new `fs::read` is introduced in the
> protocol layer. The gate call sites (`src/protocol/tools.rs:2463`, `:2838`, `:8258`, `:8667`,
> `:8772` on main) MUST survive the migration and any merge/rebase (FR-321). The spec-023
> refusal batteries are the named tripwire (SC-309).

> **INV-4 (resource reads never shared-cacheable)**: `resources/read` results always carry
> `ttl_ms = 0` and `cache_scope = CacheScope::Private`. Two independent justifications, either
> alone sufficient: (a) a shared cache entry is a cross-workspace information leak — reads serve
> live per-workspace repository state behind bearer auth (`src/protocol/resources.rs:142`); and
> (b) any nonzero TTL widens the window in which a well-behaved client keeps serving content
> the spec-023 admission gate (INV-3) has since refused. **Cache hints are HINTS, not
> enforcement** (Codex F5): a client's stale-on-error policy may return an expired private entry
> on refresh failure (verification report; upstream guide), so `ttl_ms = 0` minimizes but cannot
> guarantee next-read demotion on the client side — the server-side guarantee remains the
> admission gate itself, which refuses the content regardless of what a client cached.
> Exact-consistency clients must disable stale-on-error; state this beside the SYMFORGE_SURFACE
> docs. SC-313 is the named test. Unlike the
> FR-level `ttl_ms` numbers on list surfaces, this scope is **not** reopenable at plan time.

### Functional Requirements

**Dependency & mechanical chase**

- **FR-301**: `Cargo.toml` MUST declare `rmcp = "3.1"` (3.1.0 or later), not 3.0.x — 3.1.0 carries
  the negotiation fix (#1093), stateless-metadata validation (#1089/#1091), and the MRTR decode
  fix (#1097). No MSRV work: `rust-toolchain.toml` pins 1.96.0 ≥ upstream's 1.88 floor.
- **FR-302**: The removed `Meta` alias MUST be renamed to `MetaObject` at
  `src/protocol/result_status.rs:1`, `:142`, and `src/protocol/edit_tools.rs:271` (the same three
  sites cover the deprecated-removed change). The SFB09 `_meta` wire contract and its tests MUST
  be unchanged — `MetaObject` is a transparent JSON-map wrapper.
- **FR-303**: `with_stateful_mode(false)` at `src/server/mcp_http.rs:119` MUST become
  `with_legacy_session_mode(false)` (verified: `StreamableHttpServerConfig::
  with_legacy_session_mode(self, bool) -> Self` on docs.rs/rmcp/3.1.0). The value stays `false`:
  sessionless for every protocol generation; no legacy-client session support is wanted. This is
  the only production call site repo-wide.
- **FR-304**: The manual `call_tool` (`src/protocol/mod.rs:1529`) MUST widen its return type to
  `Result<CallToolResponse, ErrorData>` and the manual `read_resource` (`mod.rs:1505`) to
  `Result<ReadResourceResponse, ErrorData>`, appending `.into()` on Complete-only results (the
  `From` impls are provided upstream; `ErrorData` is the type rmcp aliases as `McpError` — this
  spec uses `ErrorData` throughout). `get_prompt` stays macro-owned (`mod.rs:1460`) and absorbs
  the change with zero edits. No new match arms: zero `ServerResult` matches exist repo-wide.
  The `read_resource` widening MUST NOT disturb the admission-gated delegation it wraps (INV-3).
  *RESOLVED (round 3, source-verified)*: 3.1.0's `ToolRouter::call` returns
  `Result<CallToolResponse, ErrorData>` — the router already wraps handler results in
  `Complete`, so the manual `call_tool` needs NO trailing conversion (signature widening only).
  `read_resource_uri()` still returns the internal `ReadResourceResult`, so the manual
  `read_resource` maps once at the trait boundary: `.map(ReadResourceResponse::Complete)` —
  which strengthens INV-1 (the response enum exists only at the boundary; all internal resource
  logic stays on `ReadResourceResult`).
- **FR-305**: The exhaustive `ListToolsResult` literal at `src/protocol/mod.rs:1557` MUST be
  updated in a single edit to account for `result_type: Option<ResultType>`, `ttl_ms: Option<u64>`,
  and `cache_scope: Option<CacheScope>` (all verified on docs.rs/rmcp/3.1.0). **Pinned STRUCT shape** (H1 rewrite, under FR-A6 which wins on wire questions): every
  manually constructed list/read result sets `result_type: Some(ResultType::Complete)`
  explicitly — key-absence (what bare `..Default::default()` would likely produce) was
  considered and rejected because upstream constructors default to `Some(ResultType::COMPLETE)`
  and one pinned struct shape is unambiguous. Three layers, kept distinct: (1) **struct
  construction** pins `Some(Complete)`; (2) **direct `serde_json` in the existing batteries**
  consequently emits the key — tolerated additively by key-plucking, NOT asserted (SC-302);
  (3) **the transport wire is version-aware per FR-A6** — modern-negotiated peers see
  `"resultType": "complete"`, legacy peers have the key STRIPPED by the SDK; owner tests 1-2
  pin BOTH transport shapes in new fixtures. `..Default::default()`
  remains acceptable for *other* fields, matching the neighbors at `mod.rs:1486`/`:1499` (which
  FR-310's cache-hint edits touch anyway and bring under the same pinned shape). No symforge code
  may **read** `result_type`.
- **FR-306**: The `StreamableHttpService` construction at `src/server/mcp_http.rs:42`, `:110`,
  `:126-129` MUST be chased to the 3.x `StreamableHttpService::new` signature; the `LocalSessionManager` generic/argument is RETAINED — 3.1.0's
  `StreamableHttpService::new(service_factory, session_manager: Arc<M>, config)` still requires
  it (round-3 source verification; the earlier "drop if permitted" is resolved: not permitted);
  only the config method renames (`with_stateful_mode` → `with_legacy_session_mode`);
  `with_json_response(true)` (`mcp_http.rs:121`) MUST be re-verified as retained (present on
  3.1.0's `StreamableHttpServerConfig`). The bearer-auth middleware MUST remain layered in front
  of `/mcp` for every protocol generation — the re-mount goes through the same
  `build_mcp_router` + `apply_bearer_auth` path (`mcp_http.rs:126-131`, `:159-164`) whose refusal
  semantics `tests/serve_auth.rs` pins (SC-308); a re-mount that reorders or drops the auth layer
  is non-conforming even if every parity test passes. No state migration is performed:
  `SymForgeServer` is `Clone` with all cross-call state in shared `Arc`s (`mod.rs:158-199`) and
  the factory clones per request (`mcp_http.rs:114`) — already the 3.x model.

**Protocol conformance**

- **FR-307** *(premise corrected, round 3, SOURCE-verified)*: rmcp 3.1.0's DEFAULT
  `ServerHandler::supported_protocol_versions()` already returns
  `ProtocolVersion::KNOWN_VERSIONS`, which INCLUDES `V_2026_07_28` (verified verbatim in
  rmcp/handler/server.rs:340 and model.rs:181-187; `LATEST` staying 2025-11-25 is a separate
  fact about the alias, not the supported set). The override is therefore NOT needed to
  activate 2026-07-28 — it is a deliberate **allow-list freeze**: because Cargo.toml uses the
  semver range `"3.1"`, a future rmcp 3.x update could otherwise auto-advertise a NEW protocol
  revision symforge has never tested. The server MUST override
  `supported_protocol_versions()` with the explicit frozen set (all of today's
  `KNOWN_VERSIONS`, including `V_2026_07_28`), so protocol exposure changes only by deliberate
  edit. Extend, never shrink: the 2025-06-18 pins at `tests/graceful_degradation.rs:57` and
  `tests/watcher_b_p0_1_regression.rs:61` MUST keep negotiating.
- **FR-308**: `server/discover` MUST return the symforge identity, derived from the existing
  `get_info()`/`with_server_info` override (`src/protocol/mod.rs:1474`), with `V_2026_07_28`
  visible in supported versions. No new `DiscoverResult`/`server_info` construction sites are
  introduced (zero exist).
- **FR-309**: The stateless `/mcp` path MUST be verified under 3.1.0's strict metadata validation
  (#1089/#1091) by **explicitly pinning**
  `.with_stateless_protocol_metadata_required(false)` (round 3: the 3.1.0 default IS `false`,
  source-verified — pin it anyway so a future default flip cannot change the mixed-endpoint
  posture silently) such that: version-headered 2026-07-28 requests are accepted — and still
  subject to modern request-metadata, header/body-agreement, and standard-header validation —
  while header-less legacy requests keep today's HTTP-200 JSON-RPC error semantics
  (`src/server/mcp_http.rs:117`), never a transport-level rejection. This FR is AUTHORITATIVE over the adopted owner-report acceptance test 15
  (research.md §Reconciliation): "missing metadata is rejected" applies only to the modern /
  metadata-required path; header-less legacy requests keep HTTP-200 JSON-RPC error semantics by
  design.

**Cache hints (SEP-2549)**

- **FR-310**: All four static list surfaces MUST carry cache hints: `list_tools`
  (`src/protocol/mod.rs:1557`), the two resource-list results (`mod.rs:1486`, `:1499`), and
  `list_prompts` — which requires a **new manual `list_prompts` override** (macro-owned today at
  `mod.rs:1460`), following the same manual-override pattern as `call_tool`. The same edits set
  `result_type: Some(ResultType::Complete)` per FR-305's pinned wire shape.
- **FR-311**: The static list surfaces MUST set `ttl_ms = 3_600_000` (1 hour — author's proposal
  per owner decision 4) and `cache_scope = CacheScope::Public`, and `tools/list` ordering MUST be
  deterministic, verified by test — a cacheable surface must not shuffle. `Public` rests on two
  recorded properties, **both load-bearing and neither weakenable later** on the grounds that
  "the list is the surface": (a) the surface is fixed per-process and identical for every caller
  (`SYMFORGE_SURFACE` is env-driven, `mod.rs:1552`), and `resource_definitions()` is verified
  static — it embeds no workspace data (`src/protocol/resources.rs:61-100`); (b) a stale cached
  *full* `tools/list` seen against a compact-surface server is harmless **only because** the
  INV-2 dispatch gate rejects off-surface calls. **(c) Deployment assumption (binding, M2)**:
  `CacheScope::Public` assumes ONE surface configuration per HTTP origin. A shared cache keyed
  only by URL in front of mixed-surface instances (e.g. a reverse proxy fronting a full-surface
  and a compact-surface symforge under one origin) would DISCLOSE full-surface tool schemas to
  compact-deployment clients — INV-2 blocks the calls but not the schema disclosure. Operators
  running mixed surfaces behind one origin must not share a public cache; document this beside
  the SYMFORGE_SURFACE docs.
- **FR-312**: `read_resource` (`src/protocol/resources.rs:142`) MUST set `ttl_ms = 0` and
  `cache_scope = CacheScope::Private`, per INV-4 — it serves live per-workspace repository state
  behind bearer auth, MUST never be eligible for a shared cache, and any nonzero TTL would keep
  serving content the spec-023 admission gate (INV-3) has since refused.

**Lifecycle & workspace binding**

- **FR-313**: The legacy roots path is retained as-is: `on_initialized`
  (`src/protocol/mod.rs:1576`) → `bind_workspace_from_client_roots` (`mod.rs:1211-1263`) under the
  existing scoped `#[allow(deprecated)]` (documented upgrade path at `mod.rs:1204-1210`). No
  modern-lifecycle re-trigger, no lazy bind, no per-request `_meta` capability fallback at
  `mod.rs:1252` is built. The deletion expectation MUST be documented now (tool
  description/CLAUDE.md): **when rmcp removes the Roots API (`Peer::list_roots`),
  `bind_workspace_from_client_roots` is deleted** and clients are directed to `index_folder`.
- **FR-314**: Workspace binding for discover-lifecycle clients MUST be served entirely by the
  shipped fallback chain — `SYMFORGE_WORKSPACE_ROOT` > `index_folder` > CWD walk — and MUST be
  proven by new tests that bind correctly without any `notifications/initialized`, asserted **per
  rung of the chain in isolation**, plus an assertion that the server never solicits `list_roots`
  outside `on_initialized` and never blocks waiting for roots (research.md new-test 3; SC-312).
- **FR-319**: Because the fallback chain cannot *guarantee* a correct binding for a modern client
  (the stale-CWD retarget fires only from `on_initialized`), the residual failure MUST be loud.
  **Scoping is not attachment** (Codex F1): `with_project_evidence_scope`
  (`src/protocol/mod.rs:1540-1544`) only binds the evidence slot for the dispatch; attachment
  happens exclusively in `ResultStatus::into_call_tool_result`
  (`src/protocol/result_status.rs:121-142`), which plain-`String`-returning tools (e.g. `health`,
  `src/protocol/tools.rs:7115`) and `resources/read` never pass through. Therefore this feature
  MUST attach project evidence **centrally** to every `CallToolResult` (statused or not) and to
  `resources/read` results, with an explicit **unbound marker** when no workspace is bound —
  e.g. by folding evidence into the result `_meta` at the `call_tool`/`read_resource` seam after
  the router returns, not per-handler. The `_meta`-parity consequence (previously meta-less
  results gain a `_meta` key) MUST be documented against SC-303's battery — key-plucking
  tolerates the addition, but the parity exception is stated, not silent. SC-316 MUST cover: a
  plain-`String` tool (e.g. `health`), a `resources/read`, the negative foreign-binding case,
  and the explicit unbound case. No new binding machinery is built — this is disclosure, not
  re-binding.

**Security gates under migration**

- **FR-320**: A new test MUST drive an off-surface `tools/call` through the **real rmcp dispatch
  path** under `SYMFORGE_SURFACE=compact` — via the `tests/serve_http_attach.rs` HTTP harness
  and/or an in-process `ServerHandler::call_tool` invocation — and assert the compact-surface
  `InvalidRequest` rejection, plus an on-surface compact tool succeeding on the same path
  (SC-315). Rationale: the existing battery (`tests/surface_default.rs:139`, `:158`, `:178`)
  exercises the gate *function*, not the *routing*; only a dispatch-path test can detect the 3.x
  macro ceasing to defer to the manual override (INV-2 names SC-315 as the binding tripwire).
- **FR-321**: Implementation MUST branch from a base containing commit `114b793` (spec-023
  raw-read admission gate, post-8.16.9 main); the drafting branch `feat/knowledge-llm-sift` does
  **not** contain it, so a migration branched or conflict-resolved against pre-023 code could
  silently drop gate call sites with every pre-existing SC battery green. All `read_gate` call
  sites (`src/protocol/tools.rs:2463`, `:2838`, `:8258`, `:8667`, `:8772` on main) MUST survive
  the migration and any merge/rebase; the `read_resource_uri` → `get_file_content` delegation
  MUST be preserved (INV-3); the spec-023 refusal batteries run unchanged (SC-309).

**Verification & closure**

- **FR-315**: The four upstream change-ids ruled out without formal adjudication
  (`tasks-extension`, `http-headers-2243`, `event-store`, `sep2260-association`) MUST each be
  confirmed by one grep during Phase 1 before being closed, with the grep and its zero-hit result
  recorded (research.md Q5). Expected greps: `#[task_handler]`/`TaskMetadata`/`.with_task`;
  `x-mcp-header`; `catch_unwind` near session managers; custom `SessionManager` impls.
- **FR-316**: Stale documentation MUST be updated in the same feature: the module docs at
  `src/server/mcp_http.rs:11-28`, the doc comment at `tests/serve_http_attach.rs:23-30`, and the
  lifecycle claims at `src/main.rs:236`, and the `on_initialized` doc comment at
  `src/protocol/mod.rs:1570-1576` — its "shared by both transports" claim is false for the
  hook itself (`serve_directly` skips handshake enforcement, so it never fires over `/mcp`);
  the comment must scope roots binding to stdio/legacy-lifecycle clients.
- **FR-317**: Byte-budget decision rule (research.md Q6): if 3.x `Tool` serde grows fields and the
  A-025 ≤ 1500 B budget (or H1 ≤ 5000 B, `src/stel/surface_list.rs`) fails with no symforge code
  change, the failure MUST be escalated to the owner as a budget decision — raise the budget or
  strip fields at serialization. The assertion MUST NOT be silently weakened and fields MUST NOT
  be silently stripped.
- **FR-318**: The existing regression batteries (§Success Criteria SC-301…SC-309) run unchanged as
  gates. Any assertion edit MUST be justified by a wire-visible, spec-conformant change (e.g. the
  pinned `resultType` key per FR-305) — never by convenience.

### Key Entities

- **Protocol generation / lifecycle mode**: legacy (`initialize` → `notifications/initialized`,
  may expose roots) vs. modern 2026-07-28 (discover lifecycle, no `initialized` notification).
  Determines whether `on_initialized` ever runs — the axis of the one silent failure mode.
- **Response envelope boundary**: `CallToolResult`/`String` everywhere inside symforge;
  `CallToolResponse`/`ReadResourceResponse` only at the two manual trait methods. The MRTR line
  drawn by INV-1.
- **Session posture**: `legacy_session_mode = false` — sessionless for every protocol generation;
  the 2.x `with_stateful_mode(false)` intent carried forward under its new name.
- **Cache hint**: the `(ttl_ms, cache_scope)` pair per result surface. Two policies exist: static
  list surfaces (long TTL, `Public` — FR-311) and live reads (zero TTL, `Private` — frozen as
  INV-4). No third policy.
- **Workspace binding source**: client roots (legacy lifecycle only, on the deletion clock) vs.
  the fallback chain (`SYMFORGE_WORKSPACE_ROOT` > `index_folder` > CWD walk) for everyone else —
  with per-dispatch `_meta` project evidence as the disclosure channel for the residual
  wrong-binding case (FR-319).
- **Compact-surface gate**: the manual `call_tool` override that filters off-surface calls; its
  relationship to the `#[tool_handler]` macro is a frozen contract (INV-2), not an implementation
  detail — and its tripwire must run through real dispatch (FR-320).
- **Raw-read admission gate**: `read_gate::admit_disk_read`/`admit_worktree_text` (spec 023,
  main @ `114b793`) — the single gate for all raw-disk content lanes. The MCP resource surface
  inherits it only via the `read_resource_uri` → `get_file_content` delegation; frozen as INV-3.

## Success Criteria *(mandatory)*

### Measurable Outcomes

**Existing batteries — run unchanged, all green:**

- **SC-301**: `tests/serve_http_attach.rs` passes unchanged — stateless `tools/list` +
  `tools/call` over reqwest with HTTP-vs-dispatch parity; the direct regression gate for the
  sessionless-http/stateful-rename chase.
- **SC-302**: `tests/conformance.rs:754-773` and `tests/stel_golden_replay.rs:101-102` pass
  unchanged, including the `structuredContent`-absence assertion at `conformance.rs:757`. These
  batteries serialize results directly via `serde_json` and key-pluck — they are regression
  gates for `_meta`/`structuredContent` and TOLERATE the new `resultType` key without asserting
  it. Version-aware `resultType` transport behavior (modern emits, legacy stripped — FR-A6) is
  pinned by the NEW fixtures of owner tests 1-2, not by these batteries.
- **SC-303**: The `serialized_tool_result` `_meta`/result-status contract battery
  (`src/protocol/tools.rs:13189-13224`) passes unchanged — the SFB09 wire shape survives the
  `Meta` → `MetaObject` rename byte-for-byte.
- **SC-304**: The `SYMFORGE_SURFACE=compact` rejection tests pass unchanged under the 3.x
  `#[tool_handler]` macro — necessary but **not** sufficient for INV-2: this battery calls
  `enforce_compact_surface()` directly (`tests/surface_default.rs:139`, `:158`, `:178`) without
  binding a transport; SC-315 is the binding tripwire.
- **SC-305**: `tests/graceful_degradation.rs:57` and `tests/watcher_b_p0_1_regression.rs:61` pass
  unchanged — 2025-06-18 negotiation preserved under 3.1.0's fixed negotiation (#1093).
- **SC-306**: The byte-budget tests (H1 ≤ 5000 B, A-025 ≤ 1500 B, `src/stel/surface_list.rs`) pass
  — or a recorded owner budget decision exists per FR-317. No silent weakening.
- **SC-307**: The prompts router/shape tests (`prompts.rs:706`, `:910`, `:949`) pass unchanged —
  `Prompt.arguments` and `ContentBlock::Text`/`ResourceLink` matches hold.
- **SC-308**: `tests/serve_auth.rs` passes unchanged — non-loopback bind with no key refuses to
  start; missing/wrong Bearer yields 401. Pins that the FR-306 constructor chase keeps
  `apply_bearer_auth` layered in front of `/mcp`.
- **SC-309**: The spec-023 raw-read admission-gate refusal batteries (the `read_gate` refusal
  tests in `src/protocol/tools.rs`, e.g. at `:30757`, `:32471`, `:32499` on main) pass unchanged
  — the INV-3 tripwire (FR-321).

**New tests — each its own criterion (research.md §Test surface labels in parentheses):**

- **SC-310** *(new test 1 — negotiation)*: `initialize` with `protocolVersion: 2026-07-28`
  negotiates 2026-07-28, and in the same test battery 2025-06-18 still negotiates 2025-06-18.
- **SC-311** *(new test 2 — discover-first lifecycle)*: an authenticated, version-headered
  `server/discover` sent as the FIRST request on a fresh `/mcp` connection returns the symforge
  identity and lists `V_2026_07_28` in supported versions; then `tools/list`, `tools/call`,
  `prompts/list`, `resources/list`, and `resources/read` all succeed on the same connection.
  The test asserts EXPLICITLY (round 3): no `initialize` was ever issued, no
  `notifications/initialized` was ever issued, and `on_initialized` therefore never ran —
  the full service surface works with zero handshake.
- **SC-312** *(new test 3 — modern binding)*: workspace binding resolves correctly for a client
  that never sends `notifications/initialized`, asserted **per rung** of the fallback chain in
  isolation (env override / `index_folder` / CWD walk), plus an assertion that the server never
  solicits `list_roots` outside `on_initialized` and never blocks waiting for roots (FR-314).
- **SC-313** *(new test 4 — cache hints)*: `ttl_ms`/`cache_scope` are present on all five
  list/read results, with `Private` on resource reads (INV-4) and `Public` on the four static
  lists, and `tools/list` ordering is deterministic (FR-311).
- **SC-314** *(new test 5 — strict metadata)*: on the stateless `/mcp` path under 3.1.0's
  validation (#1089/#1091), with the `with_stateless_protocol_metadata_required` posture pinned
  per FR-309, version-headered 2026-07-28 requests are accepted and header-less legacy requests
  keep HTTP-200 JSON-RPC error semantics — run **with bearer auth enabled**, so the accept/reject
  semantics are proven behind auth, not on an unauthenticated bind. Modern NEGATIVE cases (round 3): (i) `MCP-Protocol-Version: 2026-07-28` with missing
  required `_meta` → rejected; (ii) header version vs `_meta.protocolVersion` disagreement →
  rejected; (iii) `Mcp-Method`/`Mcp-Name` disagreeing with the JSON-RPC body → rejected;
  (iv) correctly formed modern request behind bearer auth → accepted — proving both halves
  of the mixed endpoint, not only the happy path.
- **SC-315** *(new test 6 — compact gate via real dispatch)*: under `SYMFORGE_SURFACE=compact`,
  an off-surface `tools/call` driven through the real rmcp dispatch path (the
  `tests/serve_http_attach.rs` HTTP harness and/or in-process `ServerHandler::call_tool`) is
  rejected with the compact-surface `InvalidRequest` error, and an on-surface compact tool
  succeeds on the same path — the binding INV-2 tripwire (FR-320).
- **SC-316** *(new test 7 — binding evidence disclosure)*: a modern (no-handshake) client
  observes the bound workspace root in result `_meta` project evidence on (a) a statused tool,
  (b) a plain-`String` tool (e.g. `health`), and (c) a `resources/read`; the negative case
  (server bound to repository A, client expecting repository B) shows the evidence disclosing
  the foreign binding rather than results passing silently; and an unbound server discloses the
  explicit unbound marker (FR-319).

**Closure:**

- **SC-316b** *(new test 8 — legacy roots adapter interop; owner test 17)*: a legacy stdio
  client driving `initialize` → `notifications/initialized` → `roots/list` gets its workspace
  bound from client roots exactly as today, verified end-to-end (US2 heritage; the adopted
  owner-report test 17 previously had no named criterion — Codex F6).
- **SC-317**: The four unadjudicated change-ids are each closed with a recorded zero-hit grep
  (FR-315).
- **SC-318**: The full gate is green: `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`,
  `cargo build --release`, and `cd npm && npm test`.

## Assumptions

- research.md is the adjudicated authority for change-to-site mapping; this spec does not re-derive
  it. Where research.md offered options, the owner decisions in the header resolve them.
- API signatures verified against docs.rs/rmcp/3.1.0 on 2026-08-03:
  `ServerHandler::call_tool → Result<CallToolResponse, ErrorData>`,
  `read_resource → Result<ReadResourceResponse, ErrorData>` (`McpError` is upstream's alias for
  `ErrorData`; this spec uses `ErrorData` throughout),
  `supported_protocol_versions() → Cow<'static, [ProtocolVersion]>` (with `V_2026_07_28`
  present and `LATEST = V_2025_11_25`), `ListToolsResult { tools, result_type, meta, next_cursor,
  ttl_ms, cache_scope }` (constructors default `result_type` to `Some(ResultType::COMPLETE)`),
  `StreamableHttpServerConfig::with_legacy_session_mode(self, bool) -> Self` with
  `with_json_response` retained and `with_stateless_protocol_metadata_required(bool)` present,
  `ServerHandler::discover` and overridable `list_prompts` both real, and
  `model::MetaObject` / `model::RequestMetaObject` / `model::CacheScope { Public, Private }` all
  present.
- ~~Deferred signature~~ RESOLVED (round 3): `ToolRouter::call` returns
  `Result<CallToolResponse, ErrorData>` (source-verified) — no conversion in `call_tool`;
  `read_resource` maps `ReadResourceResponse::Complete` at the boundary. INV-1 strengthened.
- The `ttl_ms` values in FR-311/FR-312 are the spec author's proposal (owner decision 4). Changing
  a list-surface `ttl_ms` number at plan time does not reopen this spec; changing the
  list-surface *scope* (`Public` → `Private`) is an FR change requiring justification; making
  resource reads shared-cacheable in any form violates INV-4 and reopens the spec.
- The pinned `resultType` STRUCT shape (FR-305, explicit `Some(ResultType::Complete)`) matches
  upstream constructor defaults; flipping to key-absence is a struct/serde-visible change that
  reopens FR-305's three-layer contract and the owner-test-1/2 transport fixtures — SC-302's
  batteries tolerate the key either way and are NOT the tripwire for it (Codex F7).
- The implementation base contains `114b793` (FR-321) — this is a merge/rebase gate to verify at
  branch time, not an assumption to trust later; the drafting branch does not satisfy it.
- MSRV needs no work: `rust-toolchain.toml` pins 1.96.0; CI installs from the pin; no
  `rust-version` declaration exists to falsify; rmcp is not re-exported through the embed facade.
- Error-code requirements of the 2026-07-28 revision are pre-satisfied: resource-not-found is
  already `invalid_params` (-32602) at `src/protocol/resources.rs:136`/`:140`, and nothing emits
  codes in the reserved -32020..-32099 band.
- Zero `ServerResult` matches, zero protocol-union-enum references, and zero
  `DiscoverResult`/`server_info` construction sites exist repo-wide (research.md) — the
  nonexhaustive-enums change cannot bite, and FR-308 needs no new construction.
- No new dependency is introduced beyond the rmcp version bump.

## Out of Scope

- **Subscriptions/listen**: list-changed, resource-update, and logging notifications; MCP ping;
  `logging/setLevel`; any GET stream (HTTP runs `json_response=true`, `mcp_http.rs:121` — there is
  no stream to supersede).
- **The tasks extension**: no `#[task_handler]`, `TaskMetadata`, or `.with_task` — confirmed
  absent, closed by the FR-315 grep.
- **Sampling and MCP Logging capabilities**: not advertised today, not added here.
- **The modern roots re-trigger** (research.md decision 3, option a): no lazy once-per-connection
  bind, no `AtomicBool` guard, no `RequestMetaObject` capability fallback. Rejected by owner
  decision 2 — it would die inside Roots' 12-month removal window. FR-319's evidence disclosure
  is the accepted substitute for guaranteed re-binding.
- **MRTR emission**: the server never emits `InputRequired` and no `RequestStateCodec` is built
  (INV-1). Enabling interactive tool flows is a separate future feature if ever wanted.
- **Legacy session support** (`legacy_session_mode = true`) and any HTTP+SSE transport.
- **rmcp client-side changes**: OAuth stack, `ClientLifecycleMode`, client response cache,
  `get_stream`, scope accumulation, DCR binding — symforge embeds no MCP client.
- **Deleting `bind_workspace_from_client_roots`**: documented as the expected end state (FR-313)
  but executed only when rmcp actually removes the Roots API, not in this feature.

<!-- Review dispositions (2026-08-03):
CONFIRMED, folded in:
- R2 HIGH (INV-2 tripwire insufficient — surface_default.rs calls enforce_compact_surface()
  directly): FR-320 + SC-315 added; INV-2 amended to name SC-315 as the binding tripwire; US3
  Independent Test + AS4 updated. Line anchors :139/:158/:178 re-verified against the tree.
- R2 HIGH (spec-023 raw-read admission gate absent from spec; drafting branch lacks 114b793 —
  re-verified: src/protocol/read_gate.rs absent on this branch): INV-3 added; FR-321 (base
  requirement + call-site survival) added; SC-309 (023 refusal batteries) added; header
  "Implementation base" line added; edge case for the read_resource seam added; FR-304 amended.
- R2 HIGH (US4-AS3 "never stale or foreign" undeliverable): took the reviewer's preferred
  option — FR-319 (evidence disclosure via result_status::with_project_evidence_scope) + SC-316
  (positive + negative test); US4 narrative/AS3 reworded to "never *silently*"; "one silent
  failure mode" section updated.
- R2 MEDIUM (bearer auth unpinned): FR-306 auth-layering sentence; SC-308 (serve_auth.rs)
  added to run-unchanged gates; SC-314 required to run behind bearer auth.
- R2 MEDIUM (read CacheScope only FR-level): promoted to frozen INV-4 with both justifications
  (cross-workspace leak + gate-refusal latency); FR-312 and Assumptions updated.
- R2 LOW (FR-311 unrecorded dependencies): both justifications (static resource_definitions,
  INV-2 dispatch gate) recorded in FR-311 as non-weakenable.
- R2 LOW (US2-AS2 HTTP never fired on_initialized): AS2 scoped to stdio with the
  serve_directly rationale.
- R1 MEDIUM (strict-metadata knob unnamed): FR-309 now requires asserting/explicitly setting
  with_stateless_protocol_metadata_required, with the failure remediation stated; SC-314 updated.
- R1 LOW (FR-305 Default vs resultType tension): wire shape pinned to explicit
  Some(ResultType::Complete); edge case, SC-302, US6-AS2, and Assumptions aligned.
- R1 LOW (US4 scenarios exceed SC-310): resolved by broadening, not trimming — SC-312 now
  requires per-rung isolation + no-roots-solicitation/no-blocking assertions (coverage kept).
Nitpicks — applied where free, noted here per instruction:
- R1 NIT (McpError vs ErrorData): same type (alias); normalized to ErrorData throughout with
  one alias note. Naming-only.
- R1 NIT (US2-AS3 "byte-identical" unmeasurable): softened to "posture unchanged" with SC-301
  parity named as the measurable proxy.
- R1 NIT (FR-312 misfiles tools/list determinism): moved to FR-311 / SC-313. Cosmetic refiling,
  no semantic change.
Corrections made while merging:
- INV-1 anchor dispatch_tool_result_for_tests corrected 1390 → 1386 (verified against the tree;
  matches R2's citation).
- SCs renumbered for the added gates: existing batteries SC-301..309, new tests SC-310..316,
  closure SC-317..318; all internal cross-references updated (research.md's new-test numbering
  1..5 retained as parenthetical labels).
Not adopted:
- R1's alternative for the US4/SC gap ("trim US4's scenarios") — rejected in favor of broadening,
  per the no-shrink instruction.
- R2's optional alternative for US4 ("delete the claim, accept residual risk") — the stronger
  FR+test option was taken instead.
-->


---

## Addendum A — 2026-08-03 amendments (BINDING; drafted after the base document)

Source: research.md §Amendments + §Reconciliation (owner-agent verification
report, preserved as `VERIFICATION-REPORT-owner-agent-2026-08-03.md`). These
extend the FRs above; on conflict, this addendum wins.

**FR-A1 (invariant wording).** Everywhere this spec says the server "never
emits `InputRequired`", read: **never emits `InputRequired` OR `Task`**.
`CallToolResponse` has THREE variants — `Complete(CallToolResult)`,
`InputRequired(InputRequiredResult)`, `Task(CreateTaskResult)` (SEP-2663) —
and is `#[non_exhaustive]` (verified verbatim, docs.rs 3.1.0
rmcp/model/mrtr.rs). Every result crosses `.into()` from a `CallToolResult`;
the Tasks extension is out of scope.

**FR-A2 (wildcard-arm coding rule).** No code in this repo matches on
`CallToolResponse`, `GetPromptResponse`, `ReadResourceResponse`, or
`ServerResult` today (verified: zero matches repo-wide). Any FUTURE `match` on these non-exhaustive enums requires a wildcard arm
— the COMPILER enforces this (informative, not a reviewable rule; Codex low-cleanup); the
Phase-1 grep merely records the zero-match baseline.

**FR-A3 (constructor rule, round-3 refinement).** NON-EXHAUSTIVE models MUST use documented
constructors/builders (Rust enforces it externally): `InputRequiredResult`, `ListRootsResult`,
and — round-3 source finding — **`ReadResourceResult`**. The paginated list results
(`ListToolsResult`, `ListPromptsResult`, `ListResourcesResult`, `ListResourceTemplatesResult`)
are intentionally EXHAUSTIVE — struct literals are legal there (which is why `mod.rs:1557`
compiles) — but as a symforge maintainability POLICY the builder style is preferred where it
exists, because constructors seed `result_type: Some(ResultType::Complete)` for free, e.g.
`ListToolsResult::with_all_items(tools).with_ttl_ms(3_600_000).with_cache_scope(CacheScope::Public)`
and `ReadResourceResult::new(contents).with_ttl_ms(0).with_cache_scope(CacheScope::Private)`
(PR-B only; PR-A uses plain constructors with no cache builders so protocol policy stays in
PR-B).

**FR-A4 (mixed-major dependency gate).** Phase 0 and CI assert — EXECUTABLY — that the
dependency graph contains exactly one major version of `rmcp` and of `rmcp-macros`, and that it
is major 3. `cargo tree -d` is NOT that assertion (it lists any duplicated crate's subtrees, so
its output/exit status cannot distinguish one major from two — Codex F4). Use `cargo
metadata --format-version 1` and assert that the set of distinct major versions for packages
named `rmcp` and `rmcp-macros` equals {3} (e.g. a jq/python one-liner in CI; owner-report
acceptance test 18). Two rmcp majors in one graph produce identically-named, incompatible
types.

**FR-A5 (cargo features — RESOLVED, round 3).** Both declared features
(`transport-io`, `transport-streamable-http-server`) still exist on rmcp 3.1.0, and `server`
is in the default feature set — the PR-A manifest keeps the existing feature list and only
changes the version to `"3.1"`. NEW closure assertion: the campaign's `Cargo.lock` must resolve
rmcp to EXACTLY 3.1.0 (record `cargo tree -p rmcp` output in the PR) — the semver range is
fine for maintenance, but this campaign's compile-on-paper evidence is specifically against
3.1.0.

**FR-A6 (version-aware wire fixtures).** The SDK emits
`resultType: "complete"` for modern-negotiated peers and STRIPS it for
legacy peers. Golden JSON fixtures and snapshot assertions must be split or
parameterized by negotiated version — one universal fixture is wrong in one
direction or the other. (Owner-report acceptance tests 1 and 2 adopted.)

**Delivery structure (owner directive).** The campaign lands as TWO PRs
under this one spec: **PR-A** = Phases 0-1 (mechanical compile chase at
protocol parity; the full existing suite is a pure regression gate — zero
intended behavior change), **PR-B** = Phases 2-4 (cache hints, protocol
advertisement, lifecycle + new tests, docs). Owner-report acceptance tests
adopted into the test surface: 1, 2 (version-aware wire), 15 (strict
metadata validation), 16 (fallback negotiation), 17 (roots adapter
interop), 18 (mixed-major CI check).
