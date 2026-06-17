# Feature Specification: SymForge Harness Onboarding & Config Hub (v8 8.1)

**Feature Branch**: `005-v8-harness-onboarding`

**Created**: 2026-06-16

**Status**: Draft

**Input**: Operator onboarding + OS-wide MCP harness config scan/apply with backups + first-run wizard; the "easy LLM/harness onboarding" pillar of the singular-server vision. Builds on `004` `symforge serve`.

## Overview

Make attaching any MCP harness to a running `symforge serve` a **zero-hand-editing** experience. SymForge discovers the MCP client config files already on the machine, and writes/refreshes a SymForge attach entry (the `004` serve URL + Bearer key) into the chosen ones — always backing up first, idempotently, with a dry-run preview. A first-run/post-update banner guides the operator to the attach (and, once `006` lands, the admin) URL.

Closes binding gaps **G-040** (first-run/post-update onboarding) and **G-041** (harness scan + config apply) from `docs/v8-gap-closure-plan.md`. Out of scope: the admin GUI itself (`006`), AAP panel (`007`), and per-harness distinct key issuance/rotation (`006`).

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Hands-free harness attach (Priority: P1)

An operator has SymForge serving on a host and wants their MCP client (Claude Code, Claude Desktop, Codex, Gemini CLI, Kilo Code, Cursor) to use it. They run one scan-and-apply command; SymForge finds the client's config, adds or refreshes the SymForge attach entry (serve URL + key), and the client connects on next start — the operator never opens or edits a JSON file.

**Why this priority**: This is the core "onboarding" value and the reason the feature exists. Without it the operator is back to hand-editing client configs — the exact friction the vision removes.

**Independent Test**: Against fixture client config files (a populated one, an empty one, one with a stale SymForge entry), run scan + apply and confirm each ends with a correct, well-formed SymForge attach entry pointing at the given serve URL + key.

**Acceptance Scenarios**:

1. **Given** a discovered client config with no SymForge entry, **When** the operator applies, **Then** a correct SymForge attach entry (serve URL + Bearer key) is added and the file remains valid for that client.
2. **Given** a config that already has a SymForge entry (possibly stale), **When** the operator applies, **Then** the entry is refreshed in place (no duplicate) to the current URL + key.
3. **Given** several installed clients, **When** the operator scans, **Then** each discovered client is listed with its current SymForge-attach status (absent / present-current / present-stale).

---

### User Story 2 - Safe by construction: preview, idempotent, restorable (Priority: P2)

Because this edits the operator's real client configs, every apply is reversible and predictable: a dry-run shows exactly what would change before anything is written; every write is preceded by a timestamped backup that can be restored; re-running apply with the same inputs changes nothing further.

**Why this priority**: Editing a user's editor/agent configs is high-trust. Safety (backup, dry-run, idempotency) is what makes the apply acceptable to run at all.

**Independent Test**: Dry-run a change and confirm no file is modified; apply and confirm a timestamped backup exists and restores the prior content byte-for-byte; apply twice and confirm the second apply is a no-op.

**Acceptance Scenarios**:

1. **Given** a pending change, **When** the operator runs dry-run, **Then** the planned additions/refreshes are reported and **no** config file is modified.
2. **Given** an apply, **When** it writes a config, **Then** a timestamped backup of the prior content is created first and a restore returns the file to its exact prior bytes.
3. **Given** an already-applied config, **When** apply runs again with the same serve URL + key, **Then** nothing changes (idempotent).

---

### User Story 3 - First-run / post-update guidance (Priority: P3)

After install/update or starting `serve`, the operator sees a one-time banner with the attach URL (and admin URL once `006` exists), with the option to open it; SymForge remembers that onboarding was shown so it does not nag on every subsequent run, but re-surfaces after a version update.

**Why this priority**: Discoverability — it turns "I installed it, now what?" into a guided path. Valuable but secondary to the scan/apply that does the actual work.

**Independent Test**: On a clean onboarding state, confirm the banner shows once; on the next run, confirm it does not repeat; after a simulated version change, confirm it re-surfaces.

**Acceptance Scenarios**:

1. **Given** a fresh onboarding state, **When** the operator runs install/update/serve, **Then** the attach banner (and browser-open offer) is shown and the shown-state is recorded.
2. **Given** onboarding already shown for the current version, **When** the operator runs again, **Then** the banner is not repeated.
3. **Given** a version change since last shown, **When** the operator runs, **Then** onboarding re-surfaces.

### Edge Cases

