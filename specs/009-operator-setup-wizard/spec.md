# Feature Specification: Operator Setup Wizard & In-Harness Admin Command

**Feature Branch**: `009-operator-setup-wizard`

**Created**: 2026-06-17

**Status**: Draft

**Input**: User description: "symforge-operator-setup-wizard — interactive cross-platform onboarding wizard + in-harness admin command (8.1 onboarding pillar, builds on 004 serve, 005 harness scan/apply/backup, 006 admin GUI). Take an operator from a bare `npm install -g symforge` to a configured, running SymForge with zero hand-editing, plus a `/symforge-admin` in-harness command that opens the running admin dashboard. Fix the default serve port colliding with local services."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Collision-free serve port (Priority: P1)

An operator runs `symforge serve` with no explicit address. Instead of always binding the fixed port that may already be taken by another local service, SymForge binds a port it has verified is free, so the server it starts is the server the operator actually reaches.

**Why this priority**: This is a live, reproduced failure — the fixed default port was already owned by another local service, so `serve` bound a dead second listener and the dashboard returned "not found" while looking superficially "started". It is small, foundational, and unblocks every server-mode flow below. Shippable alone.

**Independent Test**: Occupy the historical default port with a dummy listener, run `serve` with no explicit address, and confirm SymForge reports and binds a different, reachable port (a request to the dashboard on the reported port succeeds), with no silent dead-listener.

**Acceptance Scenarios**:

1. **Given** the historical default port is free, **When** the operator runs `serve` with no explicit address, **Then** SymForge binds a verified-free port and prints the exact reachable URL.
2. **Given** the historical default port is occupied by another process, **When** the operator runs `serve` with no explicit address, **Then** SymForge binds a different verified-free port (never the occupied one) and prints that URL — the dashboard is reachable on the printed URL.
3. **Given** the operator passes an explicit address, **When** that address is free, **Then** SymForge honors it exactly; **When** it is occupied, **Then** SymForge fails loudly with the conflict instead of binding a dead second listener.

---

### User Story 2 - Guided setup wizard (Priority: P1)

A new operator, having only run `npm install -g symforge`, runs a single guided command. It scans their machine, shows them what it found, asks a few plain questions, applies the choices safely (with restorable backups), starts the server if they chose one, and hands them a working dashboard link.

**Why this priority**: This is the headline of the onboarding pillar — it converts "binary installed but nothing configured, and the operator doesn't know the next command" into "configured and running in one guided flow". Delivers the core user value.

**Independent Test**: Run the wizard against fixture harness configs with scripted answers; confirm it reports the detected harnesses/OS/suggested-free-port, applies exactly the chosen harness entries (each modified file has a restorable backup, re-running adds no duplicates), and — for a server-mode answer — reports a running dashboard URL that is reachable.

**Acceptance Scenarios**:

1. **Given** a bare install, **When** the operator starts the wizard, **Then** it presents a read-only summary of the detected operating system, each known harness's state (not installed / configured / stale), any already-running server, and a suggested free port — having changed nothing yet.
2. **Given** the summary, **When** the operator chooses an installation type, which harnesses to configure, and a port (pre-filled with the verified-free suggestion), **Then** the wizard restates the exact actions it will take before performing them.
3. **Given** confirmed choices that include configuring harnesses, **When** the wizard applies them, **Then** every modified harness config has a restorable timestamped backup, the SymForge entry is present and current in each chosen config, and no duplicate entries are created.
4. **Given** a server-mode choice, **When** the wizard finishes applying, **Then** it has a server running on a verified-free port and displays both the dashboard URL and the attach URL, and offers to open the dashboard in the default browser.
5. **Given** a completed setup, **When** the operator re-runs the wizard, **Then** it detects the existing configuration and running server, offers to refresh rather than duplicate, and leaves a clean idempotent result.
6. **Given** an automation/test context, **When** the wizard is driven with pre-supplied answers (non-interactive), **Then** it completes the same flow without reading the terminal, network, or spawning a real browser.

---

### User Story 3 - In-harness admin command (Priority: P2)

While working inside their coding harness, an operator wants the SymForge dashboard without leaving the harness or remembering a command. They invoke an in-harness command (or a single CLI verb) and the dashboard opens — reusing the already-running server if there is one, or starting one if not.

