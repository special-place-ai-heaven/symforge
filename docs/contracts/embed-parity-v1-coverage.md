# Embed v1 coverage and acceptance — working inventory

As of 2026-10-10. Local unpublished work against SymForge 11.5.6. API additions are under `symforge::embed::parity`; frozen flat V11 types remain intact. This inventory is an implementation worklist, not a completed parity declaration. A row is complete only after full options, shared-engine behavior, authority, lifecycle and meaningful native/MCP fixture comparisons are verified. An Unsupported response does not close a current shipped capability gap.

## Current tools

| Standalone capability | Native path / required parity | Current verification |
|---|---|---|
| health | Source-bound health, counts, snapshot verification and trust | Basic native host fixture passes; complete recovery metadata pending |
| health_compact | Same health state, compact presentation | Complete compact projection pending |
| status | Loading/current/blocked progress without false current claim | Host fixture passes; full standalone comparison pending |
| index_folder | Admitted source open/reset, refresh and lifecycle controls | Existing V11 binding baseline passes; full operation options pending |
| checkpoint_now | Shared writer, resolved state placement, verification and source-move outcome | Native verified checkpoint fixture passes; race/recovery coverage pending |
| analyze_file_impact | Real disk change admission/reindex and impact, with explicit observations | Captured source vs local Git fixture passes; complete disk workflow pending |
| detect_impact | Complete repository change/dependency analysis | Pending shared native projection |
| what_changed | Current source/repository changes with filters | Pending shared native projection |
| diff_symbols | Local Git and current-byte symbol changes | Native fixture passes; full modes/comparison pending |
| validate_file_syntax | Captured byte-exact parser diagnostics and partial state | Native fixture passes; complete standalone comparison pending |
| get_repo_map | Full map detail/filter/budget semantics | Shared map/context fixtures pass, including scoped knowledge exclusion and whole-result row accounting; final comparisons pending |
| get_file_context | Outline/imports/consumers/tests/sections and bounds | Full section projection fixture passes; complete pre-token result cache is implemented and needs broader final comparisons |
| get_file_content | Exact bytes, full selectors, estimates, pagination and bounds | Eight rebuilt Windows read fixtures pass, including single-line UTF-8 reconstruction, replacement-root and case-alias refusal, and explicit stale-publication detection after a completed write. Full synchronous freshening still needs verification |
| get_symbol | Selectors, batches, exact source/span verification | Three full symbol-read/inspect fixtures pass, including batch selectors, ambiguity, estimate and retained full context; final surface comparisons pending |
| get_symbol_context | Callers/callees/types, bounded context | Four rebuilt fixtures pass, including default/bundle/trace, truthful truncation, knowledge-only sections and kind-before-ambiguity selection; final standalone comparisons pending |
| inspect_match | Search evidence expanded through shared context | Full nested evidence fixture passes through shared native projection; final surface comparisons pending |
| search_symbols | Full language/kind/scope/noise filters and shared ranking | Four rebuilt shared search fixtures pass, including complete retained context before token projection; candidate MCP comparison pending |
| search_text | Terms, regex, whole-word, structural, scopes, grouping, ranking, follow-refs and caps | Shared rich planner and native fixtures pass; caller-order regression observed red and deterministic shared fix passes |
| search_files | Shared pre-cap scope/noise filtering, ranking and path resolution | Two rich native fixtures pass, including scope-before-resolution regression; candidate MCP comparison pending |
| search_knowledge | Current admitted dossier search, policy and provenance | Shared full projection and native fixture pass; federation and candidate MCP comparison pending |
| review_knowledge | Shared review engine and stable dossier/result hashes | Native live review fixture passes; standalone comparison pending |
| find_references | Full directions/kinds/modes/implementations and bounds | Four rebuilt rich fixtures pass, including nested evidence accounting and implementation estimates that do not consume budget for omitted rows; final standalone comparisons pending |
| find_dependents | Shared dependency state and admitted source | Shared text/Mermaid/DOT and name redirect fixture passes, including corrected nested evidence accounting |
| symforge_retrieve | Full shared context composition and token budget semantics | Exact served-page, overflow and source isolation fixtures pass; reset followed by identical content now keeps the expired handle invalid |
| explore | Full ranking, depth 1–3, imports/concepts/derived clusters | Shared full scorer extracted; native execution and comparisons pending |
| ask | Full intent classification, route and execution | Shared classifier extracted; complete native execution pending |
| conventions | Same language/project pattern evidence | Shared engine extracted; typed projection/comparison pending |
| edit_plan | Pinned selectors/references, guarded preview and operation identity | Basic native plan/replace fixture passes; full guidance comparison pending |
| context_inventory | Explicit source-bound host session | Native source-bound session fixture passes; host wire session and full comparison pending |
| investigation_suggest | Same evidence/routing with explicit session state | Shared session projection implemented; full native/MCP comparison pending |
| replace_symbol_body | Exact shared splice/guarded writer, source/publication/preimage guards, replay | Native preview/apply/replay/stale/cancel fixtures pass; MCP extraction suite pending |
| edit_within_symbol | Exact occurrence/near-line/replace-all/whitespace selection | Shared selector extracted; focused live fixture pending |
| insert_symbol | Shared positioning/docs/indent/newline semantics and guarded writes | Native insert/delete fixture passes; full MCP comparison pending |
| delete_symbol | Shared symbol span/doc/comment handling and guarded writes | Native insert/delete fixture passes; full MCP comparison pending |
| batch_edit | Shared staged multi-file commit, rollback, one replay and exact preimages | Seven native batch fixtures pass, including exact replacement-footprint overlap and delete-cleanup offset regression; replacement-root replay audit pending |
| batch_insert | Same complete staging and transaction semantics | Native multi-anchor transaction fixture passes; full MCP comparison pending |
| batch_rename | Full reference/text rename staging, hook/reroute and transaction semantics | Shared reference planner wired into MCP/native; native two-file rename and post-refresh replay fixture passes |
| symforge_edit | Facade routes to those same complete edit operations | Pending native facade/catalog verification |
| curate_knowledge | Shared preview/apply, expected-publication guards including pending recovery, safe full diff, refresh/replay | Linux passes all 28 tests, including actual source- and state-directory replacement. Windows passes 27 with two explicit platform setup skips. Copied-state child effects and durable physical-root binding are verified; admission-lifetime state anchoring is staged for rebuilt verification |
| secret_remediate (when enabled) | Shared externalize/encrypt/dismiss planner and safe application/receipts | Four preceding native fixtures and real SOPS encrypt/decrypt pass. Selected-bytes runner is shared. Rebuilt guarded MCP externalize fixture passes after resolving the server's existing state placement; complete final comparisons remain open |
| symforge (compact facade) | Same read/guidance routing; optional presentation, no engine downgrade | Pending complete native facade/catalog verification |

