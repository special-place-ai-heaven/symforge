# Quickstart: validating the correctness floor

Run long cargo commands through Terminal Commander, one at a time, `-j 4` (the constitution's cap; the machine limit in CLAUDE.md is 8).

## Per slice

1. The slice's new tests fail before its change (record the job id) and pass after.
2. Gates: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo clippy --no-default-features --features embed,__test-internals --lib -- -D warnings`; `cargo test --lib --bins --tests -- --test-threads=1` (the constitution's serial suite; `cargo nextest run --workspace` is the faster inner loop and is also run); `cargo bench --bench observed_refresh_gate_v1 -- --test`; `cargo test --no-default-features --features embed --lib -- --test-threads=1`; `cargo build --release`; `node scripts/verify-tools.cjs --bin <target>/release/symforge` and the same with `--fixture verify-tools-real`; `npm --prefix npm run test`; `git diff --check`.

## Scenarios against a release build

Fixture: `tests/fixtures/reliability/` (a small crate with a known answer table).

| # | Call | Expected |
|---|---|---|
| 1 | references of `target_fn`, scoped to its defining file | all sites in the answer table are confirmed, including the `super::` call; no "Unresolved" block |
| 2 | dependents of `leaf.rs` | lists `mod.rs` (module declaration); does not list the file importing `other::leaf` |
| 3 | rename dry run of `target_fn` | "will change" sites equal scenario 1's confirmed sites; the same-spelling method call on another type is under "left alone" |
| 4 | file content, lines 1-75 of a 300-line file, no budget argument | 75 numbered lines |
| 5 | file content, lines 1-20 of an 8-line file | 8 lines and "file ends at line 8" |
| 6 | repository map with defaults | `src/` entries present; with `path="src", depth=1` only `src/` one level deep |
| 7 | repository map with `dept=1` | `invalid_request` listing valid arguments |
| 8 | any query on file B while file A has a stale unreadable record | no freshness line |
| 9 | edit a file with another program so that it gains a match, search at once for that text | the new match is found (the file was not in the previous result) |
| 10 | symbol search "estimate tokens bytes" | symbols containing all three words, labelled as an all-words match |
| 11 | text search with no code match but documentation matches | `proven_empty` with the count of documentation matches and the call that shows them |

## Comparison runs

- Text: for each identifier in a sampled list, the resolver's occurrence set equals `rg -F --case-sensitive <identifier>` filtered by the resolver's own per-language identifier-boundary rule (not `-w`, which shares the blind spot for names such as `valid?` or `$el`), over the same files (repositories: this one and the benchmark's pinned set).
- Changes: `what_changed` path count equals `git status --porcelain` entry count.

## Acceptance re-run

The 13 failing tasks from `inputs/adoption-sweep-2026-11-11.md`, same prompts, contextless agent, release build of this branch installed as the MCP server. Record calls-to-answer and any built-in tool use per task.