**Why this priority**: A convenience that makes the dashboard a one-action affordance from where the operator already works. Valuable, but it depends on the server existing (US1/US2) and is not required for the core onboarding.

**Independent Test**: With a server already running on the remembered port, invoke the admin verb and confirm it opens/points to that same running dashboard (no second server). With none running, invoke it and confirm it starts one on a free port and opens it. Confirm the in-harness command file is installed for a harness that supports command files, and an equivalent affordance exists for one that does not.

**Acceptance Scenarios**:

1. **Given** a server already running on the remembered port, **When** the operator invokes the admin verb, **Then** it reuses that server and opens/points to its dashboard URL — it does not start a second server.
2. **Given** no server running, **When** the operator invokes the admin verb, **Then** it starts one on a verified-free port and opens/points to its dashboard URL.
3. **Given** configuration via the standard setup/init path, **When** a harness supports command files, **Then** a `symforge-admin` command is installed there that resolves to opening the running dashboard; **When** a harness does not, **Then** an equivalent protocol-native affordance is available that returns the dashboard URL.

---

### Edge Cases

- The suggested/explicit port becomes occupied between the scan and the bind (race) — the server picks the next verified-free port (default path) or fails loudly (explicit path), never a dead listener.
- No known harness is installed — the wizard still completes (server-only or "nothing to configure"), and says so plainly.
- A harness config is malformed or unreadable — the wizard reports that target as needing attention and continues with the others; it never corrupts a file or aborts the whole run on one bad target.
- A harness config carries a byte-order mark or unusual encoding — parsing handles it (reuses the existing BOM-safe path); the file is not garbled.
- The environment has no display / no default browser (headless, CI, container) — the wizard prints the URL and skips the open instead of erroring.
- The operator declines every action — the wizard exits having changed nothing, with a restorable state.
- A server is already running on a different port than remembered — the admin/setup flow reuses the running one and reconciles the remembered port rather than starting a duplicate.
- Re-running after a partial/aborted setup — idempotent: no duplicate config entries, no orphaned second server.

## Requirements *(mandatory)*

### Functional Requirements

**Collision-free serve port (US1)**

- **FR-001**: When started with no explicit bind address, the operator server MUST bind a port it has verified is free at start time, and MUST print the exact reachable URL it bound.
- **FR-002**: The operator server MUST NOT bind a second, non-serving listener on an already-occupied port; an occupied explicit address MUST surface a loud conflict error, and the no-explicit-address path MUST select a different free port instead.
- **FR-003**: An explicitly provided bind address MUST be honored exactly when free (no silent substitution).

**Setup wizard (US2)**

- **FR-004**: The system MUST provide a single guided setup entry point that first performs a read-only scan and reports: the operating system, the state of each known harness (not installed / configured-current / configured-stale / unreadable), whether a server is already running, and a suggested verified-free port — modifying nothing during the scan.
- **FR-005**: The wizard MUST let the operator choose an installation type among: in-harness (stdio) integration only; operator server with dashboard; or both.
- **FR-006**: The wizard MUST let the operator select which detected harnesses to configure and confirm or edit the bind port, pre-filled with the verified-free suggestion.
- **FR-007**: The wizard MUST apply an automatic authentication policy: a loopback bind requires no key; a non-loopback (network) bind requires the operator to supply or have the wizard generate a key (consistent with the existing secure-default refuse-to-start rule).
- **FR-008**: Before performing any change, the wizard MUST restate the exact actions it will take (which files, which server) and proceed only on confirmation (or pre-supplied answers in non-interactive mode).
- **FR-009**: When configuring harnesses, the wizard MUST reuse the existing scan + timestamped-backup + apply machinery so every modified config has a restorable backup and re-application is idempotent with no duplicate entries.
- **FR-010**: When a server mode is chosen, the wizard MUST start the operator server on the chosen verified-free port and display both the dashboard URL and the attach URL.
- **FR-011**: After starting the server, the wizard MUST offer to open the dashboard in the default browser, and MUST degrade to just printing the URL when no browser/display is available.
- **FR-012**: The wizard MUST persist the chosen installation type and port so later runs and the admin command reuse them.
- **FR-013**: The wizard MUST be re-runnable idempotently: detecting existing configuration and a running server, it offers refresh/no-op rather than duplicating entries or starting a second server.
- **FR-014**: The wizard MUST support a fully non-interactive path (pre-supplied answers) that performs the same flow without reading the terminal, opening a browser, or contacting the network.

