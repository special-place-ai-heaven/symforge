# Feature Specification: SymForge Operator Server Spine (v8 8.1)

**Feature Branch**: `004-v8-operator-serve`

**Created**: 2026-06-16

**Status**: Draft

**Input**: User description: "SymForge operator server spine (8.1 slice) — `symforge serve` IP-attachable authenticated MCP endpoint, compact STEL surface as default, durable session-economics ledger. Server-spine slice of the singular-superior-server vision; load-bearing foundation the rest of 8.1 (onboarding hub, admin GUI, AAP panel) builds on."

## Overview

This is the **spine slice** of SymForge's "one singular superior server" vision. It delivers the load-bearing foundation: a single long-lived server an operator deploys once and any MCP harness attaches to over the network, a lean default tool surface, and economics that survive a restart. The operator GUI, harness-onboarding automation, and AAP panel are **separate later features** (005+) that depend on this spine — they are explicitly out of scope here.

This slice closes binding gaps **G-020, G-022, G-033, G-034, G-035, G-038** from `docs/v8-gap-closure-plan.md` plus the compact-surface-as-default flip, and advances the **8.1.0** release goal.

## Clarifications

### Session 2026-06-16

Resolved autonomously under the active goal (recommended engineering defaults applied, not blocking on input). Revisit any of these in a later clarify pass if you disagree.

- Q: API-key model for this slice — single static key vs. managed/hashed multi-key store? → A: **Single static operator-supplied key** (CLI flag and/or environment), constant-time comparison; hashed multi-key issuance/rotation is deferred to the operator-GUI feature (005+).
- Q: When an API key IS configured, is it required on loopback binds too? → A: **Yes** — if a key is configured it is required on all binds (including loopback); auth may be skipped ONLY when no key is configured AND the bind is loopback-only.
- Q: Durable economics-ledger store — extend the existing analytics database or a dedicated store? → A: **Dedicated SQLite database file** in the SymForge data directory (separate lifecycle from analytics), a new `stel_ledger_events` table, WAL mode + busy timeout, reusing the existing rusqlite persistence pattern.
- Q: Does the compact-3-default flip ship in this feature, and how is backward compatibility preserved? → A: **Ships here.** Default surface becomes compact-3; the existing `SYMFORGE_SURFACE` environment control gains an explicit `full` value as the documented backward-compatible opt-out (the `meta` profile is retained). The default change is the v8 surface cutover, not a silent break.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Attach a remote agent to a deployed server (Priority: P1)

An operator runs one command on the machine that holds a repository. A teammate (or an agent on another machine) points their MCP harness at that server over the network using a URL and an API key, and immediately gets the same code-intelligence answers they would get from a local install — with no per-machine indexing and no hand-edited stdio config.

**Why this priority**: This is the headline capability and the reason the spine exists. Without an authenticated network-attachable endpoint, none of the "singular server" vision is reachable. Every later feature (GUI, onboarding) assumes this exists.

**Independent Test**: From a second machine (or a second loopback client with auth forced on), attach to `http://HOST:PORT/mcp` with a Bearer key, run a representative code-intelligence request, and confirm the answer matches the local stdio path for the same repository state.

**Acceptance Scenarios**:

1. **Given** a server started and bound to a non-loopback address with an API key, **When** a remote harness connects to `/mcp` presenting the correct Bearer key, **Then** the MCP session initializes and tool calls return results equivalent to the local stdio surface.
2. **Given** the same server, **When** a harness connects to `/mcp` with a missing or wrong Bearer key, **Then** the connection is rejected and no tool call executes.
3. **Given** an operator on the repo host, **When** they run the single serve command, **Then** the process prints the attach URL and stays up as one long-lived server (no separate daemon/sidecar processes required for the MCP surface).

---

### User Story 2 - Lean default tool surface (Priority: P2)

An agent connecting to SymForge (over stdio or the new server) is presented with the compact 3-tool surface (`symforge`, `symforge_edit`, `status`) by default, instead of the legacy 32-tool list — so harnesses spend fewer tokens on the tool catalog and route through the economics layer by default. The legacy full surface remains available via explicit opt-out for clients that still need it.

