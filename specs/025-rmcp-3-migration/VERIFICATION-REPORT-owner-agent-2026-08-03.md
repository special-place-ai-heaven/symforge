# RMCP 2.2.0 → 3.1.0: migration map

**Upgrade directly to 3.1.0.** Do not stop at 3.0.0: 3.1.0 fixes decoding of metadata-bearing `input_required` results, corrects version negotiation through `supported_protocol_versions`, and tightens validation of modern stateless HTTP metadata and headers. Those fixes are directly relevant to an MRTR-aware server.

Treat this as **two migrations**:

1. **Rust API migration:** make the code compile against the widened handlers, new enums, task API, renamed fields, and Rust 1.88.
2. **Protocol migration:** deliberately decide when to negotiate MCP `2026-07-28`, because changing the crate version alone does **not** automatically activate the modern protocol. `ProtocolVersion::LATEST` still points to `2025-11-25`. ([Docs.rs][1])

## Source separation

Your proposed division is correct:

* **Migration guide and MCP changelog/spec:** what changed, compatibility behavior, and why the protocol changed.
* **`docs.rs/rmcp/3.1.0`:** exact type names, variants, fields, constructors, and handler signatures.
* **RMCP 3.1.0 changelog only:** patch-level fixes and newly strict behavior between 3.0.x and 3.1.0.

That distinction matters because the migration guide’s MRTR section describes `CallToolResponse` mainly as `Complete | InputRequired`, while the final 3.1.0 Rust API also contains the Tasks-extension variant `Task(CreateTaskResult)`. For compile-time truth, the 3.1.0 rustdoc wins. ([Docs.rs][2])

---

## Prioritized migration map

