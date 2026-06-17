# Feature Specification: SymForge Admin GUI (v8 8.1)

**Feature Branch**: `006-v8-admin-gui` (campaign on `review/v8-004-operator-serve`)

**Created**: 2026-06-16

**Status**: Draft

**Input**: Operator web UI on the serve process — configuration, economics stats, system diagnostics, API-key management. The "GUI for config + stats + diagnostics" pillar of the singular-server vision. Builds on `004` serve + `005` onboarding.

## Overview

A local operator web UI served by the same `symforge serve` process at `/admin`, backed by a versioned `/api/v1` on the same server. It lets an operator see and manage their SymForge instance without the CLI: live token-economics stats (from the durable ledger), system/process diagnostics, attached-harness status, and API-key management (mint/rotate/revoke against a hashed key store). Closes binding gaps **G-037** (operator web UI + `/api/v1`), **G-039** (hashed product API-key store + rotation), **G-042** (ops telemetry). This is the operator stack required for the **8.1.0** tag (O1-O8 in `docs/v8-admin-ui.md`).

Out of scope: AAP admin panel/presets (`008`, G-043/044); new STEL/MCP tools; the reference-quality program.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Operator dashboard renders live (Priority: P1)

An operator opens the admin URL printed by `serve` and sees, in the browser, a dashboard with their instance's live state: current token-economics summary (served vs. bypassed, net-vs-manual savings from the durable ledger), the active tool surface, attached-harness status, and basic system/process info — without touching the CLI.

**Why this priority**: The dashboard is the product's visible face of the economics promise and the headline "GUI" deliverable. Without it rendering with real data, the GUI pillar does not exist.

**Independent Test**: With `serve` running and some ledger activity, load `/admin` in a browser (authenticated) and confirm the dashboard renders real economics + system data (not placeholders/zeros), and that the underlying `/api/v1` endpoints return that data as structured JSON.

**Acceptance Scenarios**:

1. **Given** a running server with recorded economics, **When** the operator loads `/admin` with a valid session/key, **Then** the dashboard renders the real ledger summary (event counts, net-vs-manual) and the active surface profile.
2. **Given** the server, **When** an unauthenticated request hits `/admin` or `/api/v1/*` on a non-loopback bind, **Then** it is rejected (same auth contract as `004` `/mcp`).
3. **Given** the dashboard is open, **When** new economics events occur and the operator refreshes, **Then** the updated totals are reflected.

---

### User Story 2 - API-key management (Priority: P2)

An operator manages the server's API keys from the UI: see existing keys (by label/fingerprint, never the raw secret after creation), mint a new key, rotate, and revoke — backed by a store that keeps only hashed keys.

**Why this priority**: Operating a network server long-term requires key lifecycle (rotation/revocation), and it is the prerequisite for per-harness distinct keys. Secondary to the dashboard that proves the GUI works.

**Independent Test**: Mint a key via the UI/API, confirm it authenticates against `/mcp`; revoke it, confirm it no longer authenticates; confirm the store persists only a hash (raw secret shown once at creation, never retrievable later).

**Acceptance Scenarios**:

1. **Given** the key store, **When** the operator mints a key, **Then** the raw secret is shown exactly once and only a hash is persisted.
2. **Given** an active key, **When** the operator revokes it, **Then** subsequent `/mcp` attaches with that key are rejected.
3. **Given** the store, **When** listing keys, **Then** labels/fingerprints/created-times are shown but never the raw secret.

---

### User Story 3 - System & ops diagnostics (Priority: P3)

The operator sees diagnostics: host resource usage, SymForge process/PID + uptime, active sessions, and indexed projects — to answer "is it healthy and what is it doing?" at a glance.

**Why this priority**: Operational visibility; valuable but the dashboard (US1) and key mgmt (US2) deliver the core operator value first.

**Independent Test**: Load the diagnostics view and confirm it shows the live process/uptime, session(s), and indexed project(s) matching the server's actual state; `/api/v1/system` returns the same structured data.

**Acceptance Scenarios**:

1. **Given** the running server, **When** the operator opens diagnostics, **Then** the SymForge PID, uptime, active session(s), and indexed project(s) are shown and match reality.
2. **Given** `/api/v1/system`, **When** queried with auth, **Then** it returns structured host/process/session/index telemetry.

