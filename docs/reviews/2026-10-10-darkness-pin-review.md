# Owner review packet, append-only, no pins updated

Darkness pin review for branch `feat/embed-parity-v1` (HEAD 9deb978c, merge base with main 05ce8ffd).
Pins live in `tests/preventive_runtime_dark_v11.rs`. Nothing in that file was changed; this packet is for the owner to accept or reject.

## Headline

- `dark_call_edges_appear_only_in_the_wired_roster` fails two ways: 29 files outside the roster now hold edges, and the roster row `src/protocol/knowledge_curation.rs` now holds **zero** edges, so the anti-vacuity check ("roster row holds no call edge") also fails. Its edges moved with its code into `src/knowledge/curation.rs`.
- The roster `WIRED_PRODUCTION_FILES` has **19** entries, not 21.
- `excluded_runtime_source_set_matches_reviewed_baseline` fails: the excluded set grew from 20 paths to 75.
- `full_source_set_matches_reviewed_darkness_baseline` will also fail: 295 tracked files under `src/` against 203 pinned.

## How the test defines an edge

The sweep walks every regular file under `src/` (any extension) except `src/index_lifecycle/`. A line is an edge if it contains the literal text `index_lifecycle`. Exempt are seven exact allowlisted lines (the internals mount pair, two lib.rs re-import arms, two live_index alias arms, one quoting comment in lifecycle_identity.rs) and any line whose first non-whitespace bytes are `//` and that contains neither `"` nor `*/`. Alias spellings such as `crate::live_index::index_lifecycle::` count. Edges in a roster file are wiring; edges anywhere else fail.

Method: the sweep was replicated in a script over `git show` for HEAD and for the merge base. Every file in section A had zero edges at the merge base.

## A. The 29 files outside the roster

### (i) MCP handlers rewired to the shared guidance engine: 15 files, target `guidance` only

| File | Edge lines |
|---|---|
| src/protocol/format.rs | 47 |
| src/protocol/read_gate.rs | 13 |
| src/protocol/search_tools.rs | 4 |
| src/protocol/read_tools.rs | 3 |
| src/protocol/ccr.rs | 1 |
| src/protocol/conventions.rs | 1 |
| src/protocol/explore.rs | 1 |
| src/protocol/investigation.rs | 1 |
| src/protocol/knowledge_model.rs | 1 |
| src/protocol/search_format.rs | 1 |
| src/protocol/session.rs | 1 |
| src/protocol/smart_query.rs | 1 |
| src/protocol/withheld.rs | 1 |
| src/knowledge/search.rs | 1 |
| src/knowledge/search_contract.rs | 1 |

Nearly all are `pub use` / `use crate::index_lifecycle::guidance::...` re-exports: the handler code moved into the dark directory and the old modules became shims.

### (ii) Embed-only facade children of the rostered `src/embed.rs`: 9 files

| File | Edge lines | Targets |
|---|---|---|
| src/embed/parity.rs | 1 | embed_query |
| src/embed/parity/host.rs | 4 | embed_host |
| src/embed/parity/replay.rs | 6 | physical_root (holds PhysicalRootLease) |
| src/embed/parity/edit.rs | 1 | public_api |
| src/embed/parity/knowledge.rs | 1 | public_api |
| src/embed/parity/federation.rs | 1 | embed_federation |
| src/embed/parity/session.rs | 1 | embed_session |
| src/embed/parity/read.rs | 1 | guidance |
| src/embed/parity/reference.rs | 1 | guidance |

### (iii) Write-authority callers moved out of protocol into new shared modules: 5 files

| File | Edge lines | Targets |
|---|---|---|
| src/knowledge/curation.rs | 12 | activation, physical_root. Replaces the 3,337 lines removed from protocol/knowledge_curation.rs |
| src/idempotency.rs | 6 | activation (ProjectSourceAuthority, project_source_authority) |
| src/edit_safety/atomic_write.rs | 3 | activation, authority (AuthorityRefusal) |
| src/edit_safety/batch_commit.rs | 3 | activation, physical_root (WriteAuthority, WriteReceipt) |
| src/protocol/secret_remediate.rs | 6 raw matches | Not read under the session deny rule on `secret` paths; count includes comment lines. Owner must classify |

### Roster files whose edge count changed (not failures)

| File | Main | HEAD |
|---|---|---|
| src/protocol/tools.rs | 3 | 46 |
| src/live_index/persist.rs | 19 | 51 |
| src/sidecar/handlers.rs | 9 | 15 |
| src/daemon.rs | 39 | 44 |
| src/server/serve.rs | 5 | 6 |
| src/sidecar/mod.rs | 2 | 3 |
| src/protocol/edit_tools.rs | 2 | 3 |
| src/protocol/edit.rs | 4 | 2 |
| src/protocol/knowledge_curation.rs | 4 | 0 (stale row, fails) |