| Priority        | Area             | 2.2.0 → 3.1.0 action                                                                                                                                                                               | Main gotcha                                                                                                                                                                 | Clear advantage                                                                                                         |
| --------------- | ---------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| **P0**          | Toolchain        | Build with Rust **1.88 or newer**. Update directly pinned `rmcp-macros` dependencies as well.                                                                                                      | A transitive crate may leave both RMCP 2.x and 3.x in the graph. Identically named types from different major versions are incompatible.                                    | Declared and CI-tested minimum Rust version. ([GitHub][3])                                                              |
| **P0**          | Manual handlers  | Change `call_tool`, `get_prompt`, and `read_resource` to return their new response enums. Existing completed results can be converted with `.into()`.                                              | Macro-generated handlers largely hide this; your manual `ServerHandler` does not. ([Docs.rs][4])                                                                            | Enables MRTR and Tasks without replacing the existing completed-result models.                                          |
| **P0**          | Result dispatch  | Add `ServerResult::InputRequiredResult`; also handle task results where relevant. Add `_` to matches because the protocol unions are now non-exhaustive.                                           | Adding only the MRTR arm may still fail because `#[non_exhaustive]` requires a wildcard outside RMCP. ([Docs.rs][5])                                                        | Future protocol additions should no longer repeatedly break downstream exhaustive matches.                              |
| **P0**          | Protocol version | Explicitly advertise/select `V_2026_07_28` when that is intended. Audit `supported_protocol_versions()` and client lifecycle configuration.                                                        | `ProtocolVersion::LATEST == V_2025_11_25`; crate 3.1.0 does not mean the connection uses MCP 2026. ([Docs.rs][1])                                                           | One binary can retain legacy interoperability while progressively enabling the modern protocol.                         |
| **P1**          | MRTR             | Model `input_required` as a normal intermediate result rather than an error. Preserve or seal `requestState`.                                                                                      | At least one of `input_requests` or `request_state` must be present. It is valid only for `tools/call`, `prompts/get`, and `resources/read`. ([Docs.rs][6])                 | Proper interactive approval, elicitation, sampling, and other multi-step operations without keeping an SSE stream open. |
| **P1**          | Tasks extension  | Replace the old experimental Tasks API with `CallToolResponse::Task`, `tasks/get`, `tasks/update`, `tasks/cancel`, and the new `TaskManager`.                                                      | There is **no compatibility shim** for the old `TaskMetadata`, `tasks/list`, `tasks/result`, `#[task_handler]`, or `OperationProcessor` design. ([GitHub][3])               | Official long-running-operation lifecycle with status, input, TTL, and cancellation.                                    |
| **P1**          | MRTR vs Tasks    | Choose one initial response path per tool invocation: completed result, MRTR, or task.                                                                                                             | A task-backed invocation must not return ordinary MRTR `InputRequiredResult`. Mid-task input goes through `TaskContext::request_input()` and `tasks/update`. ([Docs.rs][2]) | Separates short interactive retries from durable asynchronous work.                                                     |
| **P1**          | Streamable HTTP  | Rename `stateful_mode` to `legacy_session_mode`. Externalize any state needed between modern requests.                                                                                             | `2026-07-28` HTTP is always protocol-stateless; the legacy setting cannot turn modern sessions back on. ([GitHub][3])                                                       | Requests can be handled by any server instance without protocol-level sticky sessions.                                  |
| **P1**          | HTTP metadata    | Preserve modern request `_meta`, protocol version, `Mcp-Method`, `Mcp-Name`, and promoted parameter headers through proxies and test harnesses.                                                    | 3.1.0 is deliberately stricter. Incomplete hand-written requests that happened to pass under 3.0 may now be rejected. ([GitHub][3])                                         |                                                                                                                         |
| **P1**          | Wire snapshots   | Make snapshots version-aware for `resultType`.                                                                                                                                                     | Modern responses carry `"resultType":"complete"`; legacy responses omit it. One universal golden JSON file will be wrong. ([GitHub][3])                                     | Correct typed discrimination while preserving old-client compatibility.                                                 |
| **P2**          | Roots            | Keep `ListRootsResult` only in a clearly marked compatibility adapter; migrate new designs toward tool parameters, resource URIs, or server configuration.                                         | This is an **inherited deprecation since 2.0.0**, not a new 3.1 breaking change. There is no renamed replacement type. ([Docs.rs][7])                                       | Workspace scope becomes explicit and usable in stateless or multi-instance deployments.                                 |
| **P2**          | Model fields     | Update `Annotations.last_modified` consumers to handle `Option<String>` and `ToolResultContent.structured_content` consumers to handle arbitrary `serde_json::Value`. Use concrete metadata types. | Object-only assumptions and automatic RFC-3339 parsing may now be wrong. ([GitHub][3])                                                                                      | Lossless proxying, broader output schemas, array/primitive outputs, and schema-compliant timestamps.                    |
| **P2**          | Caching          | Decide whether to emit `ttlMs`/`cacheScope`; audit client stale-on-error and private-cache partitioning.                                                                                           | A client may return expired cached data as success when refresh fails unless configured otherwise. ([GitHub][3])                                                            | Lower `tools/list`, resource-list, and resource-read latency and load.                                                  |
| **Conditional** | Subscriptions    | Move modern subscriptions to `subscriptions/listen`.                                                                                                                                               | Modern HTTP does not use the old standalone GET stream, session ID, or automatic replay model. ([GitHub][3])                                                                | Transport-neutral, request-scoped notification streams.                                                                 |
| **Conditional** | OAuth            | Migrate to `AuthorizationRequest`, `resolve_metadata()`, and native boxed HTTP errors.                                                                                                             | Stricter issuer and discovered-resource behavior may expose previously tolerated identity-provider errors. ([GitHub][3])                                                    | Better OAuth error provenance, audience selection, and mix-up protection.                                               |

---

# The three mandatory entries

## 1. `InputRequiredResult` and the manual `ServerHandler`

The exact handler return-type migration is:

```rust
call_tool(...)     -> Result<CallToolResponse, McpError>
get_prompt(...)    -> Result<GetPromptResponse, McpError>
read_resource(...) -> Result<ReadResourceResponse, McpError>
```

Existing synchronous completion paths should normally become:

```rust
let result: CallToolResult = execute_tool(...).await?;
Ok(result.into())
```

The exact 3.1.0 response shapes are:

