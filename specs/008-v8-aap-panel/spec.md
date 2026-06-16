# Feature Specification: SymForge AAP Operator Panel & Presets (v8 8.1)

**Feature Branch**: `008-v8-aap-panel` (campaign on `review/v8-004-operator-serve`)

**Created**: 2026-06-16

**Status**: Draft

**Input**: AAP-specific operator convenience — admin AAP panel + AAP-aware harness presets. Closes gap **G-044** (E6-E9, A9 from `docs/v8-aap-integration.md`). Builds on `006` admin GUI + `005` harness hub. (G-043 AAP embed retained + G-045 embed isolation are already satisfied and verified clean at every checkpoint.)

## Overview

When SymForge runs next to an Agent Army Professionals (AAP) checkout, the operator gets AAP-aware convenience instead of being forced through the generic MCP-client setup: the admin GUI gains an **AAP panel** (detect the sibling repo, show whether AAP uses the embed and/or MCP path, compare the embedded `symforge` crate version against AAP's `Cargo.lock` pin and warn on drift, list AAP-indexed roots) and the onboarding flow gains **AAP presets** (embed-only vs `serve`-URL) that never clobber AAP's embed path dependency. This is **operator convenience** (gap-plan "8.1 convenience"), not a blocker, and requires **no changes to the AAP repo** — it is symforge-side detection + presentation.

Out of scope: changing the AAP repo or its adapter; the E4 "future" items (shared serve instance, vsock proxy); any change to the `embed`/`server` feature isolation (already enforced).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - AAP panel in the admin GUI (Priority: P1)

An operator running SymForge beside an AAP checkout opens `/admin` and sees an AAP panel that detects the sibling AAP repo, shows the integration mode (embed active / MCP URL configured / both), and compares the SymForge crate version against AAP's pinned `symforge` version — warning if they have drifted.

**Why this priority**: The version-drift warning + integration-mode visibility is the core operator value (it prevents the silent embed-pin drift that breaks AAP). Without it, the AAP integration is invisible to the operator.

**Independent Test**: Against a fixture sibling dir containing an AAP-shaped `Cargo.lock` pinning a `symforge` version, load the AAP panel (or its `/api/v1` endpoint) and confirm it reports detected=true, the integration mode, the two versions, and a drift flag when they differ.

**Acceptance Scenarios**:

1. **Given** a detected sibling AAP repo whose `Cargo.lock` pins symforge X, **When** the operator views the AAP panel, **Then** it shows AAP detected, the pinned version X, the running crate version, and a drift warning iff X != running.
2. **Given** no sibling AAP repo, **When** the operator views the AAP panel, **Then** it shows "AAP not detected" cleanly (no error), and the rest of the dashboard is unaffected.
3. **Given** a detected AAP repo, **When** the panel renders, **Then** the integration mode (embed / MCP-URL / both) is shown based on what is configured.

---

### User Story 2 - One-click AAP presets (Priority: P2)

The operator can copy ready-made AAP integration snippets from the panel: the **embed** `Cargo.toml` snippet (`symforge = { path = "../symforge", features = ["embed"] }`) and, when `serve` is running, the **MCP serve-URL** preset to register SymForge in AAP's MCP settings — without hand-assembling either.

**Why this priority**: Removes the remaining manual step for wiring AAP, but secondary to simply *seeing* the integration state (US1).

**Independent Test**: Request the presets for a detected AAP repo (with and without a running serve) and confirm the embed snippet is always offered and the serve-URL preset is offered with the correct URL/key only when serve is active.

**Acceptance Scenarios**:

1. **Given** a detected AAP repo, **When** the operator requests presets, **Then** the embed `Cargo.toml` snippet is provided (path + `features=["embed"]`).
2. **Given** a running `serve`, **When** the operator requests presets, **Then** an "AAP + serve URL" preset with the attach URL + key is provided.
3. **Given** the presets, **When** applied, **Then** they NEVER replace AAP's embed path dependency with a stdio-spawn config (the 7.x anti-pattern).

---

### User Story 3 - AAP-aware harness scan + onboarding banner (Priority: P3)

The harness hub (`005`) recognizes AAP as a special case rather than a generic MCP-client JSON: it detects the AAP workspace root (env `AAP_ROOT` or sibling path) and offers AAP-appropriate presets (embed-only default, or HTTP MCP), and the first-run banner mentions both `/admin` and the AAP embed path when a sibling AAP repo is detected.

**Why this priority**: Polishes the onboarding for AAP users; valuable but after the panel + presets that deliver the core value.

**Independent Test**: With a fixture AAP root present, run the harness scan and confirm AAP appears as its own entry (not mis-scanned as a Cursor/Claude JSON), with embed-only and HTTP options; confirm the banner text mentions the embed path when AAP is detected.

**Acceptance Scenarios**:

1. **Given** an AAP root (via `AAP_ROOT` or sibling), **When** the harness scan runs, **Then** AAP is listed as a distinct, AAP-typed entry (not treated as a generic MCP JSON file).
2. **Given** AAP detected, **When** onboarding shows the banner, **Then** it references both `/admin` and the AAP embed path.
3. **Given** an AAP entry, **When** a preset is applied, **Then** the embed path dep is never overwritten with a stdio spawn config; any write is backed up first (reuses `005` backup).

### Edge Cases

- Sibling AAP repo present but `Cargo.lock` missing/unparseable → panel shows "detected, pin unknown" rather than crashing.
- `AAP_ROOT` set but path absent → not-detected, reported cleanly.
- Both embed and MCP configured → integration mode shows "both".
- AAP repo detected but no symforge pin in its lock → drift comparison shows "no pin found", no false drift warning.
- Multiple candidate sibling paths → deterministic precedence (`AAP_ROOT` over `../Agent_Army_Professionals`), documented.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST detect a sibling AAP repo via `AAP_ROOT` (if set) else a conventional sibling path, and report detected / not-detected without error in either case.
- **FR-002**: When AAP is detected, the system MUST read AAP's `Cargo.lock` to find the pinned `symforge` version and compare it to the running crate version, flagging drift when they differ (and "pin unknown" when not found).
- **FR-003**: The admin GUI MUST present an AAP panel showing detection state, integration mode (embed / MCP-URL / both), the pinned vs running versions, the drift flag, and any AAP-indexed roots; behind the same auth as the rest of `/admin`.
- **FR-004**: The system MUST provide AAP presets: the embed `Cargo.toml` snippet (always, when detected) and the serve-URL MCP preset (when `serve` is active) — and MUST NOT replace AAP's embed path dependency with a stdio-spawn configuration.
- **FR-005**: The harness scan (`005`) MUST treat AAP as a distinct AAP-typed target (detected via `AAP_ROOT`/sibling), not as a generic MCP-client JSON file, and offer embed-only vs HTTP presets.
- **FR-006**: When AAP is detected, the onboarding banner MUST mention both `/admin` and the AAP embed path.
- **FR-007**: Any config write for AAP MUST be backed up first (reuse `005` backup) and MUST be verifiable against fixtures without mutating a real AAP checkout.
- **FR-008**: All of this MUST remain behind the `server` feature; the `embed` build MUST stay free of it (G-045 invariant preserved).

### Key Entities *(include if feature involves data)*

- **AAP detection** — detected flag, resolved root path, source (env vs sibling).
- **Embed-pin comparison** — AAP-pinned symforge version, running crate version, drift flag (or "unknown").
- **Integration mode** — embed / MCP-URL / both / none.
- **AAP preset** — embed `Cargo.toml` snippet; serve-URL MCP preset (URL + key) when serve is active.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: With a fixture AAP sibling whose lock pins a different symforge version, the AAP panel/endpoint reports detected + the drift warning; with a matching pin, no drift.
- **SC-002**: With no AAP sibling, the AAP panel reports not-detected cleanly and the rest of `/admin` is unaffected.
- **SC-003**: The embed `Cargo.toml` snippet is always offered for a detected AAP; the serve-URL preset only when serve is active; an AAP embed path dep is never overwritten by a stdio config.
- **SC-004**: The harness scan lists AAP as a distinct AAP-typed entry, never mis-handled as a generic MCP JSON.
- **SC-005**: The embed build remains free of `server`/network code (verified by the existing embed-clean gate).

## Assumptions

- Builds on `006` admin GUI + `005` harness hub + `004` serve. No AAP-repo changes; detection is read-only against the sibling checkout / its `Cargo.lock`.
- Sibling resolution precedence: `AAP_ROOT` env, then a conventional sibling path (`../Agent_Army_Professionals`); documented + deterministic.
- Convenience feature (gap-plan "8.1 convenience"), non-blocking; embed (G-043) + isolation (G-045) are already satisfied.
- Verification uses fixture AAP-shaped directories (a `Cargo.lock` with a symforge pin); the developer's real AAP checkout is never mutated.

## Out of Scope (later / non-8.1)

- Any change to the AAP repo, its `aap-code-intel` adapter, or the embed API.
- E4 future items: a shared `serve` instance for AAP + multiple harnesses; vsock proxy to the guest agent.
- Writing into AAP's live backend DB (only fixture/config-template level here).
