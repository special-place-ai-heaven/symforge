# Quickstart: CCR Output Compression (011)

**Branch**: `011-ccr-output-compression` · **Date**: 2026-06-18

Runnable validation for each user story. Run from repo root on Windows/Linux/macOS.

## Prerequisites

```powershell
cd E:\project\symforge
cargo build --release
```

Use a small fixture repo or symforge self-index with `SYMFORGE_SURFACE=full` for
retrieve tool visibility.

## Gate (required before done)

```powershell
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
```

## US1 — Session cache hit

```powershell
cargo test --test session_cache_hit -- --test-threads=1
```

**Manual smoke** (stdio MCP or test harness):

1. `get_file_context` path=`src/main.rs`
2. Repeat same call → expect `Decision: cache_hit`
3. Same call with `force_refresh=true` → full body
4. `get_symbol` for a known symbol → repeat → cache hit

**Pass**: SC-001 — cache-hit body <20% size of full for ≥2KB file.

## US2 — CCR retrieve

```powershell
cargo test --test ccr_retrieve -- --test-threads=1
```

**Manual smoke**:

1. `search_text` query matching many hits, `max_tokens=500`
2. Response contains `symforge_retrieve` hash footer
3. `symforge_retrieve` hash=`...` → full output matches uncapped run (save both, `diff`)

**Pass**: SC-002 byte-identical round-trip.

## US3 — Search compaction

```powershell
cargo test --test search_compaction -- --test-threads=1
```

**Fixture**: file with 50 `info` lines and 2 `ERROR` lines containing query term.

**Pass**: SC-003 — both ERROR lines in capped output.

## US4 — Dedup hint

In `session_cache_hit` or dedicated test:

1. Fetch with `force_refresh=true` twice
2. Second response ends with `[session: same`

## US5 — Economics (P3)

After US1+US2 land:

1. Run scripted session via test or serve
2. Inspect STEL ledger / admin summary for `cache_hit` and CCR byte counters

## Regression guards

```powershell
cargo test --test persist_compression_ratio -- --test-threads=1
cargo test frecency -- --test-threads=1
cargo check --no-default-features --features embed
```

**Pass**: SC-005 + Principle VI embed isolation.

## Headroom reference (optional)

Compare behavior against `E:\project\headroom` CCR footers — SymForge footers
should name `symforge_retrieve`, not `headroom_retrieve`. No runtime dependency.
