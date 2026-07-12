# Phase 1 — Observable contract deltas

Each delta is the *observable* before→after behavior a fail-first test asserts.
No schema/persistence change; these are behavior corrections to existing tools.
"Before" is verified-2026-07-12 source behavior; re-confirm at implementation.

## `batch_rename` (US1)

| Aspect | Before | After |
|---|---|---|
| Write set on a common name | every bare-name ref across the index, incl. unrelated files | only refs proven to resolve to the target identity |
| Name-only / dynamic matches | written silently | non-writable by default; reported as uncertain |
| Exact binding unavailable | proceeds and writes | fails closed (no staged write) |
| `dry_run` default | `false` (apply writes immediately) | unchanged default, but apply cannot write unproven matches |
| Trust label | `Constrained` even for name-derived matches | reflects real binding confidence; never "constrained" over name-only |
| Recorded harness result | `SF-batch_rename-002` → "source mutation escaped the frozen allowlist" | same case writes zero unrelated bytes |

**Invariant**: no source byte outside the resolved-identity reference set is
ever written. Uncertain matches are disclosed, not applied.

## `detect_impact` (US2)

| Aspect | Before | After |
|---|---|---|
| Changed-symbol seed | every symbol of every changed file | only body-diff added/modified/removed symbols |
| Comment-only / whitespace edit | reports changed symbols | reports zero changed symbols |
| Call-edge resolution | caller → every same-bare-name def repo-wide | scoped to resolved/qualified target; no edge invented when unresolved |
| Blast node identity | bare name (duplicate indistinguishable `main`) | carries path/kind; distinct nodes distinguishable |
| Entry-point tag | any `fn main` → Critical | only the actual reachable entry point |
| Leaf-edit output | ~20 changed / 11 blast / 1278 tokens | 1 changed + hop-1 `{mid}`, bounded tokens |

## `analyze_file_impact` (US2, secondary)

| Aspect | Before | After |
|---|---|---|
| Same-file callers | filtered out (`fp != path`) | included and typed as calls |
| Caller lookup | residual bare-name `find_references_for_name(...,None,false)` | typed identity; **existing parent-type narrowing preserved** |

## `search_symbols` / `search_text` / `find_references` (US3)

| Aspect | Before | After |
|---|---|---|
| Matching-local `project` selector | refused (blanket `local_cross_project_refusal`) | proceeds; equals no-selector result |
| Foreign selector | refused | still refused, typed invalid-request |
| `projects` incl. `"*"` | inconsistent (blanket refuse here; under-refuse elsewhere) | consistent: match-local proceed, foreign/over-broad refuse |

## `find_dependents` (US3)

| Aspect | Before | After |
|---|---|---|
| `from .mod import x` importer | missed (relative import dropped) | reported, resolved via import-prefix depth vs importer package |
| Same-stem unrelated module | can appear as false consumer | excluded |

## `symforge_edit` (US4)

| Aspect | Before | After |
|---|---|---|
| Identical `{key, request, if_match}` replay | rejected by now-stale `if_match` (3/3) | returns stored result, zero writes |
| Same key, changed request | (n/a — rejected earlier) | idempotency conflict |
| New key, stale `if_match` | fails concurrency | still fails concurrency (unchanged) |

## watcher cold start (US5b)

| Aspect | Before | After |
|---|---|---|
| Tracked edit after cold start | `GenerationMismatch` → removed, never re-indexed (watcher captured pre-reload gen) | indexed (gen re-synced with the fire-and-forget cold reload) |
| Genuine in-flight stale mutation | fenced/rejected (correct) | still fenced/rejected (fence untouched) |

## `health` (US5a)

| Aspect | Before | After |
|---|---|---|
| "reconcile repairs" count | includes `GenerationMismatch` no-ops | only `StaleReindexed`/`StaleRemoved` |
| Rejected mismatch attempts | folded into repairs | surfaced separately or omitted |

## `status(reset_calibration=true)` daemon (US5c)

| Aspect | Before | After |
|---|---|---|
| Durable calibration after reset (daemon) | untouched; response `Found` (silent no-op) | proxy-owned durable store cleared to `deferred`; honest receipt |
| Local (non-daemon) reset | clears correctly | unchanged |

## `validate_file_syntax` (US6)

| Aspect | Before | After |
|---|---|---|
| Valid UTF-8-BOM JSON | rejected at 1:1 | `ok`, identical accounting to no-BOM |
| JSONC (trailing commas/comments) | `ok` (deliberate) | `ok` (unchanged — must not regress) |
| Genuinely malformed JSON | fails | fails at oracle location (unchanged) |
| `estimate=true` | inert (ignored) | tagged estimate within tolerance, or flag removed |

## Cross-cutting invariants (all items)

- Determinism: identical state → identical output/order (deterministic
  tie-breaks where ordering changes).
- Frecency-neutral: read/discovery paths (US2/US3/US6) write no frecency.
- Transport parity: shared handler/formatter changes (US1 label, US2 blast node,
  US5 counter) assert stdio↔serve parity.
- Embed: `cargo check --no-default-features --features embed` green.
- Fail-closed: US1/US4 never write on uncertainty/replay-mismatch.
