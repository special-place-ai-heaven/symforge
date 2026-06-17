# Contract: Post-Edit Impact Footer

**Feature**: 007 · **Requirements**: FR-001..FR-004 · **Story**: US1, US5

## Grammar

```text
footer        := "[impact: " count " dependents" cochanges? "]"
count         := <non-negative integer>      ; distinct dependent FILE count
cochanges     := " · cochanges: " path-list
path-list     := path ("," " " path){0,K-1}  ; K = 3 by default
path          := <forward-slash relative path of a co-change partner>
```

Examples:

```text
[impact: 3 dependents · cochanges: src/protocol/format.rs, src/daemon.rs]
[impact: 3 dependents]
[impact: 0 dependents]
```

The footer is appended on its own, after any existing trust/stale/reroute
suffixes, separated by a single newline. It is plain text; the existing
machine-readable trust envelope is unchanged (FR-004).

## When present

Appended **only** on a successful structural mutation that actually writes:

- `replace_symbol_body`, `insert_symbol`, `delete_symbol`, `edit_within_symbol`
- `batch_edit`, `batch_rename`, `batch_insert` (the `Ok` arm only)
- `symforge_edit` **apply** path (inherits via the inner handler / `tool_body`)

## When omitted

- Any failed / rejected edit (NotFound, Ambiguous, InvalidRequest, write failure)
  — single-symbol tools early-return before the tail; batch tools omit in `Err`.
- `symforge_edit` `AlreadyApplied` branch (no fresh mutation, no inner handler).
- Dry-run / preview without a real write (no blast radius from an unwritten edit).

## Co-change clause rules

- Present only when `git_temporal().state == Ready` AND `co_changes` is non-empty
  for the edited file's path.
- Source: `GitFileHistory.co_changes` (strong, Jaccard ≥ 0.15, ≥ 2 shared
  commits). Never `weak_co_changes` (advisory).
- Truncated to the top K (default 3) partners, in the order `co_changes` already
  provides (pre-sorted by coupling).
- On `Pending`/`Computing`/`Unavailable`, the clause is omitted with no note (the
  footer degrades to `[impact: N dependents]`).

## Counting rules

- `N` = distinct dependent **files**, via
  `capture_find_dependents_view(path).files.len()` — NOT the raw per-reference
  count (which double-counts a file with multiple references).
- `N = 0` is rendered explicitly (`[impact: 0 dependents]`), not omitted.

## Forbidden content

Footer text MUST NOT contain any `classify_edit_output` sentinel substring:
`Error`, `unavailable`, `byte range`, `Write failed`, `[DRY RUN]`,
`Write semantics:`, `Ambiguous:`, `Symbol not found:`. (Prevents success→failure
misclassification in the `_tool` wrappers.)

## Test obligations

- Edit a symbol with 3 known dependents → footer reports `3 dependents`.
- Repo with co-change history → `cochanges:` lists the expected partner(s).
- Symbol with 0 dependents, no history → `[impact: 0 dependents]`.
- Failed edit → no footer line.
- Footer is identical on first apply and idempotency replay (append before
  `complete_mutation_replay`).
- `symforge_edit` apply success carries the footer; `AlreadyApplied` does not.
