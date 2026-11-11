# Implementation Plan: Tool reliability, correctness floor

**Branch**: `feat/036-tool-reliability` | **Date**: 2026-11-11 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/036-tool-reliability/spec.md`

## Summary

Make every tool answer correct, complete for what was asked, and explicit about what kind of answer it is. Seven slices, each shippable alone and each landing its failing tests first: output budgets by section (A1), refusal of unknown and inapplicable arguments at decode time (A2), per-answer disk verification built on the existing freshen, with an operating-system response table (A3), one reference resolver with an exact-occurrence floor (A4a references, A4b dependents, A4c rename, impact and edit planning), and a six-kind outcome in new result metadata (A5). Decisions and code locations are in [research.md](research.md), which includes the findings of the design review of 2026-11-11.

## Technical Context

**Language/Version**: Rust (toolchain pinned by the repo; 1.99.1 as of this plan)

**Primary Dependencies**: rmcp (vendored), tree-sitter grammars, serde. One new crate: `serde_ignored`, to report arguments the decoder did not consume (research D1). Windows NT status lookup is a two-line `extern "system"` declaration, not a dependency.

**Storage**: none new; the in-memory index and its existing snapshot format are unchanged

**Testing**: `cargo nextest` (default features), `cargo test --no-default-features --features embed --lib`, the tool-correctness harness `scripts/verify-tools.cjs`, npm tests

**Target Platform**: Windows, Linux, macOS

**Project Type**: single Rust crate (library plus server binary) with an npm wrapper

**Performance Goals**: the per-answer disk check adds one `stat` per file used; a reference query on this repository stays under the current p95 plus 20%, measured by the existing `observed_refresh_gate_v1` bench style on the resolver

**Constraints**: tools/list stays within its byte budgets (full 63,500, core 30,000); embed and server lanes stay byte-equal where parity tests pin them; the source pin and workflow pins in `tests/preventive_runtime_dark_v11.rs` are refreshed by their own oracle after `rustfmt`; frozen tree `specs/020-repository-knowledge-index/` is not edited

**Scale/Scope**: about 40 tools; 6 consumers of the resolver; 38 functional requirements

## Constitution Check

| Principle | How this plan meets it |
|---|---|
| I. Reporting invariant | The feature is this principle applied to tool output: "complete" only when no site is unresolved; freshness stated only from files checked in this call; a refusal for every argument not honoured. Each new status line names what it observed in its task. |
| II. RED-first evidence | Every slice's first tasks are its failing tests, with the job id recorded. Each refusal test carries an accepting control in the same test (a valid call that succeeds). |
| III. Frozen contracts win | No frozen file is edited. The pinned result-status vocabulary (`tests/conformance.rs:800`) and the public `OutcomeClass` are left unchanged; the six-kind outcome is a new key (research D7). |
| IV. Verification gates | The full gate list is in [quickstart.md](quickstart.md): the serial suite, the embed gate, the bench smoke, release build and harness, npm. It runs per slice through Terminal Commander, one cargo process at a time. |
| V. Unrepresentable over checked | `CandidateSite` carries its class and rule in one enum so a site without a decision cannot be built; write paths take the `Confirmed` variant's payload, so a probable site cannot be passed to them; `Partial` cannot be constructed without its continuation; the "complete" line is rendered from a method that returns `None` when unresolved sites exist. |
| VI. Independent review | One adversarial review per slice PR, with a cfg sweep. A3 has `cfg(windows)` and `cfg(unix)` bodies; its Unix rows are unverified until Linux CI runs them and the PR says so. Every slice changes behaviour, so each pauses for the owner's approval before merge. |

Gate result: pass. No violation to justify.

## Project Structure

### Documentation (this feature)

```text
specs/036-tool-reliability/
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/tool-outcome.md
├── inputs/                      # sweep results and design notes of 2026-11-11
└── tasks.md                     # from /speckit-tasks
```

### Source Code (repository root)

```text
src/
├── protocol/
│   ├── tool_contract.rs         # NEW  A1: primary/optional sections; A2: conditions table, decode_strict
│   ├── mod.rs                   # refusal produced before RepeatKey::observe
│   ├── read_tools.rs            # input docs brought in line with the table
│   ├── tools.rs, edit_tools.rs  # handlers switch to sectioned budgets and the resolver
│   ├── edit_plan.rs             # accepts a path
│   └── result_status.rs         # A5: new symforge/outcome key; version 1 status filled from it
├── index_lifecycle/guidance/
│   ├── source.rs                # sectioned budget next to enforce_token_budget_flagged
│   ├── file_read.rs             # explicit ranges, line numbers, end-of-file note
│   ├── freshen.rs               # A3: size check, classified stat errors
│   ├── search_envelope.rs       # envelope from the files used
│   ├── symbol_context.rs, impact.rs
├── live_index/
│   ├── resolve.rs               # NEW  A4a: occurrence scan, CandidateSite, ResolvedSet, rules
│   ├── query.rs                 # find_references / find_dependents become views of resolve.rs
│   ├── single_file.rs           # A3: freshen_file_if_stale extended
│   └── store.rs                 # A3: in-memory indexed_at; fault-injection unit tests beside StableReadAccess
├── paths.rs                     # A3: existing full-error classifier extended
├── discovery/
│   ├── os_response.rs           # NEW  A3: table; NT status captured at the failure site
│   └── mod.rs                   # access_error_kind callers move to the classifier
├── daemon.rs                    # A2: decode_params uses decode_strict
├── edit_safety/rename.rs        # rename sites from the resolver
└── embed/parity/                # same guidance calls; goldens regenerated where text changes

