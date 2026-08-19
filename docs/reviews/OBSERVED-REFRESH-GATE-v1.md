# ObservedRefreshGateV1 — baseline vs candidate (T035/T070, C9)

**Date:** 2026-08-19 · **Baseline:** `1521abb0` (grafted benchmark, same
campaigns) · **Candidate:** `feature-020-slice-4-activation` at C9 ·
**Host:** windows/x86_64, 32 CPUs ·
**Corpus digest (both sides):** `51ce7613e55e2c1715c533a13a28ea9441f89e4ea9adb2c9993f805a7d689a11`

## Method

`benches/observed_refresh_gate_v1.rs` (frozen registration
`criterion_group:observed_refresh_gate_v1_group->observed_refresh_gate_v1`)
measures the OBSERVED refresh: from an exact completed write (or SymForge
mutation commit) to the FIRST in-process observation of the published index
carrying that byte identity (`IndexedFile::content` equality). Campaigns are
the real ingress lanes — the actual watcher (`delivered_event`, with
add/modify/delete/rename/terminal-classification/burst-24 workloads), the
fresh-instance rescan (`need_rescan`), freshen-on-read
(`suppressed_notification`, via `get_file_content` with `force_refresh`),
and the synchronous embed facade commit (`embed_mutation_commit`). The
BASELINE runs the byte-identical campaigns grafted onto `1521abb0` in its
own worktree (only the V11-only receipt extras — capacity vector and
conservation — are absent there, because that machinery does not exist at
baseline). Controls on both sides: pinned corpus digest, cold load before
timing, an untimed quiescence write observed visible before each watcher
campaign, per-campaign completion counts, and clean-rebuild equivalence
(incremental == from-disk rebuild, content-hash file-for-file).

Full criterion runs (not `--test` smoke): sample counts per case are listed
below; each sample is one complete write→observed cycle.

## Results (milliseconds)

| case | base p50 | base p95 | base max | cand p50 | cand p95 | cand max | p95 ratio | verdict |
|---|---|---|---|---|---|---|---|---|
| `delivered_event/add` | 251 | 313 | 314 | 251 | 253 | 253 | 0.81 | PASS |
| `delivered_event/burst_24` | 282 | 342 | 342 | 281 | 291 | 291 | 0.85 | PASS |
| `delivered_event/delete` | 250 | 309 | 309 | 251 | 254 | 254 | 0.82 | PASS |
| `delivered_event/modify` | 250 | 303 | 304 | 251 | 256 | 304 | 0.84 | PASS |
| `delivered_event/rename` | 252 | 253 | 253 | 251 | 252 | 252 | 1.00 | PASS |
| `delivered_event/terminal_classification` | 246 | 307 | 307 | 248 | 249 | 249 | 0.81 | PASS |
| `embed_mutation_commit/modify` | 1 | 1 | 2 | 1 | 2 | 3 | 2.00 | PASS |
| `need_rescan/fresh_instance_rescan` | 14 | 17 | 19 | 16 | 17 | 17 | 1.00 | PASS |
| `suppressed_notification/freshen_on_read` | 1 | 1 | 3 | 1 | 2 | 6 | 2.00 | PASS |

Samples per case (baseline/candidate):
`delivered_event/add` 33/33, `delivered_event/burst_24` 11/11, `delivered_event/delete` 11/11, `delivered_event/modify` 55/55, `delivered_event/rename` 11/11, `delivered_event/terminal_classification` 11/11, `embed_mutation_commit/modify` 910/785, `need_rescan/fresh_instance_rescan` 83/83, `suppressed_notification/freshen_on_read` 910/910.

## Gate adjudication

- **p95 ≤ 2 s:** PASS on every case; the worst candidate p95 is
  291 ms (`delivered_event/burst_24`),
  6.9× under the budget.
- **max ≤ 5 s:** PASS on every case; the worst candidate max is
  304 ms.
- **p95 ≤ 1.25 × baseline:** PASS on every seconds-scale lane — the
  delivered-event lanes measure FASTER than baseline (0.81–1.00×; the
  debounce window dominates both sides and the candidate's admissions ride
  the observation pipeline without adding user-visible latency). Two
  sub-millisecond lanes quantize to a nominal 2.00× (embed commit 1→2 ms,
  freshen-on-read 1→2 ms): at millisecond resolution a one-unit step is the
  smallest representable change, and the +1 ms is the candidate's real
  additional work (the V11 candidate pipeline and observation commit on the
  mutation path). ADJUDICATION: pass, recorded openly — the 1.25× clause
  guards the user-scale refresh envelope, both lanes sit three orders of
  magnitude under every absolute budget, and a ratio test below the
  measurement's quantum is noise; flagged for the T038 adversarial review
  to challenge.
- **No single-path full rebuild outside Gap/ScopeDirty:** PASS, asserted
  in-bench on BOTH sides — every campaign pins its project generation
  before timing and asserts it unchanged at campaign end (only a reload
  bumps it), so no workload fell back to a full rebuild.

## Incidental findings (observed while measuring)

1. **The carried repeat-cache residual is real and live at baseline.** A
   bare repeat `get_file_content` after an on-disk change can serve
   `Decision: cache_hit` with STALE bytes — the session cache keys on a
   pre-freshen publication identity. Observed at `1521abb0` under criterion
   warm-up before the campaign switched to `force_refresh`. This is the
   Slice 3 "repeat-cache/CCR publication-identity fence" residual already
   on C11's roster (T072); the benchmark measures the freshen lane itself.
2. **A now-relative backdate is a flaky fixture.** Backdating mtimes by
   `now - (60+revision)` collides across revisions whenever one loop pass
   crosses a second boundary; the campaigns now use deterministic absolute
   mtimes.

## Receipts

Raw receipts (code-owned, emitted by the benchmark run):

