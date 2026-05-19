# SRTK14 Integrity Sidecar Scope Decision

Date: 2026-05-19

Status: deferred. No integrity sidecar implementation is authorized for the current `.symforge/` surface.

## ADR Scope

ADR 0015 already narrows trust coverage to project configuration inputs:

- `.symforge/config.toml`, if present.
- Files under `.symforge/config/`, if present.
- Later explicit project-config paths only when a later ADR or goal names them.

ADR 0015 also excludes volatile runtime artifacts from project-config trust coverage:

- `.symforge/index.bin`
- `.symforge/frecency.db`
- `.symforge/coupling.db`
- `.symforge/sidecar.port`
- `.symforge/sidecar.session`
- `.symforge/hook-adoption.log`
- `.symforge/tee/**`

Its integrity-sidecar rule remains binding: sidecars are deferred until SymForge adds executable or security-sensitive project-local behavior that needs a per-file tamper baseline.

## Current Source Status

- The worktree has no `.symforge/` directory, no `.symforge/config.toml`, no `.symforge/config/`, and no tracked `.symforge` files.
- `src/edit_safety/integrity.rs` does not exist. `src/edit_safety/` contains `mod.rs`, `tee.rs`, and `trust.rs`.
- `src/hash.rs` already exposes SHA-256 helpers, but SRTK14 does not use them for a new sidecar implementation.
- `src/edit_safety/trust.rs` implements project-config trust for only `.symforge/config.toml` and `.symforge/config/`.
- `src/protocol/tools.rs` adds LOG_ONLY or ENFORCE warning evidence only when project-config trust inputs exist.
- `src/cli/trust.rs` provides the selected operator control surface for project-config trust status, accept, and revoke.
- `src/sidecar/port_file.rs` writes runtime port, PID, and session files. They locate the running local sidecar and are cleaned up as runtime state.
- `src/live_index/persist.rs` writes `.symforge/index.bin` as a serialized index snapshot.
- `src/edit_safety/tee.rs` writes `.symforge/tee/**` recovery snapshots for edit safety.

ADR 0015's initial "current source status" bullets were written before SRTK06 and SRTK07. The current code now includes `src/edit_safety/trust.rs` and CLI/protocol warning integration. That drift does not authorize integrity sidecars because the implemented security boundary is the user-local project-config trust store, not per-file sidecar baselines for volatile runtime files.

## Decision

Do not implement hash sidecars now.

The current `.symforge/` behavior is runtime state, derived stores, sidecar locator files, project-config trust inputs, and edit-safety recovery snapshots. None of those files is an installed command-rewrite hook script or an executable project-local behavior comparable to RTK's hook risk. Adding hash sidecars for the current runtime files would create noise without a security boundary that the existing project-config trust store does not already cover.

Later integrity-sidecar implementation remains blocked until `.symforge/` behavior changes to include executable or security-sensitive project-local content that needs a per-file tamper baseline. If such a later task is authorized, keep its scope minimal and require:

- sidecar format `<hex_hash>  <filename>\n`;
- Unix `0o444` as a speed bump only, not a security boundary;
- status vocabulary `Verified`, `Tampered`, `NoBaseline`, `OrphanedHash`, and `NotInstalled`.

## RTK Boundary

This is a selective SymForge decision, not RTK bulk integration. SRTK14 imports no RTK runtime code, shell hooks, hook installers, command rewriting, Claude permission parsing, CLI output filters, OpenClaw plugin code, Homebrew formula code, HTTP telemetry, or new dependencies.

## Public Surface

SRTK14 introduces no new public type, port, route, migration, event, feature flag, environment variable, status enum, MCP tool name, alias, or response contract.
