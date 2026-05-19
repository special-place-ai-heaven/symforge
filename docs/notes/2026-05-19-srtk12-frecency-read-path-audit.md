# SRTK12 Frecency Read-Path Audit

Date: 2026-05-19

Status: already covered in production code; strengthened with one helper-level regression test.

## Source Status

- `FrecencyStore::open_existing_readonly` checks for the DB path before opening SQLite and does not create parent directories, a DB file, or schema.
- `ranking_scores_for_paths` first reuses an existing cached persistent writer when one exists. If no writer is cached, it consults `.symforge/frecency.db` through `open_existing_readonly` only.
- Session history is read from the process-local in-memory session cache. Missing persistent and session history returns `Ok(None)`.
- `search_files rank_by="frecency"` calls `ranking_scores_for_paths` only after collecting ordinary path candidates. Search tools still do not bump frecency.

## Test Coverage

- Missing DB and helper-level repeated read path: `ranking_scores_without_history_repeatedly_stays_footprint_free`.
- Discovery-only no-footprint behavior: `search_files_does_not_bump`, `search_files_frecency_rank_does_not_create_db_when_empty`, `search_text_does_not_bump`, and `search_symbols_does_not_bump`.
- Existing persistent history: `rank_by_frecency_env_unset_uses_existing_persistent_history`.
- Session history without persistent footprint: `commitment_read_collects_session_history_with_env_unset`.
- Cached writer visibility and same-process contention: `cached_writer_post_reset_visible_to_ranking` and `concurrent_bump_and_rank_under_persistent_policy_completes`.

## Audit Decision

No production behavior patch was needed. Repeated no-history read-path calls do not open SQLite and do not create `.symforge/` because the read-only branch exits before `Connection::open_with_flags` when the DB path is absent. Existing-DB ranking is already covered by call-time frecency tests, and same-process persistent writer reuse is already covered by cached-writer tests.

This is a selective SymForge audit and test hardening task, not an RTK bulk integration. No RTK runtime code, shell hooks, telemetry, CLI output filtering, or new dependencies were imported.

## Verification

- `git branch --show-current; git diff --check` - passed; branch was `symforge-rtk-surgical`. Git emitted existing LF-to-CRLF working-copy warnings only.
- `cargo test ranking_scores_without_history_repeatedly_stays_footprint_free -- --test-threads=1` - passed; 1 unit test passed.
- `cargo test --test frecency_ranking -- --test-threads=1` - passed; 21 tests passed.
- `cargo test --test call_time_frecency -- --test-threads=1` - passed; 7 tests passed.
- `cargo check` - passed.
- `cargo test --all-targets -- --test-threads=1` - passed; full suite passed with existing ignored perf/AAP full-smoke tests unchanged.
- `cargo build --release` - passed.