- A client is not installed (config path absent) → reported as not-found, skipped, never created in the wrong place.
- A config file is malformed/unparseable → reported clearly; not silently overwritten; backup-then-skip or refuse, never corrupt.
- A config encoded with a UTF-8 BOM or unusual whitespace → parsed and written back preserving the client's expected format.
- Apply interrupted mid-write → the backup guarantees recovery; no half-written config left as the live file.
- Two SymForge entries already present (user-made mess) → de-duplicated to one refreshed entry, reported.
- Serve URL is loopback vs. a LAN address → the entry written reflects exactly the operator-supplied attach URL + key; no guessing.
- Permission denied writing a config → reported with the path; other clients still processed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST discover the MCP client config files for a known set of harnesses on the host (reusing the client-path knowledge already in the existing init flow) and report, per client, whether a SymForge attach entry is absent, present-current, or present-stale.
- **FR-002**: The system MUST add a SymForge attach entry (the `004` serve URL + Bearer key) to a chosen client config when absent, preserving the file's required structure and validity for that client.
- **FR-003**: The system MUST refresh an existing SymForge entry in place (no duplicate) when present, and de-duplicate multiple pre-existing SymForge entries to a single current one.
- **FR-004**: The system MUST create a timestamped backup of a config file's prior content before modifying it, and MUST provide a way to restore that backup to the exact prior bytes.
- **FR-005**: The system MUST support a dry-run mode that reports planned changes without modifying any file.
- **FR-006**: Apply MUST be idempotent: re-running with the same serve URL + key produces no further change.
- **FR-007**: The system MUST handle absent, malformed, BOM-encoded, or permission-denied config files without corrupting them and without aborting the whole run — each target is reported and processing continues.
- **FR-008**: The system MUST be invokable from the CLI (a scan/apply command, e.g. `init --scan`) and MUST default to non-destructive behavior (scan/dry-run) unless apply is explicitly requested.
- **FR-009**: After install/update or `serve`, the system MUST show a one-time onboarding banner with the attach URL (and an offer to open it), and MUST record onboarding-shown state so it does not repeat until the next version change.
- **FR-010**: All config writes MUST be verifiable against fixture configs in tests, and MUST NOT require mutating the developer's real live harness configs to validate.

### Key Entities *(include if feature involves data)*

- **Harness target**: a known MCP client — id/name, config file path(s), config format/shape, and how a SymForge attach entry is expressed for it.
- **Harness registry**: the catalog of known harness targets and the per-client logic to detect/add/refresh the SymForge entry.
- **Attach entry**: the SymForge MCP server entry written into a client config — the serve URL and Bearer key (from `004`).
- **Backup**: a timestamped copy of a config file's prior content, mapped to its source path for restore.
- **Onboarding state**: persisted record of whether/when the onboarding banner was shown and for which version.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: From a clean state, one scan+apply leaves a target MCP harness correctly configured to attach to the running serve, with the operator editing **zero** JSON by hand.
- **SC-002**: 100% of config modifications are preceded by a restorable backup; a restore reproduces the prior file byte-for-byte.
- **SC-003**: Re-running apply with unchanged inputs results in **zero** further file changes (idempotent) and **zero** duplicate SymForge entries.
- **SC-004**: Dry-run modifies **zero** files while accurately reporting every change that a real apply would make.
- **SC-005**: A malformed or inaccessible client config never results in a corrupted live config and never aborts processing of the other clients.
- **SC-006**: The onboarding banner appears exactly once per version (shown-then-suppressed until the next version change).

## Assumptions

- Builds on `004`: `symforge serve` exists and provides the attach URL + Bearer key this feature writes into client configs.
- Reuses the existing client config-path knowledge and BOM-safe parsing in the current init flow rather than re-deriving it.
- **Single operator key** (from `004`) is applied across harnesses in this slice; per-harness distinct keys / rotation are a later GUI feature.
- The known-harness set matches the clients the current init flow already supports (Claude Code, Claude Desktop, Codex, Gemini CLI, Kilo Code, Cursor); adding more clients is incremental.
- Verification uses fixture config files + dry-run/backup inspection; the developer's live configs are never mutated by tests.
- Onboarding "version" is the SymForge build version; a change re-surfaces the banner.

## Out of Scope (later features)

- The admin web UI / GUI and its HTTP API + telemetry (`006`: G-037/039/042).
- AAP admin panel + presets (`007`: G-043/044).
- Per-harness distinct key issuance, rotation, and a hashed multi-key store (`006`).
- Any change to the STEL tool surface or new MCP tools.