The default 39-tool inventory and additional enabled remediation/compact facade must be compared to the actual candidate build's tool surface, not only this handwritten table.

## Resources and prompts

The library/service contract must expose equivalent admitted semantics for repository health, outline, map, uncommitted changes, tool catalog and glossary; file-context/content and symbol-detail/context resource templates; and all eight shipped admin, review, architecture, triage, onboard, refactor, debug and knowledge-hygiene prompts. The eighth prompt was confirmed by the actual installed MCP inventory; the older AGENTS list contains seven. Shared content and request validation must remain aligned with MCP. These paths are assigned and pending; static names alone are not resource/prompt parity.

## Additional native capabilities

Lossless paginated graph rows preserve parser symbol/reference ordering, file indices, enclosing-symbol indices, unresolved/ambiguous references and degraded state. Continuations bind the serving publication, including a process-instance nonce. A budget too small to produce a row refuses explicitly. Stable ordering, same-line identity, no-progress refusal and actual two-process stale-cursor rejection fixtures pass.

One `HostRuntimeOwner` can serve source-bound rooms on a shared bounded runtime. Nine rebuilt Windows host fixtures pass for same-source room isolation, independent close, last-lease cleanup, owner-only global shutdown, refusal after shutdown, redacted withheld findings and warm snapshot reopening. The rebuilt wire suite passes all three fixtures, including duplicate active room IDs and independent room replay scopes. Native query and resource-envelope regressions pass for complete-frame admission before session/cache history commits and post-callback cancellation. This is process-local reuse, not proof that a host can see a private guest overlay.

Explicit admitted-source federation passes three native fixtures for selector validation before execution, attributable per-source claims, aggregate bounds, alias deduplication and decoded payload accounting. Lower-only session cache policy and concurrent reset/query revisions pass two fixtures; the three session retrieval/isolation/reset fixtures also pass. The actual first-child cancellation regression now retains the source refusal and passes.

