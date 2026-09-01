# Contract: Repeat Notice

Delivered on the 3rd+ identical serve of an eligible read tool with continuously-equal, positively-observed project evidence within one observed session (see plan.md Design US1). Two carriers, both present or both absent — never one without the other. **This document is the single normative source for the notice's byte-exact text and `_meta` shape**; other artifacts defer to it.

## 1. `_meta` entry

Key: `symforge/repeat_notice` (constant `REPEAT_NOTICE_META_KEY` in `src/protocol/result_status.rs`, beside `RESULT_STATUS_META_KEY`).

```json
{
  "contract_version": 1,
  "repeat_count": 3,
  "tool": "search_symbols",
  "request_hash": "<hex of RequestHash>",
  "evidence_generation": 42
}
```

- `contract_version`: starts at 1; bump on any shape change.
- `repeat_count`: total serves of this fingerprint in the current run (≥ 3).
- `tool`: tool name as dispatched.
- `request_hash`: hex encoding of the `RequestHash` fingerprint (diagnostic; lets a client correlate).
- `evidence_generation`: the `generation` field of the witnessed (unchanged) `ProjectEvidence`.

Single-writer: only the `call_tool` seam writes this key, after `symforge/project_evidence` attachment. Absence of the key is meaningful — no claim was possible or warranted.

## 2. Appended text

Appended (with `\n\n` separator) to the response's **final text content block** — original content is a strict byte prefix (spec FR-004):

```
Repeat notice: identical request served {N}x with no index change published in between (project evidence unchanged). The result cannot differ until the index changes - change the request instead of retrying.
```

(ASCII hyphen before "change the request"; `{N}` is the decimal repeat count. Byte-canonical here — plan.md defers to this string.)

Wording constraints (binding):
- MUST say "published" — the observation is publication-level, not disk-level (research.md R6).
- MUST NOT claim the files are unchanged.
- MUST NOT alter `isError`, `ResultStatus`, or any prior content bytes.

## Non-emission guarantees (the contract's negative space)

The notice MUST NOT appear when ANY of:
- fewer than 3 serves of the fingerprint in the current run,
- the tool is not in `REPEAT_ELIGIBLE_TOOLS` (5 tools — data-model.md),
- the value under `symforge/project_evidence` on ANY serve of the run does not deserialize as a full `ProjectEvidence` — this includes the `{"bound": false, "reason": "project_evidence_unavailable"}` marker (the key itself is ALWAYS present at the seam, `src/protocol/result_status.rs:151-167`; "absence" on the wire does not exist — the observable condition is deserialization failure),
- the deserialized evidence carries the `"unbound"` placeholder `project_id` (`result_status.rs:149-150` — no project to be current about),
- any evidence field differs from the run's stored evidence (generation, index_state, counts, root, identity — full struct equality),
- the request carries a `projects` argument (set-valued fan-out: the daemon/adapter structurally withhold per-project evidence there, so runs never accumulate — the deserialization rule above enforces this; a dedicated oracle pins it),
- no session identity is observable for the request's lane (spec FR-002 — unattributable counts never accumulate),
- the response's `ResultStatus` is observed as `InternalFailure`, or — on lanes where `OutcomeClass` is unobservable (daemon-proxied plain-String bodies carry no `symforge/result_status`) — the response has `isError == true` (conservative clearing; research.md R5),
- the tracker was cleared (capacity) since the prior serve.

Each bullet is a test oracle with a paired positive control (Constitution II).