```rust
CallToolResponse =
    Complete(CallToolResult)
  | InputRequired(InputRequiredResult)
  | Task(CreateTaskResult)

GetPromptResponse =
    Complete(GetPromptResult)
  | InputRequired(InputRequiredResult)

ReadResourceResponse =
    Complete(ReadResourceResult)
  | InputRequired(InputRequiredResult)
```

All three enums are non-exhaustive, so consuming code needs a wildcard arm. The `Task` variant on `CallToolResponse` is especially easy to miss when reading only the MRTR portion of the migration guide. ([Docs.rs][2])

The model itself is:

```rust
InputRequiredResult {
    result_type: ResultType,
    input_requests: Option<InputRequests>,
    request_state: Option<String>,
    meta: Option<MetaObject>,
}
```

Use one of its constructors rather than a struct literal:

```rust
InputRequiredResult::new(Some(requests), Some(state))
InputRequiredResult::from_input_requests(requests)
InputRequiredResult::from_request_state(state)
result.with_meta(meta)
```

At least one of `input_requests` and `request_state` is mandatory. The state is opaque to the client and is echoed back on retry. ([Docs.rs][6])

### Manual `ServerResult` dispatch

Your result dispatcher should conceptually look like this:

```rust
match result {
    ServerResult::CallToolResult(done) => handle_completed_tool(done),
    ServerResult::InputRequiredResult(input) => handle_input_required(input),
    ServerResult::CreateTaskResult(created) => handle_created_task(created),

    // Other explicitly supported result types...

    _ => handle_unknown_or_future_result(result),
}
```

`ServerResult::InputRequiredResult` is a real, direct enum variant in RMCP 3.1.0. It is not nested inside `CallToolResult`, and it should not be treated as an error result. ([Docs.rs][5])

### Critical MRTR security gotcha

Do not put unsigned trusted state directly into `requestState`. The client is supposed to echo it unchanged, but a malicious or faulty peer can modify it. For stateless operation, either:

* use the RMCP `request-state` feature and `RequestStateCodec`,
* put only an opaque random handle in it and keep authoritative state in shared storage, or
* integrity-protect and expiry-bind your own serialized state.

The official migration guide explicitly treats echoed `requestState` as untrusted and provides an HMAC-based codec. ([GitHub][3])

### Client-side MRTR gotcha

High-level `RunningService` calls can automatically drive MRTR, with a default maximum of ten rounds. APIs ending in `*_once` return the intermediate response enum instead. This changes the operational meaning of a seemingly ordinary call: it may now involve several network requests and several client-side input operations. ([GitHub][3])

For tests and proxies, cover both:

```text
call_tool()       -> expects a final CallToolResult after MRTR is driven
call_tool_once()  -> may return Complete, InputRequired, or Task
```

---

## 2. `ListRootsResult` is deprecated, but not removed

The exact 3.1.0 model remains:

```rust
ListRootsResult {
    roots: Vec<Root>,
    meta: Option<MetaObject>,
}
```

It is both `#[non_exhaustive]` and deprecated since RMCP 2.0.0. Use:

```rust
ListRootsResult::new(roots).with_meta(meta)
```

rather than a struct literal. ([Docs.rs][7])

The migration requirement should say:

> Existing roots behavior remains supported in the compatibility lane. No new feature may introduce a hard dependency on roots. New workspace or project scoping must use explicit tool parameters, resource URIs, or server configuration.

There is **no** `ListRootsResultV2`, replacement enum, or mechanical rename. The protocol recommendation is architectural rather than syntactic. Roots, sampling, and logging are annotation-only deprecations in this release and continue to function during the deprecation window. ([Model Context Protocol Blog][8])

A good downstream arrangement is:

```rust
#[allow(deprecated)]
mod legacy_roots_adapter {
    // The only place allowed to expose or consume ListRootsResult.
}
```

That prevents `-D warnings` from forcing you to either remove compatibility immediately or suppress deprecations project-wide.

