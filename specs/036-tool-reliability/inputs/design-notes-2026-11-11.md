# Tool reliability design notes (2026-11-11) — input for a spec, not a repo doc

Owner standard (verbatim, see memory feedback-tool-adoption-test-method): built-in tools "work ALWAYS";
"no assumptions and 100% declarative results based on known patterns, known OS responses, environments";
"assumptions can be addressed with probes and prechecks"; "build a superior index … registry … guidance-response mechanisms".

## Root causes verified in code / live (installed 11.11.0 daemon, tree feat/core-tool-profile)

RC1 Global freshness taint. health: `Dispositions … unreadable=10`, `freshness=degraded[ObservationFailed,ReconciliationPending]`.
    Direct probe (scratchpad/probe_unreadable.py): 1,760 working-tree paths, 0 unreadable, 0 long paths. So the 10 are stale/transient records.
    Effect: every result says "Source authority: index (UNVERIFIED against disk)" (search_envelope.rs `from_freshness`: any Degraded => that label).
    Hook adoption this session: routed 404/2726 (15%), sidecar errors 2230 (consistent with audit SF-ADOPT-01; not re-measured).
    Code: `discovery::access_error_kind(io::ErrorKind)` (src/discovery/mod.rs:1549) keeps 4 kinds (NotFound, PermissionDenied, InvalidData, ResourceExhausted) else Other.
    `raw_os_error()` is never consulted for file access (only ESRCH in process checks). PR #857 adds a 3-sweep settle timer + names files; that is a timer, not a cause.
RC2 find_references(path=…) drops provable-elsewhere hits silently. Live: name-only = 8 refs/3 files; with path="src/protocol/surface_probe.rs" = 6 refs/2 files.
    Dropped: src/protocol/deferred_stdio.rs:301 `super::surface_probe::enforce_compact_surface(..)` and the test `use` line. Both runs print "Completeness: full for current scope".
RC3 find_dependents is import-heuristic. Live: "No file-level dependents found" for src/protocol/repeat.rs (used by src/protocol/mod.rs via `mod repeat;` + `repeat::Type` paths).
    Code src/live_index/query.rs:3221 `find_dependents_for_file`: stem matching (`matches_target_stem` on any path segment => false positives for same-leaf modules),
    module-path prefix on Import refs, qualified *Call* refs only, re-exports 2 hops. `mod x;` is not an edge; qualified type paths / assoc fns are not matched.
RC4 Truncation is positional. `enforce_token_budget_flagged` (guidance/source.rs:123) and `cap_file_content_output` (guidance/file_read.rs:564) cut the assembled text's tail at a line boundary.
    Hit myself: explicit 75-line range read cut at default 1000 tokens (1025 needed); get_symbol with explicit max_tokens=3000 cut a 3091-token body.
    Sweep: get_file_context ~680, get_symbol_context 376 ("Callees (5):" no names), get_repo_map ~1000 alphabetical (never reached src/).
RC5 Conditional params are silent no-ops. GetRepoMapInput (src/protocol/read_tools.rs:269): `path`, `depth` "only used when detail=tree"; 9 such phrases in tool input docs.
    Unknown fields: `deny_unknown_fields` only on embed parity structs; MCP inputs use lenient_* deserializers and drop unknown keys (get_file_content schema is the exception).
RC6 Output shape. find_references prints "AST declaration headers … bytes a..b" blocks (~60 lines for 8 refs); search_text gives `path / fn / > line: text` (~20 lines) for the same question.
RC7 Usage (health, this daemon session, ~250 calls incl. the sweep): 26 tools called; never called: explore, inspect_match, analyze_file_impact, ask, context_inventory,
    investigation_suggest, symforge_retrieve, health_compact, checkpoint_now, index_folder, knowledge review/curate, secret_remediate, symforge facade.
    Also: "Stale-mutation rejections: 21", "edit tool calls without working_directory (last hour): 20" (cause not yet read).

Not yet read in code (symptom only): search_symbols multi-word miss; search_text docs exclusion + noise filter; batch_rename "uncertain" same-file sites; edit_plan no path;
what_changed 72 vs git 25; detect_impact all "high"; stale text after external Edit.

## Mechanisms (one pipeline every tool call passes through)

S0 Contract check: per-tool declarative contract; unknown param => error naming valid ones; conditional param => imply the mode or error.
S1 Sync gate: watcher sync token (watchman "sync cookie" pattern) with bounded wait; startup probe decides if the watcher can be trusted; else S3 only.
S2 Candidates with a lexical floor: exact identifier occurrences (same engine as search_text, already equal to ripgrep on 6 real cases) => ONE resolver classifies each:
   confirmed(same-file | import path | qualified path normalised incl. super::/self::/crate:: | unique name) / unresolved / excluded(comment,string). Nothing dropped.
   find_references, find_dependents, get_symbol_context, batch_rename, detect_impact, edit_plan all become views of that resolver.
