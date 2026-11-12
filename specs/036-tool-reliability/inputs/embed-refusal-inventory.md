# Embed refusal text inventory (spec 036)

Owner requirement: an LLM calling the embed lane (`HostRoom::dispatch_wire`) gets the same
bytes the MCP lane (`tools/call`) prints for the same call, in `HostRefusal.message` or
the reply's `rendered` text.

**Method.** Every query row below is one case of a parity test in
`tests/reliability_refusal_parity.rs` (`query_refusals_carry_the_mcp_text_on_the_embed_lane`,
`ambiguous_path_selectors_carry_the_mcp_text_on_the_embed_lane`,
`multi_file_answers_are_ordered_and_finished_alike`,
`a_facade_step_refusal_prints_the_step_text`): the same call on the same fixture
(`tests/fixtures/reliability`, plus the extra files each test writes) through a real MCP
stdio server and a real host room, bytes compared. A row marked `Same` asserts equality. A
`differs` row asserts that the lanes still differ, that the embed lane refuses with the
listed `kind/query_kind`, and that the MCP text starts with the listed prefix, so a drift
on either lane, or a change that closes the gap, fails the test. Before-columns come from
the RED runs (Terminal Commander jobs `job_01a1ce5cb14e772e8200a0ab0f193449`, slice 1, and
`job_01a1ce99870376c19ebcd16e2ac5fc35`, `job_01a1ce9bdc947289b9f41805b3633040`,
`job_01a1cea4dd4772bdb3ec461d2a732d0e`, `job_01a1cea62ec47264b8f9ab2dd411d387`, round 2).
`a_step_refusal_text_does_not_leak_into_a_later_refusal` pins that one refusal's text
never rides on a later refusal (facade and `ask`).

Status legend: **equal** (already byte-identical, now pinned), **fixed** (made identical
in this slice), **differs** (left, with the reason).

## Query lane (MCP tools with an embed `Query` request)