tests/
├── fixtures/reliability/        # NEW  small crate with a known answer table
├── reliability_output.rs        # NEW  A1
├── reliability_contract.rs      # NEW  A2
├── reliability_freshness.rs     # NEW  A3 (outside edits, stale records; fault injection is unit-tested in store.rs)
├── reliability_resolver.rs      # NEW  A4 (subset law, rename writes only confirmed, ripgrep comparison)
└── reliability_outcome.rs       # NEW  A5
```

**Structure Decision**: the existing single crate. Three new source modules and one fixture crate; everything else is an edit to a file that already owns the behaviour.

## Slices

Each slice is one PR. A slice starts only when the one before it has passed its gates, because most of them edit `src/protocol/tools.rs`.

| Slice | Requirements | Depends on | Observable change |
|---|---|---|---|
| A1 Output | FR-024 to FR-032 | none | more content by default, numbered lines, map ordering |
| A2 Request contract | FR-001 to FR-004 | none | refusals for arguments ignored today |
| A3 Disk verification | FR-015 to FR-023, FR-037 | none | caveats leave clean answers; outside edits seen at once; health names files |
| A4a Resolver and references | FR-005 to FR-008, FR-010, FR-012, FR-013, FR-035 | A1 (listing shape), A3 (answers rest on fresh files) | more sites listed, classes shown |
| A4b Dependents | FR-009, FR-011 | A4a | false dependents removed, module declarations counted |
| A4c Rename, impact, edit planning | FR-008a, FR-014, FR-036 | A4a | rename applies confirmed sites that were "uncertain" before; never writes probable ones |
| A5 Outcome | FR-033, FR-034, FR-038 | A1 to A4 produce the cases | new metadata key on every result |

A1 opens by removing the input-size-derived output budget (research D4) and tracing the one cut whose cause is not yet located.

## Risks

- **Refusing unknown arguments breaks a client that sends extras.** The hooks and the npm wrapper in this repository are checked in A2; the release note names the change. Serde aliases and nested objects are honoured because detection happens at decode time.
- **Larger default answers cost tokens.** SC-008 bounds growth at 20% on the sweep tasks; sectioned budgets drop optional sections first, and the freshness and trust lines removed by A3 offset part of it.
- **The occurrence scan and the comment/string re-parse are slower than the reverse index on common names.** Re-parsing is limited to files with occurrences the index has no record for and cached per index generation; A4a measures a common name before any consumer switches, and stops at the hard cap with a `partial` outcome.
- **A rename that writes the wrong site corrupts code.** Only confirmed sites are written, the type system keeps probable sites out of the write path, and FR-036 tests it with a same-spelling method on another type.
- **Checking every in-scope file for empty and complete claims costs one `stat` per file.** Bounded by a file count, above which the answer says how many files it did not compare.
- **Embed parity.** Goldens move in A1 and A3. Each regenerated golden is listed with the reason in the commit.

## Complexity Tracking

No constitution violation to justify.
