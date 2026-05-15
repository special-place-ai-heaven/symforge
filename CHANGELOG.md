# Changelog

## [7.7.0](https://github.com/special-place-administrator/symforge/compare/v7.6.2...v7.7.0) (2026-05-11)


### Features

* **explore:** append rank-signal footer documenting score composition ([d9eecb5](https://github.com/special-place-administrator/symforge/commit/d9eecb53809adbe8035948cd73034b352f666110))
* **health:** surface empty-index reason as actionable banner ([9100d8b](https://github.com/special-place-administrator/symforge/commit/9100d8b77ccd48a77a31d9b734ece9d831af9c41))
* **health:** surface reconcile repairs on idle watcher line ([34e97fb](https://github.com/special-place-administrator/symforge/commit/34e97fb2caa766b29d5bf13f48af442431f39b0c))
* **query:** add vendor and personal-tooling path predicates ([e8fe429](https://github.com/special-place-administrator/symforge/commit/e8fe4299fbb7ab16fa2a11976cbcde73ad2edfa7))
* **search:** default-exclude vendor and personal-tooling paths ([f804d21](https://github.com/special-place-administrator/symforge/commit/f804d214636b305c19876b9dea669a8e5d6b003b))


### Bug Fixes

* **discovery:** allow projects named tmp/var/home not under filesystem root ([5af8ccc](https://github.com/special-place-administrator/symforge/commit/5af8ccc1519bf77186966eeb8b5c211def5a08af))
* **frecency:** defer DB open from boot to first commitment bump ([0f2b723](https://github.com/special-place-administrator/symforge/commit/0f2b723ecfa886dfcc2eca00edcd40c01fbf39f2))
* **sidecar:** SO_REUSEADDR + deterministic shutdown for parallel test fan-out ([e77b009](https://github.com/special-place-administrator/symforge/commit/e77b009a85e5a783d6179e295bfff60bc469009f))

## [7.6.2](https://github.com/special-place-administrator/symforge/compare/v7.6.1...v7.6.2) (2026-04-24)


### Bug Fixes

* **io:** add path-named context to startup file-write sites ([7c04744](https://github.com/special-place-administrator/symforge/commit/7c04744a0ef0afd62b1575084badf248c5ba694d))

## [7.6.1](https://github.com/special-place-administrator/symforge/compare/v7.6.0...v7.6.1) (2026-04-24)


### Bug Fixes

* **daemon:** fix spawn_blocking/governor races and stale PID cleanup ([f58afbc](https://github.com/special-place-administrator/symforge/commit/f58afbc72ca942cdeb05bc5cba75c49b330c5ae3))

## [7.6.0](https://github.com/special-place-administrator/symforge/compare/v7.5.2...v7.6.0) (2026-04-21)


### Features

* **get-file-content:** add offset/limit aliases and deny_unknown_fields ([5c69175](https://github.com/special-place-administrator/symforge/commit/5c691755b85f760be94b3535cb73952be70a89df))
* **get-file-content:** normalize offset/limit aliases before proxy call ([afd2f75](https://github.com/special-place-administrator/symforge/commit/afd2f750522edfb399d1fa702fbedf329bbdd3d3))
* **get-file-content:** universal 60 KB byte cap on output ([945e151](https://github.com/special-place-administrator/symforge/commit/945e151728fba3754224bffedb4411ce68b4e888))


### Bug Fixes

* **get-file-content:** address review issues A/B/C ([09fc76a](https://github.com/special-place-administrator/symforge/commit/09fc76afd2932d2e7448ffaf2095cdd08d2e1dec))
* **get-file-content:** agent-agnostic resilience — offset/limit aliases, deny_unknown_fields, 60 KB cap ([4ab20de](https://github.com/special-place-administrator/symforge/commit/4ab20de12d140f48916a6f90cdbc981bb3523bef))

## [Unreleased]

### Bug Fixes

* **get-file-content:** accept `offset`/`limit` as aliases for `start_line`/`end_line` (Read-tool idiom) so agents using the Claude Code `Read` idiom get sliced windows instead of full-file returns
* **get-file-content:** reject unknown input fields with an explicit error naming the unknown field, instead of silently ignoring them
* **get-file-content:** cap all responses at 60 KB; oversized output is truncated at a line boundary with a footer suggesting narrower read modes

## [7.5.0](https://github.com/special-place-administrator/symforge/compare/v7.4.6...v7.5.0) (2026-04-18)


### Features

* **coupling:** land co-change coupling feature (T1–3.2) ([d5d2619](https://github.com/special-place-administrator/symforge/commit/d5d2619ddd2f29aa0311631a404fc7067847e212))
* **edit-and-ranker-hooks#5:** add commit_distance helper in git.rs ([2daa4be](https://github.com/special-place-administrator/symforge/commit/2daa4be360509ec4e46a61a7fe2f4741d9a65dc3))
* **frecency-ranking#2:** SQLite-backed frecency store with bump + decay scoring ([dc31124](https://github.com/special-place-administrator/symforge/commit/dc31124e6a2b09735d23dd56bb025a0c15e0bd6f))
* **frecency-ranking#3:** wire bump hooks + no-bump discovery guards ([d804291](https://github.com/special-place-administrator/symforge/commit/d804291a4fcb679e7c9662a5736a359725f54fcd))
* **frecency-ranking#4:** add rank_by param + fusion to search_files ([74eaa02](https://github.com/special-place-administrator/symforge/commit/74eaa02ba212fe3cb1d31c3aad4fab4158d55882))
* **frecency-ranking#5:** wire graduated HEAD-change reset into LiveIndex boot path ([1adf995](https://github.com/special-place-administrator/symforge/commit/1adf995fbabb7930ace88579f8310ca72a0ce678))
* **frecency-ranking#7:** wire bump hooks to real FrecencyStore ([4a07e4c](https://github.com/special-place-administrator/symforge/commit/4a07e4c6d6aca4ec55fff62fd799b187b8903fa6))
* **frecency-ranking:** cache FrecencyStore per workspace + busy_timeout safety net ([a197f05](https://github.com/special-place-administrator/symforge/commit/a197f058dcfd05e91699fdec1ead3fccd114d269))
* **git:** add head_sha helper for HEAD resolution ([3778e7f](https://github.com/special-place-administrator/symforge/commit/3778e7fac81d1493e5a3153b136578d3efebe800))
* **worktree-awareness#2:** add src/worktree.rs with canonicalize + cache + resolve_target_path ([98c17dd](https://github.com/special-place-administrator/symforge/commit/98c17ddee1b702870e023d93d259d3be64eefa2d))
* **worktree-awareness#3:** plumb working_directory through 7 edit handlers ([f56fcd0](https://github.com/special-place-administrator/symforge/commit/f56fcd043cef664abb274e66001c602607714bab))
* **worktree-awareness#4:** health misuse counter + conventions answer ([e3062f0](https://github.com/special-place-administrator/symforge/commit/e3062f0c9809de4562878903d795dbebc23d3e66))
* **worktree-awareness#4:** register WorktreeAwareEditHook + emit reroute response ([79258d7](https://github.com/special-place-administrator/symforge/commit/79258d72a9873d2432fd8abe2e4ac0160363c628))


### Bug Fixes

* **ci:** skip merge commits in conventional-commits validator ([0789887](https://github.com/special-place-administrator/symforge/commit/0789887fed0d60a545bb3f2964cdcea1a6ba2b2a))
* **parsing#1:** clamp error-snippet window to UTF-8 char boundaries ([9a14a7d](https://github.com/special-place-administrator/symforge/commit/9a14a7d204fc5544616bd19566f09995bb82a38c))
* **parsing:** cap AST walk depth so recursive walkers don't stack-overflow ([77cabec](https://github.com/special-place-administrator/symforge/commit/77cabec7f483a324655c90c27c78cd08d297a6ff))

## [7.4.6](https://github.com/special-place-administrator/symforge/compare/v7.4.5...v7.4.6) (2026-04-12)


### Bug Fixes

* restore full self-hosting Rust parsing ([c8b5c51](https://github.com/special-place-administrator/symforge/commit/c8b5c5114fe5040b89bb44a60052a9c894af4117))

## [7.4.5](https://github.com/special-place-administrator/symforge/compare/v7.4.4...v7.4.5) (2026-04-12)


### Bug Fixes

* use dedicated indexing pool for Windows reindexing ([884f502](https://github.com/special-place-administrator/symforge/commit/884f5025de76e682441f1616af2e9ed8f0fe2d9c))

## [7.4.4](https://github.com/special-place-administrator/symforge/compare/v7.4.3...v7.4.4) (2026-04-12)


### Bug Fixes

* offload local index_folder reload to blocking pool ([5f851da](https://github.com/special-place-administrator/symforge/commit/5f851dac685ad2e9dacae7dfa12db6ac1ffa6436))

## [7.4.3](https://github.com/special-place-administrator/symforge/compare/v7.4.2...v7.4.3) (2026-04-10)


### Bug Fixes

* accept 'body' as alias for 'new_body' in replace_symbol_body ([8996c9b](https://github.com/special-place-administrator/symforge/commit/8996c9b1d8454e45be5e5173df6bce6195754da4))

## [7.4.2](https://github.com/special-place-administrator/symforge/compare/v7.4.1...v7.4.2) (2026-04-09)


### Performance Improvements

* add release profile optimization and Aho-Corasick multi-term search ([aed8ec8](https://github.com/special-place-administrator/symforge/commit/aed8ec8b975b69f0118dc4fbed3c9d852879f164))

## [7.4.1](https://github.com/special-place-administrator/symforge/compare/v7.4.0...v7.4.1) (2026-04-08)


### Bug Fixes

* add expando_char preprocessing for ast-grep structural patterns ([9bc10db](https://github.com/special-place-administrator/symforge/commit/9bc10db97fd318557c46b23f3028f37db2bbd0ad))

## [7.4.0](https://github.com/special-place-administrator/symforge/compare/v7.3.0...v7.4.0) (2026-04-08)


### Features

* add adaptive detail levels based on token budget ([6d873df](https://github.com/special-place-administrator/symforge/commit/6d873dfe7d3ed2458f7fbf38e20e01f8fe117b9e))
* add ast-grep structural search to search_text tool ([3c2f22c](https://github.com/special-place-administrator/symforge/commit/3c2f22c5bf466717e1eeab7c7a9d94f317f36d86))
* add max_tokens budget enforcement to 11 search/navigation tools ([3802398](https://github.com/special-place-administrator/symforge/commit/3802398209bb22e43251e1b948bf0fd7dd2d154d))
* add MCP tool annotations to all 30 tools ([2f9d44b](https://github.com/special-place-administrator/symforge/commit/2f9d44b706b7bfa17e35003ae2bd5681e05d5a5a))
* add per-result confidence scores to 5 search/navigation tools ([bd0e97d](https://github.com/special-place-administrator/symforge/commit/bd0e97dccf64e92ee1fa3b316506e7e56bf96108))


### Bug Fixes

* suppress PreToolUse hints when sidecar active, fix Windows init test ([0ed2a9b](https://github.com/special-place-administrator/symforge/commit/0ed2a9b6777599e00adc1530aff0132627020396))


### Performance Improvements

* eliminate per-query allocations in trigram search ([7c3dd1d](https://github.com/special-place-administrator/symforge/commit/7c3dd1d9ae9fcaf4ebf50e8d65d724118a047702))
* replace RwLock with ArcSwap for lock-free concurrent reads ([7fad483](https://github.com/special-place-administrator/symforge/commit/7fad4830f52ddabdf1e55bca684246409b9819e0))

## [7.3.0](https://github.com/special-place-administrator/symforge/compare/v7.2.0...v7.3.0) (2026-04-07)


### Features

* add Claude Desktop MCP server registration with Windows CWD fix ([c6c4377](https://github.com/special-place-administrator/symforge/commit/c6c437799841cf11ab3ffae910716781a8e4dc80))

## [7.2.0](https://github.com/special-place-administrator/symforge/compare/v7.1.0...v7.2.0) (2026-04-07)


### Features

* universal symbol name resolution and C++ qualified names ([59b7fa2](https://github.com/special-place-administrator/symforge/commit/59b7fa2b59da6fbc3a39da5690d014af360fd758))
* universal symbol name resolution for all languages ([2ecd65a](https://github.com/special-place-administrator/symforge/commit/2ecd65a94a63434c3a2fae36b1eafebd04197428))


### Bug Fixes

* resolve impl blocks when LLM drops the `impl` prefix ([23d01de](https://github.com/special-place-administrator/symforge/commit/23d01de2529757d9b4874e6fda44e4cffb32726b))

## [7.1.0](https://github.com/special-place-administrator/symforge/compare/v7.0.0...v7.1.0) (2026-04-07)


### Features

* C/C++ parser improvements, whitespace-flexible editing, insert-after fix ([207fb25](https://github.com/special-place-administrator/symforge/commit/207fb259c6c6f617345f789d04737ec28c3402a7))
* C/C++ parser improvements, whitespace-flexible editing, insert-after fix ([739c867](https://github.com/special-place-administrator/symforge/commit/739c867bdafb7e0b0b6b2e1e59d41192de9f35ea))

## [7.0.0](https://github.com/special-place-administrator/symforge/compare/v6.3.0...v7.0.0) (2026-04-03)


### ⚠ BREAKING CHANGES

* ships a broad trust-calibration pass across sidecar hints, read/query outputs, edit safety signaling, transactional batch operations, and harness init guidance deduplication.
* Line numbers in search_symbols, get_symbol_context, trace_symbol, inspect_match, and sidecar endpoints shift from 0-indexed to 1-indexed. Clients parsing these outputs numerically must account for the +1 change.
* rename Tokenizor → SymForge

### Features

* **01-01:** implement kind-tier disambiguation in resolve_symbol_selector ([2e11ac4](https://github.com/special-place-administrator/symforge/commit/2e11ac4985e690e034da2982fcf3b900d734d30b))
* **02-01:** hook diagnostics — verbose mode, port-missing vs stale, one-time hint ([4547428](https://github.com/special-place-administrator/symforge/commit/4547428aeb984e727947ea94a3f0e40451060216))
* 4 UX improvements from external review feedback (Wave 1) ([430a86a](https://github.com/special-place-administrator/symforge/commit/430a86a4fad65c9725f21037d070349e165f8cee))
* 4 UX improvements from external review feedback (Wave 2) ([0e773e6](https://github.com/special-place-administrator/symforge/commit/0e773e603ee51178d8cc1e36a851ada55c26e9f6))
* add 'ask' smart query tool + token metrics (Suggestions 2 & 10) ([291f544](https://github.com/special-place-administrator/symforge/commit/291f5448e59f7eba497e0dd732489a9eb0d25487))
* add 'summary' verbosity level with heuristic summaries (Suggestion 1) ([90cda29](https://github.com/special-place-administrator/symforge/commit/90cda292fda858c4460dd37c7ec3b48ef79fc65b))
* add AdmissionTier enum and size threshold constants ([48cc242](https://github.com/special-place-administrator/symforge/commit/48cc242852a06b463119757408b911b41efcf493))
* add binary content sniff with NUL, UTF-8, and control-byte heuristics ([e7bd071](https://github.com/special-place-administrator/symforge/commit/e7bd0713957a611e0fabd42f3f6c0e68b042782f))
* add estimate field to all read tool inputs + fix dry_run deserialization bug ([537fa16](https://github.com/special-place-administrator/symforge/commit/537fa165c33f403ce5b828c26c164e921792c9e5))
* add estimate parameter for context budget planning (Suggestion 4) ([b1e3239](https://github.com/special-place-administrator/symforge/commit/b1e3239ce622b77fb9d56bf2e7529e9ee72072cf))
* add extension denylist for admission control ([6159487](https://github.com/special-place-administrator/symforge/commit/6159487c7468fcb037ae2ed6a603eeb9c56c02b9))
* add LineEnding detection and normalization helpers (C1 prep) ([a82a727](https://github.com/special-place-administrator/symforge/commit/a82a72763a9060650553c68da33a0e2d5bd8dc95))
* add match-occurrence retrieval and watcher reconciliation health reporting ([5043a5a](https://github.com/special-place-administrator/symforge/commit/5043a5a64e74d7edc43a6f087572837ae2b7d501))
* add session recording to all 24 tools for complete context_inventory tracking ([679c7e3](https://github.com/special-place-administrator/symforge/commit/679c7e3715ba2ff0d9594c5bc39645e64f9f30c0))
* add SkippedFile struct and store integration for admission tiers ([b94aeeb](https://github.com/special-place-administrator/symforge/commit/b94aeeb8758ccce92733fefc74a97858b6ccf978))
* add word stemming to explore concept matching for inflected queries ([223356d](https://github.com/special-place-administrator/symforge/commit/223356dfd4598af8f59d7500979a29255a57356a))
* **adoption:** add hook outcome metrics ([cce4473](https://github.com/special-place-administrator/symforge/commit/cce4473e5024ade6de5f5eea2b6daef87ff38c2e))
* **adoption:** add workflow sidecar adapters ([143ac0b](https://github.com/special-place-administrator/symforge/commit/143ac0bfac5f09a978e4f85bc70c75f2028a0477))
* **adoption:** define owned workflows for hooks ([0185310](https://github.com/special-place-administrator/symforge/commit/0185310fdbae3fded168d5a4249792372f666234))
* **adoption:** steer protocol read workflows ([106cdda](https://github.com/special-place-administrator/symforge/commit/106cddaec9ecb7f0f1a82dae22cb54d95e942ff8))
* **adoption:** tighten hook routing for source workflows ([5b9426a](https://github.com/special-place-administrator/symforge/commit/5b9426a9aa3f6f02f3f27575602bb35f1b74a8f9))
* aggregate token savings across tool handlers ([25343e9](https://github.com/special-place-administrator/symforge/commit/25343e9b20774d3489ca9610955ca81aeda38e5b))
* analyze_file_impact shows clear status taxonomy (U4) ([263834f](https://github.com/special-place-administrator/symforge/commit/263834f3d8dd2a4c22911eed69a506edb7c13bd6))
* batch_edit dry_run mode (U5) ([4166196](https://github.com/special-place-administrator/symforge/commit/41661963168aced0d37c7931628c3dcf49f6b550))
* batch_rename supplemental qualified path scan with confidence classification ([e75f2d4](https://github.com/special-place-administrator/symforge/commit/e75f2d40d657ea668660d3160c884feaf396ec96))
* clean npm cache after install to reclaim disk space ([b1c4a35](https://github.com/special-place-administrator/symforge/commit/b1c4a353fc1d1985c31e363d1be22f7f4e17a440))
* **config:** add structured syntax diagnostics ([7360beb](https://github.com/special-place-administrator/symforge/commit/7360bebb290e50bc65fee0a63ef5118cdc72117c))
* convention-aware concept enrichment in explore ([fec34d2](https://github.com/special-place-administrator/symforge/commit/fec34d21a3f06b6ba491de94399814166ef7abea))
* conventions detection, edit planning, investigation mode (Suggestions 3, 8, 9) ([aebf288](https://github.com/special-place-administrator/symforge/commit/aebf28899278be7554f02200ea124ab269e49232))
* daemon fallback, callee dedup, token budget, search defaults ([d13e76b](https://github.com/special-place-administrator/symforge/commit/d13e76b77308b16d42bf721400f10ef6215cc896))
* derive fallback explore clusters ([2173acb](https://github.com/special-place-administrator/symforge/commit/2173acbe9da35ae966b7e25ac78706117335fcc6))
* **edit:** add dry_run to replace_symbol_body, insert_symbol, delete_symbol, edit_within_symbol ([1f401d8](https://github.com/special-place-administrator/symforge/commit/1f401d82444bf19bd86746b2346b8aa3082f9880))
* **edit:** track item byte ranges on symbols ([da3294c](https://github.com/special-place-administrator/symforge/commit/da3294c623589382bfa8dd2311944d868a6804e7))
* enrich explore with manifest dependencies for derive-only crates ([85005e2](https://github.com/special-place-administrator/symforge/commit/85005e267b04cb3e69d5cd466dd29320ed010036))
* explore filters noise by default (U1) ([f14b702](https://github.com/special-place-administrator/symforge/commit/f14b702b0a68ba6c88b5c96327cbdf5d701e5d72))
* **find_dependents:** show symbol names in mermaid and dot edge labels ([e358b77](https://github.com/special-place-administrator/symforge/commit/e358b77697162cd859fd41f72a301ee23f64d5f3))
* get_file_content mode enum for clearer API (U10) ([244be75](https://github.com/special-place-administrator/symforge/commit/244be753b08275b80f3bd1d8a11214a74f642f03))
* **get_repo_map:** paginate detail=full output with max_files parameter ([8c15c5d](https://github.com/special-place-administrator/symforge/commit/8c15c5d2bc55768123a754fa6a74c23bb9b6c131))
* git churn in ranking, expanded guidance blocks, improved tool descriptions ([4cf1e6e](https://github.com/special-place-administrator/symforge/commit/4cf1e6e782d79e52554c23aae98590fb5a60feb5))
* health shows partial parse file paths (U8) ([8560114](https://github.com/special-place-administrator/symforge/commit/856011485f82605d41e93651748bf64db1486c91))
* **health:** list failed files with error messages in health report ([7306089](https://github.com/special-place-administrator/symforge/commit/73060890c96058223dd04dffd91b06087fb4dd1a))
* implement admission gate with tiered file classification ([9e69e23](https://github.com/special-place-administrator/symforge/commit/9e69e238f50b3364545fa3844cbb5b03ae7ad925))
* improve error context on failures (Suggestion 7) ([8169a6e](https://github.com/special-place-administrator/symforge/commit/8169a6e2ed0c1a7d212e865456ecabf29018dfed))
* improve init trust detection and query routing ([82b433a](https://github.com/special-place-administrator/symforge/commit/82b433ad6f104437ae9e165d71481c42a58c1b43))
* **index:** Sprint 0 — index freshness guarantee via mtime tracking ([29d60d6](https://github.com/special-place-administrator/symforge/commit/29d60d6fe0e83f3c79856c38796bc02c62f62bea))
* **init:** add alwaysAllow to Claude MCP entry and expand CLAUDE.md guidance ([4ee5f53](https://github.com/special-place-administrator/symforge/commit/4ee5f535546e829850c6136b5785bd56b23c7732))
* **init:** harden client guidance rollout ([f30667c](https://github.com/special-place-administrator/symforge/commit/f30667ce885ba5040a4a40751b7401080fad8977))
* **json:** add JSONC comment stripping for tsconfig.json support ([c3c208f](https://github.com/special-place-administrator/symforge/commit/c3c208fb496449efe2cd14a7ee82562bd4088df9))
* lenient vec deserializer, semantic search ranking, Kilo Code init, SymForge rename plan ([c048274](https://github.com/special-place-administrator/symforge/commit/c04827422bb02a04ff4222864b0c09ff014ca70d))
* per-tool call counters in health output (U9) ([d41bfb5](https://github.com/special-place-administrator/symforge/commit/d41bfb5730d07b2b0271ee1947eec0306f07375c))
* per-tool token efficiency tracking (Suggestion 10) ([a588365](https://github.com/special-place-administrator/symforge/commit/a5883651c7bc384b431e7964bd3f0e1e04a4682c))
* rename Tokenizor → SymForge ([6366cd0](https://github.com/special-place-administrator/symforge/commit/6366cd0c7f51bc496cceb6ae255e22d95f109183))
* richer verbosity=signature includes visibility and return type (U6) ([eef2926](https://github.com/special-place-administrator/symforge/commit/eef2926f057e3020200bb13cc1dd47b9ee9bf76e))
* RTK adoption milestone — symbol disambiguation tests, hook diagnostics, docs links ([9bc3ead](https://github.com/special-place-administrator/symforge/commit/9bc3ead5c49813998a9987b3e2066398313d48db))
* search_symbols browse mode without query (U2) ([3326342](https://github.com/special-place-administrator/symforge/commit/33263428425acd01a9cfba460d96d0b5534257b5))
* **search_text:** annotate which term matched in OR-term searches ([e53a7f7](https://github.com/special-place-administrator/symforge/commit/e53a7f748e55afe0983d4710b6375edc01058397))
* session context tracking + context_inventory tool (Suggestion 6) ([40c8de5](https://github.com/special-place-administrator/symforge/commit/40c8de54c8e0de9e78b6c6cd3cdc03950e4d21ce))
* show Tier 2 tags and Tier 3 footer in repo_map ([05d23eb](https://github.com/special-place-administrator/symforge/commit/05d23eb7b23c06c607e6adacac00ae7edbb2c7dc))
* Sprint 14 — trust fixes + tiered admission control ([b7a9296](https://github.com/special-place-administrator/symforge/commit/b7a92963b3f9c55be08e73e04eba6bd70901b1bf))
* strip leading articles from ask queries and show original when transformed ([daea07b](https://github.com/special-place-administrator/symforge/commit/daea07b06f6cc39f6e1b93a540a612978b2e41ce))
* surface convention-enrichment annotation in explore output ([baed689](https://github.com/special-place-administrator/symforge/commit/baed689f3674ef5e2c4f4bb0a069a8553c6d8a48))
* tree-sitter partial-parse diagnostics and AST-based diff_symbols ([0b5f91c](https://github.com/special-place-administrator/symforge/commit/0b5f91cf6e2dd748558fbe827216a7060a575024))
* trust-calibrate SymForge release ([b585660](https://github.com/special-place-administrator/symforge/commit/b585660b707f2a3f32b8e2b64ab6bdb807f3753e))
* update init templates with new tools and guidance rules ([be7d687](https://github.com/special-place-administrator/symforge/commit/be7d687464f9d89a465e8a104904f18aa065c17d))
* wire admission gate into discovery walk ([51c73f7](https://github.com/special-place-administrator/symforge/commit/51c73f7b125d13a88179d9e1e2cf535e28839888))
* workflow recipe prompts + 3 new prompts (Suggestion 5) ([cb6ccdc](https://github.com/special-place-administrator/symforge/commit/cb6ccdccfeab232f6e529f8553da5df4db28b4ae))


### Bug Fixes

* 26 bug fixes across parsers, protocol, indexing, sidecar, and npm ([b2abebc](https://github.com/special-place-administrator/symforge/commit/b2abebc4710580cdb62fe984809c4ddc949cd8a6))
* 4 display/UX improvements in search, outline, repo_map, and get_symbol ([b3a449c](https://github.com/special-place-administrator/symforge/commit/b3a449c81a0802070b035cc41f8e44b46ad50a50))
* add exe/dll/so/dylib/class to denylist (C2-lite) ([14d0459](https://github.com/special-place-administrator/symforge/commit/14d04593df00b7cbc92643aeb5c7109026a045cb))
* add missing estimate=true handler to get_file_content ([72cd834](https://github.com/special-place-administrator/symforge/commit/72cd834b87ec852d7967557a66869e716c6ab2ff))
* add missing gitignore/noise_class field initializers across codebase ([c8088f9](https://github.com/special-place-administrator/symforge/commit/c8088f9f0953b004e015c2a707db89dae3597ced))
* add missing sibling_limit/overflow fields to initializers ([b25f4a5](https://github.com/special-place-administrator/symforge/commit/b25f4a5a34a007ad9a56757cd1a62ce7c9f92157))
* add NOT-for tips to 5 tool descriptions missing them ([14a8fad](https://github.com/special-place-administrator/symforge/commit/14a8fad0de46c1b3aa35711a8bbda3f8efee2c94))
* add panic hook to clean up sidecar port files on crash ([6758696](https://github.com/special-place-administrator/symforge/commit/6758696a02d052d6646721629a51c67ee8978f93))
* address all actionable feedback from 3 external code reviews ([61d2757](https://github.com/special-place-administrator/symforge/commit/61d2757a8b9bbc425b73eec512dc75c470be630d))
* around_symbol returns full indexed symbol span (B2) ([3b06c2a](https://github.com/special-place-administrator/symforge/commit/3b06c2a735ad3edd0ac691851a944d66797b06f9))
* batch schema parity + find_references supplemental text fallback ([dffb8b8](https://github.com/special-place-administrator/symforge/commit/dffb8b8276da05dd927b5b3530843ce1e958aa55))
* batch_edit dry_run byte count + auto-detect regex in search_text ([29474c2](https://github.com/special-place-administrator/symforge/commit/29474c21bd33b1da6858bdbb4179eaa3ac9611a1))
* batch_edit shows ROLLED BACK message on failure (B4) ([3ab8358](https://github.com/special-place-administrator/symforge/commit/3ab83587c7bbee77bd1f1cfe1b1980066630da8f))
* batch_insert no extra blank line before function (B1) ([3409548](https://github.com/special-place-administrator/symforge/commit/34095482b8275811cb5373003e367c0b07dcfec0))
* batch_rename atomic rollback on failure, batch_edit/batch_insert best-effort with correct index state ([6b332f3](https://github.com/special-place-administrator/symforge/commit/6b332f3f18484a8e14ed35731961eac98311b18f))
* batch_rename review fixes — atomic rollback, dead code, dedup ([0a7844e](https://github.com/special-place-administrator/symforge/commit/0a7844ee1b8ce77b462f1f6a3b51b53c1ce17a22))
* break infinite reconciliation loop caused by hash-skip mtime drift ([54a03f8](https://github.com/special-place-administrator/symforge/commit/54a03f88ffc6862e5096531b1f705589994120b9))
* **build:** silence tree-sitter-scss scanner warnings ([80c69e4](https://github.com/special-place-administrator/symforge/commit/80c69e4e6765ba64ba41522952b8e8b5e02e08b8))
* **bundle:** resolve impl suggestions and dependency-aware limits ([d5bfa6a](https://github.com/special-place-administrator/symforge/commit/d5bfa6aa85e3dfd719340301e1eff01d4bb1c069))
* capture qualified calls inside Rust macro bodies, surface callees in default get_symbol_context ([048b578](https://github.com/special-place-administrator/symforge/commit/048b57840b6fd820241f011f2f8533871acb7bed))
* **ci:** enforce conventional commits and verify main pushes ([5ec0b78](https://github.com/special-place-administrator/symforge/commit/5ec0b789af1012550db40128c9e501635e14de0a))
* **ci:** force Node 24 for GitHub Actions runtime ([98b666a](https://github.com/special-place-administrator/symforge/commit/98b666afe6a2157ec85b80a1c91cfa812247cfbd))
* **ci:** make npm publish idempotent ([bdddc47](https://github.com/special-place-administrator/symforge/commit/bdddc47bf1558be1f5068e0b6a25dcfa4a9f7aab))
* **ci:** tolerate force-pushed conventional commit ranges ([8a70ffa](https://github.com/special-place-administrator/symforge/commit/8a70ffaa8cd3e401b0f19563c59ac99cf08a9290))
* **ci:** use cargo check for workflow verification ([5c6a13b](https://github.com/special-place-administrator/symforge/commit/5c6a13b5cea9be1efc4df14fbbda5b21072df338))
* code review feedback — tests and safety fixes ([c2454ea](https://github.com/special-place-administrator/symforge/commit/c2454ea2bbd1ec0c2f2acb357bd9f158fd101584))
* codex audit — uncommitted symbol diff, daemon tool dispatch, lenient vec deserialization ([26f2528](https://github.com/special-place-administrator/symforge/commit/26f2528d34f6057621f12e828ae1171269966618))
* complete audit remediation — language tests, deferred fixes, dedup ([c04e849](https://github.com/special-place-administrator/symforge/commit/c04e84968e406e1662b5f36fa7df2ae923ab15fe))
* complete parking_lot::RwLock migration across live_index and protocol ([c48d865](https://github.com/special-place-administrator/symforge/commit/c48d865c2e9c717b89da9d446a51f326af5d3052))
* comprehensive codebase audit — 18 bug fixes across parsers, core engine, and protocol ([3b0cd44](https://github.com/special-place-administrator/symforge/commit/3b0cd442841748ebb30e05d2b23d13b57246ee5e))
* CSS @layer/[@container](https://github.com/container) extraction — use generic at_rule node kind ([97fc47f](https://github.com/special-place-administrator/symforge/commit/97fc47fd201c89888231e21e4f665d919c73b3fc))
* daemon lifecycle hardening — stale lock detection, fast-fail proxy, cleanup (DL1-DL4) ([8350d7e](https://github.com/special-place-administrator/symforge/commit/8350d7e3b5de2bb16d85d69a817ca459fb43e829))
* daemon proxy deadlock under concurrent tool calls + request governor ([541dd68](https://github.com/special-place-administrator/symforge/commit/541dd688e9956d75818733740764320513e9c8ab))
* **dependents:** filter false positives from non-pub symbol name collisions ([0bf3c77](https://github.com/special-place-administrator/symforge/commit/0bf3c77b1d41ea7ba383d18b83029ba15f0855a6))
* **diff_symbols:** show omission note in compact mode when files have no symbol changes ([c6eade8](https://github.com/special-place-administrator/symforge/commit/c6eade8fc0770c042c26b9a0b258a7b619537e21))
* **diff_symbols:** skip type keywords in C# const declarations ([00049d2](https://github.com/special-place-administrator/symforge/commit/00049d264058dafbbbb2ab72816cdfc8dd608164))
* emit strict MCP array schemas for optional list params ([3519470](https://github.com/special-place-administrator/symforge/commit/3519470c2cafc36cd1d4c7e9b4455b8f3020623f))
* explore depth=2 shows symbol-level callers, get_symbol uses tier disambiguation ([9a22035](https://github.com/special-place-administrator/symforge/commit/9a22035872fb0ad58026a9dc50d66f0440e98f4b))
* explore depth=2 shows symbol-level callers, get_symbol uses tier… ([4c1f588](https://github.com/special-place-administrator/symforge/commit/4c1f588fc1bd7282f74251eadb587913f89658a2))
* find_dependents resolves workspace crate paths ([418652a](https://github.com/special-place-administrator/symforge/commit/418652a42bd3e1443602cb87cd1eed2d7e4c0574))
* find_dependents resolves workspace crate paths (B4) ([f02819d](https://github.com/special-place-administrator/symforge/commit/f02819db942530bcbb974fe946d05224bb953ab6)), closes [#89](https://github.com/special-place-administrator/symforge/issues/89)
* find_dependents_for_file catches qualified calls without imports ([8e1fd7d](https://github.com/special-place-administrator/symforge/commit/8e1fd7d442c74fc20a5fa1bd6bbd69a295d42cc9))
* **find_references:** explain why classes/structs have no implementations ([7566e2a](https://github.com/special-place-administrator/symforge/commit/7566e2a3bc9a928d55b6266307738029d63487e9))
* **get_file_content:** explain why zero-symbol files have no matches ([d4420f8](https://github.com/special-place-administrator/symforge/commit/d4420f8df45c949f6bc3be1731eca17fe620c7db))
* get_file_context sections filter masked by 800-byte hook budget ([5ff4c9e](https://github.com/special-place-administrator/symforge/commit/5ff4c9e3031e099d7321c5b906ba3109f031e3f7))
* **get_symbol_context:** auto-resolve path and show empty-references message ([8b4caf5](https://github.com/special-place-administrator/symforge/commit/8b4caf5b49e6ac402744b23eddd1926762421b62))
* Go method names, SCSS $variable extraction, language filter completeness ([4156d41](https://github.com/special-place-administrator/symforge/commit/4156d41d24af8739494482ea4ba868a69fec747d))
* handle SIGTERM for daemon graceful shutdown (C5) ([3faef1f](https://github.com/special-place-administrator/symforge/commit/3faef1f772ef679f137229cc7fc9d47891c132f6))
* improve ask and edit tool ergonomics ([6ad6a33](https://github.com/special-place-administrator/symforge/commit/6ad6a33cd7eb47c3236a330c9703ec1962565e2a))
* improve reconciliation logging to diagnose stale-file loops ([cff17ed](https://github.com/special-place-administrator/symforge/commit/cff17edd4fac7250275654b90fba7c88709317ea))
* index safety hardening and tool output correctness ([c952759](https://github.com/special-place-administrator/symforge/commit/c952759567b0a1e7a2de4cd56ccb0576eaa792a8))
* index safety hardening and tool output correctness ([59720c5](https://github.com/special-place-administrator/symforge/commit/59720c5fc9e48eccb2fc41d291a804b0a69cbd55))
* **init:** canonicalize SymForge Codex guidance and allowlists ([7680757](https://github.com/special-place-administrator/symforge/commit/76807570e24995dfc05ad6df022cbd1fbab25bfa))
* investigation_suggest no longer reports empty state when session has activity ([fb85a8e](https://github.com/special-place-administrator/symforge/commit/fb85a8e5163ca9ce81ecbe777fd9bc57cad63d61))
* **kilo:** trigger release for strict-provider compatibility ([a852955](https://github.com/special-place-administrator/symforge/commit/a852955707cc9da1133f9a872d8d7b3b955988e7))
* lenient SingleEdit deserialization — accept shorthand DSL strings ([d61fd79](https://github.com/special-place-administrator/symforge/commit/d61fd7924117c7a2eee7ee703c4f1a875abc1ae1))
* make SymForge MCP schemas compatible with strict OpenAI clients ([aaa42f2](https://github.com/special-place-administrator/symforge/commit/aaa42f2aa6cb17af2a7d149abd5d0483f8086513))
* new tools broken in daemon mode — missing proxy_tool_call ([cdde2a9](https://github.com/special-place-administrator/symforge/commit/cdde2a977fc0a78622c14c5227585271201bb926))
* non-ASCII panic in doc scanning and deterministic circuit-breaker (CR1, CR2) ([1e52aaf](https://github.com/special-place-administrator/symforge/commit/1e52aaf7468d425fac21371398a631a93d6c5bfe))
* normalize exact get_file_content paths and backfill mtime_secs in integration fixtures ([fb398b1](https://github.com/special-place-administrator/symforge/commit/fb398b1dcbfdc59055fea328e995bdb1d9ba114c))
* **npm:** keep global auto-init out of workspaces ([1626dfa](https://github.com/special-place-administrator/symforge/commit/1626dfa89a6da5a9e14b0107df3eec7a0b325c61))
* **npm:** persist wrapper install metadata ([eeac029](https://github.com/special-place-administrator/symforge/commit/eeac0298ef08253fdf70042ba5d2f78f142faea2))
* pin CI/release workflows to Rust 1.94.0 matching rust-toolchain.toml ([41d23ab](https://github.com/special-place-administrator/symforge/commit/41d23ab3fd83205773f9289cf95cd142fd2cb1b9))
* pin Rust toolchain to 1.94.0 via rust-toolchain.toml ([fed0e20](https://github.com/special-place-administrator/symforge/commit/fed0e20560de0356cb86bf5dc9319af2867e770c))
* prevent async runtime starvation under concurrent subagent load ([74f1d54](https://github.com/special-place-administrator/symforge/commit/74f1d54f0f26dba97a619fb5b69e645c1d702034))
* prevent async runtime starvation under concurrent subagent load ([2ed134a](https://github.com/special-place-administrator/symforge/commit/2ed134aa34a82f258eb37be4121c341272cc85d6))
* prevent non-ASCII panic in find_qualified_usages (batch_rename crash) ([8555d0c](https://github.com/special-place-administrator/symforge/commit/8555d0ce0b3ed39acf7d170c0878d4e94267a2ef))
* qualified Rust caller resolution + context_inventory daemon proxy ([175b53e](https://github.com/special-place-administrator/symforge/commit/175b53edef61b7541310a430f87acffb08eea563))
* reindex from disk after writes, not from in-memory buffer ([d605498](https://github.com/special-place-administrator/symforge/commit/d6054988db18ad6cf82e3a82cca2c054a1c5f52b))
* **release:** add noncommercial licensing and kill-all npm updates ([17354c6](https://github.com/special-place-administrator/symforge/commit/17354c6adba9e75a09bc3929b753881552ec929a))
* reliability improvements for what_changed, ask routing, health detail, and startup test ([ec8b0a6](https://github.com/special-place-administrator/symforge/commit/ec8b0a6cc02a6a2bcc925e14e1239c27bc32edd2))
* reliability improvements for what_changed, ask, health, and startup ([181e6fa](https://github.com/special-place-administrator/symforge/commit/181e6fae3ef8cf48f2af62aff850fcef7aad2b53))
* remediate reviewer feedback from external codebase testing ([80303a9](https://github.com/special-place-administrator/symforge/commit/80303a9a9558d05ad281740552fdc95b349270f3))
* remove stale args from for_current_code_search() and suppress unused variable warning ([9652e53](https://github.com/special-place-administrator/symforge/commit/9652e53ca1c4a22d1968ba8f552191e830b47386))
* replace std::sync::Mutex with parking_lot::Mutex to prevent poison cascades ([04651d0](https://github.com/special-place-administrator/symforge/commit/04651d09f5fccbad4e4ee267b2cf4c2ee953ea44))
* resolve 16 bugs across mtime propagation, line indexing, correctness, and concurrency ([8cfda64](https://github.com/special-place-administrator/symforge/commit/8cfda649bf2bd0a017fa72643bce088f817cd1bc))
* resolve 4 bugs from code review ([31b9a0c](https://github.com/special-place-administrator/symforge/commit/31b9a0c78b5576262c74946adaf00aad8262ebcb))
* resolve 5 tool bugs from hands-on review ([6d11014](https://github.com/special-place-administrator/symforge/commit/6d1101448f993946699028e26ecf8852f81073be))
* resolve all clippy warnings and add SAFETY comments ([b687aa4](https://github.com/special-place-administrator/symforge/commit/b687aa45d5e83d3562cad64dd90a29aae8a45f62))
* restore missing [[package]] header in Cargo.lock after rebase conflict resolution ([67d9327](https://github.com/special-place-administrator/symforge/commit/67d932778c02a9899ffec904560868927604d968))
* revert worker_threads override — spawn_blocking handles concurrency ([a5d5d4e](https://github.com/special-place-administrator/symforge/commit/a5d5d4e77dfd2d438a42ef2161fd1c7111584abd))
* review feedback — Q3 robust name extraction, Q6 UTF-8 safe truncation ([41e17d2](https://github.com/special-place-administrator/symforge/commit/41e17d23a4b4ce9e89d9fdfbceb5ba089a9880a0))
* rewrite open_project_session with double-checked locking (C6) ([b04b0d0](https://github.com/special-place-administrator/symforge/commit/b04b0d099fb035ca1bac9c76c49d542cae1d8102))
* saturate token-estimate casts, harden unwrap, fix cargo fmt ([9b76caa](https://github.com/special-place-administrator/symforge/commit/9b76caa22b90e87f44490caa54ceb4f17e99a22d))
* scope query heuristics and route confidence ([df290bc](https://github.com/special-place-administrator/symforge/commit/df290bc0230746c17da1424acb215f8ed57cf360))
* security patches, parser improvements, parallelism fixes, and review follow-ups ([2b1d5cb](https://github.com/special-place-administrator/symforge/commit/2b1d5cbafed5100ea833d0f4b41da41b1d87cb27))
* show_line_numbers works with around_symbol and around_match (B3) ([4befe8a](https://github.com/special-place-administrator/symforge/commit/4befe8a8f8e734e722088632c23ac81489bf42ce))
* sidecar reliability + reviewer feedback remediation ([8b41990](https://github.com/special-place-administrator/symforge/commit/8b419904451053df06d82f06ba7921f435395af0))
* surface tool panics as immediate error responses instead of stalls ([31ae935](https://github.com/special-place-administrator/symforge/commit/31ae935642876711383897ba3b779ea4c2dc7b52))
* Swift enum/extension/protocol detection and Angular template robustness ([af34df2](https://github.com/special-place-administrator/symforge/commit/af34df266b7cb34ba4dbd24853b585554bba7308))
* symbol kind filter accepts semantic aliases (variable, function, etc.) ([2e80fb5](https://github.com/special-place-administrator/symforge/commit/2e80fb5cd96b441e256e4a7c7afb5fad20fbbfbe))
* **tests:** remove unused unix import in edit tests ([919aff6](https://github.com/special-place-administrator/symforge/commit/919aff668a88636a000c10b9f2ac1c4e3a01aba8))
* **test:** update assertion for changed zero-symbol message ([90a5722](https://github.com/special-place-administrator/symforge/commit/90a57229f697bfc541adc4d58ea35f1f2dc53295))
* thread LineEnding through all edit helpers for CRLF preservation (C1) ([dda40b4](https://github.com/special-place-administrator/symforge/commit/dda40b4c0c8a99259a93af4c9ff65bb696e9e337))
* tighten query and format heuristics ([5806f1a](https://github.com/special-place-administrator/symforge/commit/5806f1a89370e84b24ced44157c81cde8b04684a))
* **trust:** calibrate temporal signals and health output ([e2fafbe](https://github.com/special-place-administrator/symforge/commit/e2fafbea393f55fde6e2b22d499c1ad3f0d96b3c))
* **trust:** tighten discovery and context signals ([0c0fbe7](https://github.com/special-place-administrator/symforge/commit/0c0fbe72e4c2f2318654be1944abc500311e14df))
* update 5 stale test assertions to match improved error messages ([4ebb8b5](https://github.com/special-place-administrator/symforge/commit/4ebb8b56a09b538bfcc171dfdee07a35576ceb90))
* update installer test assertion for execFileSyncFn version check ([f6ed05d](https://github.com/special-place-administrator/symforge/commit/f6ed05dd601105a4a3249cf2844b4da91210c560))
* update rollback tests for tempfile-based atomic writes ([548c2bb](https://github.com/special-place-administrator/symforge/commit/548c2bbede27be2a233fe35a2d7f6b8aa0aee45b))
* use unique temp files in atomic_write_file (C3) ([dddcb16](https://github.com/special-place-administrator/symforge/commit/dddcb16eb85dc294354691e9fc470adbca5ce9bd))
* validate splice overlap in batch_rename (C4) ([0c80b74](https://github.com/special-place-administrator/symforge/commit/0c80b74903261a09837a7038998bf3d61898d274))
* watcher recv_timeout blocks tokio worker — use try_recv + async sleep ([a4b7d34](https://github.com/special-place-administrator/symforge/commit/a4b7d34db100eac9528325f6f3bdbdd58827d54d))
* wave 1 audit remediation — 12 safety and correctness fixes ([b02bd12](https://github.com/special-place-administrator/symforge/commit/b02bd1203ba78ee404e204dda80ae54328ba3642))
* wave 2 audit remediation — 10 reliability and consistency fixes ([a293819](https://github.com/special-place-administrator/symforge/commit/a2938196992b09c8ddb2a0fc40a732ff499627e0))
* wave 3 audit remediation — polish, docs, and remaining fixes ([c7c2ba8](https://github.com/special-place-administrator/symforge/commit/c7c2ba8f6129565f16a3346026ae35606f8f60f5))
* widen common-name warning in find_references to trigger on ref c… ([32b22b4](https://github.com/special-place-administrator/symforge/commit/32b22b432d54e1a02fced916ebfbdb08d21b8c1a))
* widen common-name warning in find_references to trigger on ref count alone ([2c3cc4b](https://github.com/special-place-administrator/symforge/commit/2c3cc4b472d1df4645b3008d32451ffe1a104082))
* wire session tracking into tool handlers + InsertTarget string shorthand ([5c7feb3](https://github.com/special-place-administrator/symforge/commit/5c7feb3cefed0ea9af8980a5622d82532d88e7b5))
* wrap daemon sidecar handlers with governor + spawn_blocking ([d665e41](https://github.com/special-place-administrator/symforge/commit/d665e415a5f889229ef7c5dc8a18a4c9cadc36bd))
* wrap env var manipulation in unsafe blocks for Rust 2024 edition compliance ([c363499](https://github.com/special-place-administrator/symforge/commit/c3634999e2c91fcbc57a2fb92decc5f6a217a77f))
* wrap repair_file_indices in catch_unwind to prevent double-panic abort ([e2d2e97](https://github.com/special-place-administrator/symforge/commit/e2d2e976f4bef08ca678ad5e2a2142f5f5c48f2a))

## [6.3.0](https://github.com/special-place-administrator/symforge/compare/v6.2.0...v6.3.0) (2026-04-03)


### Features

* enrich explore with manifest dependencies for derive-only crates ([85005e2](https://github.com/special-place-administrator/symforge/commit/85005e267b04cb3e69d5cd466dd29320ed010036))

## [6.2.0](https://github.com/special-place-administrator/symforge/compare/v6.1.0...v6.2.0) (2026-04-03)


### Features

* surface convention-enrichment annotation in explore output ([baed689](https://github.com/special-place-administrator/symforge/commit/baed689f3674ef5e2c4f4bb0a069a8553c6d8a48))

## [6.1.0](https://github.com/special-place-administrator/symforge/compare/v6.0.6...v6.1.0) (2026-04-03)


### Features

* add session recording to all 24 tools for complete context_inventory tracking ([679c7e3](https://github.com/special-place-administrator/symforge/commit/679c7e3715ba2ff0d9594c5bc39645e64f9f30c0))
* add word stemming to explore concept matching for inflected queries ([223356d](https://github.com/special-place-administrator/symforge/commit/223356dfd4598af8f59d7500979a29255a57356a))
* convention-aware concept enrichment in explore ([fec34d2](https://github.com/special-place-administrator/symforge/commit/fec34d21a3f06b6ba491de94399814166ef7abea))
* strip leading articles from ask queries and show original when transformed ([daea07b](https://github.com/special-place-administrator/symforge/commit/daea07b06f6cc39f6e1b93a540a612978b2e41ce))
* tree-sitter partial-parse diagnostics and AST-based diff_symbols ([0b5f91c](https://github.com/special-place-administrator/symforge/commit/0b5f91cf6e2dd748558fbe827216a7060a575024))


### Bug Fixes

* investigation_suggest no longer reports empty state when session has activity ([fb85a8e](https://github.com/special-place-administrator/symforge/commit/fb85a8e5163ca9ce81ecbe777fd9bc57cad63d61))

## [6.0.6](https://github.com/special-place-administrator/symforge/compare/v6.0.5...v6.0.6) (2026-04-03)


### Bug Fixes

* capture qualified calls inside Rust macro bodies, surface callees in default get_symbol_context ([048b578](https://github.com/special-place-administrator/symforge/commit/048b57840b6fd820241f011f2f8533871acb7bed))

## [6.0.5](https://github.com/special-place-administrator/symforge/compare/v6.0.4...v6.0.5) (2026-04-03)


### Bug Fixes

* saturate token-estimate casts, harden unwrap, fix cargo fmt ([9b76caa](https://github.com/special-place-administrator/symforge/commit/9b76caa22b90e87f44490caa54ceb4f17e99a22d))

## [6.0.4](https://github.com/special-place-administrator/symforge/compare/v6.0.3...v6.0.4) (2026-04-02)


### Bug Fixes

* scope query heuristics and route confidence ([df290bc](https://github.com/special-place-administrator/symforge/commit/df290bc0230746c17da1424acb215f8ed57cf360))
* tighten query and format heuristics ([5806f1a](https://github.com/special-place-administrator/symforge/commit/5806f1a89370e84b24ced44157c81cde8b04684a))

## [6.0.3](https://github.com/special-place-administrator/symforge/compare/v6.0.2...v6.0.3) (2026-04-02)


### Bug Fixes

* resolve all clippy warnings and add SAFETY comments ([b687aa4](https://github.com/special-place-administrator/symforge/commit/b687aa45d5e83d3562cad64dd90a29aae8a45f62))

## [6.0.2](https://github.com/special-place-administrator/symforge/compare/v6.0.1...v6.0.2) (2026-04-02)


### Bug Fixes

* reliability improvements for what_changed, ask routing, health detail, and startup test ([ec8b0a6](https://github.com/special-place-administrator/symforge/commit/ec8b0a6cc02a6a2bcc925e14e1239c27bc32edd2))
* reliability improvements for what_changed, ask, health, and startup ([181e6fa](https://github.com/special-place-administrator/symforge/commit/181e6fae3ef8cf48f2af62aff850fcef7aad2b53))

## [6.0.1](https://github.com/special-place-administrator/symforge/compare/v6.0.0...v6.0.1) (2026-04-02)


### Bug Fixes

* improve ask and edit tool ergonomics ([6ad6a33](https://github.com/special-place-administrator/symforge/commit/6ad6a33cd7eb47c3236a330c9703ec1962565e2a))

## [6.0.0](https://github.com/special-place-administrator/symforge/compare/v5.1.2...v6.0.0) (2026-04-02)


### ⚠ BREAKING CHANGES

* ships a broad trust-calibration pass across sidecar hints, read/query outputs, edit safety signaling, transactional batch operations, and harness init guidance deduplication.
* Line numbers in search_symbols, get_symbol_context, trace_symbol, inspect_match, and sidecar endpoints shift from 0-indexed to 1-indexed. Clients parsing these outputs numerically must account for the +1 change.
* rename Tokenizor → SymForge

### Features

* **01-01:** implement kind-tier disambiguation in resolve_symbol_selector ([2e11ac4](https://github.com/special-place-administrator/symforge/commit/2e11ac4985e690e034da2982fcf3b900d734d30b))
* **02-01:** hook diagnostics — verbose mode, port-missing vs stale, one-time hint ([4547428](https://github.com/special-place-administrator/symforge/commit/4547428aeb984e727947ea94a3f0e40451060216))
* 4 UX improvements from external review feedback (Wave 1) ([430a86a](https://github.com/special-place-administrator/symforge/commit/430a86a4fad65c9725f21037d070349e165f8cee))
* 4 UX improvements from external review feedback (Wave 2) ([0e773e6](https://github.com/special-place-administrator/symforge/commit/0e773e603ee51178d8cc1e36a851ada55c26e9f6))
* add 'ask' smart query tool + token metrics (Suggestions 2 & 10) ([291f544](https://github.com/special-place-administrator/symforge/commit/291f5448e59f7eba497e0dd732489a9eb0d25487))
* add 'summary' verbosity level with heuristic summaries (Suggestion 1) ([90cda29](https://github.com/special-place-administrator/symforge/commit/90cda292fda858c4460dd37c7ec3b48ef79fc65b))
* add AdmissionTier enum and size threshold constants ([48cc242](https://github.com/special-place-administrator/symforge/commit/48cc242852a06b463119757408b911b41efcf493))
* add binary content sniff with NUL, UTF-8, and control-byte heuristics ([e7bd071](https://github.com/special-place-administrator/symforge/commit/e7bd0713957a611e0fabd42f3f6c0e68b042782f))
* add estimate field to all read tool inputs + fix dry_run deserialization bug ([537fa16](https://github.com/special-place-administrator/symforge/commit/537fa165c33f403ce5b828c26c164e921792c9e5))
* add estimate parameter for context budget planning (Suggestion 4) ([b1e3239](https://github.com/special-place-administrator/symforge/commit/b1e3239ce622b77fb9d56bf2e7529e9ee72072cf))
* add extension denylist for admission control ([6159487](https://github.com/special-place-administrator/symforge/commit/6159487c7468fcb037ae2ed6a603eeb9c56c02b9))
* add first-class config file indexing and gated edit support for JSON/TOML/YAML/Markdown/.env ([4bbac75](https://github.com/special-place-administrator/symforge/commit/4bbac7599509e62fbd4bc94551eddc277fc8d68f))
* add frontend asset parsing (HTML, CSS, SCSS) ([a91b625](https://github.com/special-place-administrator/symforge/commit/a91b625739c997da8b0a1e5f8cf41b46979aeb65))
* add Html, Css, Scss to LanguageId with extension mapping ([283c592](https://github.com/special-place-administrator/symforge/commit/283c59214990c6f316247830c271d868be86a701))
* add LineEnding detection and normalization helpers (C1 prep) ([a82a727](https://github.com/special-place-administrator/symforge/commit/a82a72763a9060650553c68da33a0e2d5bd8dc95))
* add match-occurrence retrieval and watcher reconciliation health reporting ([5043a5a](https://github.com/special-place-administrator/symforge/commit/5043a5a64e74d7edc43a6f087572837ae2b7d501))
* add SkippedFile struct and store integration for admission tiers ([b94aeeb](https://github.com/special-place-administrator/symforge/commit/b94aeeb8758ccce92733fefc74a97858b6ccf978))
* add tree-sitter-html, tree-sitter-css, tree-sitter-scss dependencies ([3be5ee3](https://github.com/special-place-administrator/symforge/commit/3be5ee3dd9544cd9ff03e8d5a3dfe579764547bb))
* add unified edit_capability_for_language, rename check_edit_capability ([1742a17](https://github.com/special-place-administrator/symforge/commit/1742a1744842f376c611a7ab8b5cd24e7d214803))
* **adoption:** add hook outcome metrics ([cce4473](https://github.com/special-place-administrator/symforge/commit/cce4473e5024ade6de5f5eea2b6daef87ff38c2e))
* **adoption:** add workflow sidecar adapters ([143ac0b](https://github.com/special-place-administrator/symforge/commit/143ac0bfac5f09a978e4f85bc70c75f2028a0477))
* **adoption:** define owned workflows for hooks ([0185310](https://github.com/special-place-administrator/symforge/commit/0185310fdbae3fded168d5a4249792372f666234))
* **adoption:** steer protocol read workflows ([106cdda](https://github.com/special-place-administrator/symforge/commit/106cddaec9ecb7f0f1a82dae22cb54d95e942ff8))
* **adoption:** tighten hook routing for source workflows ([5b9426a](https://github.com/special-place-administrator/symforge/commit/5b9426a9aa3f6f02f3f27575602bb35f1b74a8f9))
* aggregate token savings across tool handlers ([25343e9](https://github.com/special-place-administrator/symforge/commit/25343e9b20774d3489ca9610955ca81aeda38e5b))
* analyze_file_impact shows clear status taxonomy (U4) ([263834f](https://github.com/special-place-administrator/symforge/commit/263834f3d8dd2a4c22911eed69a506edb7c13bd6))
* batch_edit dry_run mode (U5) ([4166196](https://github.com/special-place-administrator/symforge/commit/41661963168aced0d37c7931628c3dcf49f6b550))
* batch_rename supplemental qualified path scan with confidence classification ([e75f2d4](https://github.com/special-place-administrator/symforge/commit/e75f2d40d657ea668660d3160c884feaf396ec96))
* clean npm cache after install to reclaim disk space ([b1c4a35](https://github.com/special-place-administrator/symforge/commit/b1c4a353fc1d1985c31e363d1be22f7f4e17a440))
* **config:** add structured syntax diagnostics ([7360beb](https://github.com/special-place-administrator/symforge/commit/7360bebb290e50bc65fee0a63ef5118cdc72117c))
* conventions detection, edit planning, investigation mode (Suggestions 3, 8, 9) ([aebf288](https://github.com/special-place-administrator/symforge/commit/aebf28899278be7554f02200ea124ab269e49232))
* daemon fallback, callee dedup, token budget, search defaults ([d13e76b](https://github.com/special-place-administrator/symforge/commit/d13e76b77308b16d42bf721400f10ef6215cc896))
* derive fallback explore clusters ([2173acb](https://github.com/special-place-administrator/symforge/commit/2173acbe9da35ae966b7e25ac78706117335fcc6))
* **edit:** add dry_run to replace_symbol_body, insert_symbol, delete_symbol, edit_within_symbol ([1f401d8](https://github.com/special-place-administrator/symforge/commit/1f401d82444bf19bd86746b2346b8aa3082f9880))
* **edit:** track item byte ranges on symbols ([da3294c](https://github.com/special-place-administrator/symforge/commit/da3294c623589382bfa8dd2311944d868a6804e7))
* explore filters noise by default (U1) ([f14b702](https://github.com/special-place-administrator/symforge/commit/f14b702b0a68ba6c88b5c96327cbdf5d701e5d72))
* **find_dependents:** show symbol names in mermaid and dot edge labels ([e358b77](https://github.com/special-place-administrator/symforge/commit/e358b77697162cd859fd41f72a301ee23f64d5f3))
* get_file_content mode enum for clearer API (U10) ([244be75](https://github.com/special-place-administrator/symforge/commit/244be753b08275b80f3bd1d8a11214a74f642f03))
* **get_repo_map:** paginate detail=full output with max_files parameter ([8c15c5d](https://github.com/special-place-administrator/symforge/commit/8c15c5d2bc55768123a754fa6a74c23bb9b6c131))
* git churn in ranking, expanded guidance blocks, improved tool descriptions ([4cf1e6e](https://github.com/special-place-administrator/symforge/commit/4cf1e6e782d79e52554c23aae98590fb5a60feb5))
* health shows partial parse file paths (U8) ([8560114](https://github.com/special-place-administrator/symforge/commit/856011485f82605d41e93651748bf64db1486c91))
* **health:** list failed files with error messages in health report ([7306089](https://github.com/special-place-administrator/symforge/commit/73060890c96058223dd04dffd91b06087fb4dd1a))
* implement admission gate with tiered file classification ([9e69e23](https://github.com/special-place-administrator/symforge/commit/9e69e238f50b3364545fa3844cbb5b03ae7ad925))
* implement CSS symbol extractor with tests ([39719f4](https://github.com/special-place-administrator/symforge/commit/39719f4962657e824f9a82b9c8040be48af71cfd))
* implement HTML/Angular symbol extractor with tests ([4627b52](https://github.com/special-place-administrator/symforge/commit/4627b520009e2c8395f39593644499cae428fe75))
* implement SCSS symbol extractor with tests ([2112b1a](https://github.com/special-place-administrator/symforge/commit/2112b1a7776db40d5d63ccd12da753fba085aaa9))
* improve error context on failures (Suggestion 7) ([8169a6e](https://github.com/special-place-administrator/symforge/commit/8169a6e2ed0c1a7d212e865456ecabf29018dfed))
* improve explore relevance ranking (Q1) ([5b829ca](https://github.com/special-place-administrator/symforge/commit/5b829cab9138885daf8479b3fb3a594bc35838d5))
* improve init trust detection and query routing ([82b433a](https://github.com/special-place-administrator/symforge/commit/82b433ad6f104437ae9e165d71481c42a58c1b43))
* **index:** Sprint 0 — index freshness guarantee via mtime tracking ([29d60d6](https://github.com/special-place-administrator/symforge/commit/29d60d6fe0e83f3c79856c38796bc02c62f62bea))
* **init:** add alwaysAllow to Claude MCP entry and expand CLAUDE.md guidance ([4ee5f53](https://github.com/special-place-administrator/symforge/commit/4ee5f535546e829850c6136b5785bd56b23c7732))
* **init:** harden client guidance rollout ([f30667c](https://github.com/special-place-administrator/symforge/commit/f30667ce885ba5040a4a40751b7401080fad8977))
* **json:** add JSONC comment stripping for tsconfig.json support ([c3c208f](https://github.com/special-place-administrator/symforge/commit/c3c208fb496449efe2cd14a7ee82562bd4088df9))
* lenient vec deserializer, semantic search ranking, Kilo Code init, SymForge rename plan ([c048274](https://github.com/special-place-administrator/symforge/commit/c04827422bb02a04ff4222864b0c09ff014ca70d))
* per-tool call counters in health output (U9) ([d41bfb5](https://github.com/special-place-administrator/symforge/commit/d41bfb5730d07b2b0271ee1947eec0306f07375c))
* per-tool token efficiency tracking (Suggestion 10) ([a588365](https://github.com/special-place-administrator/symforge/commit/a5883651c7bc384b431e7964bd3f0e1e04a4682c))
* quality improvements from 3-project eval (Q1-Q6) ([8d23ff4](https://github.com/special-place-administrator/symforge/commit/8d23ff4749250d4ad85f12324280543fd8c5e403))
* quality improvements from eval feedback (Q3-Q6) ([79ac714](https://github.com/special-place-administrator/symforge/commit/79ac714eb67a2d504be76a4beec7479cfe154385))
* rename Tokenizor → SymForge ([6366cd0](https://github.com/special-place-administrator/symforge/commit/6366cd0c7f51bc496cceb6ae255e22d95f109183))
* richer verbosity=signature includes visibility and return type (U6) ([eef2926](https://github.com/special-place-administrator/symforge/commit/eef2926f057e3020200bb13cc1dd47b9ee9bf76e))
* RTK adoption milestone — symbol disambiguation tests, hook diagnostics, docs links ([9bc3ead](https://github.com/special-place-administrator/symforge/commit/9bc3ead5c49813998a9987b3e2066398313d48db))
* search_symbols browse mode without query (U2) ([3326342](https://github.com/special-place-administrator/symforge/commit/33263428425acd01a9cfba460d96d0b5534257b5))
* **search_text:** annotate which term matched in OR-term searches ([e53a7f7](https://github.com/special-place-administrator/symforge/commit/e53a7f748e55afe0983d4710b6375edc01058397))
* session context tracking + context_inventory tool (Suggestion 6) ([40c8de5](https://github.com/special-place-administrator/symforge/commit/40c8de54c8e0de9e78b6c6cd3cdc03950e4d21ce))
* show Tier 2 tags and Tier 3 footer in repo_map ([05d23eb](https://github.com/special-place-administrator/symforge/commit/05d23eb7b23c06c607e6adacac00ae7edbb2c7dc))
* Sprint 14 — trust fixes + tiered admission control ([b7a9296](https://github.com/special-place-administrator/symforge/commit/b7a92963b3f9c55be08e73e04eba6bd70901b1bf))
* trust-calibrate SymForge release ([b585660](https://github.com/special-place-administrator/symforge/commit/b585660b707f2a3f32b8e2b64ab6bdb807f3753e))
* update init templates with new tools and guidance rules ([be7d687](https://github.com/special-place-administrator/symforge/commit/be7d687464f9d89a465e8a104904f18aa065c17d))
* wire admission gate into discovery walk ([51c73f7](https://github.com/special-place-administrator/symforge/commit/51c73f7b125d13a88179d9e1e2cf535e28839888))
* wire HTML, CSS, SCSS extractors into parsing pipeline ([e740f94](https://github.com/special-place-administrator/symforge/commit/e740f947007acee5db43a321bb38152e1fed63cf))
* workflow recipe prompts + 3 new prompts (Suggestion 5) ([cb6ccdc](https://github.com/special-place-administrator/symforge/commit/cb6ccdccfeab232f6e529f8553da5df4db28b4ae))


### Bug Fixes

* 26 bug fixes across parsers, protocol, indexing, sidecar, and npm ([b2abebc](https://github.com/special-place-administrator/symforge/commit/b2abebc4710580cdb62fe984809c4ddc949cd8a6))
* 4 display/UX improvements in search, outline, repo_map, and get_symbol ([b3a449c](https://github.com/special-place-administrator/symforge/commit/b3a449c81a0802070b035cc41f8e44b46ad50a50))
* add exe/dll/so/dylib/class to denylist (C2-lite) ([14d0459](https://github.com/special-place-administrator/symforge/commit/14d04593df00b7cbc92643aeb5c7109026a045cb))
* add missing estimate=true handler to get_file_content ([72cd834](https://github.com/special-place-administrator/symforge/commit/72cd834b87ec852d7967557a66869e716c6ab2ff))
* add missing gitignore/noise_class field initializers across codebase ([c8088f9](https://github.com/special-place-administrator/symforge/commit/c8088f9f0953b004e015c2a707db89dae3597ced))
* add missing sibling_limit/overflow fields to initializers ([b25f4a5](https://github.com/special-place-administrator/symforge/commit/b25f4a5a34a007ad9a56757cd1a62ce7c9f92157))
* add NOT-for tips to 5 tool descriptions missing them ([14a8fad](https://github.com/special-place-administrator/symforge/commit/14a8fad0de46c1b3aa35711a8bbda3f8efee2c94))
* add panic hook to clean up sidecar port files on crash ([6758696](https://github.com/special-place-administrator/symforge/commit/6758696a02d052d6646721629a51c67ee8978f93))
* address all actionable feedback from 3 external code reviews ([61d2757](https://github.com/special-place-administrator/symforge/commit/61d2757a8b9bbc425b73eec512dc75c470be630d))
* around_symbol returns full indexed symbol span (B2) ([3b06c2a](https://github.com/special-place-administrator/symforge/commit/3b06c2a735ad3edd0ac691851a944d66797b06f9))
* batch schema parity + find_references supplemental text fallback ([dffb8b8](https://github.com/special-place-administrator/symforge/commit/dffb8b8276da05dd927b5b3530843ce1e958aa55))
* batch_edit dry_run byte count + auto-detect regex in search_text ([29474c2](https://github.com/special-place-administrator/symforge/commit/29474c21bd33b1da6858bdbb4179eaa3ac9611a1))
* batch_edit shows ROLLED BACK message on failure (B4) ([3ab8358](https://github.com/special-place-administrator/symforge/commit/3ab83587c7bbee77bd1f1cfe1b1980066630da8f))
* batch_insert no extra blank line before function (B1) ([3409548](https://github.com/special-place-administrator/symforge/commit/34095482b8275811cb5373003e367c0b07dcfec0))
* batch_rename atomic rollback on failure, batch_edit/batch_insert best-effort with correct index state ([6b332f3](https://github.com/special-place-administrator/symforge/commit/6b332f3f18484a8e14ed35731961eac98311b18f))
* batch_rename catches path-qualified usages ([c2243ff](https://github.com/special-place-administrator/symforge/commit/c2243ff058e0ca29848272641eed8c27eec47131))
* batch_rename catches path-qualified usages via literal scan ([2824745](https://github.com/special-place-administrator/symforge/commit/282474529928f46e3dec525470244baa3fb68873))
* batch_rename review fixes — atomic rollback, dead code, dedup ([0a7844e](https://github.com/special-place-administrator/symforge/commit/0a7844ee1b8ce77b462f1f6a3b51b53c1ce17a22))
* break infinite reconciliation loop caused by hash-skip mtime drift ([54a03f8](https://github.com/special-place-administrator/symforge/commit/54a03f88ffc6862e5096531b1f705589994120b9))
* **build:** silence tree-sitter-scss scanner warnings ([80c69e4](https://github.com/special-place-administrator/symforge/commit/80c69e4e6765ba64ba41522952b8e8b5e02e08b8))
* **bundle:** resolve impl suggestions and dependency-aware limits ([d5bfa6a](https://github.com/special-place-administrator/symforge/commit/d5bfa6aa85e3dfd719340301e1eff01d4bb1c069))
* **ci:** enforce conventional commits and verify main pushes ([5ec0b78](https://github.com/special-place-administrator/symforge/commit/5ec0b789af1012550db40128c9e501635e14de0a))
* **ci:** force Node 24 for GitHub Actions runtime ([98b666a](https://github.com/special-place-administrator/symforge/commit/98b666afe6a2157ec85b80a1c91cfa812247cfbd))
* **ci:** make npm publish idempotent ([bdddc47](https://github.com/special-place-administrator/symforge/commit/bdddc47bf1558be1f5068e0b6a25dcfa4a9f7aab))
* **ci:** tolerate force-pushed conventional commit ranges ([8a70ffa](https://github.com/special-place-administrator/symforge/commit/8a70ffaa8cd3e401b0f19563c59ac99cf08a9290))
* **ci:** use cargo check for workflow verification ([5c6a13b](https://github.com/special-place-administrator/symforge/commit/5c6a13b5cea9be1efc4df14fbbda5b21072df338))
* code review feedback — tests and safety fixes ([c2454ea](https://github.com/special-place-administrator/symforge/commit/c2454ea2bbd1ec0c2f2acb357bd9f158fd101584))
* codex audit — uncommitted symbol diff, daemon tool dispatch, lenient vec deserialization ([26f2528](https://github.com/special-place-administrator/symforge/commit/26f2528d34f6057621f12e828ae1171269966618))
* complete audit remediation — language tests, deferred fixes, dedup ([c04e849](https://github.com/special-place-administrator/symforge/commit/c04e84968e406e1662b5f36fa7df2ae923ab15fe))
* complete parking_lot::RwLock migration across live_index and protocol ([c48d865](https://github.com/special-place-administrator/symforge/commit/c48d865c2e9c717b89da9d446a51f326af5d3052))
* comprehensive codebase audit — 18 bug fixes across parsers, core engine, and protocol ([3b0cd44](https://github.com/special-place-administrator/symforge/commit/3b0cd442841748ebb30e05d2b23d13b57246ee5e))
* compute real line numbers for TOML symbols ([d939caa](https://github.com/special-place-administrator/symforge/commit/d939caa7d3a587efa7482dc9f4790e7b102e15ac))
* compute real line numbers for TOML symbols ([7dd4697](https://github.com/special-place-administrator/symforge/commit/7dd46973f52228193cc98ccb8d01361b75eef808))
* CSS @layer/[@container](https://github.com/container) extraction — use generic at_rule node kind ([97fc47f](https://github.com/special-place-administrator/symforge/commit/97fc47fd201c89888231e21e4f665d919c73b3fc))
* daemon lifecycle hardening — stale lock detection, fast-fail proxy, cleanup (DL1-DL4) ([8350d7e](https://github.com/special-place-administrator/symforge/commit/8350d7e3b5de2bb16d85d69a817ca459fb43e829))
* daemon proxy deadlock under concurrent tool calls + request governor ([541dd68](https://github.com/special-place-administrator/symforge/commit/541dd688e9956d75818733740764320513e9c8ab))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([3b1cc4e](https://github.com/special-place-administrator/symforge/commit/3b1cc4ed6ae8591ca8afc720158f2241cbec80de))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([b793579](https://github.com/special-place-administrator/symforge/commit/b793579b2f1bdfdd7fb266bfa9bf7dd76590ea9e))
* **dependents:** filter false positives from non-pub symbol name collisions ([0bf3c77](https://github.com/special-place-administrator/symforge/commit/0bf3c77b1d41ea7ba383d18b83029ba15f0855a6))
* **diff_symbols:** show omission note in compact mode when files have no symbol changes ([c6eade8](https://github.com/special-place-administrator/symforge/commit/c6eade8fc0770c042c26b9a0b258a7b619537e21))
* **diff_symbols:** skip type keywords in C# const declarations ([00049d2](https://github.com/special-place-administrator/symforge/commit/00049d264058dafbbbb2ab72816cdfc8dd608164))
* emit strict MCP array schemas for optional list params ([3519470](https://github.com/special-place-administrator/symforge/commit/3519470c2cafc36cd1d4c7e9b4455b8f3020623f))
* explore depth=2 shows symbol-level callers, get_symbol uses tier disambiguation ([9a22035](https://github.com/special-place-administrator/symforge/commit/9a22035872fb0ad58026a9dc50d66f0440e98f4b))
* explore depth=2 shows symbol-level callers, get_symbol uses tier… ([4c1f588](https://github.com/special-place-administrator/symforge/commit/4c1f588fc1bd7282f74251eadb587913f89658a2))
* find_dependents resolves workspace crate paths ([418652a](https://github.com/special-place-administrator/symforge/commit/418652a42bd3e1443602cb87cd1eed2d7e4c0574))
* find_dependents resolves workspace crate paths (B4) ([f02819d](https://github.com/special-place-administrator/symforge/commit/f02819db942530bcbb974fe946d05224bb953ab6)), closes [#89](https://github.com/special-place-administrator/symforge/issues/89)
* find_dependents_for_file catches qualified calls without imports ([8e1fd7d](https://github.com/special-place-administrator/symforge/commit/8e1fd7d442c74fc20a5fa1bd6bbd69a295d42cc9))
* **find_references:** explain why classes/structs have no implementations ([7566e2a](https://github.com/special-place-administrator/symforge/commit/7566e2a3bc9a928d55b6266307738029d63487e9))
* **get_file_content:** explain why zero-symbol files have no matches ([d4420f8](https://github.com/special-place-administrator/symforge/commit/d4420f8df45c949f6bc3be1731eca17fe620c7db))
* get_file_context sections filter masked by 800-byte hook budget ([5ff4c9e](https://github.com/special-place-administrator/symforge/commit/5ff4c9e3031e099d7321c5b906ba3109f031e3f7))
* **get_symbol_context:** auto-resolve path and show empty-references message ([8b4caf5](https://github.com/special-place-administrator/symforge/commit/8b4caf5b49e6ac402744b23eddd1926762421b62))
* Go method names, SCSS $variable extraction, language filter completeness ([4156d41](https://github.com/special-place-administrator/symforge/commit/4156d41d24af8739494482ea4ba868a69fec747d))
* handle SIGTERM for daemon graceful shutdown (C5) ([3faef1f](https://github.com/special-place-administrator/symforge/commit/3faef1f772ef679f137229cc7fc9d47891c132f6))
* improve reconciliation logging to diagnose stale-file loops ([cff17ed](https://github.com/special-place-administrator/symforge/commit/cff17edd4fac7250275654b90fba7c88709317ea))
* index safety hardening and tool output correctness ([c952759](https://github.com/special-place-administrator/symforge/commit/c952759567b0a1e7a2de4cd56ccb0576eaa792a8))
* index safety hardening and tool output correctness ([59720c5](https://github.com/special-place-administrator/symforge/commit/59720c5fc9e48eccb2fc41d291a804b0a69cbd55))
* **init:** canonicalize SymForge Codex guidance and allowlists ([7680757](https://github.com/special-place-administrator/symforge/commit/76807570e24995dfc05ad6df022cbd1fbab25bfa))
* **kilo:** trigger release for strict-provider compatibility ([a852955](https://github.com/special-place-administrator/symforge/commit/a852955707cc9da1133f9a872d8d7b3b955988e7))
* lenient SingleEdit deserialization — accept shorthand DSL strings ([d61fd79](https://github.com/special-place-administrator/symforge/commit/d61fd7924117c7a2eee7ee703c4f1a875abc1ae1))
* make SymForge MCP schemas compatible with strict OpenAI clients ([aaa42f2](https://github.com/special-place-administrator/symforge/commit/aaa42f2aa6cb17af2a7d149abd5d0483f8086513))
* new tools broken in daemon mode — missing proxy_tool_call ([cdde2a9](https://github.com/special-place-administrator/symforge/commit/cdde2a977fc0a78622c14c5227585271201bb926))
* non-ASCII panic in doc scanning and deterministic circuit-breaker (CR1, CR2) ([1e52aaf](https://github.com/special-place-administrator/symforge/commit/1e52aaf7468d425fac21371398a631a93d6c5bfe))
* normalize exact get_file_content paths and backfill mtime_secs in integration fixtures ([fb398b1](https://github.com/special-place-administrator/symforge/commit/fb398b1dcbfdc59055fea328e995bdb1d9ba114c))
* **npm:** keep global auto-init out of workspaces ([1626dfa](https://github.com/special-place-administrator/symforge/commit/1626dfa89a6da5a9e14b0107df3eec7a0b325c61))
* **npm:** persist wrapper install metadata ([eeac029](https://github.com/special-place-administrator/symforge/commit/eeac0298ef08253fdf70042ba5d2f78f142faea2))
* pin CI/release workflows to Rust 1.94.0 matching rust-toolchain.toml ([41d23ab](https://github.com/special-place-administrator/symforge/commit/41d23ab3fd83205773f9289cf95cd142fd2cb1b9))
* pin Rust toolchain to 1.94.0 via rust-toolchain.toml ([fed0e20](https://github.com/special-place-administrator/symforge/commit/fed0e20560de0356cb86bf5dc9319af2867e770c))
* prevent async runtime starvation under concurrent subagent load ([74f1d54](https://github.com/special-place-administrator/symforge/commit/74f1d54f0f26dba97a619fb5b69e645c1d702034))
* prevent async runtime starvation under concurrent subagent load ([2ed134a](https://github.com/special-place-administrator/symforge/commit/2ed134aa34a82f258eb37be4121c341272cc85d6))
* prevent non-ASCII panic in find_qualified_usages (batch_rename crash) ([8555d0c](https://github.com/special-place-administrator/symforge/commit/8555d0ce0b3ed39acf7d170c0878d4e94267a2ef))
* qualified Rust caller resolution + context_inventory daemon proxy ([175b53e](https://github.com/special-place-administrator/symforge/commit/175b53edef61b7541310a430f87acffb08eea563))
* recurse into mixin/function bodies, guard empty at-rule names ([8a8e717](https://github.com/special-place-administrator/symforge/commit/8a8e7178c079078581ac8c6e339128814e16478a))
* reindex from disk after writes, not from in-memory buffer ([d605498](https://github.com/special-place-administrator/symforge/commit/d6054988db18ad6cf82e3a82cca2c054a1c5f52b))
* **release:** add noncommercial licensing and kill-all npm updates ([17354c6](https://github.com/special-place-administrator/symforge/commit/17354c6adba9e75a09bc3929b753881552ec929a))
* remediate reviewer feedback from external codebase testing ([80303a9](https://github.com/special-place-administrator/symforge/commit/80303a9a9558d05ad281740552fdc95b349270f3))
* remove stale args from for_current_code_search() and suppress unused variable warning ([9652e53](https://github.com/special-place-administrator/symforge/commit/9652e53ca1c4a22d1968ba8f552191e830b47386))
* replace std::sync::Mutex with parking_lot::Mutex to prevent poison cascades ([04651d0](https://github.com/special-place-administrator/symforge/commit/04651d09f5fccbad4e4ee267b2cf4c2ee953ea44))
* resolve 16 bugs across mtime propagation, line indexing, correctness, and concurrency ([8cfda64](https://github.com/special-place-administrator/symforge/commit/8cfda649bf2bd0a017fa72643bce088f817cd1bc))
* resolve 4 bugs from code review ([31b9a0c](https://github.com/special-place-administrator/symforge/commit/31b9a0c78b5576262c74946adaf00aad8262ebcb))
* resolve 5 tool bugs from hands-on review ([6d11014](https://github.com/special-place-administrator/symforge/commit/6d1101448f993946699028e26ecf8852f81073be))
* restore missing [[package]] header in Cargo.lock after rebase conflict resolution ([67d9327](https://github.com/special-place-administrator/symforge/commit/67d932778c02a9899ffec904560868927604d968))
* revert worker_threads override — spawn_blocking handles concurrency ([a5d5d4e](https://github.com/special-place-administrator/symforge/commit/a5d5d4e77dfd2d438a42ef2161fd1c7111584abd))
* review feedback — Q3 robust name extraction, Q6 UTF-8 safe truncation ([41e17d2](https://github.com/special-place-administrator/symforge/commit/41e17d23a4b4ce9e89d9fdfbceb5ba089a9880a0))
* rewrite open_project_session with double-checked locking (C6) ([b04b0d0](https://github.com/special-place-administrator/symforge/commit/b04b0d099fb035ca1bac9c76c49d542cae1d8102))
* security patches, parser improvements, parallelism fixes, and review follow-ups ([2b1d5cb](https://github.com/special-place-administrator/symforge/commit/2b1d5cbafed5100ea833d0f4b41da41b1d87cb27))
* show_line_numbers works with around_symbol and around_match (B3) ([4befe8a](https://github.com/special-place-administrator/symforge/commit/4befe8a8f8e734e722088632c23ac81489bf42ce))
* sidecar reliability + reviewer feedback remediation ([8b41990](https://github.com/special-place-administrator/symforge/commit/8b419904451053df06d82f06ba7921f435395af0))
* surface tool panics as immediate error responses instead of stalls ([31ae935](https://github.com/special-place-administrator/symforge/commit/31ae935642876711383897ba3b779ea4c2dc7b52))
* Swift enum/extension/protocol detection and Angular template robustness ([af34df2](https://github.com/special-place-administrator/symforge/commit/af34df266b7cb34ba4dbd24853b585554bba7308))
* symbol kind filter accepts semantic aliases (variable, function, etc.) ([2e80fb5](https://github.com/special-place-administrator/symforge/commit/2e80fb5cd96b441e256e4a7c7afb5fad20fbbfbe))
* **tests:** remove unused unix import in edit tests ([919aff6](https://github.com/special-place-administrator/symforge/commit/919aff668a88636a000c10b9f2ac1c4e3a01aba8))
* **test:** update assertion for changed zero-symbol message ([90a5722](https://github.com/special-place-administrator/symforge/commit/90a57229f697bfc541adc4d58ea35f1f2dc53295))
* thread LineEnding through all edit helpers for CRLF preservation (C1) ([dda40b4](https://github.com/special-place-administrator/symforge/commit/dda40b4c0c8a99259a93af4c9ff65bb696e9e337))
* **trust:** calibrate temporal signals and health output ([e2fafbe](https://github.com/special-place-administrator/symforge/commit/e2fafbea393f55fde6e2b22d499c1ad3f0d96b3c))
* **trust:** tighten discovery and context signals ([0c0fbe7](https://github.com/special-place-administrator/symforge/commit/0c0fbe72e4c2f2318654be1944abc500311e14df))
* update 5 stale test assertions to match improved error messages ([4ebb8b5](https://github.com/special-place-administrator/symforge/commit/4ebb8b56a09b538bfcc171dfdee07a35576ceb90))
* update installer test assertion for execFileSyncFn version check ([f6ed05d](https://github.com/special-place-administrator/symforge/commit/f6ed05dd601105a4a3249cf2844b4da91210c560))
* update rollback tests for tempfile-based atomic writes ([548c2bb](https://github.com/special-place-administrator/symforge/commit/548c2bbede27be2a233fe35a2d7f6b8aa0aee45b))
* use unique temp files in atomic_write_file (C3) ([dddcb16](https://github.com/special-place-administrator/symforge/commit/dddcb16eb85dc294354691e9fc470adbca5ce9bd))
* validate splice overlap in batch_rename (C4) ([0c80b74](https://github.com/special-place-administrator/symforge/commit/0c80b74903261a09837a7038998bf3d61898d274))
* watcher recv_timeout blocks tokio worker — use try_recv + async sleep ([a4b7d34](https://github.com/special-place-administrator/symforge/commit/a4b7d34db100eac9528325f6f3bdbdd58827d54d))
* wave 1 audit remediation — 12 safety and correctness fixes ([b02bd12](https://github.com/special-place-administrator/symforge/commit/b02bd1203ba78ee404e204dda80ae54328ba3642))
* wave 2 audit remediation — 10 reliability and consistency fixes ([a293819](https://github.com/special-place-administrator/symforge/commit/a2938196992b09c8ddb2a0fc40a732ff499627e0))
* wave 3 audit remediation — polish, docs, and remaining fixes ([c7c2ba8](https://github.com/special-place-administrator/symforge/commit/c7c2ba8f6129565f16a3346026ae35606f8f60f5))
* widen common-name warning in find_references to trigger on ref c… ([32b22b4](https://github.com/special-place-administrator/symforge/commit/32b22b432d54e1a02fced916ebfbdb08d21b8c1a))
* widen common-name warning in find_references to trigger on ref count alone ([2c3cc4b](https://github.com/special-place-administrator/symforge/commit/2c3cc4b472d1df4645b3008d32451ffe1a104082))
* wire session tracking into tool handlers + InsertTarget string shorthand ([5c7feb3](https://github.com/special-place-administrator/symforge/commit/5c7feb3cefed0ea9af8980a5622d82532d88e7b5))
* wrap daemon sidecar handlers with governor + spawn_blocking ([d665e41](https://github.com/special-place-administrator/symforge/commit/d665e415a5f889229ef7c5dc8a18a4c9cadc36bd))
* wrap env var manipulation in unsafe blocks for Rust 2024 edition compliance ([c363499](https://github.com/special-place-administrator/symforge/commit/c3634999e2c91fcbc57a2fb92decc5f6a217a77f))
* wrap repair_file_indices in catch_unwind to prevent double-panic abort ([e2d2e97](https://github.com/special-place-administrator/symforge/commit/e2d2e976f4bef08ca678ad5e2a2142f5f5c48f2a))

## [5.1.2](https://github.com/special-place-administrator/symforge/compare/v5.1.1...v5.1.2) (2026-04-02)


### Bug Fixes

* **trust:** tighten discovery and context signals ([0c0fbe7](https://github.com/special-place-administrator/symforge/commit/0c0fbe72e4c2f2318654be1944abc500311e14df))

## [5.1.1](https://github.com/special-place-administrator/symforge/compare/v5.1.0...v5.1.1) (2026-04-01)


### Bug Fixes

* **build:** silence tree-sitter-scss scanner warnings ([80c69e4](https://github.com/special-place-administrator/symforge/commit/80c69e4e6765ba64ba41522952b8e8b5e02e08b8))
* **ci:** tolerate force-pushed conventional commit ranges ([8a70ffa](https://github.com/special-place-administrator/symforge/commit/8a70ffaa8cd3e401b0f19563c59ac99cf08a9290))
* **trust:** calibrate temporal signals and health output ([e2fafbe](https://github.com/special-place-administrator/symforge/commit/e2fafbea393f55fde6e2b22d499c1ad3f0d96b3c))

## [5.1.0](https://github.com/special-place-administrator/symforge/compare/v5.0.0...v5.1.0) (2026-04-01)


### Features

* derive fallback explore clusters ([2173acb](https://github.com/special-place-administrator/symforge/commit/2173acbe9da35ae966b7e25ac78706117335fcc6))
* improve init trust detection and query routing ([82b433a](https://github.com/special-place-administrator/symforge/commit/82b433ad6f104437ae9e165d71481c42a58c1b43))

## [5.0.0](https://github.com/special-place-administrator/symforge/compare/v4.9.10...v5.0.0) (2026-04-01)


### ⚠ BREAKING CHANGES

* ships a broad trust-calibration pass across sidecar hints, read/query outputs, edit safety signaling, transactional batch operations, and harness init guidance deduplication.

### Features

* trust-calibrate SymForge release ([b585660](https://github.com/special-place-administrator/symforge/commit/b585660b707f2a3f32b8e2b64ab6bdb807f3753e))

## [4.9.10](https://github.com/special-place-administrator/symforge/compare/v4.9.9...v4.9.10) (2026-04-01)


### Bug Fixes

* **tests:** remove unused unix import in edit tests ([919aff6](https://github.com/special-place-administrator/symforge/commit/919aff668a88636a000c10b9f2ac1c4e3a01aba8))

## [4.9.9](https://github.com/special-place-administrator/symforge/compare/v4.9.8...v4.9.9) (2026-03-31)


### Bug Fixes

* **ci:** make npm publish idempotent ([bdddc47](https://github.com/special-place-administrator/symforge/commit/bdddc47bf1558be1f5068e0b6a25dcfa4a9f7aab))

## [4.9.8](https://github.com/special-place-administrator/symforge/compare/v4.9.7...v4.9.8) (2026-03-31)


### Bug Fixes

* **ci:** force Node 24 for GitHub Actions runtime ([98b666a](https://github.com/special-place-administrator/symforge/commit/98b666afe6a2157ec85b80a1c91cfa812247cfbd))

## [4.9.7](https://github.com/special-place-administrator/symforge/compare/v4.9.6...v4.9.7) (2026-03-30)


### Bug Fixes

* **ci:** enforce conventional commits and verify main pushes ([5ec0b78](https://github.com/special-place-administrator/symforge/commit/5ec0b789af1012550db40128c9e501635e14de0a))
* **ci:** use cargo check for workflow verification ([5c6a13b](https://github.com/special-place-administrator/symforge/commit/5c6a13b5cea9be1efc4df14fbbda5b21072df338))

## [4.9.6](https://github.com/special-place-administrator/symforge/compare/v4.9.5...v4.9.6) (2026-03-30)


### Bug Fixes

* find_dependents_for_file catches qualified calls without imports ([8e1fd7d](https://github.com/special-place-administrator/symforge/commit/8e1fd7d442c74fc20a5fa1bd6bbd69a295d42cc9))

## [4.9.5](https://github.com/special-place-administrator/symforge/compare/v4.9.4...v4.9.5) (2026-03-30)


### Bug Fixes

* update 5 stale test assertions to match improved error messages ([4ebb8b5](https://github.com/special-place-administrator/symforge/commit/4ebb8b56a09b538bfcc171dfdee07a35576ceb90))

## [4.9.4](https://github.com/special-place-administrator/symforge/compare/v4.9.3...v4.9.4) (2026-03-30)


### Bug Fixes

* qualified Rust caller resolution + context_inventory daemon proxy ([175b53e](https://github.com/special-place-administrator/symforge/commit/175b53edef61b7541310a430f87acffb08eea563))

## [4.9.3](https://github.com/special-place-administrator/symforge/compare/v4.9.2...v4.9.3) (2026-03-30)


### Bug Fixes

* lenient SingleEdit deserialization — accept shorthand DSL strings ([d61fd79](https://github.com/special-place-administrator/symforge/commit/d61fd7924117c7a2eee7ee703c4f1a875abc1ae1))

## [4.9.2](https://github.com/special-place-administrator/symforge/compare/v4.9.1...v4.9.2) (2026-03-30)


### Bug Fixes

* codex audit — uncommitted symbol diff, daemon tool dispatch, lenient vec deserialization ([26f2528](https://github.com/special-place-administrator/symforge/commit/26f2528d34f6057621f12e828ae1171269966618))

## [4.9.1](https://github.com/special-place-administrator/symforge/compare/v4.9.0...v4.9.1) (2026-03-30)


### Bug Fixes

* add missing estimate=true handler to get_file_content ([72cd834](https://github.com/special-place-administrator/symforge/commit/72cd834b87ec852d7967557a66869e716c6ab2ff))

## [4.9.0](https://github.com/special-place-administrator/symforge/compare/v4.8.3...v4.9.0) (2026-03-30)


### Features

* add estimate field to all read tool inputs + fix dry_run deserialization bug ([537fa16](https://github.com/special-place-administrator/symforge/commit/537fa165c33f403ce5b828c26c164e921792c9e5))


### Bug Fixes

* remove stale args from for_current_code_search() and suppress unused variable warning ([9652e53](https://github.com/special-place-administrator/symforge/commit/9652e53ca1c4a22d1968ba8f552191e830b47386))

## [4.8.3](https://github.com/special-place-administrator/symforge/compare/v4.8.2...v4.8.3) (2026-03-30)


### Bug Fixes

* batch schema parity + find_references supplemental text fallback ([dffb8b8](https://github.com/special-place-administrator/symforge/commit/dffb8b8276da05dd927b5b3530843ce1e958aa55))

## [4.8.2](https://github.com/special-place-administrator/symforge/compare/v4.8.1...v4.8.2) (2026-03-30)


### Bug Fixes

* wire session tracking into tool handlers + InsertTarget string shorthand ([5c7feb3](https://github.com/special-place-administrator/symforge/commit/5c7feb3cefed0ea9af8980a5622d82532d88e7b5))

## [4.8.1](https://github.com/special-place-administrator/symforge/compare/v4.8.0...v4.8.1) (2026-03-30)


### Bug Fixes

* new tools broken in daemon mode — missing proxy_tool_call ([cdde2a9](https://github.com/special-place-administrator/symforge/commit/cdde2a977fc0a78622c14c5227585271201bb926))

## [4.8.0](https://github.com/special-place-administrator/symforge/compare/v4.7.2...v4.8.0) (2026-03-30)


### Features

* update init templates with new tools and guidance rules ([be7d687](https://github.com/special-place-administrator/symforge/commit/be7d687464f9d89a465e8a104904f18aa065c17d))

## [4.7.2](https://github.com/special-place-administrator/symforge/compare/v4.7.1...v4.7.2) (2026-03-30)


### Bug Fixes

* break infinite reconciliation loop caused by hash-skip mtime drift ([54a03f8](https://github.com/special-place-administrator/symforge/commit/54a03f88ffc6862e5096531b1f705589994120b9))

## [4.7.1](https://github.com/special-place-administrator/symforge/compare/v4.7.0...v4.7.1) (2026-03-30)


### Bug Fixes

* improve reconciliation logging to diagnose stale-file loops ([cff17ed](https://github.com/special-place-administrator/symforge/commit/cff17edd4fac7250275654b90fba7c88709317ea))

## [4.7.0](https://github.com/special-place-administrator/symforge/compare/v4.6.0...v4.7.0) (2026-03-30)


### Features

* conventions detection, edit planning, investigation mode (Suggestions 3, 8, 9) ([aebf288](https://github.com/special-place-administrator/symforge/commit/aebf28899278be7554f02200ea124ab269e49232))

## [4.6.0](https://github.com/special-place-administrator/symforge/compare/v4.5.0...v4.6.0) (2026-03-30)


### Features

* session context tracking + context_inventory tool (Suggestion 6) ([40c8de5](https://github.com/special-place-administrator/symforge/commit/40c8de54c8e0de9e78b6c6cd3cdc03950e4d21ce))

## [4.5.0](https://github.com/special-place-administrator/symforge/compare/v4.4.0...v4.5.0) (2026-03-30)


### Features

* add estimate parameter for context budget planning (Suggestion 4) ([b1e3239](https://github.com/special-place-administrator/symforge/commit/b1e3239ce622b77fb9d56bf2e7529e9ee72072cf))

## [4.4.0](https://github.com/special-place-administrator/symforge/compare/v4.3.0...v4.4.0) (2026-03-30)


### Features

* add 'summary' verbosity level with heuristic summaries (Suggestion 1) ([90cda29](https://github.com/special-place-administrator/symforge/commit/90cda292fda858c4460dd37c7ec3b48ef79fc65b))

## [4.3.0](https://github.com/special-place-administrator/symforge/compare/v4.2.0...v4.3.0) (2026-03-30)


### Features

* workflow recipe prompts + 3 new prompts (Suggestion 5) ([cb6ccdc](https://github.com/special-place-administrator/symforge/commit/cb6ccdccfeab232f6e529f8553da5df4db28b4ae))

## [4.2.0](https://github.com/special-place-administrator/symforge/compare/v4.1.0...v4.2.0) (2026-03-30)


### Features

* add 'ask' smart query tool + token metrics (Suggestions 2 & 10) ([291f544](https://github.com/special-place-administrator/symforge/commit/291f5448e59f7eba497e0dd732489a9eb0d25487))
* per-tool token efficiency tracking (Suggestion 10) ([a588365](https://github.com/special-place-administrator/symforge/commit/a5883651c7bc384b431e7964bd3f0e1e04a4682c))

## [4.1.0](https://github.com/special-place-administrator/symforge/compare/v4.0.0...v4.1.0) (2026-03-30)


### Features

* improve error context on failures (Suggestion 7) ([8169a6e](https://github.com/special-place-administrator/symforge/commit/8169a6e2ed0c1a7d212e865456ecabf29018dfed))

## [4.0.0](https://github.com/special-place-administrator/symforge/compare/v3.1.6...v4.0.0) (2026-03-26)


### ⚠ BREAKING CHANGES

* Line numbers in search_symbols, get_symbol_context, trace_symbol, inspect_match, and sidecar endpoints shift from 0-indexed to 1-indexed. Clients parsing these outputs numerically must account for the +1 change.
* rename Tokenizor → SymForge

### Features

* **01-01:** implement kind-tier disambiguation in resolve_symbol_selector ([2e11ac4](https://github.com/special-place-administrator/symforge/commit/2e11ac4985e690e034da2982fcf3b900d734d30b))
* **02-01:** hook diagnostics — verbose mode, port-missing vs stale, one-time hint ([4547428](https://github.com/special-place-administrator/symforge/commit/4547428aeb984e727947ea94a3f0e40451060216))
* 4 UX improvements from external review feedback (Wave 1) ([430a86a](https://github.com/special-place-administrator/symforge/commit/430a86a4fad65c9725f21037d070349e165f8cee))
* 4 UX improvements from external review feedback (Wave 2) ([0e773e6](https://github.com/special-place-administrator/symforge/commit/0e773e603ee51178d8cc1e36a851ada55c26e9f6))
* add .env file extractor ([44386b1](https://github.com/special-place-administrator/symforge/commit/44386b17adfb95f45372f2cd850588e1cf475304))
* add AdmissionTier enum and size threshold constants ([48cc242](https://github.com/special-place-administrator/symforge/commit/48cc242852a06b463119757408b911b41efcf493))
* add binary content sniff with NUL, UTF-8, and control-byte heuristics ([e7bd071](https://github.com/special-place-administrator/symforge/commit/e7bd0713957a611e0fabd42f3f6c0e68b042782f))
* add ConfigExtractor trait, EditCapability enum, key escaping ([a46f029](https://github.com/special-place-administrator/symforge/commit/a46f0299a62540e5d83c6665bae1b3125f92e955))
* add doc_byte_range field to SymbolRecord ([5699030](https://github.com/special-place-administrator/symforge/commit/569903096ad933cb1099afa0688de821aae9c6d2))
* add DocCommentSpec and scan_doc_range algorithm ([61ab5bb](https://github.com/special-place-administrator/symforge/commit/61ab5bb1ddda789f0e69a5f27181ab54a76d4e02))
* add extension denylist for admission control ([6159487](https://github.com/special-place-administrator/symforge/commit/6159487c7468fcb037ae2ed6a603eeb9c56c02b9))
* add first-class config file indexing and gated edit support for JSON/TOML/YAML/Markdown/.env ([4bbac75](https://github.com/special-place-administrator/symforge/commit/4bbac7599509e62fbd4bc94551eddc277fc8d68f))
* add frontend asset parsing (HTML, CSS, SCSS) ([a91b625](https://github.com/special-place-administrator/symforge/commit/a91b625739c997da8b0a1e5f8cf41b46979aeb65))
* add Html, Css, Scss to LanguageId with extension mapping ([283c592](https://github.com/special-place-administrator/symforge/commit/283c59214990c6f316247830c271d868be86a701))
* add JSON key-path extractor ([a3485d3](https://github.com/special-place-administrator/symforge/commit/a3485d3ef4af680770036df9b1f09d3f243509c3))
* add Json/Toml/Yaml/Markdown/Env to LanguageId, Key/Section to SymbolKind, is_config to FileClassification ([7ccd265](https://github.com/special-place-administrator/symforge/commit/7ccd2650cc9ae88018f15fe7968914e3bdb8aada))
* add LineEnding detection and normalization helpers (C1 prep) ([a82a727](https://github.com/special-place-administrator/symforge/commit/a82a72763a9060650553c68da33a0e2d5bd8dc95))
* add Markdown section extractor ([cc1a128](https://github.com/special-place-administrator/symforge/commit/cc1a128ff48908ee6828476ef951058f09c15ace))
* add match-occurrence retrieval and watcher reconciliation health reporting ([5043a5a](https://github.com/special-place-administrator/symforge/commit/5043a5a64e74d7edc43a6f087572837ae2b7d501))
* add module-path boosting to explore (Phase 0) ([2f7dac0](https://github.com/special-place-administrator/symforge/commit/2f7dac03d228beb673c16c418d78815a5e145619))
* add per-language DocCommentSpec and wire into push_symbol ([5a0fff2](https://github.com/special-place-administrator/symforge/commit/5a0fff26d9020c5aefe02d4a319727ffc99121ff))
* add SkippedFile struct and store integration for admission tiers ([b94aeeb](https://github.com/special-place-administrator/symforge/commit/b94aeeb8758ccce92733fefc74a97858b6ccf978))
* add TOML key-path extractor ([932b977](https://github.com/special-place-administrator/symforge/commit/932b977e402b8f5503988efeba135bf8dea06873))
* add tooling preference guide and challenge line to README ([8cd028c](https://github.com/special-place-administrator/symforge/commit/8cd028cd320d62b75af84da1614955f630d0a07d))
* add tree-sitter-html, tree-sitter-css, tree-sitter-scss dependencies ([3be5ee3](https://github.com/special-place-administrator/symforge/commit/3be5ee3dd9544cd9ff03e8d5a3dfe579764547bb))
* add unified edit_capability_for_language, rename check_edit_capability ([1742a17](https://github.com/special-place-administrator/symforge/commit/1742a1744842f376c611a7ab8b5cd24e7d214803))
* add YAML key-path extractor with serde_yml ([c5919e2](https://github.com/special-place-administrator/symforge/commit/c5919e20ddc5707889e32b764bf5afdac58ed1fb))
* **adoption:** add hook outcome metrics ([cce4473](https://github.com/special-place-administrator/symforge/commit/cce4473e5024ade6de5f5eea2b6daef87ff38c2e))
* **adoption:** add workflow sidecar adapters ([143ac0b](https://github.com/special-place-administrator/symforge/commit/143ac0bfac5f09a978e4f85bc70c75f2028a0477))
* **adoption:** define owned workflows for hooks ([0185310](https://github.com/special-place-administrator/symforge/commit/0185310fdbae3fded168d5a4249792372f666234))
* **adoption:** steer protocol read workflows ([106cdda](https://github.com/special-place-administrator/symforge/commit/106cddaec9ecb7f0f1a82dae22cb54d95e942ff8))
* **adoption:** tighten hook routing for source workflows ([5b9426a](https://github.com/special-place-administrator/symforge/commit/5b9426a9aa3f6f02f3f27575602bb35f1b74a8f9))
* aggregate token savings across tool handlers ([25343e9](https://github.com/special-place-administrator/symforge/commit/25343e9b20774d3489ca9610955ca81aeda38e5b))
* analyze_file_impact shows clear status taxonomy (U4) ([263834f](https://github.com/special-place-administrator/symforge/commit/263834f3d8dd2a4c22911eed69a506edb7c13bd6))
* batch_edit dry_run mode (U5) ([4166196](https://github.com/special-place-administrator/symforge/commit/41661963168aced0d37c7931628c3dcf49f6b550))
* batch_rename supplemental qualified path scan with confidence classification ([e75f2d4](https://github.com/special-place-administrator/symforge/commit/e75f2d40d657ea668660d3160c884feaf396ec96))
* bump index snapshot version to 3 for doc_byte_range ([c002fcc](https://github.com/special-place-administrator/symforge/commit/c002fccfe02f569a28b62615433d99ea7a3be1e0))
* clean npm cache after install to reclaim disk space ([b1c4a35](https://github.com/special-place-administrator/symforge/commit/b1c4a353fc1d1985c31e363d1be22f7f4e17a440))
* concept+remainder merging in explore ([c6e93bd](https://github.com/special-place-administrator/symforge/commit/c6e93bd04ea9035a9c3a9ab279539f9ebb8ba1d1))
* config file parsing — all extractors, pipeline integration, edit gating, test fixes ([bde2f3d](https://github.com/special-place-administrator/symforge/commit/bde2f3d1a80bdd72cc22ed10d227963e4a651d44))
* **config:** add structured syntax diagnostics ([7360beb](https://github.com/special-place-administrator/symforge/commit/7360bebb290e50bc65fee0a63ef5118cdc72117c))
* daemon fallback, callee dedup, token budget, search defaults ([d13e76b](https://github.com/special-place-administrator/symforge/commit/d13e76b77308b16d42bf721400f10ef6215cc896))
* edit tools use doc_byte_range for splice boundaries ([cee3ff2](https://github.com/special-place-administrator/symforge/commit/cee3ff29d43078448555b4479a4de399d9731b9e))
* **edit:** add dry_run to replace_symbol_body, insert_symbol, delete_symbol, edit_within_symbol ([1f401d8](https://github.com/special-place-administrator/symforge/commit/1f401d82444bf19bd86746b2346b8aa3082f9880))
* **edit:** track item byte ranges on symbols ([da3294c](https://github.com/special-place-administrator/symforge/commit/da3294c623589382bfa8dd2311944d868a6804e7))
* expand CONCEPT_MAP and add word-boundary matching ([f94e07d](https://github.com/special-place-administrator/symforge/commit/f94e07dc29cb51f2055cf05ebea91946f02f1f1a))
* explore filters noise by default (U1) ([f14b702](https://github.com/special-place-administrator/symforge/commit/f14b702b0a68ba6c88b5c96327cbdf5d701e5d72))
* **find_dependents:** show symbol names in mermaid and dot edge labels ([e358b77](https://github.com/special-place-administrator/symforge/commit/e358b77697162cd859fd41f72a301ee23f64d5f3))
* gate edit tools by config file EditCapability ([7613d11](https://github.com/special-place-administrator/symforge/commit/7613d112cfa8c688c85d498808c4f7c740fcb9ce))
* get_file_content falls back to raw disk read for non-source files ([9bf8ba5](https://github.com/special-place-administrator/symforge/commit/9bf8ba5cdc7811b1d24c340d78880a01510b8130))
* get_file_content mode enum for clearer API (U10) ([244be75](https://github.com/special-place-administrator/symforge/commit/244be753b08275b80f3bd1d8a11214a74f642f03))
* **get_repo_map:** paginate detail=full output with max_files parameter ([8c15c5d](https://github.com/special-place-administrator/symforge/commit/8c15c5d2bc55768123a754fa6a74c23bb9b6c131))
* git churn in ranking, expanded guidance blocks, improved tool descriptions ([4cf1e6e](https://github.com/special-place-administrator/symforge/commit/4cf1e6e782d79e52554c23aae98590fb5a60feb5))
* health shows partial parse file paths (U8) ([8560114](https://github.com/special-place-administrator/symforge/commit/856011485f82605d41e93651748bf64db1486c91))
* **health:** list failed files with error messages in health report ([7306089](https://github.com/special-place-administrator/symforge/commit/73060890c96058223dd04dffd91b06087fb4dd1a))
* implement admission gate with tiered file classification ([9e69e23](https://github.com/special-place-administrator/symforge/commit/9e69e238f50b3364545fa3844cbb5b03ae7ad925))
* implement CSS symbol extractor with tests ([39719f4](https://github.com/special-place-administrator/symforge/commit/39719f4962657e824f9a82b9c8040be48af71cfd))
* implement HTML/Angular symbol extractor with tests ([4627b52](https://github.com/special-place-administrator/symforge/commit/4627b520009e2c8395f39593644499cae428fe75))
* implement SCSS symbol extractor with tests ([2112b1a](https://github.com/special-place-administrator/symforge/commit/2112b1a7776db40d5d63ccd12da753fba085aaa9))
* improve explore relevance ranking (Q1) ([5b829ca](https://github.com/special-place-administrator/symforge/commit/5b829cab9138885daf8479b3fb3a594bc35838d5))
* include doc comments in symbol body extraction ([679682b](https://github.com/special-place-administrator/symforge/commit/679682b03d9a9548b8b67639c3257b6a5a63ff9c))
* **index:** Sprint 0 — index freshness guarantee via mtime tracking ([29d60d6](https://github.com/special-place-administrator/symforge/commit/29d60d6fe0e83f3c79856c38796bc02c62f62bea))
* **init:** add alwaysAllow to Claude MCP entry and expand CLAUDE.md guidance ([4ee5f53](https://github.com/special-place-administrator/symforge/commit/4ee5f535546e829850c6136b5785bd56b23c7732))
* **init:** harden client guidance rollout ([f30667c](https://github.com/special-place-administrator/symforge/commit/f30667ce885ba5040a4a40751b7401080fad8977))
* integrate config extractors into parsing pipeline ([961c25b](https://github.com/special-place-administrator/symforge/commit/961c25b36a75ab4ea033ce5921f4368ab41a4495))
* **json:** add JSONC comment stripping for tsconfig.json support ([c3c208f](https://github.com/special-place-administrator/symforge/commit/c3c208fb496449efe2cd14a7ee82562bd4088df9))
* lenient vec deserializer, semantic search ranking, Kilo Code init, SymForge rename plan ([c048274](https://github.com/special-place-administrator/symforge/commit/c04827422bb02a04ff4222864b0c09ff014ca70d))
* per-tool call counters in health output (U9) ([d41bfb5](https://github.com/special-place-administrator/symforge/commit/d41bfb5730d07b2b0271ee1947eec0306f07375c))
* PreToolUse hook intercepts Grep/Read/Glob/Edit with Tokenizor suggestions ([1c78000](https://github.com/special-place-administrator/symforge/commit/1c780008662d9fb1c5cf1d7e573df64e04a23807))
* PreToolUse hook now intercepts config files for Tokenizor suggestions ([ee88bff](https://github.com/special-place-administrator/symforge/commit/ee88bff2f5341e026cbed90b6da7a1dcaaef33eb))
* quality improvements from 3-project eval (Q1-Q6) ([8d23ff4](https://github.com/special-place-administrator/symforge/commit/8d23ff4749250d4ad85f12324280543fd8c5e403))
* quality improvements from eval feedback (Q3-Q6) ([79ac714](https://github.com/special-place-administrator/symforge/commit/79ac714eb67a2d504be76a4beec7479cfe154385))
* rename Tokenizor → SymForge ([6366cd0](https://github.com/special-place-administrator/symforge/commit/6366cd0c7f51bc496cceb6ae255e22d95f109183))
* richer verbosity=signature includes visibility and return type (U6) ([eef2926](https://github.com/special-place-administrator/symforge/commit/eef2926f057e3020200bb13cc1dd47b9ee9bf76e))
* RTK adoption milestone — symbol disambiguation tests, hook diagnostics, docs links ([9bc3ead](https://github.com/special-place-administrator/symforge/commit/9bc3ead5c49813998a9987b3e2066398313d48db))
* search_symbols browse mode without query (U2) ([3326342](https://github.com/special-place-administrator/symforge/commit/33263428425acd01a9cfba460d96d0b5534257b5))
* **search_text:** annotate which term matched in OR-term searches ([e53a7f7](https://github.com/special-place-administrator/symforge/commit/e53a7f748e55afe0983d4710b6375edc01058397))
* show Tier 2 tags and Tier 3 footer in repo_map ([05d23eb](https://github.com/special-place-administrator/symforge/commit/05d23eb7b23c06c607e6adacac00ae7edbb2c7dc))
* Sprint 14 — trust fixes + tiered admission control ([b7a9296](https://github.com/special-place-administrator/symforge/commit/b7a92963b3f9c55be08e73e04eba6bd70901b1bf))
* wire admission gate into discovery walk ([51c73f7](https://github.com/special-place-administrator/symforge/commit/51c73f7b125d13a88179d9e1e2cf535e28839888))
* wire HTML, CSS, SCSS extractors into parsing pipeline ([e740f94](https://github.com/special-place-administrator/symforge/commit/e740f947007acee5db43a321bb38152e1fed63cf))


### Bug Fixes

* 26 bug fixes across parsers, protocol, indexing, sidecar, and npm ([b2abebc](https://github.com/special-place-administrator/symforge/commit/b2abebc4710580cdb62fe984809c4ddc949cd8a6))
* 4 display/UX improvements in search, outline, repo_map, and get_symbol ([b3a449c](https://github.com/special-place-administrator/symforge/commit/b3a449c81a0802070b035cc41f8e44b46ad50a50))
* add 'burst' to file watching concept symbol_queries ([930d8e8](https://github.com/special-place-administrator/symforge/commit/930d8e87b053af0c1035e6518ca8b0c3ea46b1ee))
* add exe/dll/so/dylib/class to denylist (C2-lite) ([14d0459](https://github.com/special-place-administrator/symforge/commit/14d04593df00b7cbc92643aeb5c7109026a045cb))
* add missing gitignore/noise_class field initializers across codebase ([c8088f9](https://github.com/special-place-administrator/symforge/commit/c8088f9f0953b004e015c2a707db89dae3597ced))
* add missing sibling_limit/overflow fields to initializers ([b25f4a5](https://github.com/special-place-administrator/symforge/commit/b25f4a5a34a007ad9a56757cd1a62ce7c9f92157))
* add NOT-for tips to 5 tool descriptions missing them ([14a8fad](https://github.com/special-place-administrator/symforge/commit/14a8fad0de46c1b3aa35711a8bbda3f8efee2c94))
* add panic hook to clean up sidecar port files on crash ([6758696](https://github.com/special-place-administrator/symforge/commit/6758696a02d052d6646721629a51c67ee8978f93))
* add total hit limit to find_references ([d592e13](https://github.com/special-place-administrator/symforge/commit/d592e136df4d78c04be59c6a3e73edc7d1fad2c2))
* address all actionable feedback from 3 external code reviews ([61d2757](https://github.com/special-place-administrator/symforge/commit/61d2757a8b9bbc425b73eec512dc75c470be630d))
* around_line error, diff note, language-scoped warnings, dry-run ([a6a5f70](https://github.com/special-place-administrator/symforge/commit/a6a5f701fc1bfb56f7e13cb01f732383e101950a))
* around_symbol returns full indexed symbol span (B2) ([3b06c2a](https://github.com/special-place-administrator/symforge/commit/3b06c2a735ad3edd0ac691851a944d66797b06f9))
* auto-correct double-escaped regex patterns in search_text ([e98cd4d](https://github.com/special-place-administrator/symforge/commit/e98cd4d6671e129b1bc775ec4dde28129931b22e))
* batch_edit dry_run byte count + auto-detect regex in search_text ([29474c2](https://github.com/special-place-administrator/symforge/commit/29474c21bd33b1da6858bdbb4179eaa3ac9611a1))
* batch_edit shows ROLLED BACK message on failure (B4) ([3ab8358](https://github.com/special-place-administrator/symforge/commit/3ab83587c7bbee77bd1f1cfe1b1980066630da8f))
* batch_insert no extra blank line before function (B1) ([3409548](https://github.com/special-place-administrator/symforge/commit/34095482b8275811cb5373003e367c0b07dcfec0))
* batch_rename atomic rollback on failure, batch_edit/batch_insert best-effort with correct index state ([6b332f3](https://github.com/special-place-administrator/symforge/commit/6b332f3f18484a8e14ed35731961eac98311b18f))
* batch_rename catches path-qualified usages ([c2243ff](https://github.com/special-place-administrator/symforge/commit/c2243ff058e0ca29848272641eed8c27eec47131))
* batch_rename catches path-qualified usages via literal scan ([2824745](https://github.com/special-place-administrator/symforge/commit/282474529928f46e3dec525470244baa3fb68873))
* batch_rename review fixes — atomic rollback, dead code, dedup ([0a7844e](https://github.com/special-place-administrator/symforge/commit/0a7844ee1b8ce77b462f1f6a3b51b53c1ce17a22))
* **bundle:** resolve impl suggestions and dependency-aware limits ([d5bfa6a](https://github.com/special-place-administrator/symforge/commit/d5bfa6aa85e3dfd719340301e1eff01d4bb1c069))
* code review feedback — tests and safety fixes ([c2454ea](https://github.com/special-place-administrator/symforge/commit/c2454ea2bbd1ec0c2f2acb357bd9f158fd101584))
* complete audit remediation — language tests, deferred fixes, dedup ([c04e849](https://github.com/special-place-administrator/symforge/commit/c04e84968e406e1662b5f36fa7df2ae923ab15fe))
* complete parking_lot::RwLock migration across live_index and protocol ([c48d865](https://github.com/special-place-administrator/symforge/commit/c48d865c2e9c717b89da9d446a51f326af5d3052))
* comprehensive codebase audit — 18 bug fixes across parsers, core engine, and protocol ([3b0cd44](https://github.com/special-place-administrator/symforge/commit/3b0cd442841748ebb30e05d2b23d13b57246ee5e))
* compute real line numbers for TOML symbols ([d939caa](https://github.com/special-place-administrator/symforge/commit/d939caa7d3a587efa7482dc9f4790e7b102e15ac))
* compute real line numbers for TOML symbols ([7dd4697](https://github.com/special-place-administrator/symforge/commit/7dd46973f52228193cc98ccb8d01361b75eef808))
* CSS @layer/[@container](https://github.com/container) extraction — use generic at_rule node kind ([97fc47f](https://github.com/special-place-administrator/symforge/commit/97fc47fd201c89888231e21e4f665d919c73b3fc))
* daemon lifecycle hardening — stale lock detection, fast-fail proxy, cleanup (DL1-DL4) ([8350d7e](https://github.com/special-place-administrator/symforge/commit/8350d7e3b5de2bb16d85d69a817ca459fb43e829))
* daemon proxy deadlock under concurrent tool calls + request governor ([541dd68](https://github.com/special-place-administrator/symforge/commit/541dd688e9956d75818733740764320513e9c8ab))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([3b1cc4e](https://github.com/special-place-administrator/symforge/commit/3b1cc4ed6ae8591ca8afc720158f2241cbec80de))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([b793579](https://github.com/special-place-administrator/symforge/commit/b793579b2f1bdfdd7fb266bfa9bf7dd76590ea9e))
* **dependents:** filter false positives from non-pub symbol name collisions ([0bf3c77](https://github.com/special-place-administrator/symforge/commit/0bf3c77b1d41ea7ba383d18b83029ba15f0855a6))
* **diff_symbols:** show omission note in compact mode when files have no symbol changes ([c6eade8](https://github.com/special-place-administrator/symforge/commit/c6eade8fc0770c042c26b9a0b258a7b619537e21))
* **diff_symbols:** skip type keywords in C# const declarations ([00049d2](https://github.com/special-place-administrator/symforge/commit/00049d264058dafbbbb2ab72816cdfc8dd608164))
* edit_within splice range + multi-line block comment detection ([4dd4ea8](https://github.com/special-place-administrator/symforge/commit/4dd4ea84de658b5170d09d42c0200d3bd171ae2b))
* emit strict MCP array schemas for optional list params ([3519470](https://github.com/special-place-administrator/symforge/commit/3519470c2cafc36cd1d4c7e9b4455b8f3020623f))
* explore depth=2 shows symbol-level callers, get_symbol uses tier disambiguation ([9a22035](https://github.com/special-place-administrator/symforge/commit/9a22035872fb0ad58026a9dc50d66f0440e98f4b))
* explore depth=2 shows symbol-level callers, get_symbol uses tier… ([4c1f588](https://github.com/special-place-administrator/symforge/commit/4c1f588fc1bd7282f74251eadb587913f89658a2))
* explore multi-term scoring with enclosing symbol injection ([5f36dab](https://github.com/special-place-administrator/symforge/commit/5f36dab73ac26247945d34f769e182e48e6bfbe6))
* explore text search max_per_file too low for symbol injection ([4a5b67e](https://github.com/special-place-administrator/symforge/commit/4a5b67edf156e27cfe21e1f8e4c0b86fea03ff9e))
* filter explore noise from CONCEPT_MAP self-matching and generic terms ([8addfaa](https://github.com/special-place-administrator/symforge/commit/8addfaaa4646144458ea1c71b1a9fa1de7728826))
* find_dependents resolves workspace crate paths ([418652a](https://github.com/special-place-administrator/symforge/commit/418652a42bd3e1443602cb87cd1eed2d7e4c0574))
* find_dependents resolves workspace crate paths (B4) ([f02819d](https://github.com/special-place-administrator/symforge/commit/f02819db942530bcbb974fe946d05224bb953ab6)), closes [#89](https://github.com/special-place-administrator/symforge/issues/89)
* find_references file count + get_repo_map full path filter ([89cc588](https://github.com/special-place-administrator/symforge/commit/89cc5880c4c7ffb8ed2e09dd9c8bd9d7e573fa52))
* **find_references:** explain why classes/structs have no implementations ([7566e2a](https://github.com/special-place-administrator/symforge/commit/7566e2a3bc9a928d55b6266307738029d63487e9))
* follow_refs shows same-file callers and empty-result signal ([b3acea1](https://github.com/special-place-administrator/symforge/commit/b3acea139692468421ed85ef655ff958baad008d))
* Gemini CLI init writes correct timeout (120000ms) and trust setting ([b8616eb](https://github.com/special-place-administrator/symforge/commit/b8616eb1f5621872e050940b6756c9a04e707668))
* **get_file_content:** explain why zero-symbol files have no matches ([d4420f8](https://github.com/special-place-administrator/symforge/commit/d4420f8df45c949f6bc3be1731eca17fe620c7db))
* get_file_context sections filter masked by 800-byte hook budget ([5ff4c9e](https://github.com/special-place-administrator/symforge/commit/5ff4c9e3031e099d7321c5b906ba3109f031e3f7))
* **get_symbol_context:** auto-resolve path and show empty-references message ([8b4caf5](https://github.com/special-place-administrator/symforge/commit/8b4caf5b49e6ac402744b23eddd1926762421b62))
* Go method names, SCSS $variable extraction, language filter completeness ([4156d41](https://github.com/special-place-administrator/symforge/commit/4156d41d24af8739494482ea4ba868a69fec747d))
* handle SIGTERM for daemon graceful shutdown (C5) ([3faef1f](https://github.com/special-place-administrator/symforge/commit/3faef1f772ef679f137229cc7fc9d47891c132f6))
* index purge on file delete, richer default symbol context, path-scoped implementations ([ec634b5](https://github.com/special-place-administrator/symforge/commit/ec634b5aae1d7dcec5137b70757282efb9c578d2))
* index purge on file delete, richer default symbol context, path-scoped implementations ([1cb084b](https://github.com/special-place-administrator/symforge/commit/1cb084b0444f89d825cae0d2d60e631e0cbd54c5))
* index safety hardening and tool output correctness ([c952759](https://github.com/special-place-administrator/symforge/commit/c952759567b0a1e7a2de4cd56ccb0576eaa792a8))
* index safety hardening and tool output correctness ([59720c5](https://github.com/special-place-administrator/symforge/commit/59720c5fc9e48eccb2fc41d291a804b0a69cbd55))
* **init:** canonicalize SymForge Codex guidance and allowlists ([7680757](https://github.com/special-place-administrator/symforge/commit/76807570e24995dfc05ad6df022cbd1fbab25bfa))
* insert_before uses blank line separator when no doc comments ([2253f7d](https://github.com/special-place-administrator/symforge/commit/2253f7da0c14cb4c6d858442af375e5d8552bebe))
* **kilo:** trigger release for strict-provider compatibility ([a852955](https://github.com/special-place-administrator/symforge/commit/a852955707cc9da1133f9a872d8d7b3b955988e7))
* make SymForge MCP schemas compatible with strict OpenAI clients ([aaa42f2](https://github.com/special-place-administrator/symforge/commit/aaa42f2aa6cb17af2a7d149abd5d0483f8086513))
* non-ASCII panic in doc scanning and deterministic circuit-breaker (CR1, CR2) ([1e52aaf](https://github.com/special-place-administrator/symforge/commit/1e52aaf7468d425fac21371398a631a93d6c5bfe))
* non-blocking cold-start indexing for faster MCP discovery ([acb8743](https://github.com/special-place-administrator/symforge/commit/acb874307fc01eaf162cf76de4cba2dc1e942ba8))
* normalize exact get_file_content paths and backfill mtime_secs in integration fixtures ([fb398b1](https://github.com/special-place-administrator/symforge/commit/fb398b1dcbfdc59055fea328e995bdb1d9ba114c))
* **npm:** keep global auto-init out of workspaces ([1626dfa](https://github.com/special-place-administrator/symforge/commit/1626dfa89a6da5a9e14b0107df3eec7a0b325c61))
* **npm:** persist wrapper install metadata ([eeac029](https://github.com/special-place-administrator/symforge/commit/eeac0298ef08253fdf70042ba5d2f78f142faea2))
* pin CI/release workflows to Rust 1.94.0 matching rust-toolchain.toml ([41d23ab](https://github.com/special-place-administrator/symforge/commit/41d23ab3fd83205773f9289cf95cd142fd2cb1b9))
* pin Rust toolchain to 1.94.0 via rust-toolchain.toml ([fed0e20](https://github.com/special-place-administrator/symforge/commit/fed0e20560de0356cb86bf5dc9319af2867e770c))
* prevent async runtime starvation under concurrent subagent load ([74f1d54](https://github.com/special-place-administrator/symforge/commit/74f1d54f0f26dba97a619fb5b69e645c1d702034))
* prevent async runtime starvation under concurrent subagent load ([2ed134a](https://github.com/special-place-administrator/symforge/commit/2ed134aa34a82f258eb37be4121c341272cc85d6))
* prevent non-ASCII panic in find_qualified_usages (batch_rename crash) ([8555d0c](https://github.com/special-place-administrator/symforge/commit/8555d0ce0b3ed39acf7d170c0878d4e94267a2ef))
* recurse into mixin/function bodies, guard empty at-rule names ([8a8e717](https://github.com/special-place-administrator/symforge/commit/8a8e7178c079078581ac8c6e339128814e16478a))
* reindex from disk after writes, not from in-memory buffer ([d605498](https://github.com/special-place-administrator/symforge/commit/d6054988db18ad6cf82e3a82cca2c054a1c5f52b))
* **release:** add noncommercial licensing and kill-all npm updates ([17354c6](https://github.com/special-place-administrator/symforge/commit/17354c6adba9e75a09bc3929b753881552ec929a))
* rem_euclid for timestamps, generic pub(...) visibility in diff_symbols ([f597c78](https://github.com/special-place-administrator/symforge/commit/f597c78fabcec1ce18b4e20665516cd8fd772537))
* remediate reviewer feedback from external codebase testing ([80303a9](https://github.com/special-place-administrator/symforge/commit/80303a9a9558d05ad281740552fdc95b349270f3))
* replace std::sync::Mutex with parking_lot::Mutex to prevent poison cascades ([04651d0](https://github.com/special-place-administrator/symforge/commit/04651d09f5fccbad4e4ee267b2cf4c2ee953ea44))
* resolve 16 bugs across mtime propagation, line indexing, correctness, and concurrency ([8cfda64](https://github.com/special-place-administrator/symforge/commit/8cfda649bf2bd0a017fa72643bce088f817cd1bc))
* resolve 4 bugs from code review ([31b9a0c](https://github.com/special-place-administrator/symforge/commit/31b9a0c78b5576262c74946adaf00aad8262ebcb))
* resolve 5 tool bugs from hands-on review ([6d11014](https://github.com/special-place-administrator/symforge/commit/6d1101448f993946699028e26ecf8852f81073be))
* restore missing [[package]] header in Cargo.lock after rebase conflict resolution ([67d9327](https://github.com/special-place-administrator/symforge/commit/67d932778c02a9899ffec904560868927604d968))
* revert worker_threads override — spawn_blocking handles concurrency ([a5d5d4e](https://github.com/special-place-administrator/symforge/commit/a5d5d4e77dfd2d438a42ef2161fd1c7111584abd))
* review feedback — Q3 robust name extraction, Q6 UTF-8 safe truncation ([41e17d2](https://github.com/special-place-administrator/symforge/commit/41e17d23a4b4ce9e89d9fdfbceb5ba089a9880a0))
* rewrite open_project_session with double-checked locking (C6) ([b04b0d0](https://github.com/special-place-administrator/symforge/commit/b04b0d099fb035ca1bac9c76c49d542cae1d8102))
* search_symbols file count + find_references missing cross-file type refs ([8d40874](https://github.com/special-place-administrator/symforge/commit/8d4087446f8f7c980f5e5abe6b16366e6cc5f697))
* security patches, parser improvements, parallelism fixes, and review follow-ups ([2b1d5cb](https://github.com/special-place-administrator/symforge/commit/2b1d5cbafed5100ea833d0f4b41da41b1d87cb27))
* show_line_numbers works with around_symbol and around_match (B3) ([4befe8a](https://github.com/special-place-administrator/symforge/commit/4befe8a8f8e734e722088632c23ac81489bf42ce))
* sidecar reliability + reviewer feedback remediation ([8b41990](https://github.com/special-place-administrator/symforge/commit/8b419904451053df06d82f06ba7921f435395af0))
* surface tool panics as immediate error responses instead of stalls ([31ae935](https://github.com/special-place-administrator/symforge/commit/31ae935642876711383897ba3b779ea4c2dc7b52))
* Swift enum/extension/protocol detection and Angular template robustness ([af34df2](https://github.com/special-place-administrator/symforge/commit/af34df266b7cb34ba4dbd24853b585554bba7308))
* symbol kind filter accepts semantic aliases (variable, function, etc.) ([2e80fb5](https://github.com/special-place-administrator/symforge/commit/2e80fb5cd96b441e256e4a7c7afb5fad20fbbfbe))
* **test:** update assertion for changed zero-symbol message ([90a5722](https://github.com/special-place-administrator/symforge/commit/90a57229f697bfc541adc4d58ea35f1f2dc53295))
* thread LineEnding through all edit helpers for CRLF preservation (C1) ([dda40b4](https://github.com/special-place-administrator/symforge/commit/dda40b4c0c8a99259a93af4c9ff65bb696e9e337))
* update installer test assertion for execFileSyncFn version check ([f6ed05d](https://github.com/special-place-administrator/symforge/commit/f6ed05dd601105a4a3249cf2844b4da91210c560))
* update rollback tests for tempfile-based atomic writes ([548c2bb](https://github.com/special-place-administrator/symforge/commit/548c2bbede27be2a233fe35a2d7f6b8aa0aee45b))
* use unique temp files in atomic_write_file (C3) ([dddcb16](https://github.com/special-place-administrator/symforge/commit/dddcb16eb85dc294354691e9fc470adbca5ce9bd))
* UX improvements from third review ([648218f](https://github.com/special-place-administrator/symforge/commit/648218fe4b70378133ec115dd93cd5a089b44bbc))
* validate splice overlap in batch_rename (C4) ([0c80b74](https://github.com/special-place-administrator/symforge/commit/0c80b74903261a09837a7038998bf3d61898d274))
* watcher recv_timeout blocks tokio worker — use try_recv + async sleep ([a4b7d34](https://github.com/special-place-administrator/symforge/commit/a4b7d34db100eac9528325f6f3bdbdd58827d54d))
* wave 1 audit remediation — 12 safety and correctness fixes ([b02bd12](https://github.com/special-place-administrator/symforge/commit/b02bd1203ba78ee404e204dda80ae54328ba3642))
* wave 2 audit remediation — 10 reliability and consistency fixes ([a293819](https://github.com/special-place-administrator/symforge/commit/a2938196992b09c8ddb2a0fc40a732ff499627e0))
* wave 3 audit remediation — polish, docs, and remaining fixes ([c7c2ba8](https://github.com/special-place-administrator/symforge/commit/c7c2ba8f6129565f16a3346026ae35606f8f60f5))
* widen common-name warning in find_references to trigger on ref c… ([32b22b4](https://github.com/special-place-administrator/symforge/commit/32b22b432d54e1a02fced916ebfbdb08d21b8c1a))
* widen common-name warning in find_references to trigger on ref count alone ([2c3cc4b](https://github.com/special-place-administrator/symforge/commit/2c3cc4b472d1df4645b3008d32451ffe1a104082))
* wrap daemon sidecar handlers with governor + spawn_blocking ([d665e41](https://github.com/special-place-administrator/symforge/commit/d665e415a5f889229ef7c5dc8a18a4c9cadc36bd))
* wrap env var manipulation in unsafe blocks for Rust 2024 edition compliance ([c363499](https://github.com/special-place-administrator/symforge/commit/c3634999e2c91fcbc57a2fb92decc5f6a217a77f))
* wrap repair_file_indices in catch_unwind to prevent double-panic abort ([e2d2e97](https://github.com/special-place-administrator/symforge/commit/e2d2e976f4bef08ca678ad5e2a2142f5f5c48f2a))


### Performance Improvements

* incremental reverse index updates on file mutation ([e85c445](https://github.com/special-place-administrator/symforge/commit/e85c445b7d7e017a961e4de5da851a6ca0e0cd01))

## [3.1.5](https://github.com/special-place-administrator/symforge/compare/v3.1.4...v3.1.5) (2026-03-25)


### Bug Fixes

* emit strict MCP array schemas for optional list params ([3519470](https://github.com/special-place-administrator/symforge/commit/3519470c2cafc36cd1d4c7e9b4455b8f3020623f))
* make SymForge MCP schemas compatible with strict OpenAI clients ([aaa42f2](https://github.com/special-place-administrator/symforge/commit/aaa42f2aa6cb17af2a7d149abd5d0483f8086513))

## [3.1.4](https://github.com/special-place-administrator/symforge/compare/v3.1.3...v3.1.4) (2026-03-23)


### Bug Fixes

* add panic hook to clean up sidecar port files on crash ([6758696](https://github.com/special-place-administrator/symforge/commit/6758696a02d052d6646721629a51c67ee8978f93))
* replace std::sync::Mutex with parking_lot::Mutex to prevent poison cascades ([04651d0](https://github.com/special-place-administrator/symforge/commit/04651d09f5fccbad4e4ee267b2cf4c2ee953ea44))
* sidecar reliability + reviewer feedback remediation ([8b41990](https://github.com/special-place-administrator/symforge/commit/8b419904451053df06d82f06ba7921f435395af0))
* wrap daemon sidecar handlers with governor + spawn_blocking ([d665e41](https://github.com/special-place-administrator/symforge/commit/d665e415a5f889229ef7c5dc8a18a4c9cadc36bd))
* wrap repair_file_indices in catch_unwind to prevent double-panic abort ([e2d2e97](https://github.com/special-place-administrator/symforge/commit/e2d2e976f4bef08ca678ad5e2a2142f5f5c48f2a))

## [3.1.3](https://github.com/special-place-administrator/symforge/compare/v3.1.2...v3.1.3) (2026-03-22)


### Bug Fixes

* explore depth=2 shows symbol-level callers, get_symbol uses tier disambiguation ([9a22035](https://github.com/special-place-administrator/symforge/commit/9a22035872fb0ad58026a9dc50d66f0440e98f4b))
* explore depth=2 shows symbol-level callers, get_symbol uses tier… ([4c1f588](https://github.com/special-place-administrator/symforge/commit/4c1f588fc1bd7282f74251eadb587913f89658a2))

## [3.1.2](https://github.com/special-place-administrator/symforge/compare/v3.1.1...v3.1.2) (2026-03-21)


### Bug Fixes

* widen common-name warning in find_references to trigger on ref c… ([32b22b4](https://github.com/special-place-administrator/symforge/commit/32b22b432d54e1a02fced916ebfbdb08d21b8c1a))
* widen common-name warning in find_references to trigger on ref count alone ([2c3cc4b](https://github.com/special-place-administrator/symforge/commit/2c3cc4b472d1df4645b3008d32451ffe1a104082))

## [3.1.1](https://github.com/special-place-administrator/symforge/compare/v3.1.0...v3.1.1) (2026-03-21)


### Bug Fixes

* add NOT-for tips to 5 tool descriptions missing them ([14a8fad](https://github.com/special-place-administrator/symforge/commit/14a8fad0de46c1b3aa35711a8bbda3f8efee2c94))

## [3.1.0](https://github.com/special-place-administrator/symforge/compare/v3.0.1...v3.1.0) (2026-03-21)


### Features

* 4 UX improvements from external review feedback (Wave 1) ([430a86a](https://github.com/special-place-administrator/symforge/commit/430a86a4fad65c9725f21037d070349e165f8cee))
* 4 UX improvements from external review feedback (Wave 2) ([0e773e6](https://github.com/special-place-administrator/symforge/commit/0e773e603ee51178d8cc1e36a851ada55c26e9f6))

## [3.0.1](https://github.com/special-place-administrator/symforge/compare/v3.0.0...v3.0.1) (2026-03-21)


### Bug Fixes

* 4 display/UX improvements in search, outline, repo_map, and get_symbol ([b3a449c](https://github.com/special-place-administrator/symforge/commit/b3a449c81a0802070b035cc41f8e44b46ad50a50))

## [3.0.0](https://github.com/special-place-administrator/symforge/compare/v2.0.11...v3.0.0) (2026-03-20)


### ⚠ BREAKING CHANGES

* Line numbers in search_symbols, get_symbol_context, trace_symbol, inspect_match, and sidecar endpoints shift from 0-indexed to 1-indexed. Clients parsing these outputs numerically must account for the +1 change.
* rename Tokenizor → SymForge

### Features

* **01-01:** implement kind-tier disambiguation in resolve_symbol_selector ([2e11ac4](https://github.com/special-place-administrator/symforge/commit/2e11ac4985e690e034da2982fcf3b900d734d30b))
* **02-01:** hook diagnostics — verbose mode, port-missing vs stale, one-time hint ([4547428](https://github.com/special-place-administrator/symforge/commit/4547428aeb984e727947ea94a3f0e40451060216))
* add .env file extractor ([44386b1](https://github.com/special-place-administrator/symforge/commit/44386b17adfb95f45372f2cd850588e1cf475304))
* add AdmissionTier enum and size threshold constants ([48cc242](https://github.com/special-place-administrator/symforge/commit/48cc242852a06b463119757408b911b41efcf493))
* add binary content sniff with NUL, UTF-8, and control-byte heuristics ([e7bd071](https://github.com/special-place-administrator/symforge/commit/e7bd0713957a611e0fabd42f3f6c0e68b042782f))
* add ConfigExtractor trait, EditCapability enum, key escaping ([a46f029](https://github.com/special-place-administrator/symforge/commit/a46f0299a62540e5d83c6665bae1b3125f92e955))
* add depth parameter to explore for enriched symbol analysis ([a81fdad](https://github.com/special-place-administrator/symforge/commit/a81fdad79b2ff06b96e9e841041e617675182652))
* add doc_byte_range field to SymbolRecord ([5699030](https://github.com/special-place-administrator/symforge/commit/569903096ad933cb1099afa0688de821aae9c6d2))
* add DocCommentSpec and scan_doc_range algorithm ([61ab5bb](https://github.com/special-place-administrator/symforge/commit/61ab5bb1ddda789f0e69a5f27181ab54a76d4e02))
* add extension denylist for admission control ([6159487](https://github.com/special-place-administrator/symforge/commit/6159487c7468fcb037ae2ed6a603eeb9c56c02b9))
* add first-class config file indexing and gated edit support for JSON/TOML/YAML/Markdown/.env ([4bbac75](https://github.com/special-place-administrator/symforge/commit/4bbac7599509e62fbd4bc94551eddc277fc8d68f))
* add frontend asset parsing (HTML, CSS, SCSS) ([a91b625](https://github.com/special-place-administrator/symforge/commit/a91b625739c997da8b0a1e5f8cf41b46979aeb65))
* add git2 library wrapper for in-process git operations ([dc5e146](https://github.com/special-place-administrator/symforge/commit/dc5e146cccc1a8f78af7916104cf9ae87c2ab375))
* add Html, Css, Scss to LanguageId with extension mapping ([283c592](https://github.com/special-place-administrator/symforge/commit/283c59214990c6f316247830c271d868be86a701))
* add JSON key-path extractor ([a3485d3](https://github.com/special-place-administrator/symforge/commit/a3485d3ef4af680770036df9b1f09d3f243509c3))
* add Json/Toml/Yaml/Markdown/Env to LanguageId, Key/Section to SymbolKind, is_config to FileClassification ([7ccd265](https://github.com/special-place-administrator/symforge/commit/7ccd2650cc9ae88018f15fe7968914e3bdb8aada))
* add LineEnding detection and normalization helpers (C1 prep) ([a82a727](https://github.com/special-place-administrator/symforge/commit/a82a72763a9060650553c68da33a0e2d5bd8dc95))
* add Markdown section extractor ([cc1a128](https://github.com/special-place-administrator/symforge/commit/cc1a128ff48908ee6828476ef951058f09c15ace))
* add match-occurrence retrieval and watcher reconciliation health reporting ([5043a5a](https://github.com/special-place-administrator/symforge/commit/5043a5a64e74d7edc43a6f087572837ae2b7d501))
* add module-path boosting to explore (Phase 0) ([2f7dac0](https://github.com/special-place-administrator/symforge/commit/2f7dac03d228beb673c16c418d78815a5e145619))
* add per-language DocCommentSpec and wire into push_symbol ([5a0fff2](https://github.com/special-place-administrator/symforge/commit/5a0fff26d9020c5aefe02d4a319727ffc99121ff))
* add routing hint, code_only flag, and update stale tool descriptions ([b941eff](https://github.com/special-place-administrator/symforge/commit/b941effdef0b608b01bb6394330f65459107f403))
* add SkippedFile struct and store integration for admission tiers ([b94aeeb](https://github.com/special-place-administrator/symforge/commit/b94aeeb8758ccce92733fefc74a97858b6ccf978))
* add TOML key-path extractor ([932b977](https://github.com/special-place-administrator/symforge/commit/932b977e402b8f5503988efeba135bf8dea06873))
* add tooling preference guide and challenge line to README ([8cd028c](https://github.com/special-place-administrator/symforge/commit/8cd028cd320d62b75af84da1614955f630d0a07d))
* add tree-sitter-html, tree-sitter-css, tree-sitter-scss dependencies ([3be5ee3](https://github.com/special-place-administrator/symforge/commit/3be5ee3dd9544cd9ff03e8d5a3dfe579764547bb))
* add unified edit_capability_for_language, rename check_edit_capability ([1742a17](https://github.com/special-place-administrator/symforge/commit/1742a1744842f376c611a7ab8b5cd24e7d214803))
* add YAML key-path extractor with serde_yml ([c5919e2](https://github.com/special-place-administrator/symforge/commit/c5919e20ddc5707889e32b764bf5afdac58ed1fb))
* **adoption:** add hook outcome metrics ([cce4473](https://github.com/special-place-administrator/symforge/commit/cce4473e5024ade6de5f5eea2b6daef87ff38c2e))
* **adoption:** add workflow sidecar adapters ([143ac0b](https://github.com/special-place-administrator/symforge/commit/143ac0bfac5f09a978e4f85bc70c75f2028a0477))
* **adoption:** define owned workflows for hooks ([0185310](https://github.com/special-place-administrator/symforge/commit/0185310fdbae3fded168d5a4249792372f666234))
* **adoption:** steer protocol read workflows ([106cdda](https://github.com/special-place-administrator/symforge/commit/106cddaec9ecb7f0f1a82dae22cb54d95e942ff8))
* **adoption:** tighten hook routing for source workflows ([5b9426a](https://github.com/special-place-administrator/symforge/commit/5b9426a9aa3f6f02f3f27575602bb35f1b74a8f9))
* aggregate token savings across tool handlers ([25343e9](https://github.com/special-place-administrator/symforge/commit/25343e9b20774d3489ca9610955ca81aeda38e5b))
* analyze_file_impact shows clear status taxonomy (U4) ([263834f](https://github.com/special-place-administrator/symforge/commit/263834f3d8dd2a4c22911eed69a506edb7c13bd6))
* batch_edit dry_run mode (U5) ([4166196](https://github.com/special-place-administrator/symforge/commit/41661963168aced0d37c7931628c3dcf49f6b550))
* batch_rename supplemental qualified path scan with confidence classification ([e75f2d4](https://github.com/special-place-administrator/symforge/commit/e75f2d40d657ea668660d3160c884feaf396ec96))
* bump index snapshot version to 3 for doc_byte_range ([c002fcc](https://github.com/special-place-administrator/symforge/commit/c002fccfe02f569a28b62615433d99ea7a3be1e0))
* clean npm cache after install to reclaim disk space ([b1c4a35](https://github.com/special-place-administrator/symforge/commit/b1c4a353fc1d1985c31e363d1be22f7f4e17a440))
* concept+remainder merging in explore ([c6e93bd](https://github.com/special-place-administrator/symforge/commit/c6e93bd04ea9035a9c3a9ab279539f9ebb8ba1d1))
* config file parsing — all extractors, pipeline integration, edit gating, test fixes ([bde2f3d](https://github.com/special-place-administrator/symforge/commit/bde2f3d1a80bdd72cc22ed10d227963e4a651d44))
* **config:** add structured syntax diagnostics ([7360beb](https://github.com/special-place-administrator/symforge/commit/7360bebb290e50bc65fee0a63ef5118cdc72117c))
* daemon fallback, callee dedup, token budget, search defaults ([d13e76b](https://github.com/special-place-administrator/symforge/commit/d13e76b77308b16d42bf721400f10ef6215cc896))
* edit tools use doc_byte_range for splice boundaries ([cee3ff2](https://github.com/special-place-administrator/symforge/commit/cee3ff29d43078448555b4479a4de399d9731b9e))
* **edit:** add dry_run to replace_symbol_body, insert_symbol, delete_symbol, edit_within_symbol ([1f401d8](https://github.com/special-place-administrator/symforge/commit/1f401d82444bf19bd86746b2346b8aa3082f9880))
* **edit:** add Tier 2 batch tools — batch_edit, batch_rename, batch_insert ([7090b15](https://github.com/special-place-administrator/symforge/commit/7090b15f23de9dd2813c87d6d2ce3619aaef8c77))
* **edit:** Tier 2 batch tools — batch_edit, batch_rename, batch_insert ([859271d](https://github.com/special-place-administrator/symforge/commit/859271d7a1b7f2954335002e2aa3c8588cae2109))
* **edit:** track item byte ranges on symbols ([da3294c](https://github.com/special-place-administrator/symforge/commit/da3294c623589382bfa8dd2311944d868a6804e7))
* expand CONCEPT_MAP and add word-boundary matching ([f94e07d](https://github.com/special-place-administrator/symforge/commit/f94e07dc29cb51f2055cf05ebea91946f02f1f1a))
* explore filters noise by default (U1) ([f14b702](https://github.com/special-place-administrator/symforge/commit/f14b702b0a68ba6c88b5c96327cbdf5d701e5d72))
* **find_dependents:** show symbol names in mermaid and dot edge labels ([e358b77](https://github.com/special-place-administrator/symforge/commit/e358b77697162cd859fd41f72a301ee23f64d5f3))
* gate edit tools by config file EditCapability ([7613d11](https://github.com/special-place-administrator/symforge/commit/7613d112cfa8c688c85d498808c4f7c740fcb9ce))
* get_file_content falls back to raw disk read for non-source files ([9bf8ba5](https://github.com/special-place-administrator/symforge/commit/9bf8ba5cdc7811b1d24c340d78880a01510b8130))
* get_file_content mode enum for clearer API (U10) ([244be75](https://github.com/special-place-administrator/symforge/commit/244be753b08275b80f3bd1d8a11214a74f642f03))
* **get_repo_map:** paginate detail=full output with max_files parameter ([8c15c5d](https://github.com/special-place-administrator/symforge/commit/8c15c5d2bc55768123a754fa6a74c23bb9b6c131))
* git churn in ranking, expanded guidance blocks, improved tool descriptions ([4cf1e6e](https://github.com/special-place-administrator/symforge/commit/4cf1e6e782d79e52554c23aae98590fb5a60feb5))
* health shows partial parse file paths (U8) ([8560114](https://github.com/special-place-administrator/symforge/commit/856011485f82605d41e93651748bf64db1486c91))
* **health:** list failed files with error messages in health report ([7306089](https://github.com/special-place-administrator/symforge/commit/73060890c96058223dd04dffd91b06087fb4dd1a))
* implement admission gate with tiered file classification ([9e69e23](https://github.com/special-place-administrator/symforge/commit/9e69e238f50b3364545fa3844cbb5b03ae7ad925))
* implement CSS symbol extractor with tests ([39719f4](https://github.com/special-place-administrator/symforge/commit/39719f4962657e824f9a82b9c8040be48af71cfd))
* implement HTML/Angular symbol extractor with tests ([4627b52](https://github.com/special-place-administrator/symforge/commit/4627b520009e2c8395f39593644499cae428fe75))
* implement SCSS symbol extractor with tests ([2112b1a](https://github.com/special-place-administrator/symforge/commit/2112b1a7776db40d5d63ccd12da753fba085aaa9))
* improve explore relevance ranking (Q1) ([5b829ca](https://github.com/special-place-administrator/symforge/commit/5b829cab9138885daf8479b3fb3a594bc35838d5))
* include doc comments in symbol body extraction ([679682b](https://github.com/special-place-administrator/symforge/commit/679682b03d9a9548b8b67639c3257b6a5a63ff9c))
* **index:** Sprint 0 — index freshness guarantee via mtime tracking ([29d60d6](https://github.com/special-place-administrator/symforge/commit/29d60d6fe0e83f3c79856c38796bc02c62f62bea))
* **init:** add alwaysAllow to Claude MCP entry and expand CLAUDE.md guidance ([4ee5f53](https://github.com/special-place-administrator/symforge/commit/4ee5f535546e829850c6136b5785bd56b23c7732))
* **init:** harden client guidance rollout ([f30667c](https://github.com/special-place-administrator/symforge/commit/f30667ce885ba5040a4a40751b7401080fad8977))
* integrate config extractors into parsing pipeline ([961c25b](https://github.com/special-place-administrator/symforge/commit/961c25b36a75ab4ea033ce5921f4368ab41a4495))
* **json:** add JSONC comment stripping for tsconfig.json support ([c3c208f](https://github.com/special-place-administrator/symforge/commit/c3c208fb496449efe2cd14a7ee82562bd4088df9))
* lenient vec deserializer, semantic search ranking, Kilo Code init, SymForge rename plan ([c048274](https://github.com/special-place-administrator/symforge/commit/c04827422bb02a04ff4222864b0c09ff014ca70d))
* per-tool call counters in health output (U9) ([d41bfb5](https://github.com/special-place-administrator/symforge/commit/d41bfb5730d07b2b0271ee1947eec0306f07375c))
* PreToolUse hook intercepts Grep/Read/Glob/Edit with Tokenizor suggestions ([1c78000](https://github.com/special-place-administrator/symforge/commit/1c780008662d9fb1c5cf1d7e573df64e04a23807))
* PreToolUse hook now intercepts config files for Tokenizor suggestions ([ee88bff](https://github.com/special-place-administrator/symforge/commit/ee88bff2f5341e026cbed90b6da7a1dcaaef33eb))
* quality improvements from 3-project eval (Q1-Q6) ([8d23ff4](https://github.com/special-place-administrator/symforge/commit/8d23ff4749250d4ad85f12324280543fd8c5e403))
* quality improvements from eval feedback (Q3-Q6) ([79ac714](https://github.com/special-place-administrator/symforge/commit/79ac714eb67a2d504be76a4beec7479cfe154385))
* rename Tokenizor → SymForge ([6366cd0](https://github.com/special-place-administrator/symforge/commit/6366cd0c7f51bc496cceb6ae255e22d95f109183))
* replace git CLI with git2 library in tools and diff_symbols ([db3824a](https://github.com/special-place-administrator/symforge/commit/db3824aff0197cdc7408a82d71add37e6ae2b2e2))
* replace git log CLI with git2 library in temporal analysis ([f6877eb](https://github.com/special-place-administrator/symforge/commit/f6877ebc81e3d8f6a6ebc6001cdbecc29979292c))
* richer verbosity=signature includes visibility and return type (U6) ([eef2926](https://github.com/special-place-administrator/symforge/commit/eef2926f057e3020200bb13cc1dd47b9ee9bf76e))
* RTK adoption milestone — symbol disambiguation tests, hook diagnostics, docs links ([9bc3ead](https://github.com/special-place-administrator/symforge/commit/9bc3ead5c49813998a9987b3e2066398313d48db))
* search_symbols browse mode without query (U2) ([3326342](https://github.com/special-place-administrator/symforge/commit/33263428425acd01a9cfba460d96d0b5534257b5))
* **search_text:** annotate which term matched in OR-term searches ([e53a7f7](https://github.com/special-place-administrator/symforge/commit/e53a7f748e55afe0983d4710b6375edc01058397))
* show Tier 2 tags and Tier 3 footer in repo_map ([05d23eb](https://github.com/special-place-administrator/symforge/commit/05d23eb7b23c06c607e6adacac00ae7edbb2c7dc))
* Sprint 14 — trust fixes + tiered admission control ([b7a9296](https://github.com/special-place-administrator/symforge/commit/b7a92963b3f9c55be08e73e04eba6bd70901b1bf))
* update README for 24 tools, add CLAUDE.md, rename prompts to tokenizor-* prefix ([949738f](https://github.com/special-place-administrator/symforge/commit/949738f88ce47fa3ede4a5e919127787678017bb))
* wire admission gate into discovery walk ([51c73f7](https://github.com/special-place-administrator/symforge/commit/51c73f7b125d13a88179d9e1e2cf535e28839888))
* wire HTML, CSS, SCSS extractors into parsing pipeline ([e740f94](https://github.com/special-place-administrator/symforge/commit/e740f947007acee5db43a321bb38152e1fed63cf))


### Bug Fixes

* 26 bug fixes across parsers, protocol, indexing, sidecar, and npm ([b2abebc](https://github.com/special-place-administrator/symforge/commit/b2abebc4710580cdb62fe984809c4ddc949cd8a6))
* add 'burst' to file watching concept symbol_queries ([930d8e8](https://github.com/special-place-administrator/symforge/commit/930d8e87b053af0c1035e6518ca8b0c3ea46b1ee))
* add exe/dll/so/dylib/class to denylist (C2-lite) ([14d0459](https://github.com/special-place-administrator/symforge/commit/14d04593df00b7cbc92643aeb5c7109026a045cb))
* add missing gitignore/noise_class field initializers across codebase ([c8088f9](https://github.com/special-place-administrator/symforge/commit/c8088f9f0953b004e015c2a707db89dae3597ced))
* add missing sibling_limit/overflow fields to initializers ([b25f4a5](https://github.com/special-place-administrator/symforge/commit/b25f4a5a34a007ad9a56757cd1a62ce7c9f92157))
* add total hit limit to find_references ([d592e13](https://github.com/special-place-administrator/symforge/commit/d592e136df4d78c04be59c6a3e73edc7d1fad2c2))
* address all actionable feedback from 3 external code reviews ([61d2757](https://github.com/special-place-administrator/symforge/commit/61d2757a8b9bbc425b73eec512dc75c470be630d))
* address review findings — diff_symbols filter, search prefix matching, impact messaging, chunk line numbers ([44298df](https://github.com/special-place-administrator/symforge/commit/44298df66c3b742f2736fcd33151884b2118cbe6))
* address review findings — OR search terms, range validation, depth 3, schema docs, insert spacing, token counter ([249f987](https://github.com/special-place-administrator/symforge/commit/249f9876196de831e1dc5d6a34b8dd5cf284e1f1))
* around_line error, diff note, language-scoped warnings, dry-run ([a6a5f70](https://github.com/special-place-administrator/symforge/commit/a6a5f701fc1bfb56f7e13cb01f732383e101950a))
* around_symbol returns full indexed symbol span (B2) ([3b06c2a](https://github.com/special-place-administrator/symforge/commit/3b06c2a735ad3edd0ac691851a944d66797b06f9))
* auto-correct double-escaped regex patterns in search_text ([e98cd4d](https://github.com/special-place-administrator/symforge/commit/e98cd4d6671e129b1bc775ec4dde28129931b22e))
* auto-indent replace_symbol_body + update edit tool descriptions ([a113c7b](https://github.com/special-place-administrator/symforge/commit/a113c7b5444656cb3935a22defdede73ba538c2f))
* auto-indent replace_symbol_body + update edit tool descriptions ([d1d22ca](https://github.com/special-place-administrator/symforge/commit/d1d22ca3b26b59fcd107ad62e294abe10a0de678))
* auto-indent replace_symbol_body + update edit tool descriptions ([39f8fec](https://github.com/special-place-administrator/symforge/commit/39f8fec7b95ccdb2d383c9089f6d267f4a86b69c))
* auto-indent replace_symbol_body + update edit tool descriptions ([a021b5b](https://github.com/special-place-administrator/symforge/commit/a021b5bd22ea75be5cb3e0599c8abbb2c0c1011a))
* batch_edit dry_run byte count + auto-detect regex in search_text ([29474c2](https://github.com/special-place-administrator/symforge/commit/29474c21bd33b1da6858bdbb4179eaa3ac9611a1))
* batch_edit shows ROLLED BACK message on failure (B4) ([3ab8358](https://github.com/special-place-administrator/symforge/commit/3ab83587c7bbee77bd1f1cfe1b1980066630da8f))
* batch_insert no extra blank line before function (B1) ([3409548](https://github.com/special-place-administrator/symforge/commit/34095482b8275811cb5373003e367c0b07dcfec0))
* batch_rename atomic rollback on failure, batch_edit/batch_insert best-effort with correct index state ([6b332f3](https://github.com/special-place-administrator/symforge/commit/6b332f3f18484a8e14ed35731961eac98311b18f))
* batch_rename catches path-qualified usages ([c2243ff](https://github.com/special-place-administrator/symforge/commit/c2243ff058e0ca29848272641eed8c27eec47131))
* batch_rename catches path-qualified usages via literal scan ([2824745](https://github.com/special-place-administrator/symforge/commit/282474529928f46e3dec525470244baa3fb68873))
* batch_rename review fixes — atomic rollback, dead code, dedup ([0a7844e](https://github.com/special-place-administrator/symforge/commit/0a7844ee1b8ce77b462f1f6a3b51b53c1ce17a22))
* **bundle:** resolve impl suggestions and dependency-aware limits ([d5bfa6a](https://github.com/special-place-administrator/symforge/commit/d5bfa6aa85e3dfd719340301e1eff01d4bb1c069))
* code review feedback — tests and safety fixes ([c2454ea](https://github.com/special-place-administrator/symforge/commit/c2454ea2bbd1ec0c2f2acb357bd9f158fd101584))
* complete audit remediation — language tests, deferred fixes, dedup ([c04e849](https://github.com/special-place-administrator/symforge/commit/c04e84968e406e1662b5f36fa7df2ae923ab15fe))
* complete parking_lot::RwLock migration across live_index and protocol ([c48d865](https://github.com/special-place-administrator/symforge/commit/c48d865c2e9c717b89da9d446a51f326af5d3052))
* comprehensive codebase audit — 18 bug fixes across parsers, core engine, and protocol ([3b0cd44](https://github.com/special-place-administrator/symforge/commit/3b0cd442841748ebb30e05d2b23d13b57246ee5e))
* compute real line numbers for TOML symbols ([d939caa](https://github.com/special-place-administrator/symforge/commit/d939caa7d3a587efa7482dc9f4790e7b102e15ac))
* compute real line numbers for TOML symbols ([7dd4697](https://github.com/special-place-administrator/symforge/commit/7dd46973f52228193cc98ccb8d01361b75eef808))
* CSS @layer/[@container](https://github.com/container) extraction — use generic at_rule node kind ([97fc47f](https://github.com/special-place-administrator/symforge/commit/97fc47fd201c89888231e21e4f665d919c73b3fc))
* daemon lifecycle hardening — stale lock detection, fast-fail proxy, cleanup (DL1-DL4) ([8350d7e](https://github.com/special-place-administrator/symforge/commit/8350d7e3b5de2bb16d85d69a817ca459fb43e829))
* daemon proxy deadlock under concurrent tool calls + request governor ([541dd68](https://github.com/special-place-administrator/symforge/commit/541dd688e9956d75818733740764320513e9c8ab))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([3b1cc4e](https://github.com/special-place-administrator/symforge/commit/3b1cc4ed6ae8591ca8afc720158f2241cbec80de))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([b793579](https://github.com/special-place-administrator/symforge/commit/b793579b2f1bdfdd7fb266bfa9bf7dd76590ea9e))
* **dependents:** filter false positives from non-pub symbol name collisions ([0bf3c77](https://github.com/special-place-administrator/symforge/commit/0bf3c77b1d41ea7ba383d18b83029ba15f0855a6))
* **diff_symbols:** show omission note in compact mode when files have no symbol changes ([c6eade8](https://github.com/special-place-administrator/symforge/commit/c6eade8fc0770c042c26b9a0b258a7b619537e21))
* **diff_symbols:** skip type keywords in C# const declarations ([00049d2](https://github.com/special-place-administrator/symforge/commit/00049d264058dafbbbb2ab72816cdfc8dd608164))
* disable git2 SSH/HTTPS features to remove OpenSSL dependency ([0167936](https://github.com/special-place-administrator/symforge/commit/0167936069c2481cdbb07fb4e4bb5acdbd483131))
* edit_within splice range + multi-line block comment detection ([4dd4ea8](https://github.com/special-place-administrator/symforge/commit/4dd4ea84de658b5170d09d42c0200d3bd171ae2b))
* explore multi-term scoring with enclosing symbol injection ([5f36dab](https://github.com/special-place-administrator/symforge/commit/5f36dab73ac26247945d34f769e182e48e6bfbe6))
* explore text search max_per_file too low for symbol injection ([4a5b67e](https://github.com/special-place-administrator/symforge/commit/4a5b67edf156e27cfe21e1f8e4c0b86fea03ff9e))
* filter explore noise from CONCEPT_MAP self-matching and generic terms ([8addfaa](https://github.com/special-place-administrator/symforge/commit/8addfaaa4646144458ea1c71b1a9fa1de7728826))
* find_dependents follows pub use re-export chains for Rust modules ([a48aee2](https://github.com/special-place-administrator/symforge/commit/a48aee2b7180dc0b669f72b8b94f4e36b0284e0a))
* find_dependents resolves workspace crate paths ([418652a](https://github.com/special-place-administrator/symforge/commit/418652a42bd3e1443602cb87cd1eed2d7e4c0574))
* find_dependents resolves workspace crate paths (B4) ([f02819d](https://github.com/special-place-administrator/symforge/commit/f02819db942530bcbb974fe946d05224bb953ab6)), closes [#89](https://github.com/special-place-administrator/symforge/issues/89)
* find_references file count + get_repo_map full path filter ([89cc588](https://github.com/special-place-administrator/symforge/commit/89cc5880c4c7ffb8ed2e09dd9c8bd9d7e573fa52))
* **find_references:** explain why classes/structs have no implementations ([7566e2a](https://github.com/special-place-administrator/symforge/commit/7566e2a3bc9a928d55b6266307738029d63487e9))
* follow_refs shows same-file callers and empty-result signal ([b3acea1](https://github.com/special-place-administrator/symforge/commit/b3acea139692468421ed85ef655ff958baad008d))
* Gemini CLI init writes correct timeout (120000ms) and trust setting ([b8616eb](https://github.com/special-place-administrator/symforge/commit/b8616eb1f5621872e050940b6756c9a04e707668))
* **get_file_content:** explain why zero-symbol files have no matches ([d4420f8](https://github.com/special-place-administrator/symforge/commit/d4420f8df45c949f6bc3be1731eca17fe620c7db))
* get_file_context sections filter masked by 800-byte hook budget ([5ff4c9e](https://github.com/special-place-administrator/symforge/commit/5ff4c9e3031e099d7321c5b906ba3109f031e3f7))
* **get_symbol_context:** auto-resolve path and show empty-references message ([8b4caf5](https://github.com/special-place-administrator/symforge/commit/8b4caf5b49e6ac402744b23eddd1926762421b62))
* Go method names, SCSS $variable extraction, language filter completeness ([4156d41](https://github.com/special-place-administrator/symforge/commit/4156d41d24af8739494482ea4ba868a69fec747d))
* handle SIGTERM for daemon graceful shutdown (C5) ([3faef1f](https://github.com/special-place-administrator/symforge/commit/3faef1f772ef679f137229cc7fc9d47891c132f6))
* index purge on file delete, richer default symbol context, path-scoped implementations ([ec634b5](https://github.com/special-place-administrator/symforge/commit/ec634b5aae1d7dcec5137b70757282efb9c578d2))
* index purge on file delete, richer default symbol context, path-scoped implementations ([1cb084b](https://github.com/special-place-administrator/symforge/commit/1cb084b0444f89d825cae0d2d60e631e0cbd54c5))
* index safety hardening and tool output correctness ([c952759](https://github.com/special-place-administrator/symforge/commit/c952759567b0a1e7a2de4cd56ccb0576eaa792a8))
* index safety hardening and tool output correctness ([59720c5](https://github.com/special-place-administrator/symforge/commit/59720c5fc9e48eccb2fc41d291a804b0a69cbd55))
* **init:** canonicalize SymForge Codex guidance and allowlists ([7680757](https://github.com/special-place-administrator/symforge/commit/76807570e24995dfc05ad6df022cbd1fbab25bfa))
* insert_before uses blank line separator when no doc comments ([2253f7d](https://github.com/special-place-administrator/symforge/commit/2253f7da0c14cb4c6d858442af375e5d8552bebe))
* **kilo:** trigger release for strict-provider compatibility ([a852955](https://github.com/special-place-administrator/symforge/commit/a852955707cc9da1133f9a872d8d7b3b955988e7))
* non-ASCII panic in doc scanning and deterministic circuit-breaker (CR1, CR2) ([1e52aaf](https://github.com/special-place-administrator/symforge/commit/1e52aaf7468d425fac21371398a631a93d6c5bfe))
* non-blocking cold-start indexing for faster MCP discovery ([acb8743](https://github.com/special-place-administrator/symforge/commit/acb874307fc01eaf162cf76de4cba2dc1e942ba8))
* normalize exact get_file_content paths and backfill mtime_secs in integration fixtures ([fb398b1](https://github.com/special-place-administrator/symforge/commit/fb398b1dcbfdc59055fea328e995bdb1d9ba114c))
* **npm:** keep global auto-init out of workspaces ([1626dfa](https://github.com/special-place-administrator/symforge/commit/1626dfa89a6da5a9e14b0107df3eec7a0b325c61))
* **npm:** persist wrapper install metadata ([eeac029](https://github.com/special-place-administrator/symforge/commit/eeac0298ef08253fdf70042ba5d2f78f142faea2))
* pin CI/release workflows to Rust 1.94.0 matching rust-toolchain.toml ([41d23ab](https://github.com/special-place-administrator/symforge/commit/41d23ab3fd83205773f9289cf95cd142fd2cb1b9))
* pin Rust toolchain to 1.94.0 via rust-toolchain.toml ([fed0e20](https://github.com/special-place-administrator/symforge/commit/fed0e20560de0356cb86bf5dc9319af2867e770c))
* prevent async runtime starvation under concurrent subagent load ([74f1d54](https://github.com/special-place-administrator/symforge/commit/74f1d54f0f26dba97a619fb5b69e645c1d702034))
* prevent async runtime starvation under concurrent subagent load ([2ed134a](https://github.com/special-place-administrator/symforge/commit/2ed134aa34a82f258eb37be4121c341272cc85d6))
* prevent non-ASCII panic in find_qualified_usages (batch_rename crash) ([8555d0c](https://github.com/special-place-administrator/symforge/commit/8555d0ce0b3ed39acf7d170c0878d4e94267a2ef))
* recurse into mixin/function bodies, guard empty at-rule names ([8a8e717](https://github.com/special-place-administrator/symforge/commit/8a8e7178c079078581ac8c6e339128814e16478a))
* reindex from disk after writes, not from in-memory buffer ([d605498](https://github.com/special-place-administrator/symforge/commit/d6054988db18ad6cf82e3a82cca2c054a1c5f52b))
* **release:** add noncommercial licensing and kill-all npm updates ([17354c6](https://github.com/special-place-administrator/symforge/commit/17354c6adba9e75a09bc3929b753881552ec929a))
* rem_euclid for timestamps, generic pub(...) visibility in diff_symbols ([f597c78](https://github.com/special-place-administrator/symforge/commit/f597c78fabcec1ce18b4e20665516cd8fd772537))
* remediate reviewer feedback from external codebase testing ([80303a9](https://github.com/special-place-administrator/symforge/commit/80303a9a9558d05ad281740552fdc95b349270f3))
* resolve 16 bugs across mtime propagation, line indexing, correctness, and concurrency ([8cfda64](https://github.com/special-place-administrator/symforge/commit/8cfda649bf2bd0a017fa72643bce088f817cd1bc))
* resolve 4 bugs from code review ([31b9a0c](https://github.com/special-place-administrator/symforge/commit/31b9a0c78b5576262c74946adaf00aad8262ebcb))
* resolve 5 tool bugs from hands-on review ([6d11014](https://github.com/special-place-administrator/symforge/commit/6d1101448f993946699028e26ecf8852f81073be))
* resolve all actionable issues from external review ([3e11288](https://github.com/special-place-administrator/symforge/commit/3e112880a6802182bf59c5159373fc3ab636a240))
* restore missing [[package]] header in Cargo.lock after rebase conflict resolution ([67d9327](https://github.com/special-place-administrator/symforge/commit/67d932778c02a9899ffec904560868927604d968))
* revert worker_threads override — spawn_blocking handles concurrency ([a5d5d4e](https://github.com/special-place-administrator/symforge/commit/a5d5d4e77dfd2d438a42ef2161fd1c7111584abd))
* review feedback — Q3 robust name extraction, Q6 UTF-8 safe truncation ([41e17d2](https://github.com/special-place-administrator/symforge/commit/41e17d23a4b4ce9e89d9fdfbceb5ba089a9880a0))
* rewrite open_project_session with double-checked locking (C6) ([b04b0d0](https://github.com/special-place-administrator/symforge/commit/b04b0d099fb035ca1bac9c76c49d542cae1d8102))
* search_symbols file count + find_references missing cross-file type refs ([8d40874](https://github.com/special-place-administrator/symforge/commit/8d4087446f8f7c980f5e5abe6b16366e6cc5f697))
* security patches, parser improvements, parallelism fixes, and review follow-ups ([2b1d5cb](https://github.com/special-place-administrator/symforge/commit/2b1d5cbafed5100ea833d0f4b41da41b1d87cb27))
* show_line_numbers works with around_symbol and around_match (B3) ([4befe8a](https://github.com/special-place-administrator/symforge/commit/4befe8a8f8e734e722088632c23ac81489bf42ce))
* surface tool panics as immediate error responses instead of stalls ([31ae935](https://github.com/special-place-administrator/symforge/commit/31ae935642876711383897ba3b779ea4c2dc7b52))
* Swift enum/extension/protocol detection and Angular template robustness ([af34df2](https://github.com/special-place-administrator/symforge/commit/af34df266b7cb34ba4dbd24853b585554bba7308))
* symbol kind filter accepts semantic aliases (variable, function, etc.) ([2e80fb5](https://github.com/special-place-administrator/symforge/commit/2e80fb5cd96b441e256e4a7c7afb5fad20fbbfbe))
* **test:** update assertion for changed zero-symbol message ([90a5722](https://github.com/special-place-administrator/symforge/commit/90a57229f697bfc541adc4d58ea35f1f2dc53295))
* thread LineEnding through all edit helpers for CRLF preservation (C1) ([dda40b4](https://github.com/special-place-administrator/symforge/commit/dda40b4c0c8a99259a93af4c9ff65bb696e9e337))
* type-aware reference filtering reduces false positive warnings in replace_symbol_body ([b24dd0c](https://github.com/special-place-administrator/symforge/commit/b24dd0cdcb5dbed44edb077959c48972563b8845))
* update installer test assertion for execFileSyncFn version check ([f6ed05d](https://github.com/special-place-administrator/symforge/commit/f6ed05dd601105a4a3249cf2844b4da91210c560))
* update rollback tests for tempfile-based atomic writes ([548c2bb](https://github.com/special-place-administrator/symforge/commit/548c2bbede27be2a233fe35a2d7f6b8aa0aee45b))
* use unique temp files in atomic_write_file (C3) ([dddcb16](https://github.com/special-place-administrator/symforge/commit/dddcb16eb85dc294354691e9fc470adbca5ce9bd))
* UX improvements from third review ([648218f](https://github.com/special-place-administrator/symforge/commit/648218fe4b70378133ec115dd93cd5a089b44bbc))
* validate splice overlap in batch_rename (C4) ([0c80b74](https://github.com/special-place-administrator/symforge/commit/0c80b74903261a09837a7038998bf3d61898d274))
* watcher recv_timeout blocks tokio worker — use try_recv + async sleep ([a4b7d34](https://github.com/special-place-administrator/symforge/commit/a4b7d34db100eac9528325f6f3bdbdd58827d54d))
* wave 1 audit remediation — 12 safety and correctness fixes ([b02bd12](https://github.com/special-place-administrator/symforge/commit/b02bd1203ba78ee404e204dda80ae54328ba3642))
* wave 2 audit remediation — 10 reliability and consistency fixes ([a293819](https://github.com/special-place-administrator/symforge/commit/a2938196992b09c8ddb2a0fc40a732ff499627e0))
* wave 3 audit remediation — polish, docs, and remaining fixes ([c7c2ba8](https://github.com/special-place-administrator/symforge/commit/c7c2ba8f6129565f16a3346026ae35606f8f60f5))
* wrap env var manipulation in unsafe blocks for Rust 2024 edition compliance ([c363499](https://github.com/special-place-administrator/symforge/commit/c3634999e2c91fcbc57a2fb92decc5f6a217a77f))


### Performance Improvements

* incremental reverse index updates on file mutation ([e85c445](https://github.com/special-place-administrator/symforge/commit/e85c445b7d7e017a961e4de5da851a6ca0e0cd01))

## [2.0.11](https://github.com/special-place-administrator/symforge/compare/v2.0.10...v2.0.11) (2026-03-20)


### Bug Fixes

* watcher recv_timeout blocks tokio worker — use try_recv + async sleep ([a4b7d34](https://github.com/special-place-administrator/symforge/commit/a4b7d34db100eac9528325f6f3bdbdd58827d54d))

## [2.0.10](https://github.com/special-place-administrator/symforge/compare/v2.0.9...v2.0.10) (2026-03-20)


### Bug Fixes

* 26 bug fixes across parsers, protocol, indexing, sidecar, and npm ([b2abebc](https://github.com/special-place-administrator/symforge/commit/b2abebc4710580cdb62fe984809c4ddc949cd8a6))

## [2.0.9](https://github.com/special-place-administrator/symforge/compare/v2.0.8...v2.0.9) (2026-03-20)


### Bug Fixes

* CSS @layer/[@container](https://github.com/container) extraction — use generic at_rule node kind ([97fc47f](https://github.com/special-place-administrator/symforge/commit/97fc47fd201c89888231e21e4f665d919c73b3fc))

## [2.0.8](https://github.com/special-place-administrator/symforge/compare/v2.0.7...v2.0.8) (2026-03-20)


### Bug Fixes

* comprehensive codebase audit — 18 bug fixes across parsers, core engine, and protocol ([3b0cd44](https://github.com/special-place-administrator/symforge/commit/3b0cd442841748ebb30e05d2b23d13b57246ee5e))

## [2.0.7](https://github.com/special-place-administrator/symforge/compare/v2.0.6...v2.0.7) (2026-03-20)


### Bug Fixes

* symbol kind filter accepts semantic aliases (variable, function, etc.) ([2e80fb5](https://github.com/special-place-administrator/symforge/commit/2e80fb5cd96b441e256e4a7c7afb5fad20fbbfbe))

## [2.0.6](https://github.com/special-place-administrator/symforge/compare/v2.0.5...v2.0.6) (2026-03-20)


### Bug Fixes

* Swift enum/extension/protocol detection and Angular template robustness ([af34df2](https://github.com/special-place-administrator/symforge/commit/af34df266b7cb34ba4dbd24853b585554bba7308))

## [2.0.5](https://github.com/special-place-administrator/symforge/compare/v2.0.4...v2.0.5) (2026-03-20)


### Bug Fixes

* complete audit remediation — language tests, deferred fixes, dedup ([c04e849](https://github.com/special-place-administrator/symforge/commit/c04e84968e406e1662b5f36fa7df2ae923ab15fe))
* Go method names, SCSS $variable extraction, language filter completeness ([4156d41](https://github.com/special-place-administrator/symforge/commit/4156d41d24af8739494482ea4ba868a69fec747d))

## [2.0.4](https://github.com/special-place-administrator/symforge/compare/v2.0.3...v2.0.4) (2026-03-20)


### Bug Fixes

* wave 3 audit remediation — polish, docs, and remaining fixes ([c7c2ba8](https://github.com/special-place-administrator/symforge/commit/c7c2ba8f6129565f16a3346026ae35606f8f60f5))

## [2.0.3](https://github.com/special-place-administrator/symforge/compare/v2.0.2...v2.0.3) (2026-03-20)


### Bug Fixes

* wave 2 audit remediation — 10 reliability and consistency fixes ([a293819](https://github.com/special-place-administrator/symforge/commit/a2938196992b09c8ddb2a0fc40a732ff499627e0))

## [2.0.2](https://github.com/special-place-administrator/symforge/compare/v2.0.1...v2.0.2) (2026-03-20)


### Bug Fixes

* resolve 4 bugs from code review ([31b9a0c](https://github.com/special-place-administrator/symforge/commit/31b9a0c78b5576262c74946adaf00aad8262ebcb))
* wave 1 audit remediation — 12 safety and correctness fixes ([b02bd12](https://github.com/special-place-administrator/symforge/commit/b02bd1203ba78ee404e204dda80ae54328ba3642))

## [2.0.1](https://github.com/special-place-administrator/symforge/compare/v2.0.0...v2.0.1) (2026-03-20)


### Bug Fixes

* resolve 5 tool bugs from hands-on review ([6d11014](https://github.com/special-place-administrator/symforge/commit/6d1101448f993946699028e26ecf8852f81073be))

## [2.0.0](https://github.com/special-place-administrator/symforge/compare/v1.9.0...v2.0.0) (2026-03-20)


### ⚠ BREAKING CHANGES

* Line numbers in search_symbols, get_symbol_context, trace_symbol, inspect_match, and sidecar endpoints shift from 0-indexed to 1-indexed. Clients parsing these outputs numerically must account for the +1 change.

### Bug Fixes

* resolve 16 bugs across mtime propagation, line indexing, correctness, and concurrency ([8cfda64](https://github.com/special-place-administrator/symforge/commit/8cfda649bf2bd0a017fa72643bce088f817cd1bc))

## [1.9.0](https://github.com/special-place-administrator/symforge/compare/v1.8.1...v1.9.0) (2026-03-20)


### Features

* aggregate token savings across tool handlers ([25343e9](https://github.com/special-place-administrator/symforge/commit/25343e9b20774d3489ca9610955ca81aeda38e5b))
* **edit:** add dry_run to replace_symbol_body, insert_symbol, delete_symbol, edit_within_symbol ([1f401d8](https://github.com/special-place-administrator/symforge/commit/1f401d82444bf19bd86746b2346b8aa3082f9880))
* **find_dependents:** show symbol names in mermaid and dot edge labels ([e358b77](https://github.com/special-place-administrator/symforge/commit/e358b77697162cd859fd41f72a301ee23f64d5f3))
* **get_repo_map:** paginate detail=full output with max_files parameter ([8c15c5d](https://github.com/special-place-administrator/symforge/commit/8c15c5d2bc55768123a754fa6a74c23bb9b6c131))
* **health:** list failed files with error messages in health report ([7306089](https://github.com/special-place-administrator/symforge/commit/73060890c96058223dd04dffd91b06087fb4dd1a))
* **json:** add JSONC comment stripping for tsconfig.json support ([c3c208f](https://github.com/special-place-administrator/symforge/commit/c3c208fb496449efe2cd14a7ee82562bd4088df9))
* **search_text:** annotate which term matched in OR-term searches ([e53a7f7](https://github.com/special-place-administrator/symforge/commit/e53a7f748e55afe0983d4710b6375edc01058397))


### Bug Fixes

* **dependents:** filter false positives from non-pub symbol name collisions ([0bf3c77](https://github.com/special-place-administrator/symforge/commit/0bf3c77b1d41ea7ba383d18b83029ba15f0855a6))
* **diff_symbols:** show omission note in compact mode when files have no symbol changes ([c6eade8](https://github.com/special-place-administrator/symforge/commit/c6eade8fc0770c042c26b9a0b258a7b619537e21))
* **diff_symbols:** skip type keywords in C# const declarations ([00049d2](https://github.com/special-place-administrator/symforge/commit/00049d264058dafbbbb2ab72816cdfc8dd608164))
* **find_references:** explain why classes/structs have no implementations ([7566e2a](https://github.com/special-place-administrator/symforge/commit/7566e2a3bc9a928d55b6266307738029d63487e9))
* **get_file_content:** explain why zero-symbol files have no matches ([d4420f8](https://github.com/special-place-administrator/symforge/commit/d4420f8df45c949f6bc3be1731eca17fe620c7db))
* **get_symbol_context:** auto-resolve path and show empty-references message ([8b4caf5](https://github.com/special-place-administrator/symforge/commit/8b4caf5b49e6ac402744b23eddd1926762421b62))
* remediate reviewer feedback from external codebase testing ([80303a9](https://github.com/special-place-administrator/symforge/commit/80303a9a9558d05ad281740552fdc95b349270f3))
* **test:** update assertion for changed zero-symbol message ([90a5722](https://github.com/special-place-administrator/symforge/commit/90a57229f697bfc541adc4d58ea35f1f2dc53295))

## [1.8.1](https://github.com/special-place-administrator/symforge/compare/v1.8.0...v1.8.1) (2026-03-20)


### Bug Fixes

* index safety hardening and tool output correctness ([c952759](https://github.com/special-place-administrator/symforge/commit/c952759567b0a1e7a2de4cd56ccb0576eaa792a8))
* index safety hardening and tool output correctness ([59720c5](https://github.com/special-place-administrator/symforge/commit/59720c5fc9e48eccb2fc41d291a804b0a69cbd55))

## [1.8.0](https://github.com/special-place-administrator/symforge/compare/v1.7.0...v1.8.0) (2026-03-20)


### Features

* **01-01:** implement kind-tier disambiguation in resolve_symbol_selector ([2e11ac4](https://github.com/special-place-administrator/symforge/commit/2e11ac4985e690e034da2982fcf3b900d734d30b))
* **02-01:** hook diagnostics — verbose mode, port-missing vs stale, one-time hint ([4547428](https://github.com/special-place-administrator/symforge/commit/4547428aeb984e727947ea94a3f0e40451060216))
* RTK adoption milestone — symbol disambiguation tests, hook diagnostics, docs links ([9bc3ead](https://github.com/special-place-administrator/symforge/commit/9bc3ead5c49813998a9987b3e2066398313d48db))


### Bug Fixes

* wrap env var manipulation in unsafe blocks for Rust 2024 edition compliance ([c363499](https://github.com/special-place-administrator/symforge/commit/c3634999e2c91fcbc57a2fb92decc5f6a217a77f))

## [1.7.0](https://github.com/special-place-administrator/symforge/compare/v1.6.0...v1.7.0) (2026-03-20)


### Features

* daemon fallback, callee dedup, token budget, search defaults ([d13e76b](https://github.com/special-place-administrator/symforge/commit/d13e76b77308b16d42bf721400f10ef6215cc896))

## [1.6.0](https://github.com/special-place-administrator/symforge/compare/v1.5.0...v1.6.0) (2026-03-19)


### Features

* **adoption:** add hook outcome metrics ([cce4473](https://github.com/special-place-administrator/symforge/commit/cce4473e5024ade6de5f5eea2b6daef87ff38c2e))


### Bug Fixes

* **npm:** keep global auto-init out of workspaces ([1626dfa](https://github.com/special-place-administrator/symforge/commit/1626dfa89a6da5a9e14b0107df3eec7a0b325c61))

## [1.5.0](https://github.com/special-place-administrator/symforge/compare/v1.4.0...v1.5.0) (2026-03-19)


### Features

* **adoption:** add workflow sidecar adapters ([143ac0b](https://github.com/special-place-administrator/symforge/commit/143ac0bfac5f09a978e4f85bc70c75f2028a0477))
* **adoption:** define owned workflows for hooks ([0185310](https://github.com/special-place-administrator/symforge/commit/0185310fdbae3fded168d5a4249792372f666234))
* **adoption:** steer protocol read workflows ([106cdda](https://github.com/special-place-administrator/symforge/commit/106cddaec9ecb7f0f1a82dae22cb54d95e942ff8))
* **adoption:** tighten hook routing for source workflows ([5b9426a](https://github.com/special-place-administrator/symforge/commit/5b9426a9aa3f6f02f3f27575602bb35f1b74a8f9))
* **init:** harden client guidance rollout ([f30667c](https://github.com/special-place-administrator/symforge/commit/f30667ce885ba5040a4a40751b7401080fad8977))

## [1.4.0](https://github.com/special-place-administrator/symforge/compare/v1.3.1...v1.4.0) (2026-03-19)


### Features

* **config:** add structured syntax diagnostics ([7360beb](https://github.com/special-place-administrator/symforge/commit/7360bebb290e50bc65fee0a63ef5118cdc72117c))
* **edit:** track item byte ranges on symbols ([da3294c](https://github.com/special-place-administrator/symforge/commit/da3294c623589382bfa8dd2311944d868a6804e7))

## [1.3.1](https://github.com/special-place-administrator/symforge/compare/v1.3.0...v1.3.1) (2026-03-19)


### Bug Fixes

* **init:** canonicalize SymForge Codex guidance and allowlists ([7680757](https://github.com/special-place-administrator/symforge/commit/76807570e24995dfc05ad6df022cbd1fbab25bfa))

## [1.3.0](https://github.com/special-place-administrator/symforge/compare/v1.2.4...v1.3.0) (2026-03-19)


### Features

* add match-occurrence retrieval and watcher reconciliation health reporting ([5043a5a](https://github.com/special-place-administrator/symforge/commit/5043a5a64e74d7edc43a6f087572837ae2b7d501))


### Bug Fixes

* normalize exact get_file_content paths and backfill mtime_secs in integration fixtures ([fb398b1](https://github.com/special-place-administrator/symforge/commit/fb398b1dcbfdc59055fea328e995bdb1d9ba114c))

## [1.2.4](https://github.com/special-place-administrator/symforge/compare/v1.2.3...v1.2.4) (2026-03-18)


### Bug Fixes

* **release:** add noncommercial licensing and kill-all npm updates ([17354c6](https://github.com/special-place-administrator/symforge/commit/17354c6adba9e75a09bc3929b753881552ec929a))

## [1.2.3](https://github.com/special-place-administrator/symforge/compare/v1.2.2...v1.2.3) (2026-03-18)


### Bug Fixes

* **npm:** persist wrapper install metadata ([eeac029](https://github.com/special-place-administrator/symforge/commit/eeac0298ef08253fdf70042ba5d2f78f142faea2))

## [1.2.2](https://github.com/special-place-administrator/symforge/compare/v1.2.1...v1.2.2) (2026-03-18)


### Bug Fixes

* **bundle:** resolve impl suggestions and dependency-aware limits ([d5bfa6a](https://github.com/special-place-administrator/symforge/commit/d5bfa6aa85e3dfd719340301e1eff01d4bb1c069))

## [1.2.1](https://github.com/special-place-administrator/symforge/compare/v1.2.0...v1.2.1) (2026-03-18)


### Bug Fixes

* **kilo:** trigger release for strict-provider compatibility ([a852955](https://github.com/special-place-administrator/symforge/commit/a852955707cc9da1133f9a872d8d7b3b955988e7))

## [1.2.0](https://github.com/special-place-administrator/symforge/compare/v1.1.0...v1.2.0) (2026-03-18)


### Features

* **init:** add alwaysAllow to Claude MCP entry and expand CLAUDE.md guidance ([4ee5f53](https://github.com/special-place-administrator/symforge/commit/4ee5f535546e829850c6136b5785bd56b23c7732))

## [1.1.0](https://github.com/special-place-administrator/symforge/compare/v1.0.0...v1.1.0) (2026-03-17)


### Features

* **index:** Sprint 0 — index freshness guarantee via mtime tracking ([29d60d6](https://github.com/special-place-administrator/symforge/commit/29d60d6fe0e83f3c79856c38796bc02c62f62bea))

## [1.0.0](https://github.com/special-place-administrator/symforge/compare/v0.33.0...v1.0.0) (2026-03-17)


### ⚠ BREAKING CHANGES

* rename Tokenizor → SymForge

### Features

* rename Tokenizor → SymForge ([6366cd0](https://github.com/special-place-administrator/symforge/commit/6366cd0c7f51bc496cceb6ae255e22d95f109183))


### Bug Fixes

* restore missing [[package]] header in Cargo.lock after rebase conflict resolution ([67d9327](https://github.com/special-place-administrator/symforge/commit/67d932778c02a9899ffec904560868927604d968))

## [Unreleased]

### ⚠ BREAKING CHANGES

* **rename:** Tokenizor has been renamed to SymForge. All binaries, env vars, config paths, and npm package names have changed.
  - Binary: `tokenizor-mcp` → `symforge`
  - npm: `tokenizor-mcp` → `symforge`
  - Crate: `tokenizor_agentic_mcp` → `symforge`
  - Home dir: `~/.tokenizor/` → `~/.symforge/`
  - Project dir: `.tokenizor/` → `.symforge/`
  - Env vars: `TOKENIZOR_*` → `SYMFORGE_*`
  - MCP server name: `tokenizor` → `symforge`

## [0.33.0](https://github.com/special-place-administrator/symforge/compare/v0.32.3...v0.33.0) (2026-03-17)


### Features

* git churn in ranking, expanded guidance blocks, improved tool descriptions ([4cf1e6e](https://github.com/special-place-administrator/symforge/commit/4cf1e6e782d79e52554c23aae98590fb5a60feb5))
* lenient vec deserializer, semantic search ranking, Kilo Code init, SymForge rename plan ([c048274](https://github.com/special-place-administrator/symforge/commit/c04827422bb02a04ff4222864b0c09ff014ca70d))

## [0.32.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.32.2...v0.32.3) (2026-03-17)


### Bug Fixes

* complete parking_lot::RwLock migration across live_index and protocol ([c48d865](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c48d865c2e9c717b89da9d446a51f326af5d3052))

## [0.32.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.32.1...v0.32.2) (2026-03-16)


### Bug Fixes

* address all actionable feedback from 3 external code reviews ([61d2757](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/61d2757a8b9bbc425b73eec512dc75c470be630d))

## [0.32.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.32.0...v0.32.1) (2026-03-16)


### Bug Fixes

* update rollback tests for tempfile-based atomic writes ([548c2bb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/548c2bbede27be2a233fe35a2d7f6b8aa0aee45b))

## [0.32.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.31.5...v0.32.0) (2026-03-16)


### Features

* `tokenizor-mcp init` now registers MCP server + bumps to v0.2.1 ([b2126ed](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2126eda812237e2bc3dc03b7ef6d2f961c94735))
* **06-03:** wire token savings from sidecar into MCP health tool ([b8827f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8827f852646346f6a659bd4c8c242d3c5cb3181))
* **07-01:** add C/C++ xref queries and grammar integration tests ([cbe36ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cbe36caf706bc76a35effaf764abb78eadaaa809))
* **07-02:** create TrigramIndex module and integrate into LiveIndex ([7ee7e94](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7ee7e944a5b77c4f361d5d52ff4cfff5b1f265e1))
* **07-02:** wire trigram search, scored symbol ranking, and file tree tool ([3abdb3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3abdb3b55c5ab0c35902c6cae208a2d2e71a48bb))
* **07-03:** add persistence module with snapshot types and serialize/deserialize ([2fe7168](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2fe716875dda0aea9579c0e9c77686a3695afd40))
* **07-03:** wire persistence into main.rs with shutdown hook and startup load path ([4c07981](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4c079817877fd3f4bd43d83389bb809f1e82964c))
* **07:** add symbol extraction for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([e33decb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e33decb122f3d48838849e7da32e4ea5da336401))
* **07:** add xref queries for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([2a7f2c4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2a7f2c4c67652ae4664919099b93a37f322015f0))
* **07:** upgrade tree-sitter to 0.26 and enable PHP, Swift, Perl parsing ([a83e536](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a83e5365c1b9bf39ad2b60eafcaf76a774255a81))
* add .env file extractor ([44386b1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/44386b17adfb95f45372f2cd850588e1cf475304))
* add AdmissionTier enum and size threshold constants ([48cc242](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/48cc242852a06b463119757408b911b41efcf493))
* add around-line file content reads ([413abe7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/413abe7c867edfd26752daa4bf6e7f8b0795f886))
* add around-match file content reads and refresh README ([9406955](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/94069558f45b8b2a0ea57e45410a58a8e96940a8))
* add binary content sniff with NUL, UTF-8, and control-byte heuristics ([e7bd071](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e7bd0713957a611e0fabd42f3f6c0e68b042782f))
* add ConfigExtractor trait, EditCapability enum, key escaping ([a46f029](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a46f0299a62540e5d83c6665bae1b3125f92e955))
* add depth parameter to explore for enriched symbol analysis ([a81fdad](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a81fdad79b2ff06b96e9e841041e617675182652))
* add deterministic file content chunking ([731bebd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/731bebd00c8042476eb180529e984e78e57b7423))
* add doc_byte_range field to SymbolRecord ([5699030](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/569903096ad933cb1099afa0688de821aae9c6d2))
* add DocCommentSpec and scan_doc_range algorithm ([61ab5bb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/61ab5bb1ddda789f0e69a5f27181ab54a76d4e02))
* add exact-selector context navigation ([36243df](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36243dffe29b34b91bb5183c4bd0f237d9c145e2))
* add exact-selector symbol context lookup ([fc28210](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fc2821033c082fc8c002785501652289b112d01b))
* add explore tool for concept-based codebase exploration ([e7a2364](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e7a23642730253f6e4df005eecf135460db7e6cf))
* add extension denylist for admission control ([6159487](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6159487c7468fcb037ae2ed6a603eeb9c56c02b9))
* add first-class config file indexing and gated edit support for JSON/TOML/YAML/Markdown/.env ([4bbac75](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4bbac7599509e62fbd4bc94551eddc277fc8d68f))
* add frontend asset parsing (HTML, CSS, SCSS) ([a91b625](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a91b625739c997da8b0a1e5f8cf41b46979aeb65))
* add Gemini CLI support (init, MCP registration, auto-allow) ([b2429b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2429b62b4e2032521ffbb2f2d1cb8eea0615883))
* add get_co_changes, diff_symbols tools + UX improvements ([c00c2f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c00c2f84395a6a7f680cbc0d32838356ce106dfc))
* add git2 library wrapper for in-process git operations ([dc5e146](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dc5e146cccc1a8f78af7916104cf9ae87c2ab375))
* add Html, Css, Scss to LanguageId with extension mapping ([283c592](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/283c59214990c6f316247830c271d868be86a701))
* add import/export summaries to get_file_context ([62f38a9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/62f38a949bbf4009e0d94b68a7566a20c0855b37))
* add JSON key-path extractor ([a3485d3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a3485d3ef4af680770036df9b1f09d3f243509c3))
* add Json/Toml/Yaml/Markdown/Env to LanguageId, Key/Section to SymbolKind, is_config to FileClassification ([7ccd265](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7ccd2650cc9ae88018f15fe7968914e3bdb8aada))
* add LineEnding detection and normalization helpers (C1 prep) ([a82a727](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a82a72763a9060650553c68da33a0e2d5bd8dc95))
* add Markdown section extractor ([cc1a128](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cc1a128ff48908ee6828476ef951058f09c15ace))
* add Mermaid and DOT graph output for find_dependents ([127fed2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/127fed25b4ff07ce48f82fabc03750c53d98f58b))
* add module-path boosting to explore (Phase 0) ([2f7dac0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2f7dac03d228beb673c16c418d78815a5e145619))
* add per-language DocCommentSpec and wire into push_symbol ([5a0fff2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5a0fff26d9020c5aefe02d4a319727ffc99121ff))
* add recursive type resolution to get_context_bundle ([d2caaca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2caaca004c764d7593f69a2dce25ddf542e9324))
* add routing hint, code_only flag, and update stale tool descriptions ([b941eff](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b941effdef0b608b01bb6394330f65459107f403))
* add scoped search_symbols filters ([3ec4dc7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3ec4dc77d3de2f5ee764bb0438916f265824e78e))
* add SkippedFile struct and store integration for admission tiers ([b94aeeb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b94aeeb8758ccce92733fefc74a97858b6ccf978))
* add symbol-addressed edit tools (Tier 1) ([3dec094](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3dec094cb89417e7b7208caea808b151f109dbf1))
* add token savings — verbosity param, sections filter, compact modes ([0184917](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/018491738218681c5d6c85c6fee267ca321a8aaa))
* add TOML key-path extractor ([932b977](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/932b977e402b8f5503988efeba135bf8dea06873))
* add tooling preference guide and challenge line to README ([8cd028c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8cd028cd320d62b75af84da1614955f630d0a07d))
* add trait/interface implementation mapping with find_implementations tool ([4be3610](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4be36105fa605fca6b3e33e67bdba6fa79258ede))
* add tree-sitter-html, tree-sitter-css, tree-sitter-scss dependencies ([3be5ee3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3be5ee3dd9544cd9ff03e8d5a3dfe579764547bb))
* add unified edit_capability_for_language, rename check_edit_capability ([1742a17](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1742a1744842f376c611a7ab8b5cd24e7d214803))
* add YAML key-path extractor with serde_yml ([c5919e2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c5919e20ddc5707889e32b764bf5afdac58ed1fb))
* analyze_file_impact shows clear status taxonomy (U4) ([263834f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/263834f3d8dd2a4c22911eed69a506edb7c13bd6))
* auto-allow all Tokenizor tools during init (no more permission prompts) ([948f360](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/948f3605da9279ae67d76f330b737009a526abf2))
* batch_edit dry_run mode (U5) ([4166196](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/41661963168aced0d37c7931628c3dcf49f6b550))
* batch_rename supplemental qualified path scan with confidence classification ([e75f2d4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e75f2d40d657ea668660d3160c884feaf396ec96))
* bump index snapshot version to 3 for doc_byte_range ([c002fcc](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c002fccfe02f569a28b62615433d99ea7a3be1e0))
* clean npm cache after install to reclaim disk space ([b1c4a35](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b1c4a353fc1d1985c31e363d1be22f7f4e17a440))
* complete Phase B - implement trace_symbol tool ([3869941](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3869941d08a243f65b9ffc18673caac63b22f410))
* complete Phase C - implement inspect_match and locality ranking ([8d0dff9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d0dff9b781fddc03bd7fbaa9c04058042b142b1))
* complete scoped search_text upgrades ([4025717](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/402571778aced351d1cb9106204a917dfd6667dc))
* concept+remainder merging in explore ([c6e93bd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c6e93bd04ea9035a9c3a9ab279539f9ebb8ba1d1))
* config file parsing — all extractors, pipeline integration, edit gating, test fixes ([bde2f3d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bde2f3d1a80bdd72cc22ed10d227963e4a651d44))
* daemon resilience and zero-touch install ([0d3bd80](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0d3bd80614c720233ffb188ef1827500e83dbbc6))
* edit tools use doc_byte_range for splice boundaries ([cee3ff2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cee3ff29d43078448555b4479a4de399d9731b9e))
* **edit:** add Tier 2 batch tools — batch_edit, batch_rename, batch_insert ([7090b15](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7090b15f23de9dd2813c87d6d2ce3619aaef8c77))
* **edit:** Tier 2 batch tools — batch_edit, batch_rename, batch_insert ([859271d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/859271d7a1b7f2954335002e2aa3c8588cae2109))
* expand CONCEPT_MAP and add word-boundary matching ([f94e07d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f94e07dc29cb51f2055cf05ebea91946f02f1f1a))
* expand file content read ergonomics ([16b3a09](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/16b3a0965d9f05f248cbee4b2fdeff3d9117b57d))
* expand prompt context exact hint routing ([b7b0c42](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b7b0c427b562c0e07ea0b661679334d279ebe955))
* expand prompt context line hint parsing ([ad0c162](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ad0c162c74ce4c4fb6b7c2934df027df86455352))
* expand tokenizor shared MCP capabilities ([459bbb5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/459bbb59d3de7702b66649e67e3ad325ab79b021))
* explore filters noise by default (U1) ([f14b702](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f14b702b0a68ba6c88b5c96327cbdf5d701e5d72))
* extend prompt context exact alias routing ([10294c5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10294c57e4f97f9c5f8cabc85ac5b2b61fbc620e))
* extend prompt context module alias routing ([1084256](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10842566f40f7463345aabdff10e93cd5037aeb4))
* extend prompt context slash hint routing ([242ef24](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/242ef240dc6a9aecc15a064087023cdbce45b7e7))
* gate edit tools by config file EditCapability ([7613d11](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7613d112cfa8c688c85d498808c4f7c740fcb9ce))
* get_file_content falls back to raw disk read for non-source files ([9bf8ba5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9bf8ba5cdc7811b1d24c340d78880a01510b8130))
* get_file_content mode enum for clearer API (U10) ([244be75](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/244be753b08275b80f3bd1d8a11214a74f642f03))
* git temporal intelligence with churn, ownership, and co-change analysis ([d4cd579](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d4cd579e6f842db7cc2aadd14700d42dd997d411))
* health shows partial parse file paths (U8) ([8560114](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/856011485f82605d41e93651748bf64db1486c91))
* implement admission gate with tiered file classification ([9e69e23](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e69e238f50b3364545fa3844cbb5b03ae7ad925))
* implement CSS symbol extractor with tests ([39719f4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/39719f4962657e824f9a82b9c8040be48af71cfd))
* implement HTML/Angular symbol extractor with tests ([4627b52](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4627b520009e2c8395f39593644499cae428fe75))
* implement SCSS symbol extractor with tests ([2112b1a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2112b1a7776db40d5d63ccd12da753fba085aaa9))
* improve explore relevance ranking (Q1) ([5b829ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5b829cab9138885daf8479b3fb3a594bc35838d5))
* improve prompt context symbol disambiguation ([144377d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/144377daef8adeb5a7e80c87b441964ba96cd495))
* include doc comments in symbol body extraction ([679682b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/679682b03d9a9548b8b67639c3257b6a5a63ff9c))
* integrate config extractors into parsing pipeline ([961c25b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/961c25b36a75ab4ea033ce5921f4368ab41a4495))
* module-path-aware find_dependents for lib.rs, mod.rs, __init__.py, index.js ([ea2655c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ea2655c4c68f17bf42e914da5aa57e86c456f468))
* per-tool call counters in health output (U9) ([d41bfb5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d41bfb5730d07b2b0271ee1947eec0306f07375c))
* PreToolUse hook intercepts Grep/Read/Glob/Edit with Tokenizor suggestions ([1c78000](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1c780008662d9fb1c5cf1d7e573df64e04a23807))
* PreToolUse hook now intercepts config files for Tokenizor suggestions ([ee88bff](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ee88bff2f5341e026cbed90b6da7a1dcaaef33eb))
* quality improvements from 3-project eval (Q1-Q6) ([8d23ff4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d23ff4749250d4ad85f12324280543fd8c5e403))
* quality improvements from eval feedback (Q3-Q6) ([79ac714](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/79ac714eb67a2d504be76a4beec7479cfe154385))
* replace git CLI with git2 library in tools and diff_symbols ([db3824a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/db3824aff0197cdc7408a82d71add37e6ae2b2e2))
* replace git log CLI with git2 library in temporal analysis ([f6877eb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f6877ebc81e3d8f6a6ebc6001cdbecc29979292c))
* rewrite tool descriptions with NOT-for redirects, fix verbosity polish ([466f207](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/466f207b1959df08f30d04d7e6eb3338938340d0))
* richer verbosity=signature includes visibility and return type (U6) ([eef2926](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/eef2926f057e3020200bb13cc1dd47b9ee9bf76e))
* search_files changed_with parameter — find co-changing files via git temporal coupling ([95b5901](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/95b5901524cce5073d318c151d8c02c88201e621))
* search_symbols browse mode without query (U2) ([3326342](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/33263428425acd01a9cfba460d96d0b5534257b5))
* search_text follow_refs — inline callers of enclosing symbol ([ae64a9a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ae64a9a12284953e7ece3e8e0da39389e533e398))
* search_text group_by parameter — deduplicate by symbol or filter imports ([8fcf8a7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8fcf8a7fc36685ee8688b02be08461f08971be97))
* show Tier 2 tags and Tier 3 footer in repo_map ([05d23eb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/05d23eb7b23c06c607e6adacac00ae7edbb2c7dc))
* Sprint 14 — trust fixes + tiered admission control ([b7a9296](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b7a92963b3f9c55be08e73e04eba6bd70901b1bf))
* start Phase B - implement trace_symbol tool and add handoff summary ([dd3af33](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd3af336acb024ada2b76d51d5ea35428e751671))
* suppress noisy search_symbols results by default ([3255928](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32559289e0242d755af06896f0aa4d902a002a55))
* symbol-addressed edit tools + description redirects + verbosity fixes ([ba9e587](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ba9e587d2c38e08b9e0881ebfc02bbf1c18db283))
* symbol-aware context in search_text — show enclosing symbol for each match ([73ee432](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/73ee43249263c5338236c24d00b7c645da9e4d4a))
* tokenizor v2 rewrite — in-memory LiveIndex with parasitic hook integration ([3cbc63c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3cbc63c350f2cafd8b77601db3235bdbba779271))
* update README for 24 tools, add CLAUDE.md, rename prompts to tokenizor-* prefix ([949738f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/949738f88ce47fa3ede4a5e919127787678017bb))
* wire admission gate into discovery walk ([51c73f7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/51c73f7b125d13a88179d9e1e2cf535e28839888))
* wire HTML, CSS, SCSS extractors into parsing pipeline ([e740f94](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e740f947007acee5db43a321bb38152e1fed63cf))


### Bug Fixes

* **06-02:** make hook helper fns pub and fix run_hook signature in integration tests ([36d45de](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36d45defe68af2996502c94b5fa7008a5a2222ef))
* **07:** use box-drawing chars in tier headers per CONTEXT.md ([bb22570](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb225701a514c477c376e7e2bc0763c667381ed6))
* add 'burst' to file watching concept symbol_queries ([930d8e8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/930d8e87b053af0c1035e6518ca8b0c3ea46b1ee))
* add actions:write permission for workflow re-trigger ([83aeef5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/83aeef5cc953782045dd6620c75d0c48bde461d4))
* add exe/dll/so/dylib/class to denylist (C2-lite) ([14d0459](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/14d04593df00b7cbc92643aeb5c7109026a045cb))
* add missing gitignore/noise_class field initializers across codebase ([c8088f9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c8088f9f0953b004e015c2a707db89dae3597ced))
* add missing sibling_limit/overflow fields to initializers ([b25f4a5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b25f4a5a34a007ad9a56757cd1a62ce7c9f92157))
* add total hit limit to find_references ([d592e13](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d592e136df4d78c04be59c6a3e73edc7d1fad2c2))
* address review findings — diff_symbols filter, search prefix matching, impact messaging, chunk line numbers ([44298df](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/44298df66c3b742f2736fcd33151884b2118cbe6))
* address review findings — OR search terms, range validation, depth 3, schema docs, insert spacing, token counter ([249f987](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/249f9876196de831e1dc5d6a34b8dd5cf284e1f1))
* around_line error, diff note, language-scoped warnings, dry-run ([a6a5f70](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a6a5f701fc1bfb56f7e13cb01f732383e101950a))
* around_symbol returns full indexed symbol span (B2) ([3b06c2a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3b06c2a735ad3edd0ac691851a944d66797b06f9))
* auto-correct double-escaped regex patterns in search_text ([e98cd4d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e98cd4d6671e129b1bc775ec4dde28129931b22e))
* auto-indent replace_symbol_body + update edit tool descriptions ([a113c7b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a113c7b5444656cb3935a22defdede73ba538c2f))
* auto-indent replace_symbol_body + update edit tool descriptions ([d1d22ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d1d22ca3b26b59fcd107ad62e294abe10a0de678))
* auto-indent replace_symbol_body + update edit tool descriptions ([39f8fec](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/39f8fec7b95ccdb2d383c9089f6d267f4a86b69c))
* auto-indent replace_symbol_body + update edit tool descriptions ([a021b5b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a021b5bd22ea75be5cb3e0599c8abbb2c0c1011a))
* auto-merge release-please PRs for continuous deployment ([e58ce57](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e58ce57ce77d56e66219a6e7663459f72bfb8dc2))
* batch_edit dry_run byte count + auto-detect regex in search_text ([29474c2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/29474c21bd33b1da6858bdbb4179eaa3ac9611a1))
* batch_edit shows ROLLED BACK message on failure (B4) ([3ab8358](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3ab83587c7bbee77bd1f1cfe1b1980066630da8f))
* batch_insert no extra blank line before function (B1) ([3409548](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/34095482b8275811cb5373003e367c0b07dcfec0))
* batch_rename atomic rollback on failure, batch_edit/batch_insert best-effort with correct index state ([6b332f3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6b332f3f18484a8e14ed35731961eac98311b18f))
* batch_rename catches path-qualified usages ([c2243ff](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c2243ff058e0ca29848272641eed8c27eec47131))
* batch_rename catches path-qualified usages via literal scan ([2824745](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/282474529928f46e3dec525470244baa3fb68873))
* batch_rename review fixes — atomic rollback, dead code, dedup ([0a7844e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0a7844ee1b8ce77b462f1f6a3b51b53c1ce17a22))
* code review feedback — tests and safety fixes ([c2454ea](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c2454ea2bbd1ec0c2f2acb357bd9f158fd101584))
* compute real line numbers for TOML symbols ([d939caa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d939caa7d3a587efa7482dc9f4790e7b102e15ac))
* compute real line numbers for TOML symbols ([7dd4697](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7dd46973f52228193cc98ccb8d01361b75eef808))
* daemon lifecycle hardening — stale lock detection, fast-fail proxy, cleanup (DL1-DL4) ([8350d7e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8350d7e3b5de2bb16d85d69a817ca459fb43e829))
* daemon proxy deadlock under concurrent tool calls + request governor ([541dd68](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/541dd688e9956d75818733740764320513e9c8ab))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([3b1cc4e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3b1cc4ed6ae8591ca8afc720158f2241cbec80de))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([b793579](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b793579b2f1bdfdd7fb266bfa9bf7dd76590ea9e))
* disable git2 SSH/HTTPS features to remove OpenSSL dependency ([0167936](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0167936069c2481cdbb07fb4e4bb5acdbd483131))
* edit_within splice range + multi-line block comment detection ([4dd4ea8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4dd4ea84de658b5170d09d42c0200d3bd171ae2b))
* explore multi-term scoring with enclosing symbol injection ([5f36dab](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5f36dab73ac26247945d34f769e182e48e6bfbe6))
* explore text search max_per_file too low for symbol injection ([4a5b67e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4a5b67edf156e27cfe21e1f8e4c0b86fea03ff9e))
* filter explore noise from CONCEPT_MAP self-matching and generic terms ([8addfaa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8addfaaa4646144458ea1c71b1a9fa1de7728826))
* find_dependents follows pub use re-export chains for Rust modules ([a48aee2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a48aee2b7180dc0b669f72b8b94f4e36b0284e0a))
* find_dependents resolves workspace crate paths ([418652a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/418652a42bd3e1443602cb87cd1eed2d7e4c0574))
* find_dependents resolves workspace crate paths (B4) ([f02819d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f02819db942530bcbb974fe946d05224bb953ab6)), closes [#89](https://github.com/special-place-administrator/tokenizor_agentic_mcp/issues/89)
* find_references file count + get_repo_map full path filter ([89cc588](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/89cc5880c4c7ffb8ed2e09dd9c8bd9d7e573fa52))
* follow_refs shows same-file callers and empty-result signal ([b3acea1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b3acea139692468421ed85ef655ff958baad008d))
* gate Windows-specific path tests with #[cfg(windows)] ([6d9f0b9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6d9f0b9a727a215bf499bd0329c0d107823f6a5c))
* gate Windows-specific path tests with #[cfg(windows)] ([dcc963b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dcc963b4d36c2048b7ad82b9dce320bbfb22b50e))
* gate Windows-specific path tests with #[cfg(windows)] ([df57ac0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/df57ac0a8c98564c80896b91ac04c10bd18a9d7a))
* gate Windows-specific path tests with #[cfg(windows)] ([1710de0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1710de0ca89d0bffe124994d91b2b714ea389312))
* gate Windows-specific path tests with #[cfg(windows)] ([32f2964](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32f2964da4f41897ac4988393968fde7505662f5))
* gate Windows-specific path tests with #[cfg(windows)] ([a1cae52](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a1cae520cbf8809e2d4c97d6f21307012c554509))
* Gemini CLI init writes correct timeout (120000ms) and trust setting ([b8616eb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8616eb1f5621872e050940b6756c9a04e707668))
* get_file_context sections filter masked by 800-byte hook budget ([5ff4c9e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5ff4c9e3031e099d7321c5b906ba3109f031e3f7))
* handle locked binary on Windows during npm update ([b57aa8c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b57aa8cf25fbcaf8e3e6e8d6625f7f989418c43c))
* handle SIGTERM for daemon graceful shutdown (C5) ([3faef1f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3faef1f772ef679f137229cc7fc9d47891c132f6))
* hook detection for tokenizor-mcp binary name and npx cache warning ([d0ad70a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d0ad70a52297826c2187499d62e030ca55b4ee6d))
* improve context_bundle output quality and symbol_context guidance ([45eb6e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/45eb6e412bd1abf76d3430f761def6f319a3384d))
* improve file watcher burst handling and evict idle trackers ([442d240](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/442d24031d71607e4d84d47e32d70a65f2ec5a4c))
* improve search ranking, symbol diff accuracy, test filtering, and error messages ([13ebb36](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/13ebb3637075c2e1bf6f187b5f07722c2cd9ecec))
* include tool description rewrites in release ([afbad0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/afbad0cfeb117ff29a9b77d2673531ceff0941cb))
* index purge on file delete, richer default symbol context, path-scoped implementations ([ec634b5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ec634b5aae1d7dcec5137b70757282efb9c578d2))
* index purge on file delete, richer default symbol context, path-scoped implementations ([1cb084b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1cb084b0444f89d825cae0d2d60e631e0cbd54c5))
* insert_before uses blank line separator when no doc comments ([2253f7d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2253f7da0c14cb4c6d858442af375e5d8552bebe))
* install binary to ~/.tokenizor/bin/ to avoid Windows file-lock ([da9c40a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da9c40a2ccd4987d061e574dbcc5f18f13a2679c))
* keep each Where-Object filter as a single-line expression. ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))
* lenient parameter deserialization for MCP clients that stringify values ([5d613d4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5d613d4b9e06e296b3f3ce9bbd4d133a0a94b726))
* make installer tests host-agnostic ([6eb0bfa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6eb0bfada2f70bc86a4974a1f812bc7f315a63d0))
* make npm updates replace locked windows binaries ([da3f24d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da3f24d808aee39512545e29db989b7d4bb2f428))
* non-ASCII panic in doc scanning and deterministic circuit-breaker (CR1, CR2) ([1e52aaf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1e52aaf7468d425fac21371398a631a93d6c5bfe))
* non-blocking cold-start indexing for faster MCP discovery ([acb8743](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/acb874307fc01eaf162cf76de4cba2dc1e942ba8))
* npm wrapper and release pipeline for v2 ([ab794da](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ab794dae0105cd8bc49466bcab04d27c6fc38457))
* pin CI/release workflows to Rust 1.94.0 matching rust-toolchain.toml ([41d23ab](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/41d23ab3fd83205773f9289cf95cd142fd2cb1b9))
* pin Rust toolchain to 1.94.0 via rust-toolchain.toml ([fed0e20](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fed0e20560de0356cb86bf5dc9319af2867e770c))
* PowerShell -and operator parsing in install scripts ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))
* prevent analyze_file_impact from destroying index, fix close_ses… ([5af91cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5af91cf906781b8c902b7ac96637a066a572d915))
* prevent analyze_file_impact from destroying index, fix close_session deadlock ([9e29787](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e29787709f064538ff379c234172c11e43d69cd))
* prevent analyze_file_impact index corruption and close_session deadlock ([a7e6ff3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a7e6ff393713c9f3be680f62338ebecab71a6731))
* prevent async runtime starvation under concurrent subagent load ([74f1d54](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/74f1d54f0f26dba97a619fb5b69e645c1d702034))
* prevent async runtime starvation under concurrent subagent load ([2ed134a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2ed134aa34a82f258eb37be4121c341272cc85d6))
* prevent non-ASCII panic in find_qualified_usages (batch_rename crash) ([8555d0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8555d0ce0b3ed39acf7d170c0878d4e94267a2ef))
* re-trigger workflow after auto-merge for full automation ([2e73e1d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2e73e1dbcf110625c08cbd258a9b852590956b2f))
* recurse into mixin/function bodies, guard empty at-rule names ([8a8e717](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8a8e7178c079078581ac8c6e339128814e16478a))
* refuse to auto-index home dirs, drive roots, and system paths ([d459bbf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d459bbf7bac24f9747bb23f7370f62efd328d664))
* reindex from disk after writes, not from in-memory buffer ([d605498](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d6054988db18ad6cf82e3a82cca2c054a1c5f52b))
* **release:** document conventional commit requirement ([1867bce](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1867bce6a312079e6edd5b8ccf16fc0b43f4089d))
* rem_euclid for timestamps, generic pub(...) visibility in diff_symbols ([f597c78](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f597c78fabcec1ce18b4e20665516cd8fd772537))
* replace deprecated macos-13 runner with macos-latest ([4ab6e72](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4ab6e72f1bfab0f2bdefdf915e6ff3c5d0e472ef))
* resolve 6 confirmed bugs across watcher, daemon, trigram, discovery ([a50b723](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a50b7232519cd640aa9140f1e0e6c032fac43eeb))
* resolve all actionable issues from external review ([3e11288](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3e112880a6802182bf59c5159373fc3ab636a240))
* retry PR label lookup with 60s timeout for auto-merge ([1ee07e6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1ee07e61c5bae54a6bdeac8949bcd4d7b9d07b0c))
* revert worker_threads override — spawn_blocking handles concurrency ([a5d5d4e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a5d5d4e77dfd2d438a42ef2161fd1c7111584abd))
* review feedback — Q3 robust name extraction, Q6 UTF-8 safe truncation ([41e17d2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/41e17d23a4b4ce9e89d9fdfbceb5ba089a9880a0))
* rewrite open_project_session with double-checked locking (C6) ([b04b0d0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b04b0d099fb035ca1bac9c76c49d542cae1d8102))
* robust auto-merge for release-please PRs ([940b9b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/940b9b6bab4bfa22bf4455cc6474dea300f213a5))
* search_symbols file count + find_references missing cross-file type refs ([8d40874](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d4087446f8f7c980f5e5abe6b16366e6cc5f697))
* security patches, parser improvements, parallelism fixes, and review follow-ups ([2b1d5cb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2b1d5cbafed5100ea833d0f4b41da41b1d87cb27))
* show_line_numbers works with around_symbol and around_match (B3) ([4befe8a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4befe8a8f8e734e722088632c23ac81489bf42ce))
* simplify release pipeline — let PAT-triggered run handle release ([508fbc2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/508fbc247d7b54f79cce14526b8e755fca7acdba))
* single-run release pipeline — no second trigger needed ([973428e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/973428e730d1ea4d42e525597f0fa7048ca57500))
* split-brain after index_folder, empty search_symbols guard, inspect_match bounds check ([00cf4be](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/00cf4be964f1314fa7930d68e31dac2327dced27))
* surface tool panics as immediate error responses instead of stalls ([31ae935](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/31ae935642876711383897ba3b779ea4c2dc7b52))
* thread LineEnding through all edit helpers for CRLF preservation (C1) ([dda40b4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dda40b4c0c8a99259a93af4c9ff65bb696e9e337))
* type-aware reference filtering reduces false positive warnings in replace_symbol_body ([b24dd0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b24dd0cdcb5dbed44edb077959c48972563b8845))
* update installer test assertion for execFileSyncFn version check ([f6ed05d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f6ed05dd601105a4a3249cf2844b4da91210c560))
* use unique temp files in atomic_write_file (C3) ([dddcb16](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dddcb16eb85dc294354691e9fc470adbca5ce9bd))
* UX improvements from third review ([648218f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/648218fe4b70378133ec115dd93cd5a089b44bbc))
* validate splice overlap in batch_rename (C4) ([0c80b74](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0c80b74903261a09837a7038998bf3d61898d274))
* version-aware npm update + --version flag ([b935bb0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b935bb0ea7cc52916abace2435873f19dfe4c01d))


### Performance Improvements

* incremental reverse index updates on file mutation ([e85c445](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e85c445b7d7e017a961e4de5da851a6ca0e0cd01))

## [0.31.5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.31.4...v0.31.5) (2026-03-16)


### Bug Fixes

* surface tool panics as immediate error responses instead of stalls ([31ae935](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/31ae935642876711383897ba3b779ea4c2dc7b52))

## [0.31.4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.31.3...v0.31.4) (2026-03-16)


### Bug Fixes

* prevent non-ASCII panic in find_qualified_usages (batch_rename crash) ([8555d0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8555d0ce0b3ed39acf7d170c0878d4e94267a2ef))

## [0.31.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.31.2...v0.31.3) (2026-03-16)


### Bug Fixes

* pin CI/release workflows to Rust 1.94.0 matching rust-toolchain.toml ([41d23ab](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/41d23ab3fd83205773f9289cf95cd142fd2cb1b9))

## [0.31.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.31.1...v0.31.2) (2026-03-16)


### Bug Fixes

* pin Rust toolchain to 1.94.0 via rust-toolchain.toml ([fed0e20](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fed0e20560de0356cb86bf5dc9319af2867e770c))

## [0.31.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.31.0...v0.31.1) (2026-03-16)


### Bug Fixes

* daemon lifecycle hardening — stale lock detection, fast-fail proxy, cleanup (DL1-DL4) ([8350d7e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8350d7e3b5de2bb16d85d69a817ca459fb43e829))

## [0.31.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.8...v0.31.0) (2026-03-15)


### Features

* add LineEnding detection and normalization helpers (C1 prep) ([a82a727](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a82a72763a9060650553c68da33a0e2d5bd8dc95))


### Bug Fixes

* add exe/dll/so/dylib/class to denylist (C2-lite) ([14d0459](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/14d04593df00b7cbc92643aeb5c7109026a045cb))
* handle SIGTERM for daemon graceful shutdown (C5) ([3faef1f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3faef1f772ef679f137229cc7fc9d47891c132f6))
* non-ASCII panic in doc scanning and deterministic circuit-breaker (CR1, CR2) ([1e52aaf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1e52aaf7468d425fac21371398a631a93d6c5bfe))
* rewrite open_project_session with double-checked locking (C6) ([b04b0d0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b04b0d099fb035ca1bac9c76c49d542cae1d8102))
* thread LineEnding through all edit helpers for CRLF preservation (C1) ([dda40b4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dda40b4c0c8a99259a93af4c9ff65bb696e9e337))
* use unique temp files in atomic_write_file (C3) ([dddcb16](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dddcb16eb85dc294354691e9fc470adbca5ce9bd))
* validate splice overlap in batch_rename (C4) ([0c80b74](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0c80b74903261a09837a7038998bf3d61898d274))

## [0.30.8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.7...v0.30.8) (2026-03-15)


### Bug Fixes

* update installer test assertion for execFileSyncFn version check ([f6ed05d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f6ed05dd601105a4a3249cf2844b4da91210c560))

## [0.30.7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.6...v0.30.7) (2026-03-15)


### Bug Fixes

* security patches, parser improvements, parallelism fixes, and review follow-ups ([2b1d5cb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2b1d5cbafed5100ea833d0f4b41da41b1d87cb27))

## [0.30.6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.5...v0.30.6) (2026-03-15)


### Bug Fixes

* batch_edit dry_run byte count + auto-detect regex in search_text ([29474c2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/29474c21bd33b1da6858bdbb4179eaa3ac9611a1))

## [0.30.5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.4...v0.30.5) (2026-03-15)


### Bug Fixes

* get_file_context sections filter masked by 800-byte hook budget ([5ff4c9e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5ff4c9e3031e099d7321c5b906ba3109f031e3f7))

## [0.30.4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.3...v0.30.4) (2026-03-15)


### Bug Fixes

* batch_rename review fixes — atomic rollback, dead code, dedup ([0a7844e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0a7844ee1b8ce77b462f1f6a3b51b53c1ce17a22))

## [0.30.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.2...v0.30.3) (2026-03-15)


### Bug Fixes

* revert worker_threads override — spawn_blocking handles concurrency ([a5d5d4e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a5d5d4e77dfd2d438a42ef2161fd1c7111584abd))

## [0.30.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.1...v0.30.2) (2026-03-15)


### Bug Fixes

* prevent async runtime starvation under concurrent subagent load ([74f1d54](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/74f1d54f0f26dba97a619fb5b69e645c1d702034))

## [0.30.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.30.0...v0.30.1) (2026-03-15)


### Bug Fixes

* prevent async runtime starvation under concurrent subagent load ([2ed134a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2ed134aa34a82f258eb37be4121c341272cc85d6))

## [0.30.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.29.1...v0.30.0) (2026-03-15)


### Features

* add AdmissionTier enum and size threshold constants ([48cc242](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/48cc242852a06b463119757408b911b41efcf493))
* add binary content sniff with NUL, UTF-8, and control-byte heuristics ([e7bd071](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e7bd0713957a611e0fabd42f3f6c0e68b042782f))
* add extension denylist for admission control ([6159487](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6159487c7468fcb037ae2ed6a603eeb9c56c02b9))
* add SkippedFile struct and store integration for admission tiers ([b94aeeb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b94aeeb8758ccce92733fefc74a97858b6ccf978))
* batch_rename supplemental qualified path scan with confidence classification ([e75f2d4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e75f2d40d657ea668660d3160c884feaf396ec96))
* clean npm cache after install to reclaim disk space ([b1c4a35](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b1c4a353fc1d1985c31e363d1be22f7f4e17a440))
* implement admission gate with tiered file classification ([9e69e23](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e69e238f50b3364545fa3844cbb5b03ae7ad925))
* show Tier 2 tags and Tier 3 footer in repo_map ([05d23eb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/05d23eb7b23c06c607e6adacac00ae7edbb2c7dc))
* Sprint 14 — trust fixes + tiered admission control ([b7a9296](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b7a92963b3f9c55be08e73e04eba6bd70901b1bf))
* wire admission gate into discovery walk ([51c73f7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/51c73f7b125d13a88179d9e1e2cf535e28839888))


### Bug Fixes

* batch_rename atomic rollback on failure, batch_edit/batch_insert best-effort with correct index state ([6b332f3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6b332f3f18484a8e14ed35731961eac98311b18f))
* reindex from disk after writes, not from in-memory buffer ([d605498](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d6054988db18ad6cf82e3a82cca2c054a1c5f52b))

## [0.29.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.29.0...v0.29.1) (2026-03-15)


### Bug Fixes

* daemon proxy deadlock under concurrent tool calls + request governor ([541dd68](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/541dd688e9956d75818733740764320513e9c8ab))

## [0.29.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.28.2...v0.29.0) (2026-03-15)


### Features

* analyze_file_impact shows clear status taxonomy (U4) ([263834f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/263834f3d8dd2a4c22911eed69a506edb7c13bd6))
* batch_edit dry_run mode (U5) ([4166196](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/41661963168aced0d37c7931628c3dcf49f6b550))
* explore filters noise by default (U1) ([f14b702](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f14b702b0a68ba6c88b5c96327cbdf5d701e5d72))
* get_file_content mode enum for clearer API (U10) ([244be75](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/244be753b08275b80f3bd1d8a11214a74f642f03))
* health shows partial parse file paths (U8) ([8560114](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/856011485f82605d41e93651748bf64db1486c91))
* per-tool call counters in health output (U9) ([d41bfb5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d41bfb5730d07b2b0271ee1947eec0306f07375c))
* richer verbosity=signature includes visibility and return type (U6) ([eef2926](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/eef2926f057e3020200bb13cc1dd47b9ee9bf76e))
* search_symbols browse mode without query (U2) ([3326342](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/33263428425acd01a9cfba460d96d0b5534257b5))


### Bug Fixes

* add missing gitignore/noise_class field initializers across codebase ([c8088f9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c8088f9f0953b004e015c2a707db89dae3597ced))
* add missing sibling_limit/overflow fields to initializers ([b25f4a5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b25f4a5a34a007ad9a56757cd1a62ce7c9f92157))
* around_symbol returns full indexed symbol span (B2) ([3b06c2a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3b06c2a735ad3edd0ac691851a944d66797b06f9))
* batch_edit shows ROLLED BACK message on failure (B4) ([3ab8358](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3ab83587c7bbee77bd1f1cfe1b1980066630da8f))
* batch_insert no extra blank line before function (B1) ([3409548](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/34095482b8275811cb5373003e367c0b07dcfec0))
* show_line_numbers works with around_symbol and around_match (B3) ([4befe8a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4befe8a8f8e734e722088632c23ac81489bf42ce))

## [0.28.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.28.1...v0.28.2) (2026-03-15)


### Bug Fixes

* code review feedback — tests and safety fixes ([c2454ea](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c2454ea2bbd1ec0c2f2acb357bd9f158fd101584))
* review feedback — Q3 robust name extraction, Q6 UTF-8 safe truncation ([41e17d2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/41e17d23a4b4ce9e89d9fdfbceb5ba089a9880a0))

## [0.28.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.28.0...v0.28.1) (2026-03-15)


### Bug Fixes

* find_dependents resolves workspace crate paths ([418652a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/418652a42bd3e1443602cb87cd1eed2d7e4c0574))
* find_dependents resolves workspace crate paths (B4) ([f02819d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f02819db942530bcbb974fe946d05224bb953ab6)), closes [#89](https://github.com/special-place-administrator/tokenizor_agentic_mcp/issues/89)

## [0.28.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.27.3...v0.28.0) (2026-03-15)


### Features

* improve explore relevance ranking (Q1) ([5b829ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5b829cab9138885daf8479b3fb3a594bc35838d5))
* quality improvements from 3-project eval (Q1-Q6) ([8d23ff4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d23ff4749250d4ad85f12324280543fd8c5e403))
* quality improvements from eval feedback (Q3-Q6) ([79ac714](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/79ac714eb67a2d504be76a4beec7479cfe154385))

## [0.27.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.27.2...v0.27.3) (2026-03-15)


### Bug Fixes

* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([3b1cc4e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3b1cc4ed6ae8591ca8afc720158f2241cbec80de))
* delete_symbol orphaned docs, diff_symbols code_only, glob auto-prefix ([b793579](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b793579b2f1bdfdd7fb266bfa9bf7dd76590ea9e))

## [0.27.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.27.1...v0.27.2) (2026-03-15)


### Bug Fixes

* batch_rename catches path-qualified usages ([c2243ff](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c2243ff058e0ca29848272641eed8c27eec47131))
* batch_rename catches path-qualified usages via literal scan ([2824745](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/282474529928f46e3dec525470244baa3fb68873))

## [0.27.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.27.0...v0.27.1) (2026-03-15)


### Bug Fixes

* compute real line numbers for TOML symbols ([d939caa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d939caa7d3a587efa7482dc9f4790e7b102e15ac))
* compute real line numbers for TOML symbols ([7dd4697](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7dd46973f52228193cc98ccb8d01361b75eef808))

## [0.27.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.26.0...v0.27.0) (2026-03-15)


### Features

* add frontend asset parsing (HTML, CSS, SCSS) ([a91b625](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a91b625739c997da8b0a1e5f8cf41b46979aeb65))
* add Html, Css, Scss to LanguageId with extension mapping ([283c592](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/283c59214990c6f316247830c271d868be86a701))
* add tree-sitter-html, tree-sitter-css, tree-sitter-scss dependencies ([3be5ee3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3be5ee3dd9544cd9ff03e8d5a3dfe579764547bb))
* add unified edit_capability_for_language, rename check_edit_capability ([1742a17](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1742a1744842f376c611a7ab8b5cd24e7d214803))
* implement CSS symbol extractor with tests ([39719f4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/39719f4962657e824f9a82b9c8040be48af71cfd))
* implement HTML/Angular symbol extractor with tests ([4627b52](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4627b520009e2c8395f39593644499cae428fe75))
* implement SCSS symbol extractor with tests ([2112b1a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2112b1a7776db40d5d63ccd12da753fba085aaa9))
* wire HTML, CSS, SCSS extractors into parsing pipeline ([e740f94](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e740f947007acee5db43a321bb38152e1fed63cf))


### Bug Fixes

* recurse into mixin/function bodies, guard empty at-rule names ([8a8e717](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8a8e7178c079078581ac8c6e339128814e16478a))

## [0.26.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.25.0...v0.26.0) (2026-03-14)


### Features

* add .env file extractor ([44386b1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/44386b17adfb95f45372f2cd850588e1cf475304))
* add ConfigExtractor trait, EditCapability enum, key escaping ([a46f029](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a46f0299a62540e5d83c6665bae1b3125f92e955))
* add first-class config file indexing and gated edit support for JSON/TOML/YAML/Markdown/.env ([4bbac75](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4bbac7599509e62fbd4bc94551eddc277fc8d68f))
* add JSON key-path extractor ([a3485d3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a3485d3ef4af680770036df9b1f09d3f243509c3))
* add Json/Toml/Yaml/Markdown/Env to LanguageId, Key/Section to SymbolKind, is_config to FileClassification ([7ccd265](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7ccd2650cc9ae88018f15fe7968914e3bdb8aada))
* add Markdown section extractor ([cc1a128](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cc1a128ff48908ee6828476ef951058f09c15ace))
* add TOML key-path extractor ([932b977](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/932b977e402b8f5503988efeba135bf8dea06873))
* add YAML key-path extractor with serde_yml ([c5919e2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c5919e20ddc5707889e32b764bf5afdac58ed1fb))
* config file parsing — all extractors, pipeline integration, edit gating, test fixes ([bde2f3d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bde2f3d1a80bdd72cc22ed10d227963e4a651d44))
* gate edit tools by config file EditCapability ([7613d11](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7613d112cfa8c688c85d498808c4f7c740fcb9ce))
* integrate config extractors into parsing pipeline ([961c25b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/961c25b36a75ab4ea033ce5921f4368ab41a4495))
* PreToolUse hook now intercepts config files for Tokenizor suggestions ([ee88bff](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ee88bff2f5341e026cbed90b6da7a1dcaaef33eb))

## [0.25.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.24.0...v0.25.0) (2026-03-14)


### Features

* PreToolUse hook intercepts Grep/Read/Glob/Edit with Tokenizor suggestions ([1c78000](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1c780008662d9fb1c5cf1d7e573df64e04a23807))

## [0.24.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.23.2...v0.24.0) (2026-03-14)


### Features

* add tooling preference guide and challenge line to README ([8cd028c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8cd028cd320d62b75af84da1614955f630d0a07d))

## [0.23.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.23.1...v0.23.2) (2026-03-14)


### Bug Fixes

* Gemini CLI init writes correct timeout (120000ms) and trust setting ([b8616eb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8616eb1f5621872e050940b6756c9a04e707668))

## [0.23.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.23.0...v0.23.1) (2026-03-14)


### Bug Fixes

* search_symbols file count + find_references missing cross-file type refs ([8d40874](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d4087446f8f7c980f5e5abe6b16366e6cc5f697))

## [0.23.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.22.4...v0.23.0) (2026-03-14)


### Features

* get_file_content falls back to raw disk read for non-source files ([9bf8ba5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9bf8ba5cdc7811b1d24c340d78880a01510b8130))


### Bug Fixes

* find_references file count + get_repo_map full path filter ([89cc588](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/89cc5880c4c7ffb8ed2e09dd9c8bd9d7e573fa52))
* UX improvements from third review ([648218f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/648218fe4b70378133ec115dd93cd5a089b44bbc))

## [0.22.4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.22.3...v0.22.4) (2026-03-14)


### Bug Fixes

* add total hit limit to find_references ([d592e13](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d592e136df4d78c04be59c6a3e73edc7d1fad2c2))


### Performance Improvements

* incremental reverse index updates on file mutation ([e85c445](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e85c445b7d7e017a961e4de5da851a6ca0e0cd01))

## [0.22.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.22.2...v0.22.3) (2026-03-14)


### Bug Fixes

* filter explore noise from CONCEPT_MAP self-matching and generic terms ([8addfaa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8addfaaa4646144458ea1c71b1a9fa1de7728826))

## [0.22.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.22.1...v0.22.2) (2026-03-14)


### Bug Fixes

* non-blocking cold-start indexing for faster MCP discovery ([acb8743](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/acb874307fc01eaf162cf76de4cba2dc1e942ba8))

## [0.22.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.22.0...v0.22.1) (2026-03-14)


### Bug Fixes

* add 'burst' to file watching concept symbol_queries ([930d8e8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/930d8e87b053af0c1035e6518ca8b0c3ea46b1ee))

## [0.22.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.21.3...v0.22.0) (2026-03-14)


### Features

* add module-path boosting to explore (Phase 0) ([2f7dac0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2f7dac03d228beb673c16c418d78815a5e145619))
* concept+remainder merging in explore ([c6e93bd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c6e93bd04ea9035a9c3a9ab279539f9ebb8ba1d1))
* expand CONCEPT_MAP and add word-boundary matching ([f94e07d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f94e07dc29cb51f2055cf05ebea91946f02f1f1a))

## [0.21.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.21.2...v0.21.3) (2026-03-14)


### Bug Fixes

* around_line error, diff note, language-scoped warnings, dry-run ([a6a5f70](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a6a5f701fc1bfb56f7e13cb01f732383e101950a))

## [0.21.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.21.1...v0.21.2) (2026-03-14)


### Bug Fixes

* explore text search max_per_file too low for symbol injection ([4a5b67e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4a5b67edf156e27cfe21e1f8e4c0b86fea03ff9e))

## [0.21.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.21.0...v0.21.1) (2026-03-14)


### Bug Fixes

* auto-correct double-escaped regex patterns in search_text ([e98cd4d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e98cd4d6671e129b1bc775ec4dde28129931b22e))
* explore multi-term scoring with enclosing symbol injection ([5f36dab](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5f36dab73ac26247945d34f769e182e48e6bfbe6))
* follow_refs shows same-file callers and empty-result signal ([b3acea1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b3acea139692468421ed85ef655ff958baad008d))
* insert_before uses blank line separator when no doc comments ([2253f7d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2253f7da0c14cb4c6d858442af375e5d8552bebe))

## [0.21.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.6...v0.21.0) (2026-03-14)


### Features

* add doc_byte_range field to SymbolRecord ([5699030](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/569903096ad933cb1099afa0688de821aae9c6d2))
* add DocCommentSpec and scan_doc_range algorithm ([61ab5bb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/61ab5bb1ddda789f0e69a5f27181ab54a76d4e02))
* add per-language DocCommentSpec and wire into push_symbol ([5a0fff2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5a0fff26d9020c5aefe02d4a319727ffc99121ff))
* bump index snapshot version to 3 for doc_byte_range ([c002fcc](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c002fccfe02f569a28b62615433d99ea7a3be1e0))
* edit tools use doc_byte_range for splice boundaries ([cee3ff2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cee3ff29d43078448555b4479a4de399d9731b9e))
* include doc comments in symbol body extraction ([679682b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/679682b03d9a9548b8b67639c3257b6a5a63ff9c))


### Bug Fixes

* edit_within splice range + multi-line block comment detection ([4dd4ea8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4dd4ea84de658b5170d09d42c0200d3bd171ae2b))

## [0.20.6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.5...v0.20.6) (2026-03-13)


### Bug Fixes

* index purge on file delete, richer default symbol context, path-scoped implementations ([ec634b5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ec634b5aae1d7dcec5137b70757282efb9c578d2))

## [0.20.5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.4...v0.20.5) (2026-03-13)


### Bug Fixes

* index purge on file delete, richer default symbol context, path-scoped implementations ([1cb084b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1cb084b0444f89d825cae0d2d60e631e0cbd54c5))

## [0.20.4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.3...v0.20.4) (2026-03-13)


### Bug Fixes

* rem_euclid for timestamps, generic pub(...) visibility in diff_symbols ([f597c78](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f597c78fabcec1ce18b4e20665516cd8fd772537))

## [0.20.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.2...v0.20.3) (2026-03-13)


### Bug Fixes

* address review findings — OR search terms, range validation, depth 3, schema docs, insert spacing, token counter ([249f987](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/249f9876196de831e1dc5d6a34b8dd5cf284e1f1))

## [0.20.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.1...v0.20.2) (2026-03-13)


### Bug Fixes

* address review findings — diff_symbols filter, search prefix matching, impact messaging, chunk line numbers ([44298df](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/44298df66c3b742f2736fcd33151884b2118cbe6))
* find_dependents follows pub use re-export chains for Rust modules ([a48aee2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a48aee2b7180dc0b669f72b8b94f4e36b0284e0a))
* type-aware reference filtering reduces false positive warnings in replace_symbol_body ([b24dd0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b24dd0cdcb5dbed44edb077959c48972563b8845))

## [0.20.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.20.0...v0.20.1) (2026-03-13)


### Bug Fixes

* disable git2 SSH/HTTPS features to remove OpenSSL dependency ([0167936](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0167936069c2481cdbb07fb4e4bb5acdbd483131))

## [0.20.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.19.0...v0.20.0) (2026-03-13)


### Features

* add git2 library wrapper for in-process git operations ([dc5e146](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dc5e146cccc1a8f78af7916104cf9ae87c2ab375))
* replace git CLI with git2 library in tools and diff_symbols ([db3824a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/db3824aff0197cdc7408a82d71add37e6ae2b2e2))
* replace git log CLI with git2 library in temporal analysis ([f6877eb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/f6877ebc81e3d8f6a6ebc6001cdbecc29979292c))

## [0.19.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.18.0...v0.19.0) (2026-03-13)


### Features

* update README for 24 tools, add CLAUDE.md, rename prompts to tokenizor-* prefix ([949738f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/949738f88ce47fa3ede4a5e919127787678017bb))

## [0.18.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.17.1...v0.18.0) (2026-03-13)


### Features

* add depth parameter to explore for enriched symbol analysis ([a81fdad](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a81fdad79b2ff06b96e9e841041e617675182652))
* add routing hint, code_only flag, and update stale tool descriptions ([b941eff](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b941effdef0b608b01bb6394330f65459107f403))

## [0.17.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.17.0...v0.17.1) (2026-03-13)


### Bug Fixes

* resolve all actionable issues from external review ([3e11288](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3e112880a6802182bf59c5159373fc3ab636a240))

## [0.17.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.16.2...v0.17.0) (2026-03-13)


### Features

* **edit:** Tier 2 batch tools — batch_edit, batch_rename, batch_insert ([859271d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/859271d7a1b7f2954335002e2aa3c8588cae2109))

## [0.16.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.16.1...v0.16.2) (2026-03-13)


### Bug Fixes

* auto-indent replace_symbol_body + update edit tool descriptions ([a113c7b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a113c7b5444656cb3935a22defdede73ba538c2f))

## [0.16.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.16.0...v0.16.1) (2026-03-13)


### Bug Fixes

* auto-indent replace_symbol_body + update edit tool descriptions ([d1d22ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d1d22ca3b26b59fcd107ad62e294abe10a0de678))
* auto-indent replace_symbol_body + update edit tool descriptions ([39f8fec](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/39f8fec7b95ccdb2d383c9089f6d267f4a86b69c))

## [0.16.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.15.0...v0.16.0) (2026-03-13)


### Features

* add symbol-addressed edit tools (Tier 1) ([3dec094](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3dec094cb89417e7b7208caea808b151f109dbf1))
* rewrite tool descriptions with NOT-for redirects, fix verbosity polish ([466f207](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/466f207b1959df08f30d04d7e6eb3338938340d0))
* symbol-addressed edit tools + description redirects + verbosity fixes ([ba9e587](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ba9e587d2c38e08b9e0881ebfc02bbf1c18db283))

## [0.15.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.14.2...v0.15.0) (2026-03-13)


### Features

* add token savings — verbosity param, sections filter, compact modes ([0184917](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/018491738218681c5d6c85c6fee267ca321a8aaa))

## [0.14.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.14.1...v0.14.2) (2026-03-13)


### Bug Fixes

* simplify release pipeline — let PAT-triggered run handle release ([508fbc2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/508fbc247d7b54f79cce14526b8e755fca7acdba))

## [0.14.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.14.0...v0.14.1) (2026-03-13)


### Bug Fixes

* retry PR label lookup with 60s timeout for auto-merge ([1ee07e6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1ee07e61c5bae54a6bdeac8949bcd4d7b9d07b0c))

## [0.14.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.4...v0.14.0) (2026-03-13)


### Features

* `tokenizor-mcp init` now registers MCP server + bumps to v0.2.1 ([b2126ed](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2126eda812237e2bc3dc03b7ef6d2f961c94735))
* **01-01:** rewrite domain types, error.rs, lib.rs — establish v2 module skeleton ([3aa5d92](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3aa5d92570bd065f2775a08e713e513dffd284d4))
* **01-02:** implement LiveIndex store, discovery, and query modules ([0410419](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/04104198f7e9945b396952c631b5684767fc60f1))
* **01-03:** integration tests + fix retrieval_conformance.rs for v2 ([4a3b93e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4a3b93ea1105c90e9c8aee151336ab9632940d07))
* **01-03:** minimal v2 main.rs entry point ([67dc213](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/67dc213e73f86c41a29c57b5b77018fb74713bc1))
* **02-01:** add src/protocol/format.rs with all formatter functions ([368e2c0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/368e2c08dc3e02fcf7271f304280cd51ac9a6ea7))
* **02-01:** LiveIndex empty/reload/SystemTime, IndexState::Empty, SymbolKind Display ([035277b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/035277b054cd89c40cf978f7269979796274ba1c))
* **02-02:** all 10 MCP tool handlers + input param structs ([aded9b1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/aded9b1049e2637983d72c235b7e8ae67a2e6d24))
* **02-02:** TokenizorServer struct + ServerHandler impl + pub mod protocol ([8325190](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/832519015233bd5fa824a18967debc4f8b1602be))
* **02-03:** rewrite main.rs as persistent v2 MCP server ([8f38388](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8f38388fedceb5aa4c4ca03d6e85b93a0f0a1bb9))
* **03-01:** extended HealthStats with watcher fields + dynamic health_report ([50e25a1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/50e25a19a6e5b040973fd83abed4cf4b79d55cd9))
* **03-01:** LiveIndex mutation methods + watcher type stubs ([52f7cf2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/52f7cf23f8bd14550d55390cb59ea5ab2cda1c65))
* **03-02:** implement watcher core — event processing, path normalization, lifecycle ([7028a3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7028a3b467511b99ae345878e656d30c886e360f))
* **03-03:** integration tests + fix blocking recv in run_watcher ([88229cd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/88229cd5324c0c991653bf43234af156e10dee0b))
* **03-03:** wire file watcher into main.rs, TokenizorServer, and health/index_folder tools ([ddce97d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ddce97d5f2550d0840d0a360cbb34ac4e53921b3))
* **04-01:** implement tree-sitter xref extraction for all 6 languages ([12f1904](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/12f1904248ec92603fee24efc9b926eb7b3e1864))
* **04-02:** cross-reference query methods with filtering and alias resolution ([5bea7f9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5bea7f9e167024bd243f7f7ab30dd375520b7571))
* **04-02:** verify watcher xref pipeline and add XREF-08 incremental update test ([21ae69a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/21ae69a11b35e047783a15b9eb91ca608e7686be))
* **04-03:** add find_references, find_dependents, get_context_bundle tool handlers ([fe78101](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fe781012ef74ddcfe5d4f143776118df67b58a14))
* **05-01:** add sidecar module structure, port/PID file management, and new deps ([47f1606](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/47f160632efd51d85e532e20b4c6d80af89bf65f))
* **05-01:** implement sidecar router, 5 endpoint handlers, and spawn_sidecar ([dd6a61e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd6a61ef700b506e238a0ceeaa160144a976a605))
* **05-02:** add CLI types and hook subcommand with fail-open JSON output ([e4d6715](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e4d6715e3c195a716735b20a27860091b3ce720f))
* **05-02:** add tokenizor init command — idempotent settings.json merge ([5f6733e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5f6733e800157ece7d22f27f2354e0caf9a6b5cf))
* **05-03:** integration tests for sidecar, hooks, and init ([32c6d00](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32c6d00da8b105b920e837108a3d39b55ea04410))
* **05-03:** wire CLI dispatch and sidecar spawn into main.rs ([d9e4d83](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d9e4d83e2794d2c8912955ac650d740933f2c879))
* **06-01:** add TokenStats, SidecarState, build_with_budget; update router and server ([72bc6c3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/72bc6c3dcc994b53ec032f33924b99ac784ca7e9))
* **06-01:** enrich all sidecar handlers with formatted text, budget enforcement, token tracking ([32d67dc](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32d67dc273eda3651c9783571606c378dbdcd1f9))
* **06-02:** single stdin-routed PostToolUse entry with auto-migration ([c7502b2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c7502b2247ffb4ea3aab7f12c8a81b458c1c81af))
* **06-02:** stdin JSON routing, Write subcommand, abs-to-rel path conversion ([bb2d083](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb2d0830341f968d33d63e0d0ae6580fbfdc1d36))
* **06-03:** wire token savings from sidecar into MCP health tool ([b8827f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8827f852646346f6a659bd4c8c242d3c5cb3181))
* **07-01:** add C/C++ xref queries and grammar integration tests ([cbe36ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cbe36caf706bc76a35effaf764abb78eadaaa809))
* **07-02:** create TrigramIndex module and integrate into LiveIndex ([7ee7e94](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7ee7e944a5b77c4f361d5d52ff4cfff5b1f265e1))
* **07-02:** wire trigram search, scored symbol ranking, and file tree tool ([3abdb3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3abdb3b55c5ab0c35902c6cae208a2d2e71a48bb))
* **07-03:** add persistence module with snapshot types and serialize/deserialize ([2fe7168](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2fe716875dda0aea9579c0e9c77686a3695afd40))
* **07-03:** wire persistence into main.rs with shutdown hook and startup load path ([4c07981](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4c079817877fd3f4bd43d83389bb809f1e82964c))
* **07:** add symbol extraction for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([e33decb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e33decb122f3d48838849e7da32e4ea5da336401))
* **07:** add xref queries for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([2a7f2c4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2a7f2c4c67652ae4664919099b93a37f322015f0))
* **07:** upgrade tree-sitter to 0.26 and enable PHP, Swift, Perl parsing ([a83e536](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a83e5365c1b9bf39ad2b60eafcaf76a774255a81))
* add around-line file content reads ([413abe7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/413abe7c867edfd26752daa4bf6e7f8b0795f886))
* add around-match file content reads and refresh README ([9406955](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/94069558f45b8b2a0ea57e45410a58a8e96940a8))
* add deterministic file content chunking ([731bebd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/731bebd00c8042476eb180529e984e78e57b7423))
* add exact-selector context navigation ([36243df](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36243dffe29b34b91bb5183c4bd0f237d9c145e2))
* add exact-selector symbol context lookup ([fc28210](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fc2821033c082fc8c002785501652289b112d01b))
* add explore tool for concept-based codebase exploration ([e7a2364](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e7a23642730253f6e4df005eecf135460db7e6cf))
* add fully automated deployment scripts and quick start ([b2417a2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2417a2194803774d506d826778001b1f56570d4))
* add Gemini CLI support (init, MCP registration, auto-allow) ([b2429b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2429b62b4e2032521ffbb2f2d1cb8eea0615883))
* add get_co_changes, diff_symbols tools + UX improvements ([c00c2f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c00c2f84395a6a7f680cbc0d32838356ce106dfc))
* add import/export summaries to get_file_context ([62f38a9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/62f38a949bbf4009e0d94b68a7566a20c0855b37))
* add Mermaid and DOT graph output for find_dependents ([127fed2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/127fed25b4ff07ce48f82fabc03750c53d98f58b))
* add prebuilt binary distribution via npm and GitHub Releases ([ddbc5db](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ddbc5db8ace06f955dda1ef9626218314fbabae0))
* add recursive type resolution to get_context_bundle ([d2caaca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2caaca004c764d7593f69a2dce25ddf542e9324))
* add scoped search_symbols filters ([3ec4dc7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3ec4dc77d3de2f5ee764bb0438916f265824e78e))
* add trait/interface implementation mapping with find_implementations tool ([4be3610](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4be36105fa605fca6b3e33e67bdba6fa79258ede))
* auto-allow all Tokenizor tools during init (no more permission prompts) ([948f360](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/948f3605da9279ae67d76f330b737009a526abf2))
* complete Epic 1 — Reliable Local Setup and Workspace Identity ([b539b76](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b539b7692aa367e50dfbd6760ecc63af1c2148a3))
* complete Phase B - implement trace_symbol tool ([3869941](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3869941d08a243f65b9ffc18673caac63b22f410))
* complete Phase C - implement inspect_match and locality ranking ([8d0dff9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d0dff9b781fddc03bd7fbaa9c04058042b142b1))
* complete scoped search_text upgrades ([4025717](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/402571778aced351d1cb9106204a917dfd6667dc))
* **control-plane:** land story 4.3 with review fixes ([b84ce37](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b84ce37c5ea156731a46c45befbc5ec208bcbfa7))
* daemon resilience and zero-touch install ([0d3bd80](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0d3bd80614c720233ffb188ef1827500e83dbbc6))
* expand file content read ergonomics ([16b3a09](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/16b3a0965d9f05f248cbee4b2fdeff3d9117b57d))
* expand prompt context exact hint routing ([b7b0c42](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b7b0c427b562c0e07ea0b661679334d279ebe955))
* expand prompt context line hint parsing ([ad0c162](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ad0c162c74ce4c4fb6b7c2934df027df86455352))
* expand tokenizor shared MCP capabilities ([459bbb5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/459bbb59d3de7702b66649e67e3ad325ab79b021))
* extend prompt context exact alias routing ([10294c5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10294c57e4f97f9c5f8cabc85ac5b2b61fbc620e))
* extend prompt context module alias routing ([1084256](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10842566f40f7463345aabdff10e93cd5037aeb4))
* extend prompt context slash hint routing ([242ef24](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/242ef240dc6a9aecc15a064087023cdbce45b7e7))
* git temporal intelligence with churn, ownership, and co-change analysis ([d4cd579](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d4cd579e6f842db7cc2aadd14700d42dd997d411))
* implement Epic 3 — trusted code discovery and verified retrieval ([a06e67a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a06e67a094a1060435853eae6b33f07ce09b9375))
* implement Story 2.1 — durable run identity ([0c54f13](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0c54f13b7d0caf1fba907a18e581bbfd1839a6d0))
* implement Story 2.10 — invalidate indexed state for untrusted use ([d2555cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2555cf794b2cd037fdce257cd7a50e03f800e52))
* implement Story 2.11 — reject conflicting idempotent replays ([68537e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/68537e44264775e57bbfe03bc67b286b4f0f6610))
* implement Story 2.2 — quality-focus language indexing ([b6f6f6f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b6f6f6f7a11ce63fd749e7e8d4e2f5d1821bb006))
* implement Story 2.3 — persist file/symbol metadata ([a1c7342](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a1c7342af18b21ae75de0b9c1bf7266d9ddbbb15))
* implement Story 2.4 — broader language onboarding pattern ([70cc654](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/70cc654c7975ef035875c6708ab09b1bd941ac57))
* implement Story 2.5 — run status and health inspection ([9e07b71](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e07b71d2e8f1fff0644eca472fcb0d41d1a3232))
* implement Story 2.7 — cancel an active indexing run safely ([9ba4a5d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9ba4a5d2f2856a7236428e26cd2dfa924a257f0f))
* implement Story 2.8 — checkpoint long-running indexing work ([1055568](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/105556817d2cd10078dd0eab0d9c794dac674cdd))
* implement Story 2.9 — re-index managed repository deterministically ([7696d7c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7696d7cd0b32bc8eac953a37af55db2b768f9d4c))
* improve prompt context symbol disambiguation ([144377d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/144377daef8adeb5a7e80c87b441964ba96cd495))
* module-path-aware find_dependents for lib.rs, mod.rs, __init__.py, index.js ([ea2655c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ea2655c4c68f17bf42e914da5aa57e86c456f468))
* search_files changed_with parameter — find co-changing files via git temporal coupling ([95b5901](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/95b5901524cce5073d318c151d8c02c88201e621))
* search_text follow_refs — inline callers of enclosing symbol ([ae64a9a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ae64a9a12284953e7ece3e8e0da39389e533e398))
* search_text group_by parameter — deduplicate by symbol or filter imports ([8fcf8a7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8fcf8a7fc36685ee8688b02be08461f08971be97))
* start Phase B - implement trace_symbol tool and add handoff summary ([dd3af33](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd3af336acb024ada2b76d51d5ea35428e751671))
* **story-4.6:** complete IntegrityEvent instrumentation and land story ([7e8057b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7e8057bfbe9f541467afba8435b9e74ccddc8573))
* **story-4.7:** unified action classification with review fixes ([78dad2a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/78dad2a0398f028265233235556dbe00f52c919e))
* suppress noisy search_symbols results by default ([3255928](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32559289e0242d755af06896f0aa4d902a002a55))
* symbol-aware context in search_text — show enclosing symbol for each match ([73ee432](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/73ee43249263c5338236c24d00b7c645da9e4d4a))
* tokenizor v2 rewrite — in-memory LiveIndex with parasitic hook integration ([3cbc63c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3cbc63c350f2cafd8b77601db3235bdbba779271))


### Bug Fixes

* **02-01:** mark doctest as text to fix compilation ([92e5998](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/92e5998d6f15419c5cdde7da370abb3ea147153f))
* **05-02:** re-add pub mod cli to lib.rs after plan 01 metadata commit overwrote it ([79cb27e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/79cb27e01e3c26e84fa88c7c7a34e588608071fd))
* **06-02:** make hook helper fns pub and fix run_hook signature in integration tests ([36d45de](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36d45defe68af2996502c94b5fa7008a5a2222ef))
* **07:** use box-drawing chars in tier headers per CONTEXT.md ([bb22570](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb225701a514c477c376e7e2bc0763c667381ed6))
* add actions:write permission for workflow re-trigger ([83aeef5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/83aeef5cc953782045dd6620c75d0c48bde461d4))
* address code review findings for Story 2.2 ([27fd538](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/27fd538bb9604e7f3c8f8b1553743ae0b581b58b))
* address code review findings for Story 2.3 ([a6c70c0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a6c70c01b5dfe8416d998ffcfafa3f8199e1e1c6))
* address code review findings for Story 2.4 ([2383dd7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2383dd74ad9bb8659dc38a169116c5a8a4df5af4))
* address code review findings for Story 2.6 ([8609d1c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8609d1caacce4c4e026c52ec5fc3ec7cebbec9c7))
* address code review findings for Story 2.8 ([d105124](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d105124c74aa8a05ef2bedfe266cd76785c29cc5))
* auto-merge release-please PRs for continuous deployment ([e58ce57](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e58ce57ce77d56e66219a6e7663459f72bfb8dc2))
* auto-register repo on index_folder and clear invalidation on reindex ([49ad871](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/49ad871bb43fde64ceddb9e5f77a8e2ab4ddc78b))
* **ci:** drop macOS builds, keep Windows and Linux only ([d265d65](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d265d65f1cfe89d9e7e29ce12f45fe7257e3cddb))
* **ci:** use macos-latest for x64 cross-compile (macos-13 deprecated) ([bb9d65c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb9d65c8b9ebbdb8f65920830dc97fd70771f29f))
* expose typed parameter schemas for all 18 MCP tools ([c8369d8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c8369d8c138bb18e5c7b546cb4d5d296087178a6))
* gate Windows-specific path tests with #[cfg(windows)] ([6d9f0b9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6d9f0b9a727a215bf499bd0329c0d107823f6a5c))
* gate Windows-specific path tests with #[cfg(windows)] ([dcc963b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dcc963b4d36c2048b7ad82b9dce320bbfb22b50e))
* gate Windows-specific path tests with #[cfg(windows)] ([df57ac0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/df57ac0a8c98564c80896b91ac04c10bd18a9d7a))
* gate Windows-specific path tests with #[cfg(windows)] ([1710de0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1710de0ca89d0bffe124994d91b2b714ea389312))
* gate Windows-specific path tests with #[cfg(windows)] ([32f2964](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32f2964da4f41897ac4988393968fde7505662f5))
* gate Windows-specific path tests with #[cfg(windows)] ([a1cae52](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a1cae520cbf8809e2d4c97d6f21307012c554509))
* handle locked binary on Windows during npm update ([b57aa8c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b57aa8cf25fbcaf8e3e6e8d6625f7f989418c43c))
* hook detection for tokenizor-mcp binary name and npx cache warning ([d0ad70a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d0ad70a52297826c2187499d62e030ca55b4ee6d))
* improve context_bundle output quality and symbol_context guidance ([45eb6e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/45eb6e412bd1abf76d3430f761def6f319a3384d))
* improve file watcher burst handling and evict idle trackers ([442d240](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/442d24031d71607e4d84d47e32d70a65f2ec5a4c))
* improve search ranking, symbol diff accuracy, test filtering, and error messages ([13ebb36](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/13ebb3637075c2e1bf6f187b5f07722c2cd9ecec))
* include tool description rewrites in release ([afbad0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/afbad0cfeb117ff29a9b77d2673531ceff0941cb))
* install binary to ~/.tokenizor/bin/ to avoid Windows file-lock ([da9c40a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da9c40a2ccd4987d061e574dbcc5f18f13a2679c))
* keep each Where-Object filter as a single-line expression. ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))
* lenient parameter deserialization for MCP clients that stringify values ([5d613d4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5d613d4b9e06e296b3f3ce9bbd4d133a0a94b726))
* make installer tests host-agnostic ([6eb0bfa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6eb0bfada2f70bc86a4974a1f812bc7f315a63d0))
* make npm updates replace locked windows binaries ([da3f24d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da3f24d808aee39512545e29db989b7d4bb2f428))
* npm wrapper and release pipeline for v2 ([ab794da](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ab794dae0105cd8bc49466bcab04d27c6fc38457))
* PowerShell -and operator parsing in install scripts ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))
* prevent analyze_file_impact from destroying index, fix close_ses… ([5af91cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5af91cf906781b8c902b7ac96637a066a572d915))
* prevent analyze_file_impact from destroying index, fix close_session deadlock ([9e29787](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e29787709f064538ff379c234172c11e43d69cd))
* prevent analyze_file_impact index corruption and close_session deadlock ([a7e6ff3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a7e6ff393713c9f3be680f62338ebecab71a6731))
* re-trigger workflow after auto-merge for full automation ([2e73e1d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2e73e1dbcf110625c08cbd258a9b852590956b2f))
* refuse to auto-index home dirs, drive roots, and system paths ([d459bbf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d459bbf7bac24f9747bb23f7370f62efd328d664))
* **release:** document conventional commit requirement ([1867bce](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1867bce6a312079e6edd5b8ccf16fc0b43f4089d))
* replace deprecated macos-13 runner with macos-latest ([4ab6e72](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4ab6e72f1bfab0f2bdefdf915e6ff3c5d0e472ef))
* resolve 6 confirmed bugs across watcher, daemon, trigram, discovery ([a50b723](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a50b7232519cd640aa9140f1e0e6c032fac43eeb))
* robust auto-merge for release-please PRs ([940b9b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/940b9b6bab4bfa22bf4455cc6474dea300f213a5))
* simplify deployment to standard MCP lifecycle ([aebf7f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/aebf7f85703a9010793ac4030d16dde98b7167c2))
* single-run release pipeline — no second trigger needed ([973428e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/973428e730d1ea4d42e525597f0fa7048ca57500))
* split-brain after index_folder, empty search_symbols guard, inspect_match bounds check ([00cf4be](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/00cf4be964f1314fa7930d68e31dac2327dced27))
* **story-4.6:** code review fixes for operational history ([b8c9ab6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8c9ab6bda749e1e0428c4ba5706db807ea31a9d))
* version-aware npm update + --version flag ([b935bb0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b935bb0ea7cc52916abace2435873f19dfe4c01d))

## [0.13.4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.3...v0.13.4) (2026-03-13)


### Bug Fixes

* re-trigger workflow after auto-merge for full automation ([2e73e1d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2e73e1dbcf110625c08cbd258a9b852590956b2f))

## [0.13.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.2...v0.13.3) (2026-03-13)


### Bug Fixes

* robust auto-merge for release-please PRs ([940b9b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/940b9b6bab4bfa22bf4455cc6474dea300f213a5))

## [0.13.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.1...v0.13.2) (2026-03-13)


### Bug Fixes

* auto-merge release-please PRs for continuous deployment ([e58ce57](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e58ce57ce77d56e66219a6e7663459f72bfb8dc2))

## [0.13.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.13.0...v0.13.1) (2026-03-13)


### Bug Fixes

* include tool description rewrites in release ([afbad0c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/afbad0cfeb117ff29a9b77d2673531ceff0941cb))

## [0.13.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.12.0...v0.13.0) (2026-03-13)


### Features

* add get_co_changes, diff_symbols tools + UX improvements ([c00c2f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c00c2f84395a6a7f680cbc0d32838356ce106dfc))


### Bug Fixes

* lenient parameter deserialization for MCP clients that stringify values ([5d613d4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5d613d4b9e06e296b3f3ce9bbd4d133a0a94b726))

## [0.12.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.4...v0.12.0) (2026-03-13)


### Features

* add explore tool for concept-based codebase exploration ([e7a2364](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e7a23642730253f6e4df005eecf135460db7e6cf))
* add Gemini CLI support (init, MCP registration, auto-allow) ([b2429b6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2429b62b4e2032521ffbb2f2d1cb8eea0615883))
* auto-allow all Tokenizor tools during init (no more permission prompts) ([948f360](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/948f3605da9279ae67d76f330b737009a526abf2))
* search_files changed_with parameter — find co-changing files via git temporal coupling ([95b5901](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/95b5901524cce5073d318c151d8c02c88201e621))
* search_text follow_refs — inline callers of enclosing symbol ([ae64a9a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ae64a9a12284953e7ece3e8e0da39389e533e398))
* search_text group_by parameter — deduplicate by symbol or filter imports ([8fcf8a7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8fcf8a7fc36685ee8688b02be08461f08971be97))
* symbol-aware context in search_text — show enclosing symbol for each match ([73ee432](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/73ee43249263c5338236c24d00b7c645da9e4d4a))


### Bug Fixes

* gate Windows-specific path tests with #[cfg(windows)] ([6d9f0b9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6d9f0b9a727a215bf499bd0329c0d107823f6a5c))
* split-brain after index_folder, empty search_symbols guard, inspect_match bounds check ([00cf4be](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/00cf4be964f1314fa7930d68e31dac2327dced27))

## [0.11.4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.3...v0.11.4) (2026-03-13)


### Bug Fixes

* gate Windows-specific path tests with #[cfg(windows)] ([dcc963b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dcc963b4d36c2048b7ad82b9dce320bbfb22b50e))
* gate Windows-specific path tests with #[cfg(windows)] ([df57ac0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/df57ac0a8c98564c80896b91ac04c10bd18a9d7a))

## [0.11.3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.2...v0.11.3) (2026-03-13)


### Bug Fixes

* gate Windows-specific path tests with #[cfg(windows)] ([1710de0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1710de0ca89d0bffe124994d91b2b714ea389312))
* gate Windows-specific path tests with #[cfg(windows)] ([32f2964](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32f2964da4f41897ac4988393968fde7505662f5))

## [0.11.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.1...v0.11.2) (2026-03-13)


### Bug Fixes

* prevent analyze_file_impact index corruption and close_session deadlock ([a7e6ff3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a7e6ff393713c9f3be680f62338ebecab71a6731))

## [0.11.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.11.0...v0.11.1) (2026-03-13)


### Bug Fixes

* prevent analyze_file_impact from destroying index, fix close_ses… ([5af91cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5af91cf906781b8c902b7ac96637a066a572d915))
* prevent analyze_file_impact from destroying index, fix close_session deadlock ([9e29787](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e29787709f064538ff379c234172c11e43d69cd))

## [0.11.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.10.0...v0.11.0) (2026-03-13)


### Features

* complete Phase B - implement trace_symbol tool ([3869941](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3869941d08a243f65b9ffc18673caac63b22f410))
* complete Phase C - implement inspect_match and locality ranking ([8d0dff9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8d0dff9b781fddc03bd7fbaa9c04058042b142b1))
* start Phase B - implement trace_symbol tool and add handoff summary ([dd3af33](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd3af336acb024ada2b76d51d5ea35428e751671))


### Bug Fixes

* improve context_bundle output quality and symbol_context guidance ([45eb6e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/45eb6e412bd1abf76d3430f761def6f319a3384d))
* improve file watcher burst handling and evict idle trackers ([442d240](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/442d24031d71607e4d84d47e32d70a65f2ec5a4c))
* resolve 6 confirmed bugs across watcher, daemon, trigram, discovery ([a50b723](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a50b7232519cd640aa9140f1e0e6c032fac43eeb))

## [0.10.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.9.1...v0.10.0) (2026-03-12)


### Features

* git temporal intelligence with churn, ownership, and co-change analysis ([d4cd579](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d4cd579e6f842db7cc2aadd14700d42dd997d411))

## [0.9.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.9.0...v0.9.1) (2026-03-12)


### Bug Fixes

* keep each Where-Object filter as a single-line expression. ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))
* PowerShell -and operator parsing in install scripts ([e29bace](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e29bace4e0dfa80f2a7daa6cabf3ef0936436c26))

## [0.9.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.8.0...v0.9.0) (2026-03-12)


### Features

* daemon resilience and zero-touch install ([0d3bd80](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0d3bd80614c720233ffb188ef1827500e83dbbc6))

## [0.8.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.7.0...v0.8.0) (2026-03-12)


### Features

* add trait/interface implementation mapping with find_implementations tool ([4be3610](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4be36105fa605fca6b3e33e67bdba6fa79258ede))

## [0.7.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.6.0...v0.7.0) (2026-03-12)


### Features

* add import/export summaries to get_file_context ([62f38a9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/62f38a949bbf4009e0d94b68a7566a20c0855b37))

## [0.6.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.5.0...v0.6.0) (2026-03-12)


### Features

* add recursive type resolution to get_context_bundle ([d2caaca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2caaca004c764d7593f69a2dce25ddf542e9324))

## [0.5.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.4.2...v0.5.0) (2026-03-12)


### Features

* add Mermaid and DOT graph output for find_dependents ([127fed2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/127fed25b4ff07ce48f82fabc03750c53d98f58b))

## [0.4.2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.4.1...v0.4.2) (2026-03-12)


### Bug Fixes

* improve search ranking, symbol diff accuracy, test filtering, and error messages ([13ebb36](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/13ebb3637075c2e1bf6f187b5f07722c2cd9ecec))

## [0.4.1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/v0.4.0...v0.4.1) (2026-03-12)


### Bug Fixes

* **release:** document conventional commit requirement ([1867bce](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/1867bce6a312079e6edd5b8ccf16fc0b43f4089d))

## [0.4.0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/compare/tokenizor_agentic_mcp-v0.3.12...tokenizor_agentic_mcp-v0.4.0) (2026-03-12)


### Features

* `tokenizor-mcp init` now registers MCP server + bumps to v0.2.1 ([b2126ed](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2126eda812237e2bc3dc03b7ef6d2f961c94735))
* **01-01:** rewrite domain types, error.rs, lib.rs — establish v2 module skeleton ([3aa5d92](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3aa5d92570bd065f2775a08e713e513dffd284d4))
* **01-02:** implement LiveIndex store, discovery, and query modules ([0410419](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/04104198f7e9945b396952c631b5684767fc60f1))
* **01-03:** integration tests + fix retrieval_conformance.rs for v2 ([4a3b93e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4a3b93ea1105c90e9c8aee151336ab9632940d07))
* **01-03:** minimal v2 main.rs entry point ([67dc213](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/67dc213e73f86c41a29c57b5b77018fb74713bc1))
* **02-01:** add src/protocol/format.rs with all formatter functions ([368e2c0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/368e2c08dc3e02fcf7271f304280cd51ac9a6ea7))
* **02-01:** LiveIndex empty/reload/SystemTime, IndexState::Empty, SymbolKind Display ([035277b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/035277b054cd89c40cf978f7269979796274ba1c))
* **02-02:** all 10 MCP tool handlers + input param structs ([aded9b1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/aded9b1049e2637983d72c235b7e8ae67a2e6d24))
* **02-02:** TokenizorServer struct + ServerHandler impl + pub mod protocol ([8325190](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/832519015233bd5fa824a18967debc4f8b1602be))
* **02-03:** rewrite main.rs as persistent v2 MCP server ([8f38388](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8f38388fedceb5aa4c4ca03d6e85b93a0f0a1bb9))
* **03-01:** extended HealthStats with watcher fields + dynamic health_report ([50e25a1](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/50e25a19a6e5b040973fd83abed4cf4b79d55cd9))
* **03-01:** LiveIndex mutation methods + watcher type stubs ([52f7cf2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/52f7cf23f8bd14550d55390cb59ea5ab2cda1c65))
* **03-02:** implement watcher core — event processing, path normalization, lifecycle ([7028a3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7028a3b467511b99ae345878e656d30c886e360f))
* **03-03:** integration tests + fix blocking recv in run_watcher ([88229cd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/88229cd5324c0c991653bf43234af156e10dee0b))
* **03-03:** wire file watcher into main.rs, TokenizorServer, and health/index_folder tools ([ddce97d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ddce97d5f2550d0840d0a360cbb34ac4e53921b3))
* **04-01:** implement tree-sitter xref extraction for all 6 languages ([12f1904](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/12f1904248ec92603fee24efc9b926eb7b3e1864))
* **04-02:** cross-reference query methods with filtering and alias resolution ([5bea7f9](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5bea7f9e167024bd243f7f7ab30dd375520b7571))
* **04-02:** verify watcher xref pipeline and add XREF-08 incremental update test ([21ae69a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/21ae69a11b35e047783a15b9eb91ca608e7686be))
* **04-03:** add find_references, find_dependents, get_context_bundle tool handlers ([fe78101](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fe781012ef74ddcfe5d4f143776118df67b58a14))
* **05-01:** add sidecar module structure, port/PID file management, and new deps ([47f1606](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/47f160632efd51d85e532e20b4c6d80af89bf65f))
* **05-01:** implement sidecar router, 5 endpoint handlers, and spawn_sidecar ([dd6a61e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/dd6a61ef700b506e238a0ceeaa160144a976a605))
* **05-02:** add CLI types and hook subcommand with fail-open JSON output ([e4d6715](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e4d6715e3c195a716735b20a27860091b3ce720f))
* **05-02:** add tokenizor init command — idempotent settings.json merge ([5f6733e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/5f6733e800157ece7d22f27f2354e0caf9a6b5cf))
* **05-03:** integration tests for sidecar, hooks, and init ([32c6d00](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32c6d00da8b105b920e837108a3d39b55ea04410))
* **05-03:** wire CLI dispatch and sidecar spawn into main.rs ([d9e4d83](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d9e4d83e2794d2c8912955ac650d740933f2c879))
* **06-01:** add TokenStats, SidecarState, build_with_budget; update router and server ([72bc6c3](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/72bc6c3dcc994b53ec032f33924b99ac784ca7e9))
* **06-01:** enrich all sidecar handlers with formatted text, budget enforcement, token tracking ([32d67dc](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32d67dc273eda3651c9783571606c378dbdcd1f9))
* **06-02:** single stdin-routed PostToolUse entry with auto-migration ([c7502b2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c7502b2247ffb4ea3aab7f12c8a81b458c1c81af))
* **06-02:** stdin JSON routing, Write subcommand, abs-to-rel path conversion ([bb2d083](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb2d0830341f968d33d63e0d0ae6580fbfdc1d36))
* **06-03:** wire token savings from sidecar into MCP health tool ([b8827f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8827f852646346f6a659bd4c8c242d3c5cb3181))
* **07-01:** add C/C++ xref queries and grammar integration tests ([cbe36ca](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/cbe36caf706bc76a35effaf764abb78eadaaa809))
* **07-02:** create TrigramIndex module and integrate into LiveIndex ([7ee7e94](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7ee7e944a5b77c4f361d5d52ff4cfff5b1f265e1))
* **07-02:** wire trigram search, scored symbol ranking, and file tree tool ([3abdb3b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3abdb3b55c5ab0c35902c6cae208a2d2e71a48bb))
* **07-03:** add persistence module with snapshot types and serialize/deserialize ([2fe7168](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2fe716875dda0aea9579c0e9c77686a3695afd40))
* **07-03:** wire persistence into main.rs with shutdown hook and startup load path ([4c07981](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4c079817877fd3f4bd43d83389bb809f1e82964c))
* **07:** add symbol extraction for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([e33decb](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/e33decb122f3d48838849e7da32e4ea5da336401))
* **07:** add xref queries for C#, Ruby, PHP, Swift, Kotlin, Dart, Perl, Elixir ([2a7f2c4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2a7f2c4c67652ae4664919099b93a37f322015f0))
* **07:** upgrade tree-sitter to 0.26 and enable PHP, Swift, Perl parsing ([a83e536](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a83e5365c1b9bf39ad2b60eafcaf76a774255a81))
* add around-line file content reads ([413abe7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/413abe7c867edfd26752daa4bf6e7f8b0795f886))
* add around-match file content reads and refresh README ([9406955](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/94069558f45b8b2a0ea57e45410a58a8e96940a8))
* add deterministic file content chunking ([731bebd](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/731bebd00c8042476eb180529e984e78e57b7423))
* add exact-selector context navigation ([36243df](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36243dffe29b34b91bb5183c4bd0f237d9c145e2))
* add exact-selector symbol context lookup ([fc28210](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/fc2821033c082fc8c002785501652289b112d01b))
* add fully automated deployment scripts and quick start ([b2417a2](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b2417a2194803774d506d826778001b1f56570d4))
* add prebuilt binary distribution via npm and GitHub Releases ([ddbc5db](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ddbc5db8ace06f955dda1ef9626218314fbabae0))
* add scoped search_symbols filters ([3ec4dc7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3ec4dc77d3de2f5ee764bb0438916f265824e78e))
* complete Epic 1 — Reliable Local Setup and Workspace Identity ([b539b76](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b539b7692aa367e50dfbd6760ecc63af1c2148a3))
* complete scoped search_text upgrades ([4025717](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/402571778aced351d1cb9106204a917dfd6667dc))
* **control-plane:** land story 4.3 with review fixes ([b84ce37](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b84ce37c5ea156731a46c45befbc5ec208bcbfa7))
* expand file content read ergonomics ([16b3a09](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/16b3a0965d9f05f248cbee4b2fdeff3d9117b57d))
* expand prompt context exact hint routing ([b7b0c42](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b7b0c427b562c0e07ea0b661679334d279ebe955))
* expand prompt context line hint parsing ([ad0c162](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ad0c162c74ce4c4fb6b7c2934df027df86455352))
* expand tokenizor shared MCP capabilities ([459bbb5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/459bbb59d3de7702b66649e67e3ad325ab79b021))
* extend prompt context exact alias routing ([10294c5](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10294c57e4f97f9c5f8cabc85ac5b2b61fbc620e))
* extend prompt context module alias routing ([1084256](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/10842566f40f7463345aabdff10e93cd5037aeb4))
* extend prompt context slash hint routing ([242ef24](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/242ef240dc6a9aecc15a064087023cdbce45b7e7))
* implement Epic 3 — trusted code discovery and verified retrieval ([a06e67a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a06e67a094a1060435853eae6b33f07ce09b9375))
* implement Story 2.1 — durable run identity ([0c54f13](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/0c54f13b7d0caf1fba907a18e581bbfd1839a6d0))
* implement Story 2.10 — invalidate indexed state for untrusted use ([d2555cf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d2555cf794b2cd037fdce257cd7a50e03f800e52))
* implement Story 2.11 — reject conflicting idempotent replays ([68537e4](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/68537e44264775e57bbfe03bc67b286b4f0f6610))
* implement Story 2.2 — quality-focus language indexing ([b6f6f6f](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b6f6f6f7a11ce63fd749e7e8d4e2f5d1821bb006))
* implement Story 2.3 — persist file/symbol metadata ([a1c7342](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a1c7342af18b21ae75de0b9c1bf7266d9ddbbb15))
* implement Story 2.4 — broader language onboarding pattern ([70cc654](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/70cc654c7975ef035875c6708ab09b1bd941ac57))
* implement Story 2.5 — run status and health inspection ([9e07b71](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9e07b71d2e8f1fff0644eca472fcb0d41d1a3232))
* implement Story 2.7 — cancel an active indexing run safely ([9ba4a5d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/9ba4a5d2f2856a7236428e26cd2dfa924a257f0f))
* implement Story 2.8 — checkpoint long-running indexing work ([1055568](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/105556817d2cd10078dd0eab0d9c794dac674cdd))
* implement Story 2.9 — re-index managed repository deterministically ([7696d7c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7696d7cd0b32bc8eac953a37af55db2b768f9d4c))
* improve prompt context symbol disambiguation ([144377d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/144377daef8adeb5a7e80c87b441964ba96cd495))
* module-path-aware find_dependents for lib.rs, mod.rs, __init__.py, index.js ([ea2655c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ea2655c4c68f17bf42e914da5aa57e86c456f468))
* **story-4.6:** complete IntegrityEvent instrumentation and land story ([7e8057b](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/7e8057bfbe9f541467afba8435b9e74ccddc8573))
* **story-4.7:** unified action classification with review fixes ([78dad2a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/78dad2a0398f028265233235556dbe00f52c919e))
* suppress noisy search_symbols results by default ([3255928](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/32559289e0242d755af06896f0aa4d902a002a55))
* tokenizor v2 rewrite — in-memory LiveIndex with parasitic hook integration ([3cbc63c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/3cbc63c350f2cafd8b77601db3235bdbba779271))


### Bug Fixes

* **02-01:** mark doctest as text to fix compilation ([92e5998](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/92e5998d6f15419c5cdde7da370abb3ea147153f))
* **05-02:** re-add pub mod cli to lib.rs after plan 01 metadata commit overwrote it ([79cb27e](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/79cb27e01e3c26e84fa88c7c7a34e588608071fd))
* **06-02:** make hook helper fns pub and fix run_hook signature in integration tests ([36d45de](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/36d45defe68af2996502c94b5fa7008a5a2222ef))
* **07:** use box-drawing chars in tier headers per CONTEXT.md ([bb22570](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb225701a514c477c376e7e2bc0763c667381ed6))
* address code review findings for Story 2.2 ([27fd538](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/27fd538bb9604e7f3c8f8b1553743ae0b581b58b))
* address code review findings for Story 2.3 ([a6c70c0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/a6c70c01b5dfe8416d998ffcfafa3f8199e1e1c6))
* address code review findings for Story 2.4 ([2383dd7](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/2383dd74ad9bb8659dc38a169116c5a8a4df5af4))
* address code review findings for Story 2.6 ([8609d1c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/8609d1caacce4c4e026c52ec5fc3ec7cebbec9c7))
* address code review findings for Story 2.8 ([d105124](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d105124c74aa8a05ef2bedfe266cd76785c29cc5))
* auto-register repo on index_folder and clear invalidation on reindex ([49ad871](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/49ad871bb43fde64ceddb9e5f77a8e2ab4ddc78b))
* **ci:** drop macOS builds, keep Windows and Linux only ([d265d65](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d265d65f1cfe89d9e7e29ce12f45fe7257e3cddb))
* **ci:** use macos-latest for x64 cross-compile (macos-13 deprecated) ([bb9d65c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/bb9d65c8b9ebbdb8f65920830dc97fd70771f29f))
* expose typed parameter schemas for all 18 MCP tools ([c8369d8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/c8369d8c138bb18e5c7b546cb4d5d296087178a6))
* handle locked binary on Windows during npm update ([b57aa8c](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b57aa8cf25fbcaf8e3e6e8d6625f7f989418c43c))
* hook detection for tokenizor-mcp binary name and npx cache warning ([d0ad70a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d0ad70a52297826c2187499d62e030ca55b4ee6d))
* install binary to ~/.tokenizor/bin/ to avoid Windows file-lock ([da9c40a](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da9c40a2ccd4987d061e574dbcc5f18f13a2679c))
* make installer tests host-agnostic ([6eb0bfa](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/6eb0bfada2f70bc86a4974a1f812bc7f315a63d0))
* make npm updates replace locked windows binaries ([da3f24d](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/da3f24d808aee39512545e29db989b7d4bb2f428))
* npm wrapper and release pipeline for v2 ([ab794da](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/ab794dae0105cd8bc49466bcab04d27c6fc38457))
* refuse to auto-index home dirs, drive roots, and system paths ([d459bbf](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/d459bbf7bac24f9747bb23f7370f62efd328d664))
* replace deprecated macos-13 runner with macos-latest ([4ab6e72](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/4ab6e72f1bfab0f2bdefdf915e6ff3c5d0e472ef))
* simplify deployment to standard MCP lifecycle ([aebf7f8](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/aebf7f85703a9010793ac4030d16dde98b7167c2))
* **story-4.6:** code review fixes for operational history ([b8c9ab6](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b8c9ab6bda749e1e0428c4ba5706db807ea31a9d))
* version-aware npm update + --version flag ([b935bb0](https://github.com/special-place-administrator/tokenizor_agentic_mcp/commit/b935bb0ea7cc52916abace2435873f19dfe4c01d))
