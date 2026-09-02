# Contract: Repeat Notice

Delivered on the 3rd+ identical serve of an eligible read tool with continuously-equal, positively-observed project evidence within one observed session (see plan.md Design US1). Two carriers, both present or both absent — never one without the other. **This document is the single normative source for the notice's byte-exact text and `_meta` shape**; other artifacts defer to it.

## 1. `_meta` entry

Key: `symforge/repeat_notice` (constant `REPEAT_NOTICE_META_KEY` in `src/protocol/result_status.rs`, beside `RESULT_STATUS_META_KEY`).

```json
{
  "contract_version": 2,
  "repeat_count": 3,
  "tool": "search_symbols",
  "request_hash": "<hex of RequestHash>",
  "evidence_generation": 42
}
```

- `contract_version`: **2**. Bump on any shape change, and on any change to what the
  notice CLAIMS. Version 1 promised the reader that the next serve could not differ;
  version 2 reports only what was observed. The JSON shape is identical across the
  bump — a client that keyed off version 1's meaning would otherwise silently
  mis-read version 2's.
- `repeat_count`: total serves of this fingerprint in the current run (≥ 3).
- `tool`: tool name as dispatched.
- `request_hash`: hex encoding of the `RequestHash` fingerprint (diagnostic; lets a client correlate).
- `evidence_generation`: the `generation` field of the witnessed (unchanged) `ProjectEvidence`.

Single-writer: only the `call_tool` seam writes this key, after `symforge/project_evidence` attachment. Absence of the key is meaningful — no claim was possible or warranted.

**This `_meta` entry is the authoritative carrier; the appended text is not.** Any file in the repository may contain the notice's literal string, and `search_text` will render it; on hosts that permit newlines in filenames, even an untracked path echoed by the zero-hit diagnostic can forge a `\n\nRepeat notice: …` paragraph. A client that acts on the notice MUST key on `symforge/repeat_notice`, which only the seam writes. The text carrier exists because many harnesses show a model nothing else, and it is advisory (security review, 2026-09-02).

## 2. Appended text

Appended (with `\n\n` separator) to the response's **final text content block** — original content is a strict byte prefix (spec FR-004):

```
Repeat notice: identical request served {N}x. Across these serves, no index change was published and the response text before this notice was unchanged. Retrying it unchanged has not produced new information.
```

(`{N}` is the decimal repeat count. Byte-canonical here — plan.md defers to this string.)

Wording constraints (binding):
- Every factual clause MUST be retrospective, and MUST name something the tracker
  actually observed in order to reach the threshold: the serve count, evidence
  equality across the run, and equality of the rendered text. The notice MUST NOT
  predict what a future serve would return — see the round-3 rationale below.
- MUST say "published" — the observation is publication-level, not disk-level (research.md R6).
- MUST NOT claim the files are unchanged.
- MUST scope the body claim to the text BEFORE the notice: the delivered response
  necessarily differs from the previous serve, because `{N}` increments.
- The closing sentence MUST stay retrospective too, and MUST NOT prescribe a remedy.
  Two independent reviews on 2026-09-02 converged here. A closing that told the agent
  to change project state nudged it, from a read-only condition, toward creating what
  it had just failed to find. A closing that told it to change the request presupposed
  the project is static, which is false inside the watcher publication window: an agent
  that edits a tracked file and calls `search_symbols` three times before the debounce
  elapses gets three identical serves off one published bundle, and the correct action
  there is to retry the SAME request a moment later. No single prescription is right on
  both lanes, because for the index-determined tools the body moves only when the index
  does, while for the zero-hit sweep it can move with no publication at all. So the
  notice reports and stops.
- MUST NOT alter `isError`, `ResultStatus`, or any prior content bytes.

## Non-emission guarantees (the contract's negative space)