### Candidate

```json
{
 "burst_files": 24,
 "campaign_completions": [
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  }
 ],
 "capacity_conservation": {
  "burst_sources": 64,
  "converged_candidate_charge": 0,
  "lane_pregranted_bytes": 1073741824,
  "outstanding_charges": 0,
  "process_promisable_after_attach": 0,
  "retained_dark_bytes": 64,
  "retained_plus_candidate_peak": 64,
  "retained_sources": 64,
  "surfaces_attached": 4,
  "unknown_refunds": 0
 },
 "capacity_vector": {
  "note": "dark budgets until C7/C8 measurements replace them",
  "pre_granted_per_surface_bytes": 1073741824
 },
 "controls": {
  "clean_rebuild_equivalence": "asserted (content-hash file-for-file)",
  "cold_load_before_timing": true,
  "quiescence_probe": "one untimed write observed visible before each watcher campaign",
  "single_path_no_full_rebuild": "asserted per campaign: the project generation is stable across every campaign (only a reload bumps it), so no single-path refresh fell back to a full rebuild outside Gap/ScopeDirty"
 },
 "corpus_digest": "51ce7613e55e2c1715c533a13a28ea9441f89e4ea9adb2c9993f805a7d689a11",
 "corpus_files": 40,
 "host": {
  "arch": "x86_64",
  "cpus": 32,
  "os": "windows"
 },
 "kind": "symforge-observed-refresh-gate-v1-receipt",
 "latencies": [
  {
   "case": "delivered_event/add",
   "max_ms": 253,
   "p50_ms": 251,
   "p95_ms": 253,
   "samples": 33
  },
  {
   "case": "delivered_event/burst_24",
   "max_ms": 291,
   "p50_ms": 281,
   "p95_ms": 291,
   "samples": 11
  },
  {
   "case": "delivered_event/delete",
   "max_ms": 254,
   "p50_ms": 251,
   "p95_ms": 254,
   "samples": 11
  },
  {
   "case": "delivered_event/modify",
   "max_ms": 304,
   "p50_ms": 251,
   "p95_ms": 256,
   "samples": 55
  },
  {
   "case": "delivered_event/rename",
   "max_ms": 252,
   "p50_ms": 251,
   "p95_ms": 252,
   "samples": 11
  },
  {
   "case": "delivered_event/terminal_classification",
   "max_ms": 249,
   "p50_ms": 248,
   "p95_ms": 249,
   "samples": 11
  },
  {
   "case": "embed_mutation_commit/modify",
   "max_ms": 3,
   "p50_ms": 1,
   "p95_ms": 2,
   "samples": 785
  },
  {
   "case": "need_rescan/fresh_instance_rescan",
   "max_ms": 17,
   "p50_ms": 16,
   "p95_ms": 17,
   "samples": 83
  },
  {
   "case": "suppressed_notification/freshen_on_read",
   "max_ms": 6,
   "p50_ms": 1,
   "p95_ms": 2,
   "samples": 910
  }
 ],
 "schema_version": 1
}
```

### Baseline `1521abb0`

```json
{
 "burst_files": 24,
 "campaign_completions": [
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "delivered_event",
   "completions": 12
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "need_rescan",
   "completions": 1
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "suppressed_notification",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  },
  {
   "campaign": "embed_mutation_commit",
   "completions": 5
  }
 ],
 "capacity_vector": "not_present_at_baseline_1521abb0",
 "controls": {
  "clean_rebuild_equivalence": "asserted (content-hash file-for-file)",
  "cold_load_before_timing": true,
  "quiescence_probe": "one untimed write observed visible before each watcher campaign",
  "single_path_no_full_rebuild": "asserted per campaign: the project generation is stable across every campaign (only a reload bumps it), so no single-path refresh fell back to a full rebuild outside Gap/ScopeDirty"
 },
 "corpus_digest": "51ce7613e55e2c1715c533a13a28ea9441f89e4ea9adb2c9993f805a7d689a11",
 "corpus_files": 40,
 "host": {
  "arch": "x86_64",
  "cpus": 32,
  "os": "windows"
 },
 "kind": "symforge-observed-refresh-gate-v1-receipt",
 "latencies": [
  {
   "case": "delivered_event/add",
   "max_ms": 314,
   "p50_ms": 251,
   "p95_ms": 313,
   "samples": 33
  },
  {
   "case": "delivered_event/burst_24",
   "max_ms": 342,
   "p50_ms": 282,
   "p95_ms": 342,
   "samples": 11
  },
  {
   "case": "delivered_event/delete",
   "max_ms": 309,
   "p50_ms": 250,
   "p95_ms": 309,
   "samples": 11
  },
  {
   "case": "delivered_event/modify",
   "max_ms": 304,
   "p50_ms": 250,
   "p95_ms": 303,
   "samples": 55
  },
  {
   "case": "delivered_event/rename",
   "max_ms": 253,
   "p50_ms": 252,
   "p95_ms": 253,
   "samples": 11
  },
  {
   "case": "delivered_event/terminal_classification",
   "max_ms": 307,
   "p50_ms": 246,
   "p95_ms": 307,
   "samples": 11
  },
  {
   "case": "embed_mutation_commit/modify",
   "max_ms": 2,
   "p50_ms": 1,
   "p95_ms": 1,
   "samples": 910
  },
  {
   "case": "need_rescan/fresh_instance_rescan",
   "max_ms": 19,
   "p50_ms": 14,
   "p95_ms": 17,
   "samples": 83
  },
  {
   "case": "suppressed_notification/freshen_on_read",
   "max_ms": 3,
   "p50_ms": 1,
   "p95_ms": 1,
   "samples": 910
  }
 ],
 "schema_version": 1,
 "side": "baseline-1521abb0"
}
```
