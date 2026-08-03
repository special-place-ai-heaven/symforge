<!-- Provenance: rmcp3-migration-research workflow wf_810d7551-d5c, 2026-08-03.
     21 agents / 177 tool calls: 1 upstream-source reader (migration guide
     rust-sdk#969, MCP 2026-07-28 changelog, 3.0.0/3.1.0 release notes),
     4 per-area code mappers (85 rmcp use-sites), 15 per-change adversarial
     adjudications against the real code, 1 synthesis. API-signature ground
     truth for the spec phase: https://docs.rs/rmcp/3.1.0 -->

# rmcp 3.x migration — research

## Verdict

Moderate-sized, highly concentrated migration: ~10 of 19 upstream breaking changes land here, almost all confined to `src/protocol/mod.rs`, `src/protocol/result_status.rs`, `src/protocol/edit_tools.rs`, and `src/server/mcp_http.rs`, and most are mechanical signature/rename chases (Meta→MetaObject, `with_stateful_mode`→`with_legacy_session_mode`, two MRTR return-type widenings, one exhaustive struct literal). The daemon proxy, all 40 `#[tool]` handlers, and the IPC layer are untouched — they dispatch on tool-name strings and `Parameters(...)` directly, bypassing the trait surfaces that changed (src/daemon.rs:5229-5375, src/server/mod.rs:156). The riskiest area is **lifecycle-discover**: the entire workspace-binding-from-client-roots feature hangs off `on_initialized` (src/protocol/mod.rs:1576) and `peer_info()` (mod.rs:1252), both structurally dead for 2026-07-28 discover-lifecycle clients — it fails silently (server stays wrong-repo-bound, the repo's own "#1 field bug"), not at compile time, and needs a design decision plus new tests. Target **rmcp 3.1.0** (`rmcp = "3.1"`), not 3.0.0 — it adds the negotiation fix (#1093), stateless-metadata validation (#1089/#1091), and the MRTR decode fix (#1097). MSRV 1.88 is already exceeded (rust-toolchain.toml pins 1.96.0).

## Breaking changes that land here

| Change | Affected sites | Action | Risk |
|---|---|---|---|
| mrtr-results | src/protocol/mod.rs:1529 (manual `call_tool`), mod.rs:1505 (manual `read_resource`) | Widen the two manual trait-method return types to `CallToolResponse`/`ReadResourceResponse`, append `.into()` (From impls provided). `get_prompt` is macro-owned (`#[prompt_handler]`, mod.rs:1460) and absorbs the change. Zero `ServerResult` matches repo-wide → no new arms. Server never emits `InputRequired` → no `RequestStateCodec`. Re-run compact-surface rejection tests to confirm the 3.x `#[tool_handler]` still defers to the manual override (contract documented at mod.rs:1515-1519 — this guards the FR-008 security gate). | low |
| mrtr-resulttype | src/protocol/mod.rs:1557 | Exhaustive `ListToolsResult { tools, meta: None, next_cursor: None }` literal gains `result_type` — add the field or switch to `..Default::default()` like the neighbors at mod.rs:1486/1499. No code reads `result_type`; all snapshot tests pluck individual keys (tools.rs:13194-13224, tests/conformance.rs:754-773, tests/stel_golden_replay.rs:101-102), so the new `"resultType"` emission breaks nothing. | low |
| sessionless-http | src/server/mcp_http.rs:119, :42, :110, :126-129, :11-28 (docs), tests/serve_http_attach.rs:23-30 (doc comment) | Signature chase in `build_mcp_service` only: rename/delete the stateful-mode call, drop or keep the `LocalSessionManager` generic/argument per the 3.x `StreamableHttpService::new` signature, reverify `with_json_response(true)` (mcp_http.rs:121). No state migration: SymForgeServer is Clone with all cross-call state in shared Arcs (mod.rs:158-199), factory clones per request (mcp_http.rs:114) — already the 3.x model. `list_tools` is env-driven (mod.rs:1552), not per-connection. Update stale module docs. | low |
| stateful-rename | src/server/mcp_http.rs:119, :13 (doc) | `s/with_stateful_mode/with_legacy_session_mode/` — one token, guaranteed compile error until done. Behavior byte-identical for this config (sessionless was already the intent). Only production call site repo-wide. | low |
| lifecycle-discover | src/protocol/mod.rs:1576 (`on_initialized`), :1252 (`peer_info()` roots check), :1461/:1467 (version advertisement), src/main.rs:236 + src/server/mcp_http.rs:17 (stale docs) | Default `discover` derived from `get_info()` works free (identity already overridden via `with_server_info`, mod.rs:1467). The real work (SUPERSEDED per owner decision 2 — see spec FR-313/FR-319): (a) override `supported_protocol_versions()` to add `V_2026_07_28` explicitly — `LATEST` stays 2025-11-25; (b) modern clients bind via the shipped fallback chain (`SYMFORGE_WORKSPACE_ROOT` > `index_folder` > CWD walk) — NO lazy-bind, NO `RequestMetaObject` fallback (both rejected decision-3a options); (c) residual wrong binding is disclosed via per-dispatch project evidence in result `_meta` (spec FR-319); (d) new lifecycle tests. Zero `DiscoverResult`/`server_info` construction sites exist. | **high** |
| meta-split | src/protocol/result_status.rs:1, :142, src/protocol/edit_tools.rs:271 | Rename removed `Meta` alias to `MetaObject` at the import and both tuple constructions feeding `CallToolResult::with_meta`. Transparent JSON-map wrapper → SFB09 `_meta` wire contract and its tests unchanged. No `context.meta` access anywhere. | low |
| cache-hints-2549 | src/protocol/mod.rs:1557 (compile break), :1486, :1499, src/protocol/resources.rs:142, mod.rs:1460 (macro-owned `list_prompts`) | Fix the mod.rs:1557 literal, then make all five list/read surfaces spec-conformant: `list_tools` long TTL + `CacheScope::Public` (surface fixed per-process by SYMFORGE_SURFACE); resource lists likewise; `read_resource` (resources.rs:142) zero TTL + **`CacheScope::Private`** — live per-workspace repo state behind bearer auth. `ListPromptsResult` needs a manual `list_prompts` override to set hints (same pattern as manual `call_tool`). Verify deterministic tools/list ordering. Add a conformance test. | medium |
| deprecated-removed | src/protocol/result_status.rs:1, :142, src/protocol/edit_tools.rs:271 | Same three Meta sites (overlaps meta-split). Everything else already post-deprecation: plural `*RequestParams` throughout (mod.rs:37-38, 1482, 1494, 1507, 1531, 1549), `ErrorData` everywhere, zero hits on elicitation/sampling/schema/child-process removals. | low |
| protocol-latest-default | src/protocol/mod.rs:1461, :1467, src/server/mcp_http.rs:117, tests/graceful_degradation.rs:57, tests/watcher_b_p0_1_regression.rs:61 | Depend on `rmcp = "3.1"`. Zero `ProtocolVersion` references in src → server silently keeps advertising 2025-11-25 unless `V_2026_07_28` is added explicitly (extend, don't replace, so the 2025-06-18 test-client pins keep negotiating). Add a negotiation test. Reverify the stateless /mcp path against 3.1.0's strict metadata validation (#1089/#1091). No compiler guidance — invisible failure mode. | medium |
| spec-deprecations | src/protocol/mod.rs:1211-1263 (`bind_workspace_from_client_roots`), :1576 | Roots is the only deprecated surface in use, already under scoped `#[allow(deprecated)]` with the SEP-2577 upgrade path documented in-code (mod.rs:1204-1210). No forced change while Deprecated-not-removed; plan the successor (env override > `index_folder` > CWD walk — all shipped already). Error codes pre-satisfied: resource-not-found already `invalid_params` = -32602 (resources.rs:136/140); nothing in the reserved -32020..-32099 band. No Sampling, no MCP Logging capability, no HTTP+SSE transport. | medium |

## Breaking changes ruled out

| Change | Why not |
|---|---|
| nonexhaustive-enums | Zero references to any of the six protocol union enums repo-wide. Daemon proxy dispatches on `&str` tool names, not rmcp enums (src/daemon.rs:5229-5375); results cross the IPC wire as plain Strings. |
| subscriptions-listen | Server emits no list-changed/resource-update/logging notifications; capabilities are tools/prompts/resources only, no subscribe/listChanged/logging flags (mod.rs:1467-1472). No MCP ping, no `logging/setLevel`; HTTP runs `json_response=true` (mcp_http.rs:121) so no GET stream exists to supersede. |
| tasks-extension | No adjudication ran, but the code map's exhaustive rmcp-API inventory shows zero tasks-API sites (`#[task_handler]`, `TaskMetadata`, `.with_task` — none). Upstream: removals inert if unused. Confirm with one grep at implementation time. |
| http-headers-2243 | The new `S: ServerHandler` bound on `StreamableHttpService` is already satisfied — the factory hands it SymForgeServer (mcp_http.rs:114), whose ServerHandler impl is at mod.rs:1461. Header validation is automatic; no `x-mcp-header` opt-in wanted. |
| event-store | No `catch_unwind` around session managers; `LocalSessionManager` exists only to satisfy the 2.x constructor arg (mcp_http.rs:128) and likely disappears under the sessionless-http chase. Single-instance, no distributed replay requirement. |
| sep2260-association | Bundled session manager, no custom cross-process SessionManager. The only server-initiated request (`list_roots`, mod.rs:1212) fires from `on_initialized`, which never runs for modern clients — moot under this change-id; the roots exposure is owned by lifecycle-discover/spec-deprecations. |
| annotations-lastmodified | Zero `last_modified`/`with_timestamp` hits; resources are built flat with title/description/mime only (resources.rs:380-400). `ToolAnnotations` (surface_list.rs:43) is a different type, untouched. |
| structured-content-value | Zero `ToolResultContent`/`structured_content` uses; all results via `CallToolResult::success/::error` + `with_meta`. conformance.rs:757 only asserts the field is *absent* — holds under `Option<Value>`. |
| msrv-188 | rust-toolchain.toml pins 1.96.0; CI installs from the pin. No `rust-version` declaration exists to falsify; rmcp is not re-exported through the embed facade. |
| oauth-authreq (+ all client-only items) | Entirely rmcp's OAuth *client* stack; symforge embeds no MCP client. Same for `ClientLifecycleMode`, client response cache, `get_stream` signature, scope accumulation, DCR binding. |

## Design decisions the spec must settle

1. **`legacy_session_mode` posture** — keep `false` (sessionless for every protocol generation), matching today's `with_stateful_mode(false)` intent at mcp_http.rs:119. Decide whether any future legacy-client session support is wanted (recommendation: no — nothing here ever wanted sessions).
2. **Protocol-version scope** — is advertising/serving 2026-07-28 part of *this* migration, or is landing on 3.1.0 at 2025-11-25 parity the goal with 2026-07-28 as a follow-up? `LATEST` will not do it for us (zero `ProtocolVersion` refs in src; identity flows from `ServerInfo::new` at mod.rs:1467). specs/024's backlog already targets 2026-07-28 sessionless HTTP, which argues for in-scope.
3. **[RESOLVED — owner decision 2: option (b), fallback chain + FR-319 disclosure] Workspace-binding trigger for discover-lifecycle clients** — `on_initialized` (mod.rs:1576) is dead for modern clients. Options: (a) lazy once-per-connection bind on first request (AtomicBool guard, RequestContext already available in `call_tool`/`list_tools`), reading capabilities from `_meta` via `RequestMetaObject`; (b) skip roots entirely for modern clients and lean on the shipped fallback chain (SYMFORGE_WORKSPACE_ROOT > `index_folder` > CWD walk). This interacts with Roots' 12-month removal clock (mod.rs:1204-1210) — investing in (a) buys at most a year.
4. **MRTR posture in the shared dispatcher** — the server never emits `InputRequired`; the manual `call_tool`/`read_resource` wrap Complete-only via `.into()`. `dispatch_tool_result_for_tests` (mod.rs:1390), `ServerRuntime::dispatch_tool_call` (src/server/mod.rs:156), and the daemon's 40 `Parameters(...)` arms keep returning plain `CallToolResult`/String and must stay off the trait surface. The spec should state this as an invariant so nobody "helpfully" threads the response enum through the daemon.
5. **Compact-surface gate survival** — the FR-008 gate lives in the manual `call_tool` override that the `#[tool_handler]` macro must keep suppressing (contract at mod.rs:1515-1519). The spec must require re-verification under the 3.x macro plus the existing `SYMFORGE_SURFACE=compact` rejection test as the tripwire.
6. **Cache-hints policy** — concrete `ttl_ms` values per surface; `CacheScope::Private` for `read_resource` (live per-workspace state behind bearer auth, resources.rs:142) vs `Public` for the static tool list; whether to add the manual `list_prompts` override needed to set hints on the macro-owned `ListPromptsResult` (mod.rs:1460); deterministic tools/list ordering guarantee.
7. **Roots exit plan** — keep the scoped `#[allow(deprecated)]` through this migration, and pre-decide that when rmcp removes `Peer::list_roots`, `bind_workspace_from_client_roots` (mod.rs:1211-1263) is deleted, with clients directed to `index_folder`. Document that expectation now (tool description / CLAUDE.md).

## Migration order

**Phase 0 — prerequisites (no code):** bump `rmcp = "3.1"` in Cargo.toml. Toolchain already satisfies MSRV (1.96.0 pinned).

**Phase 1 — mechanical compile chase (compiler-driven, no design):**
- Meta→MetaObject rename: result_status.rs:1/:142, edit_tools.rs:271.
- `with_stateful_mode` → `with_legacy_session_mode`: mcp_http.rs:119.
- `ListToolsResult` literal: mod.rs:1557 (gains `result_type` + the two cache-hint fields in one edit).
- MRTR signature widenings + `.into()`: mod.rs:1529, mod.rs:1505.
- `StreamableHttpService` generics/constructor chase: mcp_http.rs:42/:110/:126-129 (drop `LocalSessionManager` if the 3.x signature allows); reverify `with_json_response` (mcp_http.rs:121).
- Goal: `cargo check` green. Full test suite as regression gate — no behavior should have changed.

**Phase 2 — conformance and policy (small code, needs decisions from the spec):**
- Cache hints on all five list/read results incl. the new manual `list_prompts` override.
- `supported_protocol_versions()` override adding `V_2026_07_28` (additive).

**Phase 3 — lifecycle (SUPERSEDED by owner decision 2 — see spec.md; still the only
genuinely risky phase):**
- NO lazy-bind re-trigger and NO `RequestMetaObject` capability fallback (both were
  decision-3a options, rejected). Modern discover-lifecycle clients bind via the
  already-shipped fallback chain (`SYMFORGE_WORKSPACE_ROOT` > `index_folder` > CWD walk);
  legacy clients keep `on_initialized` roots binding unchanged; per-dispatch project
  evidence in result `_meta` (spec FR-319) makes residual wrong binding loud rather than
  silent.
- New negotiation/discover/binding/evidence tests (see below).

**Phase 4 — docs:** mcp_http.rs:11-28, tests/serve_http_attach.rs:23-30 comment, src/main.rs:236 lifecycle claims.

## Test surface

**Existing batteries that pin behavior (run unchanged as regression gates):**
- `serialized_tool_result` `_meta`/result-status contract assertions (src/protocol/tools.rs:13189-13224) — pin the SFB09 wire shape through the Meta→MetaObject rename; key-plucking style tolerates the new `resultType` key.
- tests/conformance.rs:754-773 and tests/stel_golden_replay.rs:101-102 — same key-level assertions, incl. `structuredContent` absence at conformance.rs:757.
- tests/serve_http_attach.rs — drives the stateless tools/list + tools/call path over reqwest with HTTP-vs-dispatch parity; the direct regression gate for the sessionless-http/stateful-rename chase.
- `SYMFORGE_SURFACE=compact` rejection tests — tripwire for the compact-gate/macro-override contract (mod.rs:1515-1519) under the 3.x `#[tool_handler]`.
- Byte-budget tests (H1 ≤5000 B, A-025 ≤1500 B on serialized Tool entries, src/stel/surface_list.rs) — can fail with **no code change** if 3.x Tool serde emits new fields; treat a failure as a budget decision, not a bug.
- prompts.rs router/shape tests (:706, :910, :949) — pin `Prompt.arguments` and `ContentBlock::Text/ResourceLink` matches.
- Legacy-handshake clients: tests/graceful_degradation.rs:57 and tests/watcher_b_p0_1_regression.rs:61 pin 2025-06-18 negotiation — must keep passing under 3.1.0's fixed `supported_protocol_versions` negotiation (#1093).

**New tests the transport/lifecycle changes demand:**
1. Negotiation: initialize with `protocolVersion: 2026-07-28` negotiates 2026-07-28 (and 2025-06-18 still negotiates).
2. `server/discover` returns symforge identity + `V_2026_07_28` in supported versions.
3. Modern-lifecycle workspace binding: binding occurs without `notifications/initialized` (whatever mechanism decision 3 picks).
4. Cache-hints conformance: `ttl_ms`/`cache_scope` present on all five list/read results; `Private` on resource reads.
5. Stateless /mcp under 3.1.0's strict metadata validation (#1089/#1091): version-headered 2026-07-28 requests accepted, header-less legacy requests keep HTTP-200 JSON-RPC error semantics.

## Open questions for the owner

1. Is serving 2026-07-28 in scope for this migration, or is 3.1.0 at 2025-11-25 parity the deliverable (with 2026-07-28 as a fast-follow)? This gates Phases 2-3 and half the new tests.
2. Roots-based workspace binding: build the modern-lifecycle re-trigger (decision 3a, dies within Roots' 12-month removal window) or accept the env/`index_folder`/CWD fallback now and delete `bind_workspace_from_client_roots` when rmcp removes the API?
3. Cache policy numbers: TTL for the static tool/resource lists, TTL for live `read_resource`, and confirmation that resource reads must be `CacheScope::Private` given the bearer-auth /mcp bind.
4. Implementation-time verification needed: does 3.1.0's `ToolRouter::call` return `CallToolResult` or already `CallToolResponse`? Decides whether `.into()` lands inside the manual `call_tool` (mod.rs:1529) or only in the signature.
5. Four upstream changes had no formal adjudication (tasks-extension, http-headers-2243, event-store, sep2260-association) — ruled out above from the code map's API inventory; confirm each with a one-line grep during Phase 1 before closing them.
6. If 3.x Tool serde grows fields and the A-025 ≤1500 B budget fails: raise the budget or strip fields at serialization?


## Amendments — 2026-08-03, verified against docs.rs/rmcp/3.1.0

1. **CallToolResponse has THREE variants, not two** (verified verbatim,
   rmcp/model/mrtr.rs): `Complete(CallToolResult)`,
   `InputRequired(InputRequiredResult)`, `Task(CreateTaskResult)` (SEP-2663,
   `resultType: "task"`) — and the enum is `#[non_exhaustive]`. The
   mrtr-results row above said Complete/InputRequired only. Consequences:
   the MRTR invariant reads "the server never emits InputRequired OR Task";
   `From<CallToolResult> for CallToolResponse` is confirmed, so the `.into()`
   wrap plan stands; symforge constructs but never matches the enum, so no
   wildcard arms are needed today — but any FUTURE match on CallToolResponse
   or ServerResult (both non-exhaustive) requires a wildcard arm, stated as a
   coding rule in the spec.
2. **ServerHandler::call_tool signature confirmed** (docs.rs trait page):
   `Result<CallToolResponse, McpError>` with `CallToolRequestParams` — the
   research map's plural-params claim holds.
3. **Owner phasing directive**: compile first at protocol parity, then
   separately enable 2026-07-28 — the campaign delivers as TWO PRs
   (PR-A = Phases 0-1 mechanical chase, suite as pure regression gate;
   PR-B = Phases 2-4 conformance + lifecycle + new tests + docs), both under the
   single 025 spec.
4. A separate owner-side verification agent is auditing all changes since
   2.2; its output supersedes or confirms individual rows here when it lands.


## Reconciliation with the owner-agent verification report — 2026-08-03

Report preserved as `VERIFICATION-REPORT-owner-agent-2026-08-03.md`. Verdict:
it CONFIRMS every affected/ruled-out row above (3.1.0 direct, two-step
delivery, three-variant non-exhaustive CallToolResponse, `.into()` path,
stateful->legacy rename, roots deprecated-not-removed with no renamed
replacement, LATEST still 2025-11-25, 3.1.0 strictness). Deltas:

**Adopted from the report (new to this map):**
- **Mixed-major dependency check**: `cargo tree -d | rg 'rmcp|rmcp-macros'`
  in Phase 0 AND as a CI assertion (report acceptance test 18) — two rmcp
  majors in the graph produce identically-named incompatible types.
- **Version-aware wire fixtures**: modern peers get `resultType:"complete"`,
  legacy peers get it STRIPPED by the SDK — golden JSON must split by
  negotiated version, not one universal fixture. Sharpens the byte-budget /
  snapshot caveat above.
- **Cargo feature-name verification**: the report shows
  `features = ["server"]`; our Cargo.toml declares
  `["transport-io", "transport-streamable-http-server"]` — verify 3.1.0's
  actual feature set at Phase 0 (implementation-time check, added to the
  Phase-1 verification greps).
- **Constructor-only rule for non-exhaustive models**: `InputRequiredResult`,
  `ListRootsResult` etc. must be built via constructors/builders, never
  struct literals — same class as the mod.rs:1557 literal fix.
- **Exact sibling enum names confirmed**: `GetPromptResponse` and
  `ReadResourceResponse`, both `Complete | InputRequired`, both
  non-exhaustive.

**Narrowed by this repo's code map (generic advice that does NOT apply):**
- The report's P0 "ServerResult dispatch" arm: symforge has ZERO ServerResult
  matches (daemon dispatches on tool-name strings) — the item reduces to the
  wildcard-arm coding rule plus one Phase-1 verification grep.
- All MRTR-emission and Tasks-extension build-out (report P1 rows, the
  TaskManager surface, `request-state`/HMAC codec, acceptance tests 3-14):
  OUT OF SCOPE under the never-emits invariant — symforge returns
  Complete-only on every surface. The report's own sequencing endorses this
  ("compile first with complete-only responses").
- Client-side MRTR / response-cache gotchas: symforge embeds no MCP client.

**Report acceptance tests adopted into the spec's test surface:** 1, 2
(version-aware wire), 15 (strict metadata validation — NARROWED: modern/metadata-required path only; spec FR-309's legacy HTTP-200 JSON-RPC error semantics are authoritative for header-less requests), 16 (fallback
negotiation), 17 (roots adapter interop), 18 (mixed-major CI check).


## Round-3 source correction — 2026-08-03 (cloud Codex, rmcp 3.1.0 source access)

The protocol-latest-default row and the Verdict's advertisement claim are AMENDED: rmcp 3.1.0's
default `supported_protocol_versions()` returns `ProtocolVersion::KNOWN_VERSIONS`, which already
includes `V_2026_07_28` (verified in rmcp source: handler/server.rs:340, model.rs:181-187).
"Zero `ProtocolVersion` refs in src" therefore means the OPPOSITE of what this document first
claimed: the server would serve 2026-07-28 by default, and the required override is an
ALLOW-LIST FREEZE against future auto-advertisement under the `"3.1"` semver range — not an
activation step. `LATEST` = 2025-11-25 remains true but concerns only the alias. Also resolved
by the same source pass: `ToolRouter::call` returns `Result<CallToolResponse, ErrorData>`
(Q4 closed — no conversion in `call_tool`; `read_resource` maps
`ReadResourceResponse::Complete`); `StreamableHttpService::new` still requires the
session-manager argument (`LocalSessionManager` retained); Cargo feature names unchanged on
3.1.0; `with_stateless_protocol_metadata_required` defaults to `false`.