The notice MUST NOT appear when ANY of:
- fewer than 3 serves of the fingerprint in the current run,
- the tool is not in `REPEAT_ELIGIBLE_TOOLS` (5 tools — data-model.md),
- the value under `symforge/project_evidence` on ANY serve of the run does not deserialize as a full `ProjectEvidence` — this includes the `{"bound": false, "reason": "project_evidence_unavailable"}` marker (the key itself is ALWAYS present at the seam, `src/protocol/result_status.rs:151-167`; "absence" on the wire does not exist — the observable condition is deserialization failure),
- the deserialized evidence carries the `"unbound"` placeholder `project_id` (`result_status.rs:149-150` — no project to be current about),
- any evidence field differs from the run's stored evidence (generation, index_state, counts, root, identity — full struct equality),
- the rendered text content of the response (every text block, in order, before the notice is appended) differs from the run's first serve — the witness observes the RESULT as well as the evidence (added 2026-09-02 after the implementation review found that `search_text` renders a query-time untracked-file diagnostic that project evidence does not fence; a differing body restarts the run at 1),
- the index instance that served the run changed underneath the adapter (daemon reconnect, degrade to local fallback, or recovery from it): every such transition clears the tracker, because `ProjectEvidence.generation` is a per-process counter and a replacement instance can coincide with the dead one's evidence (added 2026-09-02, same review),
- the request carries a `projects` argument (set-valued fan-out: the daemon/adapter structurally withhold per-project evidence there, so runs never accumulate — the deserialization rule above enforces this; a dedicated oracle pins it),
- no session identity is observable for the request's lane (spec FR-002 — unattributable counts never accumulate),
- the response's `ResultStatus` is observed as `InternalFailure`, or — on lanes where `OutcomeClass` is unobservable (daemon-proxied plain-String bodies carry no `symforge/result_status`) — the response has `isError == true` (conservative clearing; research.md R5),
- the tracker was cleared (capacity) since the prior serve.

Each bullet is a test oracle with a paired positive control (Constitution II).

## Why there is no "unfenced input" bullet (round 3, 2026-09-02)

Review round 2 added one, and it was the right answer to the wrong question. Two
eligible renderers read state the published index does not fence — `search_text`'s
zero-hit untracked-file sweep (live `git status` plus raw worktree bytes) and
`find_references`'s on-disk admission-degradation fallback for a path the index does
not hold. Version 1's notice ended with "the result cannot differ until the index
changes", and no equality of past serves can license that sentence for a body built
from something the index never published. So round 2 withheld the notice whenever a
renderer entered such a path.

That preserved the sentence and cost the feature its primary case: an agent looping
on a query that finds nothing is exactly the shape the notice exists to break, and a
zero-hit `search_text` took the sweep arm every time. It was also incoherent from
outside — the sweep only runs when `suppressed_by_noise == 0`, so two zero-hit
searches could differ in whether they noticed, for a reason no caller can see.

Round 3 drops the sentence instead. The remaining text asserts only what the tracker
had to observe to reach the threshold, so it is true on every lane, and the two
renderers above become ordinary participants. Nothing was weakened to get there:

- The body digest already restarts the run whenever such an input moves the answer.
  `untracked_file_diagnostic_never_earns_a_notice` proved this before round 2 existed
  and still passes unchanged; `find_references_disk_fallback_notices_then_resets_when_the_body_moves`
  is the same proof on the other lane.
- Fencing the inputs instead (digesting what was read) was considered and rejected:
  a digest witnesses what one serve observed, it does not freeze the next one. A real
  fence would have to snapshot git classification, the untracked path set, metadata,
  bytes, failures, and filesystem identity — substantial machinery whose only product
  is a sentence the feature does not need.
- Widening the claim to "no file changed" was refuted outright: `git add` moves a path
  out of untracked without touching a byte, and ignore rules, repository configuration,
  path existence, permissions, and symlink resolution do the same.

The governing rule (SC-002, zero false claims) is unchanged and still dominates. What
changed is the recognition that withholding a true notice to protect an unobservable
clause is not the only way to obey it — deleting the clause obeys it better, because
it leaves nothing to withhold.
