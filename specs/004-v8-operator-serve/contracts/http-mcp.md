# Contract: `/mcp` Streamable HTTP endpoint

## Endpoint
`POST /mcp` (and the GET/stream half of MCP Streamable HTTP as the transport requires), served by `ServerRuntime`.

## Auth (FR-002..004)
- Header `Authorization: Bearer <key>`.
- If a key is configured: requests without a valid key → **HTTP 401**, no tool executes. Comparison is constant-time.
- If no key configured AND bind is loopback: auth skipped.
- Non-loopback bind with no key never reaches request handling — the server refuses to start (see cli-serve).

## Protocol
- Speaks MCP over Streamable HTTP (rmcp server transport). Supports `initialize`, `tools/list`, `tools/call`, notifications.
- `tools/list` returns the **active surface profile** (default compact-3).
- `tools/call` dispatches through the **same in-process `protocol::McpServer`** the stdio path uses → result parity (FR-005), no extra proxy hop (FR-006).

## Economics / BYPASS (FR-007)
- When STEL decides BYPASS, the tool result carries a **machine-readable** bypass signal (structured field, e.g. `do_not_retry_symforge_same_target` + cheaper-path hint), not prose only.
- Each served/bypassed call records one `stel_ledger_events` row (FR-010).

## Errors
| Condition | Response |
|-----------|----------|
| missing/invalid Bearer (key configured) | 401 |
| malformed JSON-RPC | JSON-RPC error envelope |
| tool error | JSON-RPC error / tool error result (same shape as stdio) |

## Acceptance
- Bearer-authenticated `tools/list` over `/mcp` == stdio `tools/list` for the same surface.
- A representative `tools/call` over `/mcp` == the stdio result for the same repo state (parity battery).