**Why this priority**: The compact surface is the product's token-economics promise and the intended default of the v8 era. It is currently reachable only behind an environment variable, so the default experience does not match the vision. Independent of serve — it ships value on the existing stdio path too.

**Independent Test**: Start SymForge with no surface configuration set and confirm `tools/list` returns exactly the 3 compact tools; set the explicit full-surface opt-out and confirm the legacy surface returns.

**Acceptance Scenarios**:

1. **Given** a fresh install with no surface environment override, **When** a harness requests the tool list, **Then** exactly 3 tools are advertised (compact-3).
2. **Given** a client that sets the explicit full-surface opt-out, **When** it requests the tool list, **Then** the legacy surface is returned unchanged (backward-compatible escape hatch).

---

### User Story 3 - Economics survive a restart (Priority: P3)

The per-session token-economics ledger (what SymForge saved vs. a manual baseline, per accepted/bypassed call) is written to durable storage, so an operator who restarts the server still sees the accumulated economics and a later admin GUI can chart history. Today the ledger is in-memory only and is lost on every restart.

**Why this priority**: Durable economics is the data backbone the future admin GUI (feature 006) reads, and the honest proof-of-savings the product is built on. It is independently shippable and testable without serve or the surface flip.

**Independent Test**: Record several economics events, restart the server, and confirm the prior events are still present and totals are unchanged.

**Acceptance Scenarios**:

1. **Given** a running server that has recorded economics events, **When** the server is restarted, **Then** the previously recorded events and their session/accepted/bypassed totals are still retrievable.
2. **Given** the durable store is unavailable or unwritable, **When** the server runs, **Then** it degrades honestly (continues serving, surfaces the ledger as unavailable) rather than crashing or silently dropping events.

---

### Edge Cases

- **Non-loopback bind without a key**: server MUST refuse to start (fail loud), not bind insecurely.
- **Loopback bind**: auth is optional (local-only convenience); binding to `127.0.0.1`/`::1` may proceed without a key.
- **Port already in use**: serve fails with a clear, actionable message naming the address.
- **Backward compatibility of the surface flip**: existing clients that relied on the 32-tool default get a documented opt-out; the flip must not silently break them with no recourse.
- **Durable ledger locked/corrupt at startup**: server starts, marks economics persistence unavailable, keeps serving read/edit tools.
- **Restart mid-session**: an attached harness sees a clean disconnect and can re-attach; no partial/corrupt ledger rows.
- **Embed consumers**: the embeddable library build must not gain any server/network capability or dependency as a result of this feature.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST provide a single command that launches one long-lived server process exposing the MCP surface over a network transport compatible with standard MCP remote clients (Streamable HTTP), addressable as `http://HOST:PORT/mcp`.
- **FR-002**: The server MUST authenticate connections with an operator-supplied API key presented as a Bearer credential whenever a key is configured; requests without a valid key MUST be rejected before any tool executes. The key comparison MUST be constant-time. (Single static key for this slice; managed multi-key issuance is a later feature.)
- **FR-003**: The server MUST refuse to start when bound to a non-loopback address without an API key configured (secure by default).
- **FR-004**: The server MAY skip authentication ONLY when no API key is configured AND the bind is loopback-only; if a key is configured it MUST be enforced on all binds, including loopback.
- **FR-005**: Tool calls served over the network transport MUST return results equivalent to the local stdio surface for the same repository state (transport parity).
- **FR-006**: The network surface MUST be served by the same in-process runtime that owns the index, economics layer, and request governor — without an extra inter-process proxy hop on the request path.
- **FR-007**: When the economics layer decides to step aside (BYPASS), the response MUST carry a machine-readable bypass signal (structured, not prose-only) so harnesses can route to the cheaper path deterministically.
- **FR-008**: The system MUST advertise the compact 3-tool surface (`symforge`, `symforge_edit`, `status`) as the default when no surface override is set.
- **FR-009**: The system MUST provide an explicit, documented opt-out that restores the legacy full tool surface for clients that require it.
- **FR-010**: The session-economics ledger MUST be persisted to a dedicated durable store (a SQLite database in the SymForge data directory, separate from analytics, table `stel_ledger_events`, WAL mode) such that recorded events and totals survive a process restart.
- **FR-011**: When durable economics storage is unavailable, the system MUST continue serving and report economics as unavailable rather than failing the request path or silently discarding events.
- **FR-012**: The embeddable library build (no server feature) MUST remain free of server/network dependencies and gain no network capability from this feature.
- **FR-013**: All server-spine functionality MUST be verifiable without the published MCP harness binary — via direct process launch, HTTP client, and the repository test suite (the dev harness MCP stays pinned to the published release until this ships to the package registry).