Also note the apparent contradiction is intentional: RMCP 3.1 still models roots-related operations for compatibility, including server-to-client input flows. “Deprecated” does not mean “unusable in 3.1.”

---

## 3. The Tasks extension and `task_manager`

The old 2.2 Tasks implementation and the 3.1 implementation share some concepts, but they are **not source-compatible**. The old generated `#[task_handler]`/`OperationProcessor` path was removed. The modern extension is server-directed: a client advertises support, and the server decides whether a particular `tools/call` completes synchronously or returns a task handle. `tasks/list` was removed; clients use `tasks/get`, `tasks/update`, and `tasks/cancel`. ([GitHub][3])

The crate now exposes a server-only `task_manager` runtime for SEP-2663. ([Docs.rs][9])

Its important public surface is:

```rust
TaskManager::new()

TaskManager::spawn(
    TaskOptions,
    FnOnce(TaskContext) -> TaskFuture,
) -> Task

TaskManager::get_task(&str) -> Result<DetailedTask, McpError>

TaskManager::update_task(
    &str,
    impl IntoIterator<Item = (String, serde_json::Value)>,
) -> Result<(), McpError>

TaskManager::cancel_task(&str) -> Result<(), McpError>

TaskManager::running_task_count() -> usize
TaskManager::shutdown()
```

The task operation ultimately returns:

```rust
TaskFuture =
    Future<Output = Result<CallToolResult, TaskExit>>
```

`TaskContext` provides:

```rust
task_id()
request_input(key, InputRequest)
set_status_message(message)
is_cancel_requested()
cancelled().await
```

`TaskExit` distinguishes cooperative cancellation from an actual MCP failure.

### TaskManager production gotchas

**It is process-local, not restart-durable.** Internally the bundled manager stores tasks in an `Arc<Mutex<HashMap<...>>>`. A server restart loses the task registry, and separate replicas do not see each other’s tasks. The source’s “durably observable” guarantee means the task is visible through `tasks/get` before `spawn()` returns—not that it is written to PostgreSQL, Redis, or disk.

That gives three deployment cases:

| Deployment                       | Built-in `TaskManager`                                                                     |
| -------------------------------- | ------------------------------------------------------------------------------------------ |
| Single-process stdio server      | Good fit                                                                                   |
| Single Streamable HTTP instance  | Usually acceptable, provided restart loss is acceptable                                    |
| Multiple stateless HTTP replicas | Insufficient by itself; use shared persistence or application-level task ownership/routing |

This matters because modern HTTP explicitly permits any request to land on any instance. A task created on replica A followed by `tasks/get` on replica B will otherwise look unknown. That conclusion follows from combining the protocol’s stateless routing model with the manager’s in-memory implementation. ([Model Context Protocol Blog][8])

Other task gotchas:

* Default task TTL is **300,000 ms**, and the suggested polling interval is **1,000 ms**.
* Expiration is swept opportunistically when manager methods are invoked; there is no permanent background sweeper.
* `ttl_ms: None` retains entries for the manager’s lifetime.
* Cancellation is cooperative. The operation must observe `cancelled()` or `is_cancel_requested()`.
* A task may still finish successfully after cancellation was requested if the operation continues.
* Input-request keys must remain unique over a task’s lifetime.
* Task status notifications are not yet wired through `subscriptions/listen`; clients currently need to poll `tasks/get`.   ([GitHub][3])

### MRTR input and task input are different protocols

This distinction deserves its own requirement:

| Synchronous MRTR                                                | Tasks extension                                      |
| --------------------------------------------------------------- | ---------------------------------------------------- |
| `tools/call` returns `InputRequiredResult`                      | `tools/call` returns `CreateTaskResult`              |
| Client gathers input and repeats the original request           | Client sends input through `tasks/update`            |
| State is carried in `requestState` and/or repeated request data | State lives in the task and its task context         |
| Best for short interactive workflows                            | Best for long-running, independently observable work |

Do not mix the two response mechanisms in one execution branch.

---

# Biggest hidden gotchas

## 1. You can compile successfully and still use the old protocol