| # | Tool, call | MCP text (builder) | Embed before | Status | Embed text now comes from |
|---|---|---|---|---|---|
| 1 | get_file_context, missing path | `File not found: <path>` (`file_read::not_found_file`) | refusal NotFound, no text | fixed | same builder, `HostRefusal.message`, only when the publication records no disposition for the path (see row 1a) |
| 1a | get_file_context, metadata-only binary (`src/bin.rs`) | `Not indexed: src/bin.rs is Tier 2 (metadata only) ...` (protocol `admission_tier_file_degradation_for_path`) | refusal NotFound, no text | differs: admission-tier text not carried (its builder reads disk on the MCP side) | - |
| 2 | get_file_context, `max_tokens: 0` | outline (MCP treats 0 as no budget) | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 3 | get_symbol, missing path | `File not found: <path>. Use search_files ...` / `Did you mean: <five paths>?` (`not_found_file_with_suggestions` + `suggest_similar_files`) | `File not found: <path>` | fixed | both builders moved to `guidance::file_read`; batch `targets` keep the plain text, as MCP; the stem fallback now sorts before taking five (**MCP bytes change**: was hash-map order, a different five per process; row 67) |
| 4 | get_symbol, absent name in file | `No symbol ... Close matches ...` | same, `rendered` | equal | `symbol_read::symbol_detail_from_indexed_file_with` |
| 5 | get_symbol, bare name absent | `No symbol named ... in the index ...` | same | equal | `symbol_read::resolve_symbol_path_by_name` |
| 6 | get_symbol, bare ambiguous name | `Ambiguous symbol selector: 2 symbols named ...` | same | equal | same |
| 7 | get_symbol, `name: ""` | `No symbol  in ...` | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 8 | get_symbol, batch target missing path | `File not found: <path>` | same | equal | `file_read::not_found_file` |
| 9 | get_symbol, path + ambiguous name (dup.rs) | ambiguity text | same | equal | shared detail renderer |
| 10 | get_symbol_context, name absent everywhere | `Symbol "<name>" not found in index.` | same | equal | `rendered` |
| 11 | get_symbol_context, bare ambiguous name | `Ambiguous symbol selector: 2 symbols named ... Candidate paths:` | same | equal | `format_ambiguous_symbol_context` |
| 12 | get_symbol_context, path + absent name | `Symbol not found in <path>: <name>` + savings footer | refusal NotFound, no text | fixed | `find_exact_references_for_symbol` error + `compact_savings_footer`, `message` |
| 13 | get_symbol_context, missing path | `File not found: <path>` | refusal NotFound, no text | fixed | same lookup, `message` |
| 14 | get_symbol_context, bundle, path + absent name | `No symbol ... Close matches ...` + footer | refusal NotFound, no text | fixed | `context_bundle_result_view_with_max_tokens` + footer, `message` |
| 15 | get_symbol_context, path + ambiguous name (dup.rs), default and trace mode | `Ambiguous symbol selector for dup in src/dup.rs; pass `symbol_line` ... Candidates: 3, 8` | hand-written `... pass symbol_line ... Candidates: [3, 8]` | fixed | default: the exact-reference lookup text (`rendered`); trace: `trace_symbol_result_view`; no hand-written text remains |
| 16 | get_symbol_context, bundle + ambiguous (dup.rs) | bundle ambiguity text | (as 15) | fixed | bundle view text |
| 17 | get_symbol_context, `verbosity: "bogus"` | answers (ambiguous list) | refusal UnsupportedOption | differs: embed refuses a call MCP answers | - |
| 18 | get_symbol_context, `bundle: true` without path | `Error: bundle=true requires the 'path' parameter.` | refusal InvalidRequest, no text | fixed | `validation_text`, when no earlier check refused |
| 19 | get_symbol_context, `symbol_line: 0` | answers | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 20 | get_symbol_context, `sections: ["bogus"]` | answers | refusal UnsupportedOption | differs: embed refuses a call MCP answers | - |
| 21 | find_references, name absent | `No references found for ...` | same | equal | shared view |
| 22 | find_references, `kind: "bogus"` | answers (unfiltered) | refusal UnsupportedOption | differs: embed refuses a call MCP answers | - |
| 23 | find_references, missing path | `File not found: <path>` | refusal NotFound, no text | fixed | `find_exact_references_for_symbol` error, `message` |
| 24 | find_references, path + absent name | `Symbol not found in <path>: <name>` | refusal NotFound, no text | fixed | same |
| 25 | find_references, path + ambiguous name (dup.rs) | `Ambiguous symbol selector for ... Candidates: 3, 8` | refusal AmbiguousSymbol, no text | fixed | same |
| 26 | find_references, `name: ""` | answers (no references) | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 27 | find_dependents, missing path | `No file-level dependents found ...` | same | equal | shared |
| 28 | find_dependents, `format: "bogus"` | answers | refusal UnsupportedOption | differs: embed refuses a call MCP answers | - |
| 29 | inspect_match, line past EOF | `Line 999 is out of bounds for <path> (file has 8 lines).` | refusal InvalidRequest, no text | fixed | `symbol_read::inspect_match_result_view` (already rendered at that site), `message`; an InvalidRequest row, done because the text was already built there |
| 30 | inspect_match, missing path | `File not found: <path>` | refusal NotFound, no text | fixed | `file_read::not_found_file`, `message` |
| 31 | inspect_match, `line: 0` | `Line 0 is out of bounds for <path> (file has 8 lines).` | refusal InvalidRequest, no text | fixed | validation no longer refuses line 0; the lane refuses with the shared out-of-bounds view (as row 29) |
| 32 | get_file_content, missing path | `File not found: <path>` | refusal NotFound, no text | fixed | `file_read::not_found_file`, `message` |
| 33 | get_file_content, start > end | empty | empty | equal | - |
| 34 | get_file_content, `start_line: 0` | whole file | same | equal | - |
| 35 | get_file_content, start past EOF | `<path> [error: requested range ...]` | refusal InvalidRequest, no text | fixed | the shared renderer, then `file_read::finish_file_content` (hard cap, NUL warning, explicit `max_tokens`), the same function MCP's indexed and disk branches now call; an InvalidRequest row |
| 36 | get_file_content, `mode: "bogus"` | `Unknown mode 'bogus'. Valid modes: ...` | decode refusal text | differs: embed decodes `mode` as a closed enum | - |
| 37 | get_file_content, around_symbol absent | `No symbol NOPE in ... Close matches ...` | refusal NotFound, no text | fixed | shared renderer, `message` |
| 38 | get_file_content, around_match absent | `No matches for '<text>' in <path>` | refusal NotFound, no text | fixed | shared renderer, `message` |
| 39 | get_file_content, around_symbol ambiguous (dup.rs) | ambiguity text | refusal AmbiguousSymbol, no text | fixed | shared renderer, `message` |
| 40 | search_symbols, `kind: "bogus"` | `No symbols matching ...` | refusal UnsupportedOption | differs: embed refuses a call MCP answers | - |
| 41 | search_symbols, unknown language | `Unsupported language filter. Use one of: ...` | refusal UnsupportedOption, no text | fixed | `filters::parse_language_filter` error, `message` |
| 42 | search_text, empty query | `Search requires a non-empty query or terms.` | refusal InvalidRequest, no text | fixed | `search_render::text_search_error_text`, extracted from MCP's renderer |
| 43 | search_text, unknown language | language text | refusal, no text | fixed | as 41 |
| 44 | search_text, bad regex | `Invalid regex '(': ...` | refusal InvalidRequest, no text | fixed | as 42 |
| 45 | search_text, `group_by: "bogus"` | answers | decode refusal text | differs: embed decodes `group_by` as a closed enum | - |
| 46 | search_files, empty query | `search_files requires a non-empty `query` ...` | refusal InvalidRequest, no text | fixed | `validation_text` (only for an empty, not a blank, query, as MCP) |
| 47 | get_repo_map, `max_tokens: 0` | answers | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 48 | get_repo_map, absent path | `No entries under ...` | same | equal | shared |
| 49 | get_repo_map, `detail: "bogus"` | compact map | same | equal | shared |
| 50 | validate_file_syntax, missing path | `File not found: <path>` | refusal NotFound, no text | fixed | `file_read::not_found_file`, `message`; probed on the Current-publication `Query.Syntax` lane only; the Loading-phase `validate_syntax_from_disk` lane carries the same text unprobed |
| 51 | analyze_file_impact, missing path | `Status: not found under <root> ...` | same text, root spelled `\\?\C:\...` | fixed | `guidance::file_impact` now prints `dunce::simplified(root)`; MCP bytes unchanged (its root has no prefix) |
| 52 | explore, unknown language | language text | refusal, no text | fixed | as 41 |
| 53 | explore, empty query | `Explore requires a non-empty query.` | refusal InvalidRequest, no text | fixed | `validation_text` |
| 54 | edit_plan, absent target | `Target '...' not found.` | same | equal | shared planner |
| 55 | edit_plan, ambiguous target | `Found 2 symbol(s) ...` | same lines, order varies per process | fixed | shared planner, candidates now sorted by path and line; **MCP bytes change**: the order was hash-map order and differed between two MCP runs of the same call |
| 56 | what_changed, unknown ref | `Git change detection failed: <git error>` | refusal GitUnavailable, no text | fixed | the change listing's git error, `message` |
| 57 | diff_symbols, unknown ref | `Failed to run git diff: <git error>` | refusal GitUnavailable, no text | fixed | the comparison's git error, `message` |
| 58 | search_knowledge, empty query | `Error: query must be non-empty.` | refusal InvalidRequest, no text | fixed | `knowledge::search::validate_input`'s own error, `message` |
| 59 | ask, empty query | `query requires a non-empty question.` | refusal InvalidRequest, no text | fixed | `validation_text` |
| 60 | symforge facade (compact surface), `outline src/missing.rs` | step body `File not found: src/missing.rs` inside the facade answer | step body `Error: get_file_context refused: NotFound`, other token estimate | fixed | the step's refusal text is the step body; the old line only when a step has none |
| 61 | get_file_content, metadata-only binary (`src/bin.rs`) | file bytes + NUL warning (disk read) | same | equal | disk branch |
| 62 | get_file_content, indexed file whose NUL byte lies past the 8 KiB binary sniff (`src/nulbyte.rs`) | file text + NUL warning | same | equal | an earlier `src/nul.rs` fixture was refused (StalePublication) on Windows only: the stem `nul` is the reserved NUL device name there, see below |
| 63 | get_file_content, `src/nulbyte.rs`, start past EOF | `<path> [error: ...]` + NUL warning | same | equal | `finish_file_content` |
| 64 | get_file_content, `src/bin.rs`, start past EOF | `<path> [error: ...]` + NUL warning | error line without the warning | fixed | `finish_file_content` |
| 65 | get_file_content, `src/bin.rs`, start past EOF, `max_tokens: 5` | the budget-cut text | the uncut error line | fixed | `finish_file_content` |
| 66 | get_file_content, `src/short.rs`, start past EOF, `max_tokens: 5` | the budget-cut text | the uncut error line | fixed | `finish_file_content` |
| 67 | get_symbol, `lib/shorty.py` with eight `shorty_*.rs` | `Did you mean: <first five sorted>?` | a different five per process (both lanes) | fixed | row 3 |
| 68 | edit_plan, `Makefile` with `tools/a` and `tools/b` | first sorted file + `⚠ Ambiguous file target 'Makefile' matched 2 paths (...)` | last file in map order, no note (both lanes) | fixed | **MCP bytes change**: a bare target several files end with is picked sorted-first and the ambiguity is named, as the path-shaped branch already did |
| 68a | edit_plan, `x.rs` with `fox.rs` and `box.rs` | `Target 'x.rs' not found.` | `Found file: src/box.rs` + ambiguity note (both lanes) | fixed | **MCP bytes change**: a file target matches at a path separator only; the ambiguity note lists five paths, then `and N more` |
| 68b | search_text, invalid regex with a 301-character pattern | `Invalid regex '<first 80 chars>...': <regex error>` | the whole pattern echoed before the regex error (both lanes) | fixed | **MCP bytes change**: `text_search_error_text` caps the leading echo at 80 characters |
| 69 | get_symbol_context, trace mode, path + absent name | `No symbol NOPE in src/short.rs. Close matches ...` | refusal NotFound, no text | fixed | `trace_symbol_result_view`, `message` |
| 70 | find_references, `max_tokens: 0` | answers | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 71 | find_references, `limit: 0` | answers | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 72 | find_references, `max_per_file: 0` | answers | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 73 | find_dependents, `max_tokens: 0` | answers | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 74 | find_dependents, `limit: 0` | answers | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |
| 75 | find_dependents, `max_per_file: 0` | answers | refusal InvalidRequest | differs: embed refuses a call MCP answers | - |

