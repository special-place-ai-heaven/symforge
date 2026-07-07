# Token-Cost Loop — Results Log

Append one line per round. Format:
`Round # | fixture | hypothesis | tokens before -> after | kept/reverted`

Baseline round (Round 0) records the starting score for each fixture before
any changes.

| Round | Fixture | Hypothesis | Tokens (before -> after) | Result |
|---|---|---|---|---|
| 0 | F1-get_symbol-large-fn | baseline (real tiktoken cl100k_base on `search_text_result_view` full body) | n/a -> 2350 | baseline (corrected; initial 2159 figure was a hand-transcription error that dropped inline `//` comments — fixed via raw `sed` extraction on clean main, see Round 1 note) |
| 0 | F2-search_text-default | baseline (default view, query `estimate_tokens` in `src/protocol`) | n/a -> 263 | baseline |
| 0 | F3-repo_map-compact | baseline (`get_repo_map` detail=compact, whole repo — path only scopes detail=tree) | n/a -> 1024 | baseline |
| 0 | F4-symbol_context-callers | baseline (`find_references`/context for `estimate_tokens`, compact routing) | n/a -> 149 | baseline |
| 0 | F5-find_references-verbose | baseline (`find_references` for `OutputLimits`, verbose default view) | n/a -> 396 | baseline |
| 0 | TOTAL | — | — | **4182 tokens** (corrected from 3991 after F1 fix) |
| 1 | F1-get_symbol-large-fn | Tighten the two structural-search error-arm hint strings (`InvalidStructuralPattern`, `UnsupportedStructuralLanguage`) — shorter wording, same facts (params, example, guidance) preserved; no other text or logic touched | 2350 -> 2309 | **kept** (build clean, `cargo test` shows no new failures — 2 pre-existing `stel_golden_replay` + ~9 pre-existing flaky `--lib` failures confirmed identical on clean main before this change) |
| 1 | TOTAL | — | — | **4141 tokens** (down from 4182, -41 / -1.0%) |
| 2 | F3-repo_map-compact | Drop the literal words "files"/"symbols" from every directory row in `src/sidecar/handlers.rs::repo_map_text` (scope widened here — see instructions.md), replace `{:>3} files   {:>5} symbols` with `{:>3}/{:<5}` plus a one-time header legend. Measured via real live tool calls (revert -> capture -> reapply -> capture, no hand-transcription) at matching fully-warmed 618-file index state | 1151 -> 1163 | **reverted** (regression: +12 tokens net. Per-row cost did drop (13->11 tokens/row measured in isolation), but the freed budget let more "Key types" lines fit before the response's own truncation point, more than offsetting the row savings — net loss. Lesson: `get_repo_map` compact has an implicit length budget: local density wins get reallocated to more content, not saved) |
| 2 | TOTAL | — | — | **4141 tokens** (unchanged, F3 reverted) |