This is the most counterintuitive issue. `ProtocolVersion::V_2026_07_28` exists, but `ProtocolVersion::LATEST` remains `V_2025_11_25`, and the ordinary `serve()` client lifecycle remains the legacy initialize flow. Modern clients should use `ClientLifecycleMode::Discover` or `Auto` with an explicit preferred `V_2026_07_28`. Servers should explicitly test the output of `supported_protocol_versions()`. ([Docs.rs][1])

## 2. Modern HTTP is architecturally stateless

Any state previously attached to an MCP session, handler instance, connection, or session ID needs review. Typical examples include:

* authenticated user state,
* active database transaction state,
* current workspace,
* pending approval state,
* task registry,
* temporary file ownership,
* per-session caches.

Use explicit handles and shared application storage where state must survive another request or another server instance. The protocol intentionally moved this state out of hidden transport sessions. ([Model Context Protocol Blog][8])

## 3. 3.1.0 is stricter than 3.0.x

This can expose problems in:

* hand-written integration tests,
* reverse proxies that strip MCP headers,
* custom clients that omit required request metadata,
* gateways that rewrite a method or URI without updating headers,
* server implementations advertising one version but negotiating another.

That strictness is both a migration risk and an advantage: invalid routing metadata is now rejected rather than silently trusted.

## 4. `CallToolResponse` has three outcomes

Do not write:

```rust
match response {
    CallToolResponse::Complete(result) => ...,
    CallToolResponse::InputRequired(input) => ...,
}
```

The 3.1.0 enum also has `Task`, and it is non-exhaustive:

```rust
match response {
    CallToolResponse::Complete(result) => ...,
    CallToolResponse::InputRequired(input) => ...,
    CallToolResponse::Task(task) => ...,
    _ => ...,
}
```

([Docs.rs][2])

## 5. Do not use direct struct literals for new non-exhaustive models

This affects more than matches. Types such as `InputRequiredResult` and `ListRootsResult` cannot safely be constructed externally with traditional struct literals. Use documented constructors and builders. ([Docs.rs][6])

## 6. `resultType` changes test output, not just Rust types

For a modern peer:

```json
{
  "resultType": "complete",
  "content": []
}
```

For a legacy peer, the SDK strips the discriminator. Maintain separate legacy and modern wire fixtures, or parameterize expected JSON by negotiated version. ([GitHub][3])

## 7. The automatic response cache can hide refresh failures

The client cache is useful, but its stale-on-error behavior can return an old value as `Ok(...)` after a failed refresh. For administrative, security-sensitive, or exact-consistency calls, disable stale-on-error. A connection shared across users also requires correct private-cache partitioning. ([GitHub][3])

## 8. Check the dependency graph for mixed majors

After changing Cargo dependencies, run:

```bash
cargo tree -d | rg 'rmcp|rmcp-macros'
```

If an adapter, transport wrapper, or server framework still depends on RMCP 2.x, you may end up with both versions. Then a value that displays as `rmcp::model::CallToolResult` in both places is still two unrelated Rust types. Upgrade or isolate the wrapper before writing conversion glue everywhere.

---

# Clear advantages of 3.1.0

## Modern-client compatibility without dropping legacy clients

RMCP 3.1 can model the MCP 2026-07-28 wire protocol while still omitting modern discriminators for older negotiated peers. This lets one server support a migration window instead of requiring a flag-day cutover. ([GitHub][3])

## Proper interactive tools through MRTR

Approval prompts, missing parameters, elicitation, sampling, and other server-requested input are now first-class intermediate results. They no longer need to be modeled as ad hoc tool errors or tied to a permanently open stream. ([Model Context Protocol Blog][8])

## A much cleaner long-running-task model

The official Tasks extension separates task creation, inspection, input, and cancellation. The server chooses when work becomes a task, and clients use a small lifecycle API rather than an unsafe global `tasks/list`. ([Model Context Protocol Blog][8])

## Easier horizontal HTTP deployment

The modern protocol no longer requires MCP session IDs or sticky protocol sessions. Provided your application state and task state are externalized correctly, requests can be distributed across ordinary HTTP infrastructure. ([Model Context Protocol Blog][8])

