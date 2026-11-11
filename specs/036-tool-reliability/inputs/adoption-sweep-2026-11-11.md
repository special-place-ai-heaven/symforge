# Tool adoption tests (contextless sonnet subagent, told "use SymForge", tool not named)

Installed symforge binary, working tree = feat/core-tool-profile (dirty). ~145k tokens per agent.

| Intended | First tool used | Picked? | Calls | Problems |
|---|---|---|---|---|
| get_symbol | get_symbol | yes | 1 | none |
| find_references | find_references | yes | 1 | no test/non-test label on callers; name-only match ("uniqueness unproven") |
| search_symbols | search_symbols | yes | 4 | multi-word query "estimate tokens from bytes" -> "No symbols matching" (literal name match, no word split); error text suggested `explore`, agent did not follow; fell back to regex search_text |
| get_file_context | get_file_context (+find_dependents) | yes | 8 | default output budget-limited at ~680 tokens, outline cut, consumers/references sections omitted; sections=["consumers"] returned header only; find_dependents returned "No file-level dependents found" for src/protocol/repeat.rs although src/protocol/mod.rs uses it (mod decl + crate::protocol::repeat paths); search_text "N more omitted" and "40 in-scope files withheld by admission policy"; find_references with path scopes to that file only |
| conventions | conventions | yes | 1 | repo-wide counts only: no concrete error type / test layout to copy; unwrap/expect counts lump test and non-test code; agent answered mostly from CLAUDE.md |
| find_dependents | find_dependents | yes | 5 | ~20 FALSE POSITIVES for src/protocol/read_gate.rs: files importing a different module (crate::embed::parity::edit, index_lifecycle::guidance::read_gate) listed (path-substring/same-leaf-name heuristic); truncated "and 25 more references", "7 more files"; get_file_context cut at ~680 tokens, "...and 14 more" imports |
| search_text | search_text | yes | 4 | env var read goes through a const: first search hit only the const; follow_refs named the caller of the enclosing symbol, not the read site; "3 noise-filtered match(es) suppressed" hid test hits until include_tests=true |
| get_symbol_context | get_symbol_context | yes | 6 | `sections=[]` without path -> error "sections requires the 'path' parameter" (agent passed empty sections); output truncated at 376-token budget: "Callees (5):" header with NO names; find_references AND get_symbol_context both missed production caller src/protocol/deferred_stdio.rs:301 (only search_text found it) |

Cross-cutting, seen in most runs:
- "Source authority: index (UNVERIFIED against disk)" on results; "40 in-scope files withheld by admission policy"; freshness=Degraded (ObservationFailed, ReconciliationPending) -- on a dirty tree being edited.
- Default output budgets (376/680 tokens) truncate the very section the task needed; agents then make 3-6 extra calls.
- SymForge tools are deferred in Claude Code: every agent needed a ToolSearch first.

| get_repo_map | get_repo_map | yes | 4 | default output truncated at ~1000 tokens, alphabetical, ended at specs/016 and NEVER reached src/; passing path="src", depth=1 gave identical output (params ignored or nonexistent, no error); large "Repository knowledge" block is noise for an overview; agent fell back to search_files("mod.rs") + lib.rs |
| what_changed | what_changed (+diff_symbols, then git) | yes | 5 | reported "72 paths" where git status has 25 entries (untracked dirs expanded into files); agent cross-checked with git. (Agent's full report was replaced by the stop-gate recheck note.) |

| inspect_match | get_file_content + get_file_context | NO (never considered inspect_match) | 2 | get_file_content range output has no line numbers (agent counted lines); agent said "no single tool answers which symbol contains line N" -> inspect_match name/description not discoverable for "file:line -> enclosing symbol" |
| search_files | search_files | yes | 3 | default limit truncates (73 matches, 23 omitted); every hit scored flat [0.40] so ranking carries no information; no source/test grouping |
| health | health (+status) | yes | 2 | says 9 files `unreadable` but does not name them; "Fail-open outcomes: 2278 (sidecar errors 2190)" unexplained; health/health_compact/status overlap cost an extra call; status adds nothing about freshness |
| validate_file_syntax | validate_file_syntax x3 | yes | 3 | none (one file per call; no multi-path). Supervisor stop gate forced an extra python check |
| diff_symbols | diff_symbols | yes | 1 | none seen (+96/-9/~249); report replaced by stop-gate recheck so detail lost |
| search_knowledge | search_text first, then search_knowledge | 2nd | 4 | search_text(query="quarantine", glob="*.md") -> "No matches" though docs are full of it; note "did not scan 720 in-scope knowledge-only files; use search_knowledge" only after the zero; search_knowledge coverage=degraded overflow=2083, snippets cut mid-sentence -> agent used built-in Read for exact text |
| get_file_content | get_file_content | yes | 3 | range read does not say the file ended at line 8 (asked 1-20); no line numbers |
| analyze_file_impact | what_changed + find_dependents + find_references | NO (loaded its schema, never called it) | 6 | what_changed "Parse state: degraded (lexical symbol diff)": lists modified symbols without saying what changed; find_dependents returned 274 files for src/stel/surface.rs (unusable); agent: what_changed / diff_symbols / analyze_file_impact overlap |

Harness note: the supervisor stop gate refuses subagent handbacks ("no observed verification"), which adds a non-symforge call and sometimes replaces the report.

Batch 2 launched: explore, inspect_match, analyze_file_impact, detect_impact, diff_symbols, validate_file_syntax, search_files, search_knowledge, health, get_file_content.

| detect_impact | detect_impact (+find_references) | yes | 5 | rated all 26 blast-radius nodes "high" (no discrimination); depth 1; report replaced by stop gate |
| explore | search_text x6 + get_symbol + Read x2 | NO (never loaded explore) | 10 | concept question answered by keyword search; search_text context windows too short -> built-in Read twice; first search "214 more omitted" |

Batch 3 (edit tools) uses research/tool-adoption-scratch/caseN.rs (untracked, delete after).

Batch 3 (edit tools, scratch files):
| replace_symbol_body | edit landed correctly (first 4 calls lost to stop-gate rewrite) | unknown | 7 | none reported |
| edit_within_symbol | edit landed correctly, 2-byte change (first 3 calls lost) | unknown | 5 | none reported |
| insert_symbol | insert_symbol | yes | 4 | none |
| edit_plan | edit_plan | yes | 4 | edit_plan takes a bare name, no path: returned 8 matches across case1-8 and "8 call sites across the project"; suggested sequence never mentions path scoping |
| delete_symbol | delete_symbol | yes | 7 | none reported (detail lost to stop gate) |
| batch_insert | batch_insert (dry run, then apply) | yes | 6 | none |
| batch_edit | 3 edits landed in one file (tool list lost to stop gate) | unknown | 8 | none reported |
| batch_rename | batch_rename dry run, then abandoned for batch_edit + built-in Edit | picked, unusable | 12 | dry run would rename only the definition; all 10 call/import sites "Uncertain matches (NOT applied)" incl. the 2 in the same file and the explicit `use super::case5::calc_total`; find_references with path listed all 8 same-named defs and missed the import line; batch_edit cannot address an import line; search_text stale after a built-in Edit. CAVEAT: fixture had the same fn name in 8 uncompiled sibling files outside any module tree, which is harder than a real crate. |

Sweep stopped here by owner ("we need to fix this"). Untested: ask, investigation_suggest, context_inventory, health_compact, status, symforge_retrieve, symforge / symforge_edit facade, index_folder, checkpoint_now, review_knowledge, curate_knowledge, secret_remediate.
