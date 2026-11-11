# Research: Tool reliability, correctness floor

Locations were mapped on branch `feat/036-tool-reliability` on 2026-11-11 and checked by an independent design review the same day. That review changed D1, D3, D5, D6 and D7; its findings are folded in below. Each slice re-checks its own locations before editing; line numbers move.

## D1. Where the unknown-argument check lives

- **Decision**: detect ignored keys at decode time, not by comparing with the schema. A helper `decode_strict::<P>(value)` wraps the JSON deserializer with the `serde_ignored` crate, collects every path serde did not consume, and turns a non-empty list into an invalid-request refusal that names the keys and lists the valid top-level arguments from the tool's schema (`self.tool_router.get(&tool_name)`, already used at `src/protocol/mod.rs:2275`). The helper is used at both decode points: the server lane, where rmcp's `Parameters<P>` decodes per handler (`vendor/rmcp/src/handler/server/wrapper/parameters.rs:46`), and the daemon lane's `decode_params` (`src/daemon.rs:7588`). The refusal is produced before `RepeatKey::observe` (`mod.rs:2243`) so a refused call never counts toward the repeat notice.
- **Rationale**: a top-level key comparison with the schema would refuse valid calls, because serde aliases (`symbol`, `body`: `src/protocol/edit.rs:352, 364, 398, 433, 463, 628, 1264, 1545`) do not appear in the schema's `properties`, and it would not see nested objects (`edits[]`, `targets[]`). Decode-time detection honours aliases, nesting and flattening by construction.
- **New dependency**: `serde_ignored` (dtolnay, small, no transitive dependencies beyond serde). It is the standard tool for exactly this and replaces hand-written key walking.
- **Server-lane mechanics**: the first task of the slice reads the vendored extractor and picks between a SymForge wrapper type used in handler signatures and a patch to the vendored `Parameters` (which would be recorded in `vendor/rmcp/SYMFORGE-PATCH.md`). The wrapper is preferred because crates.io installs ignore `[patch]`.
- **Daemon aliases**: `trace_symbol` and `detect_changes` are internal names resolved inside the daemon (`daemon.rs:7445, 7560`), downstream of the server seam; they are covered by the daemon-lane decode point, not by ordering.
- **Embed lane**: it already refuses unknown fields with serde's own message (`deny_unknown_fields` in `src/embed/parity/*.rs`). Its refusal text is brought to the same form and a parity test pins the two lanes.
- **Alternatives**: `deny_unknown_fields` on about 40 structs (rejected: per-struct, forgettable, does not list valid names, and conflicts with `flatten`).

## D2. Mode-conditional arguments

- **Decision**: a static table per tool of `(argument, applies when)` conditions, checked after decoding. When exactly one mode uses the argument and the caller did not choose a mode, that mode is selected; when the caller chose another mode, the call is refused naming the mode.
- **Known conditions**: `src/protocol/read_tools.rs` lines 162, 279, 281, 284, 361 and `src/protocol/tools.rs:416`. Two of the documents are already wrong: `path` on the repository map is documented as tree-only but also filters `detail="full"` (`tools.rs:2046`), and `max_files` is documented as full-only but also drives adaptive selection (`tools.rs:2030`). The table is written from the code, and a test compares the doc comments with it.
- **Documented empty values**: `get_symbol_context` publishes "empty array = all" for `sections` (`tools.rs:2547`) and enters trace mode on `sections.is_some()` (`tools.rs:2787`). That stays. The sweep failure there was the error demanding `path`; the fix is to that error, not to the empty value.

## D3. Candidate sites and the resolver