Query totals: 78 rows; 17 equal; 42 fixed; 19 differ (18 embed refuses or decodes a call
MCP answers: rows 2, 7, 17, 19, 20, 22, 26, 28, 36, 40, 45, 47, 70-75; 1
admission-tier text: row 1a).

The dup.rs rows (9, 15, 16, 25, 39) were added after the RED run; their "before" column
is read from the code they ran through, not from a recorded run.

### Query refusals with no MCP counterpart (differ by design)

| Kind | Why there is no MCP text |
|---|---|
| BudgetTooSmall | host `QueryLimits.max_bytes` / `max_results`; MCP has no per-call byte limit. Truthful: `kind` only. |
| InvalidSpan | `u32` span conversion after a clamp; MCP clamps the same way and never fails. Not reachable from an MCP-shaped call. |
| AdmissionUnavailable for `project` | embed binds one source; MCP's `foreign_project_refusal` names a connection-wide project set that does not exist here. |
| StalePublication, SourceUnavailable, GraphPublicationLeaseExpired, StaleHandle, SessionRequired, ForeignSession, PolicyMismatch, OutputAdmissionRejected, Cancelled, DeadlineExceeded | host-protocol states (publication fences, sessions, transport admission, cooperative stop); the MCP lane answers these situations with its own index-loading texts, which describe a different lifecycle. |