## B. Excluded-source baseline compared with main

`git diff --name-status main...HEAD -- src/index_lifecycle src/server_api.rs`. `src/server_api.rs` is unchanged. Path count goes from 20 to 75.

| Status | Path | Purpose |
|---|---|---|
| M | index_lifecycle/activation.rs | Project source and write authority lanes extended for shared writers |
| M | index_lifecycle/embedded.rs | Embedded runtime handle extended to serve the new parity engines |
| M | index_lifecycle/mod.rs | Declares the new embed_* engines and the guidance module |
| M | index_lifecycle/mutation.rs | Mutation lane extended for routed batch and embed writes |
| M | index_lifecycle/physical_root.rs | Physical root leases and receipts used by new outside writers |
| M | index_lifecycle/public_api.rs | Public boundary wrappers widened for embed edit and knowledge |
| M | index_lifecycle/registry.rs | Registry extended for embed room and session bookkeeping |
| A | index_lifecycle/embed_ask.rs | Embed engine for natural-language ask routing |
| A | index_lifecycle/embed_batch.rs | Embed engine for batch edit, insert and rename |
| A | index_lifecycle/embed_changes.rs | Embed engine for what_changed and uncommitted changes |
| A | index_lifecycle/embed_detect_impact.rs | Embed engine for detect_impact |
| A | index_lifecycle/embed_federation.rs | Embed engine for multi-project federation |
| A | index_lifecycle/embed_file_search.rs | Embed engine for search_files |
| A | index_lifecycle/embed_git.rs | Embed engine for git-backed reads |
| A | index_lifecycle/embed_git_config.rs | Embed engine for git configuration discovery |
| A | index_lifecycle/embed_guidance.rs | Embed adapter onto the shared guidance module |
| A | index_lifecycle/embed_host.rs | Embed host status, health and checkpoint |
| A | index_lifecycle/embed_knowledge.rs | Embed engine for knowledge review and curation |
| A | index_lifecycle/embed_knowledge_query.rs | Embed engine for search_knowledge |
| A | index_lifecycle/embed_mutation.rs | Embed engine for single-symbol edits |
| A | index_lifecycle/embed_query.rs | Embed query claims, receipts and refusals |
| A | index_lifecycle/embed_read.rs | Embed engine for file content reads |
| A | index_lifecycle/embed_read_context.rs | Embed engine for get_file_context |
| A | index_lifecycle/embed_reference.rs | Embed engine for find_references and dependents |
| A | index_lifecycle/embed_remediation.rs | Embed engine for secret remediation |
| A | index_lifecycle/embed_restore.rs | Embed engine for snapshot restore |
| A | index_lifecycle/embed_search.rs | Embed engine for search_text and search_symbols |
| A | index_lifecycle/embed_session.rs | Embed session state and lifecycle |
| A | index_lifecycle/embed_symbol.rs | Embed engine for get_symbol |
| A | index_lifecycle/embed_symbol_context.rs | Embed engine for get_symbol_context |
| A | index_lifecycle/embed_usage.rs | Embed engine for usage and context inventory |
| A | index_lifecycle/embed_wire_edit.rs | Embed wire-format adapter for edit tools |
| A | index_lifecycle/guidance/mod.rs | Root of the shared guidance engine used by MCP and embed |
| A | index_lifecycle/guidance/changes.rs | Shared what_changed rendering |
| A | index_lifecycle/guidance/compression.rs | Shared output token budgeting and compression |
| A | index_lifecycle/guidance/conventions.rs | Shared conventions tool logic |
| A | index_lifecycle/guidance/exploration.rs | Shared exploration helpers |
| A | index_lifecycle/guidance/explore.rs | Shared explore tool logic |
| A | index_lifecycle/guidance/file_read.rs | Shared file read rendering |
| A | index_lifecycle/guidance/file_search.rs | Shared search_files logic |
| A | index_lifecycle/guidance/filters.rs | Shared language and glob filter parsing |
| A | index_lifecycle/guidance/impact.rs | Shared impact analysis rendering |
| A | index_lifecycle/guidance/investigation.rs | Shared investigation suggestion logic |
| A | index_lifecycle/guidance/knowledge_model.rs | Shared knowledge model types |
| A | index_lifecycle/guidance/read_admission_format.rs | Shared read admission message formatting |
| A | index_lifecycle/guidance/read_context.rs | Shared file context assembly |
| A | index_lifecycle/guidance/read_contract.rs | Shared read tool input contracts |
| A | index_lifecycle/guidance/read_gate.rs | Shared raw-disk read admission primitives |
| A | index_lifecycle/guidance/reference_contract.rs | Shared reference tool input contracts |
| A | index_lifecycle/guidance/reference_read.rs | Shared reference reads and output limits |
| A | index_lifecycle/guidance/routing.rs | Shared natural-language tool routing |
| A | index_lifecycle/guidance/search.rs | Shared search execution |
| A | index_lifecycle/guidance/search_contract.rs | Shared search tool input contracts |
| A | index_lifecycle/guidance/search_envelope.rs | Shared search trust envelope |
| A | index_lifecycle/guidance/search_render.rs | Shared search result rendering |
| A | index_lifecycle/guidance/serde_input.rs | Shared lenient serde input helpers |
| A | index_lifecycle/guidance/session.rs | Shared session context rendering |
| A | index_lifecycle/guidance/smart_query.rs | Shared smart query parsing |
| A | index_lifecycle/guidance/source.rs | Shared source scope options |
| A | index_lifecycle/guidance/symbol_context.rs | Shared symbol context assembly |
| A | index_lifecycle/guidance/symbol_read.rs | Shared symbol read rendering |
| A | index_lifecycle/guidance/withheld.rs | Shared withheld-content notices |