Host wire parsing checks its request byte cap before deserialization and enforces the response byte cap on the complete serialized envelope. Read responses refuse oversize without truncating correctness metadata. Query `max_bytes` measures decoded payload bytes separately; escaped UTF-8 projection fixtures pass. Serialized mutation routing passes an actual 70-file commit whose response exceeds a 4 KiB frame: it returns bound committed recovery evidence, and exact-key replay after reopening with a larger frame returns the original full receipt without repeating effects.

Replay v1 previously passed eleven focused tests including actual cross-process reservation, crash states and copied-state rejection under a replacement physical root. The latest scoped database layout passes ten of eleven; the copied-state fixture copied only immediate files and missed its nested database. Its recursive copy and explicit database-copy witness are staged for an actual child rerun, preserving the physical-root refusal oracle. Native batch and curation process-restart proofs pass. Existing MCP replay stages complete records before atomic publication, redacts key/response Debug and bounds/guards persisted and replayed output. An actual child-process fault exposed Completed publication before its postimage; atomic publication and Started/Uncertain states now pass the library tests. The admitted-write versus stop race was reproduced and its slot gate passes. Original-root anchoring of all replay attestations remains an acceptance requirement.

The previous full default-feature plus Embed library run reported 3,642 passed, one legacy physical-root replay failure and four ignored. The focused legacy replay suite subsequently passed all 13 tests. A fresh full run is required after the remaining implementation. Native replay separately passes 11 tests; batches pass seven, federation three, cache limits two, sessions three, symbol read/inspect three and shared map/context two. Selected-bytes encryption passes both its actual changed-file child regression and real SOPS encrypt/decrypt. These are checkpoint results, not final acceptance.

Linux verification uses byte-exact sealed source snapshots and Rust 1.96.0. The actual source-directory and state-directory substitution regressions first failed, then passed through retained-capability writers. Linux curation now passes all 28 tests without skips. Windows normal curation/recovery passes 27 tests, with two explicit directory-rename setup skips. These checks preserve the earlier durability and failpoint tests.

## Release acceptance

The latest rebuilt Windows checkpoint passes eight read, four search, nine host, three explicit state-placement and three wire mutation fixtures, plus the outer-resource-frame regression. Shared persistence passes 111 of 112 tests; its remaining stop-path fix is staged. The focused compiler explicitly built replay v1, whose latest result is ten passed and one copied-state fixture failure described above. Native resource-catalog, full Ask and change-report parity remain open. Engine-only Git preparation has reached an unverified first implementation slice; supported linked/common/alternate/config behavior must not remain refused at acceptance.

The corrected persistent-ranking fixture actually executes its child and now passes on Windows and Linux after original-root revalidation; its earlier zero-child result is invalid. Linux also passes the background verification and snapshot-restore original-root substitution regressions. The bound persistent ranking cache still awaits its admitted Git HEAD/distance consumer. An older sealed Linux persistence run passed 86 of 89 tests; its read-failure, stop and stat-change cases require a fresh Linux rerun after the subsequent fixes. A retained loose-object Git probe passes, but does not establish pack, alternate, common-directory or full Git authority. No staged fix is accepted without its rebuilt regression.

- All rows above fully implemented through the existing shared core, with no diluted substitutes or silent filter/metadata loss.
- Engine-only native consumer builds with `default-features = false, features = ["embed"]` and no test-internals dependency; documented example runs against a real fixture.
- Full serialized claims/refusals preserve correctness metadata and effective limits. Native authority cannot be forged from JSON. Wrong root/scope/room grant and stale publications refuse deterministically.
- Meaningful actual-engine fixtures cover bounded output, graph continuation, disk races, cancellation before writes, uncertainty after started effects, crash replay, policy/remediation and source lifecycle/checkpoint recovery.
- Standalone/shared-engine tests pass after extraction, including edits, knowledge, guidance and recovery. Candidate MCP probes use the current built source version; installed 11.5.4 dogfood probes are investigation evidence only.
- Signed V11 contract, refreeze manifests and approved fixture corpus remain unchanged; compatibility gates and normal lint/format/build checks pass.
- Final capability matrix, migration examples and room integration contract describe tested behavior and any platform constraints precisely. AAP and TC agent coordination is recorded without claiming their integration completed here.