### Edge Cases

- Ledger store is Disabled (DB unavailable) → dashboard shows economics as "unavailable" rather than fake zeros or an error page.
- No economics activity yet → dashboard renders empty-state cleanly (not a crash/blank).
- API-key store unavailable → key management surfaces a clear degraded state; existing `--api-key` serve key still works (it is not stored hashed — it is the bootstrap key).
- Loopback-no-key dev mode → `/admin` follows the same auth rule as `/mcp` (open only when no key configured AND loopback).
- Browser cross-origin → `/api/v1` must apply the same Origin/Host protections discussed in the `004` review (P1-B), not be open to any web origin.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The server MUST serve an operator web UI at `/admin` and a versioned JSON API at `/api/v1/*` on the same `serve` process/port.
- **FR-002**: `/admin` and `/api/v1/*` MUST enforce the same authentication contract as `004` `/mcp` (Bearer/session; secure-by-default for non-loopback; loopback-open only when no key configured).
- **FR-003**: The dashboard MUST display the real durable-ledger economics summary (event counts, served/bypassed, net-vs-manual) and the active tool-surface profile; when the ledger store is unavailable it MUST show "unavailable", never fabricated values.
- **FR-004**: The system MUST provide an API-key store that persists only hashed keys, supporting mint (raw shown once), list (label/fingerprint/created, never raw), rotate, and revoke; revoked keys MUST be rejected at `/mcp` auth.
- **FR-005**: The system MUST expose `/api/v1/system` returning structured host/process/session/index telemetry (PID, uptime, active sessions, indexed projects, basic resource usage).
- **FR-006**: The UI MUST render correct empty-states and degraded-states (no activity, store unavailable) without crashing or blanking.
- **FR-007**: `/api/v1` MUST apply Origin/Host protections appropriate for a browser-facing surface (no open cross-origin access), consistent with the `004` security hardening.
- **FR-008**: The admin UI MUST be verifiable by an automated render check (headless browser) proving real data appears — not merely that the server returns 200.

### Key Entities *(include if feature involves data)*

- **Admin UI** — the operator-facing web app served at `/admin` (dashboard, keys, diagnostics views).
- **`/api/v1` endpoints** — structured JSON the UI consumes: economics summary, surface state, harness status, system telemetry, key management.
- **API-key record** — label, fingerprint, hash, created/rotated/revoked timestamps (never the raw secret).
- **Ledger stats view** — read-only economics summary derived from the durable `stel_ledger_events` store (`004`/`005`).
- **System snapshot** — PID/uptime/sessions/indexed-projects/resource usage at query time.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An operator loads `/admin` and sees their instance's real economics + system state rendered in the browser, verified by a headless-browser render check (real values present, not placeholders).
- **SC-002**: 100% of `/admin` + `/api/v1` requests on a non-loopback bind without valid auth are rejected.
- **SC-003**: A minted key authenticates at `/mcp`; a revoked key is rejected; the raw secret is never retrievable after creation (only a hash is stored).
- **SC-004**: The dashboard renders correct empty-state and ledger-unavailable state without error.
- **SC-005**: `/api/v1/system` telemetry matches the server's actual PID/sessions/indexed projects.
- **SC-006**: No open cross-origin access — a disallowed browser Origin is refused by `/api/v1`.

## Assumptions

- Builds on `004` (serve runtime, auth, durable ledger) and `005` (harness status). The admin server is the same process/port as `serve`.
- The `--api-key`/`--api-key-env` serve key from `004` is the bootstrap credential; the hashed multi-key store (G-039) is additive and managed via the UI.
- UI delivery approach (embedded static assets vs. a build step) is a plan-phase decision; the spec only requires it renders at `/admin` with real data and passes a headless render check.
- Verification uses a running `serve` + a headless browser / HTTP client against fixture-or-live ledger data; aligns with the project's render-evidence rule for frontend.
- O1-O8 acceptance detail lives in `docs/v8-admin-ui.md`; this spec realizes the dashboard/keys/diagnostics subset for 8.1.

## Out of Scope (later)

- AAP admin panel + presets (`008`: G-043/044).
- Multi-tenant/hosted dashboard, OAuth/SSO (explicit v8 non-goal).
- New STEL/MCP tools or reference-quality (H6/H8) work.
