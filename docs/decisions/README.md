# Architecture Decision Records (ADRs)

This directory captures decisions that shape SymForge's design — tool surface,
protocol boundaries, daemon contract, indexing model, and platform compromises.

## Why ADRs

SymForge has a long-lived public surface: 24 MCP tools, backward-compat aliases
in `src/daemon.rs`, and a tool-name registry in `src/cli/init.rs`. Many of the
"why is it like this?" questions — why a tool was consolidated, why an alias
must stay, why the daemon proxies the way it does — are not obvious from the
code alone. ADRs make those decisions discoverable so that future changes can
respect (or knowingly overturn) the constraints that produced the current
shape.

Write an ADR when:

- consolidating or splitting an MCP tool (creates a backward-compat alias
  contract that other agents depend on)
- changing the daemon proxy / `execute_tool_call` routing in `src/daemon.rs`
- removing or renaming an entry in `SYMFORGE_TOOL_NAMES`
  (`src/cli/init.rs:262-294`)
- changing the live-index query path in `src/live_index/query.rs`
- adding or dropping a tree-sitter language
- changing what client configs `symforge init` writes
- making a cross-platform compromise (Windows path handling, codegen flags,
  vendored crates — see `Cargo.toml [patch.crates-io]`)

Do **not** write an ADR for: routine bug fixes, internal refactors that
preserve the public tool surface, or release process steps (those belong in
`docs/runbooks/`).

## Existing design artifacts

These predate this index and are worth referencing from new ADRs:

- [`docs/architecture.md`](../architecture.md) — system overview
- [`docs/codex-integration-ceiling.md`](../codex-integration-ceiling.md) — the
  reasoning behind the current Codex integration ceiling; a strong example of a
  design-constraint document that an ADR would now formalize
- [`docs/release-process.md`](../release-process.md) — release mechanics
- [`docs/project_direction.md`](../project_direction.md) — strategic direction
- [`docs/provider_cli_runtime_architecture.md`](../provider_cli_runtime_architecture.md)
  — provider runtime architecture decisions

## Format (Nygard)

One file per decision: `NNNN-short-slug.md`, zero-padded sequential. Status
lifecycle: Proposed → Accepted → (Deprecated | Superseded by NNNN).

```markdown
# NNNN. <Decision title>

Date: YYYY-MM-DD
Status: Proposed | Accepted | Deprecated | Superseded by NNNN

## Context

What forces are at play? What problem is being solved? Cite the code that
makes this decision necessary (file paths, symbol names, line ranges).

## Decision

What we will do. State it as a present-tense directive, not a discussion.

## Consequences

What becomes easier and what becomes harder. Call out:
- Public surface changes (MCP tools, daemon aliases, SYMFORGE_TOOL_NAMES)
- Migration burden for clients
- New invariants future code must respect
```

## Index

| ADR  | Title                                                                 | Status   | Date       |
|------|-----------------------------------------------------------------------|----------|------------|
| 0001 | Tool Consolidation Contract and Backward-Compat Aliases               | Accepted | 2026-04-17 |
| 0002 | Parasitic Hook Integration, Not Tool Replacement                      | Accepted | 2026-04-18 |
| 0010 | Worktree-aware edit routing via `working_directory`                   | Accepted | 2026-04-18 |
| 0011 | Frecency bump policy: commitment tools only, never discovery          | Accepted | 2026-04-18 |
| 0012 | Edit-tool and Rank-signal Extension Points                            | Accepted | 2026-04-18 |
| 0013 | Coupling Signal Contract                                              | Accepted | 2026-04-18 |
| 0014 | Watcher-Subsystem Spawn-Blocking Discipline                           | Accepted | 2026-05-15 |
| 0015 | Project-config trust gating for `.symforge`                           | Accepted | 2026-05-19 |
| 0016 | Call-Time Capability Resolution                                       | Accepted | 2026-05-16 |
| 0017 | Local Tool-Call Analytics                                             | Accepted | 2026-05-19 |