## Better gateway routing and observability

Method, name, selected parameters, protocol metadata, and standardized trace-context fields can be inspected by gateways without parsing the complete JSON-RPC body. This is useful for rate limiting, audit trails, routing, and OpenTelemetry correlation. ([GitHub][3])

## Better schemas and structured outputs

Outputs can now be arrays, primitives, unions, or other valid JSON Schema roots, and structured tool content may be any JSON value. You are no longer forced to wrap every useful output in an artificial object. ([GitHub][3])

## 3.1.0 specifically stabilizes the 3.0 transition

The metadata-bearing MRTR decoding fix is reason enough not to target plain 3.0.0. The stricter protocol metadata validation and corrected supported-version negotiation also remove ambiguity that could otherwise turn into intermittent client compatibility bugs.

For a synchronous stdio-only server, most performance and scaling advantages remain dormant until a client negotiates the modern protocol. The immediate payoff there is API currency, compatibility, MRTR readiness, and the 3.1 correctness fixes—not an automatic speed increase.

---

# Recommended implementation sequence

1. **Pin the migration branch to 3.1.0 and Rust 1.88+.**

   ```toml
   [dependencies]
   rmcp = { version = "3.1.0", features = ["server"] }
   ```

   Add `request-state` only when implementing MRTR state:

   ```toml
   rmcp = {
       version = "3.1.0",
       features = ["server", "request-state"]
   }
   ```

2. **Resolve duplicate RMCP versions** before changing application code.

   ```bash
   cargo tree -d | rg 'rmcp|rmcp-macros'
   ```

3. **Make the compile-only handler migration.**

   * Update three handler return types.
   * Convert existing completed results with `.into()`.
   * Add wildcard arms to non-exhaustive enum matches.
   * Add the explicit `ServerResult::InputRequiredResult` branch.

4. **Make wire tests version-aware.**

   Test both `2025-11-25` and `2026-07-28`, particularly `resultType`, modern `_meta`, and standard HTTP headers.

5. **Implement MRTR as a separate feature slice.**

   First retain existing complete-only behavior. Then add explicit `InputRequiredResult` paths, sealed `requestState`, round limits, and retry tests.

6. **Implement Tasks separately from MRTR.**

   Start with the built-in `TaskManager` for a single-process deployment. Do not claim restart or cross-replica durability unless you add external persistence.

7. **Contain deprecated roots.**

   Keep a compatibility adapter, but prohibit new roots-dependent business logic.

8. **Only then enable modern lifecycle/configuration by default.**

   This prevents protocol behavior changes from being mixed with basic Rust compilation fixes.

---

# Acceptance tests the spec should require

1. **Legacy completed tool call:** negotiated `2025-11-25`; no `resultType` on the wire.
2. **Modern completed tool call:** negotiated `2026-07-28`; `resultType: "complete"` present.
3. **Metadata-bearing MRTR:** deserialize and round-trip `InputRequiredResult` containing `_meta`; this protects the exact 3.1.0 regression fix.
4. **MRTR request-only case:** `input_requests` present, no `request_state`.
5. **MRTR state-only case:** `request_state` present, no `input_requests`.
6. **Invalid MRTR:** both absent; construction or validation must reject it.
7. **Tampered request state:** integrity verification must fail.
8. **Manual `ServerResult` dispatch:** completed, input-required, task, and unknown/future variant behavior.
9. **Task completion:** create → get working → get completed result.
10. **Task input:** create → task becomes `input_required` → partial/full `tasks/update` → completion.
11. **Task cancellation:** cooperative cancellation becomes cancelled; an operation ignoring cancellation may still complete.
12. **Task expiry:** default/custom TTL behavior and unknown task after eviction.
13. **Process restart:** explicitly prove built-in task state is lost, unless an external store is part of the design.
14. **Two replicas:** explicitly prove how a task created on instance A is resolved when `tasks/get` reaches instance B.
15. **Modern HTTP validation:** missing or mismatched protocol metadata/header is rejected.
16. **Protocol fallback:** modern discovery preferred, legacy initialization still works.
17. **Roots adapter:** deprecated roots behavior still interoperates, while core business logic does not depend on it.
18. **Dependency graph:** CI fails if RMCP 2.x and 3.x are both present unintentionally.

