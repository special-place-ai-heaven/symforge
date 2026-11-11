# Contract: tool result outcome and refusals

Applies to every tool result on every surface (full, core, compact, embed).

## Metadata

New key `_meta["symforge/outcome"]`:

```json
{ "contract_version": 1, "outcome": "<kind>", "next_call": "...", "candidates": [], "reason": "...", "remedy": "..." }
```

`outcome` is one of `complete`, `partial`, `proven_empty`, `ambiguous`, `cannot_determine`, `invalid_request`. The other fields appear only for the kind that carries them.

The existing `_meta["symforge/result_status"]` is unchanged: contract version 1 and its six values, which `tests/conformance.rs` pins and `symforge::embed::parity::stel` re-exports. It is filled from the new outcome as: complete and partial -> `found`; proven_empty -> `empty_result` or `not_found` as today; ambiguous -> `ambiguous`; invalid_request -> `invalid_request`; cannot_determine -> `internal_failure` (an error result, so a version 1 client does not read it as empty).

## Text that accompanies each outcome

| Outcome | The text must contain |
|---|---|
| `complete` | the primary content, whole |
| `partial` | the primary content in relevance order, a count of what was left out, and one line `Next: <tool>(<arguments>)` that returns the rest |
| `proven_empty` | what was searched (file count, scope), what was not searched and why, and counts of matches in documentation, test or withheld files with the call that shows them |
| `ambiguous` | each candidate on its own line followed by the literal call that selects it |
| `cannot_determine` | the file, the classified cause, the raw operating-system code when there is one, and the remedy |
| `invalid_request` | the offending argument and either the full list of valid arguments or the mode that accepts it |

## Refusal examples

Unknown argument:

```text
invalid_request: get_repo_map does not accept "dept". Valid arguments: project, detail, include_knowledge, path, depth, max_files, estimate, max_tokens.
```

Argument outside its mode:

```text
invalid_request: "depth" applies when detail="tree"; this call set detail="compact". Call get_repo_map(detail="tree", depth=1) or drop "depth".
```

## Site listing shape (references, dependents, rename preview)

```text
<path>:<line>: <source line>            one per confirmed site, grouped by file
Probable (N):                            only when N > 0; same shape, with the rule
<path>:<line>: <source line>
Unresolved (N):                          only when N > 0
<path>:<line>: <source line>
Excluded: N in comments, M in strings    only when N + M > 0
Outside scope: K sites in J files        only when a scope removed sites
```

A completeness line is printed only when there are no unresolved sites and no unsearched source files.

A rename preview lists confirmed sites as "will change" and probable and unresolved sites as "left alone".

## Freshness

No freshness line when every file used by the answer was confirmed against disk. Otherwise one line per affected file: `Not confirmed against disk: <path> (<cause>)`. On an empty or complete claim made while the index reports lost observations: `Files created since the last scan may be missing; run index_folder to rescan.`

## Unknown arguments on the embed lane

The embed lane already refuses unknown fields through its own decoder. Its refusal text is brought to the form above so the two lanes stay byte-equal where parity tests compare them.