- **Decision**: a new module `src/live_index/resolve.rs` with its own occurrence scan over indexed file contents: case-sensitive, no result caps, byte offsets, every hit on a line. `search_text` is not reused: `TextLineMatch` has a line number and text but no column, several hits on one line collapse into one, defaults cap at 50 total and 5 per file, literal search is case-insensitive by default, and display lines are cut at 2,000 characters (`src/live_index/search.rs:632, 761, 820, 1145`).
- **Identifier boundaries** are defined per language family, because the generic `\b` rule (`search.rs:1706`) never matches names such as `valid?`, `$el` or kebab-case identifiers. Families: C-like (default), Ruby (`?`/`!` suffix), shell/PHP/Perl (`$` sigil), Lisp/CSS (hyphen).
- **Join**: each occurrence is joined to the index's `ReferenceRecord` (`src/domain/index.rs:1373`) by byte range. `line_range` there is zero-based (`:1383`); the join uses bytes to avoid an off-by-one.
- **Classes and rules**:
  - *Confirmed*: `ImportPath` (an import of the target's module path), `QualifiedPath` (a path that normalises through `self::`, `super::`, `crate::` and re-exports to the target), `ModuleDeclaration`, `IndexedReference` (a joined record the index already ties to the target symbol), `SameFileUnshadowed` (in the defining file, joined to a call or type-use record, with no binding of that name in the enclosing item).
  - *Probable*: `SameFile` without that evidence, `UniqueName` (one definition in the index), `UnnormalisedPath` (path evidence in a language with no normalisation rule yet).
  - *Excluded*: in a comment or string.
  - *Unresolved*: none of the above.
- **Why probable exists**: a name with one definition still collides with methods and fields of the same spelling on external types (`.parse()`, `new`). Writing those would corrupt code. Read tools show probable sites under their own label; write tools (`batch_rename`, anything applying an edit plan) change confirmed sites only and list the rest.
- **Exclusion source**: the index stores no syntax tree. Occurrences with a joined record are code by construction. For the remainder, the file is re-parsed with its tree-sitter grammar once per index generation and cached; only files that contain un-joined occurrences are parsed. The slice's bench measures this on a common name before the consumers switch.
- **Languages**: path normalisation is written for Rust in this phase. Other languages confirm through their indexed import records and are otherwise probable (FR-010).
- **Today's defects this replaces**: `find_references` with a `path` goes through `find_exact_references_for_symbol` (`query.rs:2199`), which returns only sites it can tie to the symbol and prints "full"; `find_dependents_for_file` (`query.rs:3221`) matches by stem (`matches_target_stem`, `query.rs:296`) and has no edge for `mod x;`.
- **Consumers**: `find_references` (`tools.rs:6308`), `find_dependents` (`tools.rs:6646`), `get_symbol_context` (`tools.rs:2570`, `guidance/symbol_context.rs`), `batch_rename` (`edit_tools.rs:2167`, `src/edit_safety/rename.rs`), `detect_impact` (`tools.rs:5603`, `guidance/impact.rs`), `edit_plan` (`tools.rs:6922`, `protocol/edit_plan.rs`), and the embed parity handlers that share the guidance layer.
- **Alternatives**: stack-graphs (archived 2025-09-09); a language-server tier (later phase; a floor is still needed when no server is present).

## D4. Output budgets

- **Cause, located**: `enforce_token_budget_flagged` (`guidance/source.rs:123`) is a no-op without a budget, but several handlers always pass one derived from the *input* size: `Some(resolve_read_max_tokens(None, raw_chars))` at `src/protocol/tools.rs:2357` (file context) and the same pattern at `tools.rs:1934, 6007, 6140`. `resolve_read_max_tokens` (`file_read.rs:715`) yields between 64 and about 1,000 tokens, so the output of a context call is capped by a number computed for a 50-line read. That fits the 680-token cut. The symbol-context cut is traced (T006, 2026-11-11). Default mode (no `sections`) passes no budget and returns every caller and callee (`src/index_lifecycle/guidance/symbol_context.rs:239` only enforces a caller-supplied `max_tokens`; the fixture test in `tests/reliability_output.rs` shows all five callees). Trace mode is different: when `sections` is present and knowledge is requested (an empty list means all sections), `max_tokens` defaults to 1,000 tokens (`src/protocol/tools.rs:2900-2903`; `:2841` does the same for knowledge-only), and `enforce_budgeted_code_context_with_knowledge` then halves the code budget in a loop until the code plus the knowledge summary fits (`src/index_lifecycle/guidance/knowledge_model.rs:417-431`), which leaves a code share of a few hundred tokens that is cut at a line boundary, hence a "Callees (5):" header with no names. A cut near 376 tokens is consistent with that loop; the exact figure was not reproduced, and the sweep agent may also have passed its own small `max_tokens`. The slice's fix is the sectioned budget (T007/T008): callers and callees are primary and are never cut before the optional knowledge and git sections.
- **Decision**: budgets are applied by section. Each tool declares its primary content and its optional sections in drop order. An explicit request (range, named symbol, named section) is not subject to the default; it is bounded only by the hard cap (`GET_FILE_CONTENT_MAX_BYTES`, 60,000 bytes, `file_read.rs:539`, and the per-tool profile in `compression.rs:23`). Past the hard cap the answer is partial with the literal continuation call.
- **Repository map**: entries ranked source roots first, then tests, then documentation and specs; `path` and `depth` honoured in every detail level.

## D5. Per-answer disk verification

- **Decision**: extend the freshen that already exists instead of adding a second one: `freshen_file_if_stale` (`src/live_index/single_file.rs:1064`) and `freshen_exact_path` (`src/index_lifecycle/guidance/freshen.rs:79`). Changes: compare size as well as modification time; stop returning `Fresh` when `stat` fails (`single_file.rs:1099`) and classify the error through D6 instead; keep an in-memory `indexed_at` beside each indexed file so the racy rule can be computed (files restored from a snapshot have none and are compared by content hash on first use).
- **Which files are checked**: for an answer with rows, the files of its rows. For an answer that claims an empty or complete result, every file in its scope, because a file that has just gained a match is not in the rows. Above a file-count bound the answer states how many files it did not compare and does not claim completeness.
- **Ordering**: the freshen takes the index write lock, so it runs before the response generation is captured. A query that finds stale files among its rows freshens them and runs once more; a second mismatch is reported, not looped on.
- **Envelope**: derived from the files checked in this call. The index-wide lost-observation signal is kept as one line on empty and complete claims only (FR-017), because it is the only evidence that files the index never saw may exist. The watcher sync token of the next phase removes that line.
- **Rationale**: `from_freshness` (`guidance/search_envelope.rs:40`) maps any index-wide `Degraded` to "UNVERIFIED against disk" on every answer.

## D6. OS response table

- **Decision**: a static table keyed by platform and raw OS error code giving class, transient flag, follow-up probe and remedy. The classifier extends the one that already takes a full error (`src/paths.rs:267`); `discovery::access_error_kind` (`src/discovery/mod.rs:1549`) callers move to it.
- **`AccessErrorKind` is not extended**: it is serde-persisted (`persist.rs:4736`). The new class is a separate in-memory type that maps down to the existing four kinds where they are stored.
- **Windows delete-pending**: `RtlGetLastNtStatus` is thread-local and must be read immediately after the failing call, so it is captured inside the access layer at the failure site and travels with the error. A classifier handed an `io::Error` later cannot recover it. `windows-sys` is in the lockfile only transitively, so the call is a two-line `extern "system"` declaration in a `cfg(windows)` module, not a new dependency.
- **Initial rows (Windows)**: 2, 3 not found; 5 access denied, probe NT status for delete-pending (0xC0000056) then list the parent; 32 sharing violation, transient; 33 lock violation, transient; 206 path too long; 1920 cannot access; 1392, 1393 corrupt. **Unix**: ENOENT, EACCES, EPERM, EBUSY (transient), ESTALE (transient), ELOOP, ENAMETOOLONG, EIO, EMFILE and ENFILE (transient).
- **Retry bound**: one budget of 1 second per tool call, shared by all files in the call.
- **Unknown codes**: reported verbatim, counted in health, outcome "cannot determine".
- **Tests**: `StableReadAccess` (`store.rs:733`) is a private trait, so fault injection lives in unit tests next to it, not in an integration test file.

## D7. Typed outcome

- **Decision**: a new metadata key `symforge/outcome` with its own type and exactly the six kinds of the spec. The existing `OutcomeClass` and `symforge/result_status` (contract version 1) are left as they are and filled from the new outcome.
- **Rationale**: `OutcomeClass` is a public exhaustive enum re-exported at `symforge::embed::parity::stel` (`src/embed/parity.rs:28`), so adding variants is a breaking API change, and `tests/conformance.rs:800` pins its six values and `contract_version: 1`.
- **Error flag**: `partial` is not an error result; `cannot_determine` and `invalid_request` are.
- **Plain-text tools**: tools that return a bare string carry no status today. The slice moves them onto the statused result path one group at a time.

## D8. What moves when output text changes

Harness cases `tests/fixtures/verify-tools/cases.jsonl` and `tests/fixtures/verify-tools-real/cases.jsonl` with their snapshots; `tests/embed_query_goldens.rs`, `tests/embed_parity.rs`, `tests/embed_disk_parity.rs`, `tests/edit_body_parity.rs`, `tests/embed_edit_plan.rs`, `tests/embed_file_impact.rs`, `tests/embed_symforge.rs`; `IMPACT_EDIT_GOLDEN` in `src/protocol/tools.rs`; the source pin and tools/list byte budgets in `tests/preventive_runtime_dark_v11.rs` and the surface tests (full budget 63,500 at `src/protocol/surface_probe.rs:440`). Each slice regenerates only the goldens its change explains and lists them in its commit.

## Prior art

rust-analyzer find-usages (text superset, then resolve); git racy timestamps; PostgreSQL's handling of Windows delete-pending; watchman sync cookies (next phase); aider's ranked repository map under a token budget.