Purposes are inferred from module names and the importing call sites, not from a full read of each file.

## C. Recommendations

| File or group | Recommendation | Reason, and cost of rerouting |
|---|---|---|
| (i) all 15 guidance callers | Accept into roster | Rerouting via `src/embed.rs` makes the protocol layer depend on the embed facade, inverting layering. A neutral alias hub (a roster file re-exporting `guidance`) removes the token but keeps the same semantic edge, which is the laundering this pin exists to catch. The only clean alternative is moving `guidance/` out of the dark directory: it reads as pure contract and render code. Cost is 30 file moves plus path rewrites in about 25 protocol and embed files; it shrinks this roster and the excluded set. Owner design call. |
| (ii) parity.rs, edit.rs, knowledge.rs, federation.rs, session.rs, read.rs, reference.rs, host.rs | Reroute via `src/embed.rs` | They are re-exports or forwarding calls from children of the declared one public door. Hoisting them into `embed.rs` as a few `pub(crate) use` lines with children using `super::` costs about 9 files and 20 lines and keeps the door single, matching the C5 roster comment. Accepting is defensible but adds 8 rows. |
| (ii) replay.rs | Accept, after review | It takes physical-root leases directly. That is write authority, not facade forwarding, and hiding it behind `embed.rs` would obscure it. |
| (iii) knowledge/curation.rs | Accept, and remove the stale `src/protocol/knowledge_curation.rs` row | The edge moved with the code. The row removal is mandatory: the stale row fails on its own. |
| (iii) idempotency.rs | Accept | Shared write-authority caller used by MCP and embed. Rerouting via `embed.rs` is wrong because MCP calls it too. |
| (iii) edit_safety/atomic_write.rs | Accept | Same reason. Routing through a protocol roster file would make edit_safety depend on protocol. |
| (iii) edit_safety/batch_commit.rs | Accept | Same reason as atomic_write.rs. |
| (iii) protocol/secret_remediate.rs | Owner reads it, then most likely accept | It had no edges on main. Confirm the six matches are code, not quoted comments. |

## D. Recomputing the pins (as the test file defines them)

1. **Roster**: edit path strings in `WIRED_PRODUCTION_FILES`. Every row must hold at least one edge. The allowlist count stays `(7, 7)` unless the allowlist itself changes.
2. **Excluded path list**: `EXCLUDED_RUNTIME_SOURCE_PATHS` must equal the sorted list of `src/`-relative, forward-slash paths of every file under `index_lifecycle/` plus `server_api.rs`. The path assertion runs first and stops the test, so fix the list before refreshing the digest. Expected count at HEAD: 75.
3. **`EXCLUDED_RUNTIME_SOURCE_PIN_V1`** is `(digest, file_count, normalized_bytes)`. Digest is lowercase-hex SHA-256 over the domain `b"symforge-excluded-runtime-source-set-v1\0"`, then the record count as u64 little-endian, then for each record sorted by path: path length (u64 LE), path bytes, content length (u64 LE), content. Content is CRLF-to-LF normalized before hashing, and the byte total is over normalized content.
4. **`FULL_SOURCE_PIN_V1`** uses the same function with domain `b"symforge-full-source-set-v1\0"` over every regular file of any extension under `src/`, walked on disk, so untracked files count. Expected 295 files at HEAD on a clean tree, against 203 pinned.
5. **Practical route**: run the test and paste the left-hand tuple from the `assert_eq` failure. Each refresh adds a comment paragraph above the pin stating what changed, the file count, and whether a new edge into `src/index_lifecycle` or `server_api.rs` appeared. This campaign's comment must say new edges exist and list them; every earlier comment says there were none.
6. **Scope**: editing the test file does not move the full-source pin, because `tests/` is outside `src/`. The workflow fingerprints are unaffected.