## Not done in this slice (prioritised)

The query rows alone exceeded the slice cap, so these were inventoried by refusal kind,
not probed:

1. **Embed stricter than MCP** (rows 2, 7, 17, 19, 20, 22, 26, 28, 36, 40, 45, 47,
   70-75): a behaviour change (accept what MCP accepts), not a text change; with the
   owner.
2. **Admission-tier text** (row 1a): MCP builds it from a disk probe in `protocol`.
3. **`ask` routes**: a routed read's refusal carries no text (it is the routed tool's
   text, not what MCP `ask` prints for it); not probed against MCP `ask`.
4. **Edit refusals** (`EditErrorKind`, 22 kinds; `from_edit` gives `edit_kind` only, except
   `ProjectConfigTrustEnforced`): SymbolNotFound, AmbiguousSymbol, OccurrenceOutOfRange,
   TextNotFound, InvalidReplacement, ConflictingTargeting, FileNotAdmitted, InvalidPath,
   TargetFileMissing, StaleContent, StaleGeneration and the rest; MCP texts live in
   `protocol/edit_tools.rs` and `stel/edit_apply.rs`.
5. **Knowledge refusals** (`KnowledgeReviewError` 5, `KnowledgeCurationError` 8,
   `WireKnowledgeError::InvalidKind`): `knowledge_kind` only.
6. **Secret remediation refusals** (`SecretRemediationRefusalKind`, 18 kinds):
   `remediation_kind` only; the embed requests have no MCP tool of the same shape
   (`EMBED_REQUEST_TOOLS`: `None`).
7. **Federation refusals** (`FederationRefusalKind`, 5 kinds): no MCP counterpart call.
8. **Host refusals** (`HostRefusalKind::Denied`, `RequestTooLarge`, `ResponseTooLarge`,
   `InvalidConfig`, ...): room rights and wire limits; MCP has no equal situation.

## Side finding

With `src/dup.rs` added to the fixture, `find_references(name="NOPE_ANYWHERE")` ended
`indexed-file parse coverage incomplete.` on MCP and `... complete.` on the embed lane
and `get_repo_map` listed the key types `A`, `B`, `Thing` in a different order on the two
lanes (observed in one run each, causes not investigated). Neither is a refusal text; both
are outside this slice, and the dup.rs fixture lives in its own test so they do not mask
the refusal rows.

## Windows reserved device names (open)

On Windows a file whose stem is a reserved device name (`NUL`, `CON`, `PRN`, `AUX`,
`COM1`-`COM9`, `LPT1`-`LPT9`, e.g. `src/nul.rs`) is indexed and served by MCP (std paths),
but the embed lane's confined read (`physical_root::open_regular_beneath`, cap-std) finds
no regular file there, so `verify_indexed_content` (`embed_read.rs`) reports
`StalePublication`. Observed with `src/nul.rs`; Linux serves it. Left open: the read goes
through the cap-std confinement boundary, which needs a security review before it may
accept device-named paths. No parity row pins it.