### Key Entities *(include if feature involves data)*

- **Server runtime**: the single in-process owner of the index, economics layer, request governor, and authentication; transports (stdio, network) are thin adapters over it.
- **API key**: an operator-supplied credential gating non-loopback access. (Single static key for this slice; multi-key management and rotation are a later GUI feature.)
- **Bind address**: host + port the server listens on; loopback vs. non-loopback determines whether a key is mandatory.
- **Tool surface profile**: which tool catalog is advertised — compact-3 (new default) or legacy-full (opt-out).
- **Economics ledger event**: one durable record of a served/bypassed call and its measured token outcome vs. the manual baseline; rows accumulate across restarts.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator can stand up a network-attachable server with a single command, and a harness on a different machine completes a representative code-intelligence task against it with results equivalent to a local install.
- **SC-002**: 100% of non-loopback server starts without a configured key are refused; 0 unauthenticated non-loopback binds are possible.
- **SC-003**: After a server restart, recorded economics totals equal the pre-restart totals (zero economics loss across restart).
- **SC-004**: A fresh install with no configuration advertises exactly 3 tools (compact-3), not the legacy 32; the documented opt-out restores the full surface in a single setting.
- **SC-005**: The embeddable library build compiles with zero server/network dependencies and zero network capability.
- **SC-006**: Network-served tool calls match local stdio results on a shared verification battery (transport parity), with no economics double-counting between transports.

## Assumptions

- **Trunk**: work proceeds on `main` (currently the 7.x release train, codename v8 program); this feature targets the 8.1.0 milestone defined in `docs/v8-gap-closure-plan.md` §0.
- **Existing foundation reused, not rebuilt**: a working in-process HTTP server, request governor, and the STEL economics layers already exist in the repository; the spine promotes and unifies them rather than starting from scratch.
- **Single static API key** for this slice; managed multi-key issuance/rotation is deferred to the operator-GUI feature (005+).
- **Surface flip is gated for safety**: compact-3 becomes the default, but a backward-compatible opt-out to the legacy surface remains; the default change lands at the v8 surface cutover, not as a silent break to existing 7.x stdio users.
- **Verification model**: the SymForge MCP in the development harness stays pinned to the published package binary; this feature's 8.x-only capabilities are verified via direct process launch, an HTTP client, and the repository test/gate suite — not by live-driving the MCP daemon.
- **One active project per server session** remains the current product constraint (durable multi-root project switching is out of scope here).
- **Salvage is reference-only**: a prior prototype server runtime, admin shell, and transport spike exist on an archived branch; they inform design but are reimplemented against the current economics layer, not cherry-picked.

## Out of Scope (separate later features)

- Operator admin web UI, its HTTP API, and system/diagnostics telemetry (gaps G-037, G-039, G-042) — the GUI pillar.
- First-run onboarding and OS-wide harness config scan + apply + per-harness key assignment (gaps G-040, G-041) — the onboarding pillar.
- AAP admin panel and presets (gaps G-043, G-044) — note the embed feature-isolation invariant (G-045) is *upheld* here but its dedicated audit is later.
- Reference-quality program (H6/H8) and any new MCP tools beyond the compact-3.