**In-harness admin command (US3)**

- **FR-015**: The system MUST provide an admin command/verb that ensures a server is running — reusing one already running on the remembered port, otherwise starting one on a verified-free port — and then opens or returns the dashboard URL.
- **FR-016**: The standard configuration path MUST install an in-harness `symforge-admin` command for harnesses that support command files, and provide a protocol-native equivalent (returning the dashboard URL) for harnesses that do not.

**Cross-cutting**

- **FR-017**: All operator-facing interactions that touch the terminal, the network/port probing, the default browser, and server-process spawning MUST sit behind injectable seams so the entire flow can be exercised in tests with scripted inputs and without real terminal, network, browser, or process side effects.
- **FR-018**: Verification MUST NOT mutate the developer's or operator's real harness configs; all apply/backup behavior MUST be validated against fixtures.
- **FR-019**: The feature MUST be confined to the server-capable build; the network-free embed build MUST remain free of it and MUST continue to compile without server/network dependencies.
- **FR-020**: The dashboard and attach URLs the wizard/admin command reports MUST be the URLs actually bound and reachable (no advertised URL that does not serve).

### Key Entities *(include if feature involves data)*

- **Operator setup profile**: the persisted result of a completed setup — chosen installation type, last bound/preferred port, and auth posture — used to make re-runs and the admin command reuse prior choices.
- **Installation type**: one of {in-harness/stdio only, server+dashboard, both}; selects which actions the wizard performs.
- **Harness target**: a known MCP client (id, human name, config location, current state: not installed / configured-current / configured-stale / unreadable) the wizard can configure.
- **Server session descriptor**: the running operator server as seen by setup/admin — its bound port, dashboard URL, attach URL, and whether it is currently reachable.
- **Port candidate**: a TCP port being evaluated for the default/suggested bind — with a verified free/occupied status at evaluation time.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: From a bare install, an operator reaches a configured state (chosen harnesses set up and, for server mode, a reachable dashboard) through a single guided command answering at most a handful of prompts — no manual config-file editing required.
- **SC-002**: 100% of harness config files modified by the wizard have a restorable backup, and re-running the wizard produces zero duplicate SymForge entries.
- **SC-003**: Across repeated default-port starts while the historical default port is occupied, the server binds a reachable free port 100% of the time and never leaves a silent non-serving listener.
- **SC-004**: Invoking the admin command opens the same already-running dashboard in a single action when one is running, and otherwise starts one and opens it — verified to never create a duplicate server when one already serves the remembered port.
- **SC-005**: Every reported dashboard/attach URL is reachable on the port actually bound (no advertised-but-dead URL) in 100% of server-mode runs.
- **SC-006**: The complete flow is validated without mutating any real operator harness config (fixtures only), and the project's full verification gate plus the network-free embed build both pass.

## Assumptions

- The wizard ORCHESTRATES existing capabilities and does not reimplement them: it reuses the operator server (004), the harness scan/timestamped-backup/apply machinery and its encoding-safe config parsing (005), and the admin dashboard + its read API (006).
- "Open in the default browser" uses the operating system's default opener; in headless/CI/container environments the open is skipped and only the URL is printed (not an error).
- The dashboard is a server-mode (operator-server) capability; in-harness (stdio) integration provides the tools but not the dashboard — choosing dashboard access implies a server mode. This is transport-specific by design (consistent with the existing serve-only dashboard) and not a transport-parity regression for tool results.
- Default non-interactive answers, when unspecified: configure detected harnesses, both integration types, loopback bind (no key), on a verified-free port.
- Serve lifecycle for this slice is start-on-demand + reuse-if-running; an always-on background service / OS service unit is out of scope.
- The in-harness command targets command-file-capable harnesses first (e.g. a command file); harnesses without command-file support get the protocol-native affordance. The exact per-harness command-file format/support is confirmed during planning.
- Docker/network targets bind a routable address with a required key and report the in-container URL; the wizard advises the port mapping but does not manage container runtimes itself.
- Persisted setup state lives in the existing SymForge home/config location; it is operator-local convenience state, not an authoritative index.
- This feature is independent of the separately-tracked 8.0.1 fixes (update-command npm-stderr surfacing + path-scoped graceful holder-stop, and the tool-parameter enum schema inlining); those are not in scope here.
