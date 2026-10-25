# Comparison and impact review — 2026-10-25

Starting tree: `e09b88ed`, SymForge 11.10.1. The owner authorized implementation
of the Differential comparison findings and explicitly required server/embed
parity for every fix. No new MCP tool or dependency was added.

## Reviewed behavior

- Resolve immutable Git comparison endpoints before selecting paths or reading
  blobs. Three-dot comparisons use the merge base; ref-to-worktree comparisons
  pin one tree and retain separate optional commit provenance.
- Isolate compatible language families in the impact graph. Rust and C# each
  retain their own namespace; JavaScript/TypeScript, C/C++, and Java/Kotlin share
  their respective family namespaces. No compiler-level FFI inference was added.
- Attribute calls to the byte-containing symbol when line-based enclosing
  metadata selects an outer class on a shared declaration line.
- Preserve one deterministic shortest call chain, source spans, and resolution
  rationale. Evidence derives from the admitted captured publication and
  contains no source snippets. Both adapters share the 256 KiB chain cap.
- Apply the public embed byte bound to measured strings and rendered evidence;
  omit complete explanations with explicit truncation when necessary, retaining
  impact rows and risk totals. Legacy DTOs deserialize with unavailable evidence.

## Independent review and source seals

An independent read-only reviewer checked the complete changed `src/` diff,
including direct/aliased calls, macros, traits, inherent methods, registration,
and re-exports. No new bridge into dark lifecycle/runtime/server API was found.
The nine core source paths are:

- `src/embed/parity/changes.rs`
- `src/embed/parity/detect_impact.rs`
- `src/git.rs`
- `src/index_lifecycle/embed_changes.rs`
- `src/index_lifecycle/embed_detect_impact.rs`
- `src/index_lifecycle/guidance/changes.rs`
- `src/index_lifecycle/guidance/impact.rs`
- `src/live_index/graph.rs`
- `src/protocol/tools.rs`

The combined-feature lint gate also required test-only cleanup in
`src/embed/parity/host.rs`, `src/index_lifecycle/embed_query.rs`, and
`src/index_lifecycle/embed_remediation.rs`. The reviewer verified that the two
moved test modules retained their contents and that error/default assertions
retained their behavior. No production body changed in that cleanup.

The lead verified the old HEAD seals before recomputing the working tree. The
reviewer independently reproduced the final values using the gate's domain
prefix, sorted UTF-8 relative paths, CRLF-to-LF normalization, and little-endian
length framing:

| Set | SHA-256 | Files | Normalized bytes |
| --- | --- | ---: | ---: |
| Full source | `e099130d822e49d4d73ee4ec1610a0799625c3be174ac2eb1120a8c0e6473aaa` | 312 | 12311462 |
| Excluded runtime source | `6014f89238cb76cdbe061e91e7ef8a693b1cefbd06a149069aea92aaa690cb9a` | 89 | 2266500 |

Only those two source pins were refreshed. Source rosters, excluded paths,
hash domains, and gate assertions were preserved. CI routing and the explicit
paired-test commands are reviewed separately from these source seals.

## Verification at source approval

Baseline affected library tests passed 48/48; preventive gates passed 10/10.
The divergent-branch, cross-language, and same-line C# regressions failed before
their fixes. Final default/server focused checks passed 63/63. Actual isolated
MCP checks passed the Rust/C#/C++ and divergent-range cases.

The final paired run passed 5/5, including actual MCP-handler versus public
embed rendered equality for Git/provenance and multi-language impact cases,
and exact shared-cap output with all 150 ordered impact rows. The constrained
embed budget and shared goldens also passed. Embed clippy and formatting passed.
Remaining broad platform/build checks are tracked in
`tasks/differential-overlap-improvements.md`; this source review does not
substitute for those results.

## CI routing and parity-gate amendment

The owner requested Linux PR checks on five custom runners labeled `fresh` and
Windows checks on GitHub-hosted `windows-latest`. Seven existing Linux jobs and
one new paired-parity job now use `[self-hosted, linux, fresh]`; Windows and macOS
routing, workflow triggers, existing commands, and action pins were retained.

The added Linux gate runs the two actual server/embed tests with both features
enabled and checks that exactly two tests passed. The Windows gate uses
nextest's matching filter with `--no-tests=fail`. These tests would otherwise
not run in the separate default-server and embed-only cells.

Independent review confirmed the normalized CI fingerprint
`b2a549dab71ee282:18461` and unchanged release-workflow fingerprint. The command
census deliberately grows from 45 lines/36 distinct commands to 48/39, across
the same two workflows, for these two parity gates and the combined-feature
Clippy gate on Linux. Guard assertions,
source seals, excluded paths, and lifecycle caller rosters remain in place.
The Clippy invocation uses Cargo's `--keep-going` to collect diagnostics across
independent targets while retaining `-D warnings` and nonzero failure.

The full default-feature suite then caught a missing provenance footer in the
uncommitted MCP symbol-diff branch (3748 passed, one failed). That branch now
pins the HEAD tree once and uses the shared gated capture/render path, matching
embed. A focused assertion checks the resolved tree; the shared golden stays
unchanged. Independent review approved the repair and recomputed the full-source
seal above; the excluded-source seal did not change.
