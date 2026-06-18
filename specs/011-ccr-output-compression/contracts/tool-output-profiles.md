# Contract: tool output profiles (US2, US3, FR-009)

Static configuration — not operator-editable in v1.

## Profile table

| tool_name | ccr_eligible | default_max_tokens | preserve_error_lines | rank_group_by_file |
|-----------|--------------|-------------------|----------------------|-------------------|
| search_text | true | 8000 | true | true |
| search_symbols | true | 8000 | false | true |
| find_references | true | 8000 | false | false |
| explore | true | 12000 | false | false |
| get_repo_map | true (detail=full only) | 16000 | false | false |
| get_file_context | false | (adaptive verbosity) | false | false |
| get_symbol | false | (adaptive verbosity) | false | false |
| get_file_content | false | resolve_read_max_tokens | false | false |

## Resolution rules

1. Agent passes `max_tokens` → use agent value.
2. Agent omits → use `default_max_tokens` from profile when row exists.
3. `ccr_eligible=false` → never call `CcrStore::insert`; use
   `enforce_token_budget` or verbosity only.
4. `get_repo_map`: CCR only when `detail=full`; compact/tree use existing caps.

## Implementation location

`ToolOutputProfile` const slice in `src/protocol/ccr.rs` (or `format.rs` if
`ccr.rs` not yet created — move when module lands).

## Tests

Unit test: each discovery tool resolves expected default when `max_tokens` None.