---

## Exact docs.rs 3.1.0 citation ledger

These are the pages the spec-kit document should attach to every named SDK type:

```text
https://docs.rs/rmcp/3.1.0/rmcp/handler/server/trait.ServerHandler.html

https://docs.rs/rmcp/3.1.0/rmcp/model/enum.CallToolResponse.html
https://docs.rs/rmcp/3.1.0/rmcp/model/enum.GetPromptResponse.html
https://docs.rs/rmcp/3.1.0/rmcp/model/enum.ReadResourceResponse.html

https://docs.rs/rmcp/3.1.0/rmcp/model/struct.InputRequiredResult.html
https://docs.rs/rmcp/3.1.0/rmcp/model/enum.ServerResult.html
https://docs.rs/rmcp/3.1.0/rmcp/model/struct.ListRootsResult.html
https://docs.rs/rmcp/3.1.0/rmcp/model/struct.ProtocolVersion.html

https://docs.rs/rmcp/3.1.0/rmcp/task_manager/index.html
https://docs.rs/rmcp/3.1.0/rmcp/task_manager/struct.TaskManager.html
https://docs.rs/rmcp/3.1.0/rmcp/task_manager/struct.TaskContext.html
https://docs.rs/rmcp/3.1.0/rmcp/task_manager/struct.TaskOptions.html
https://docs.rs/rmcp/3.1.0/rmcp/task_manager/enum.TaskExit.html
https://docs.rs/rmcp/3.1.0/rmcp/task_manager/type.TaskFuture.html

https://docs.rs/rmcp/3.1.0/rmcp/model/struct.CreateTaskResult.html
https://docs.rs/rmcp/3.1.0/rmcp/model/struct.GetTaskResult.html
https://docs.rs/rmcp/3.1.0/rmcp/model/struct.DetailedTask.html
```

The final migration verdict is therefore: **the upgrade is worthwhile, and the basic synchronous path is not difficult—but enabling MCP 2026-07-28, MRTR, or Tasks turns it into a lifecycle and state-management migration.** The safest path is to compile first with complete-only responses, then activate MRTR, Tasks, and modern stateless HTTP as separate, testable changes.

[1]: https://docs.rs/rmcp/latest/rmcp/model/struct.ProtocolVersion.html "https://docs.rs/rmcp/latest/rmcp/model/struct.ProtocolVersion.html"
[2]: https://docs.rs/rmcp/latest/rmcp/model/enum.CallToolResponse.html "https://docs.rs/rmcp/latest/rmcp/model/enum.CallToolResponse.html"
[3]: https://github.com/modelcontextprotocol/rust-sdk/discussions/969 "https://github.com/modelcontextprotocol/rust-sdk/discussions/969"
[4]: https://docs.rs/rmcp/latest/rmcp/handler/server/trait.ServerHandler.html "https://docs.rs/rmcp/latest/rmcp/handler/server/trait.ServerHandler.html"
[5]: https://docs.rs/rmcp/latest/rmcp/model/enum.ServerResult.html "https://docs.rs/rmcp/latest/rmcp/model/enum.ServerResult.html"
[6]: https://docs.rs/rmcp/latest/rmcp/model/struct.InputRequiredResult.html "https://docs.rs/rmcp/latest/rmcp/model/struct.InputRequiredResult.html"
[7]: https://docs.rs/rmcp/latest/rmcp/model/struct.ListRootsResult.html "https://docs.rs/rmcp/latest/rmcp/model/struct.ListRootsResult.html"
[8]: https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/ "https://blog.modelcontextprotocol.io/posts/2026-07-28-release-candidate/"
[9]: https://docs.rs/rmcp/latest/rmcp/ "https://docs.rs/rmcp/latest/rmcp/"