S3 Answer-scoped verification: stat (size, mtime, racy-window rule from git) each file the answer uses vs the indexed stamp; mismatch => re-read + reparse before answering.
   File errors go through the OS-response registry; transient => retry; Denied => parent-listing probe (Windows delete-pending looks like access denied).
S4 Payload-first rendering: each tool declares primary payload vs extras; explicit requests (range, named symbol, caller list) are never cut by a default; extras drop first;
   real overflow => relevance order + cursor with the literal next call. Envelope lines only when abnormal, naming the file.
S5 Typed outcome: ok | ok_partial(cursor) | none_proven(scanned what) | ambiguous(candidates + exact call) | cannot_determine(reason, remedy) | invalid_request.

Registries (small static tables): R1 OS responses (platform, raw code -> class, transient?, probe, remedy; unknown codes logged verbatim so the table grows).
R2 Environment probes (extend src/capability): case sensitivity, symlinks/junctions, mtime granularity, watcher round trip, network/WSL mount, long paths, path style,
git state, encodings/BOM, language servers present. R3 Tool contracts (params+conditions, primary payload, budget policy, canonical task, oracle).

Guidance-response: G1 do the deterministic fallback inside the call and label the tier (symbol exact -> all-words -> definition-by-text; code zero-hit -> show doc/test hits);
G2 otherwise emit the literal next call; G3 output shaped like built-ins (`path:line: text`, numbered lines); G4 measure abandonment in the field: the sidecar hooks already see
built-in Read/Grep, so "symforge call followed by built-in on the same target" is recordable per tool.

Proof: T1 differential vs ripgrep/git on real repos (existing 22-repo corpus, benchmark's 4 pinned repos); T2 metamorphic (path-scoped ⊆ name-wide and the difference is accounted for;
dependents(F) ⊇ files of references(exports(F)); rename dry-run sites == confirmed references); T3 fault injection through the injectable access layer of `stable_read_with_access`
(sharing violation, delete-pending, vanish between stat and read, mtime ties, watcher overflow); T4 default call contains the primary payload for each tool's canonical task;
T5 the contextless-subagent sweep as a benchmark arm (calls-to-answer, fallbacks).

Prior art to confirm in the spec's research step: rust-analyzer FindUsages (text search then semantic check); IntelliJ word index + resolve; watchman sync cookies / git fsmonitor;
git racy timestamps; Zoekt trigram index (only if scans get slow); aider repo map (graph rank under a token budget); LSP / SCIP precise tier (existing docs/semantic-tier-roadmap.md);
stack-graphs (maintenance status unknown to me); notify crate rescan flag on overflow.

Prior art checked by web search 2026-11-11:
- rust-analyzer ide_db::search: fast text scan gives a SUPERSET of candidates, each verified by precise name resolution (module docs). Same shape as S2. Known to miss macros.
- watchman cookies: unique file created in the watched dir; seeing it come back through the event stream proves all earlier events were delivered; sync_timeout bounds the wait;
  on Linux only accepted when delivered by the OS notification; macOS less reliable (FSEventStreamFlushSync as well). Failure mode: cookie dir deleted.
- git racy-git: a file whose mtime equals/exceeds the index timestamp cannot be trusted by stat alone; compare content hash. Timestamp resolution varies by filesystem.
- Windows: a file pending deletion but still open returns ERROR_ACCESS_DENIED to Win32; PostgreSQL tells it apart with RtlGetLastNtStatus() == STATUS_DELETE_PENDING (0xC0000056)
  and retries about one second because the state is short-lived. Directories and real ACL denials give the same Win32 code.
- aider repo map: tree-sitter tags -> reference graph -> PageRank (personalised to mentioned identifiers) -> binary search to fit a token budget (secondhand sources).
- github/stack-graphs: ARCHIVED 2025-09-09, unmaintained. Do not build on it; scope rules stay in our resolver, precise tier via LSP/SCIP.
- Serena (oraios/serena): MCP toolkit wrapping language servers for symbol find/rename/replace; tool behaviour not verified in detail.

Phases: A correctness floor (R1, S3, resolver+floor, S0, S4, tests T1-T4) -> B sync gate + probes -> C consolidation + guidance (breaking) -> D field telemetry -> E optional precise tier.
