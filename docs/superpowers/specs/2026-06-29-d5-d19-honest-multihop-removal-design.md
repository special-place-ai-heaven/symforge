# D5/D19 — Honest removal of the multi-hop fake

**Date:** 2026-06-29
**Lane:** `E:\project\symforge-012` (SymForge trust campaign, 012f frontier)
**Status:** design approved; implementation pending.
**Defect:** D5/D19 — the STEL planner claims multi-step query decomposition but
only 3 hardcoded exact-match query strings produce a 2-step plan (fake success).

## Decision (and why it reversed)

The campaign first chose **Option B — build a real full-data-flow decomposer**.
An independent cold adversarial review (`wv1e9m9sb`, 3 diverse-lens skeptics +
synthesis, all verified against real code at `baa03d6`) returned **REJECT** on
the keystone ground: **there is no real consumer.** This is overlay-redundancy
round 2 — machinery whose only consumer is the corpus authored to test it.

Verified evidence:

- `plan_multi_hop_steps` (`src/stel/planner.rs:554-612`) is a 3-entry exact-string
  lookup with hardcoded `json!{}` args.
- The 3 matched strings **are** the golden-corpus rows
  (`docs/fixtures/routes.golden.jsonl:34-36`); the only callers are the tests /
  golden replay written to exercise them (`golden_replay.rs`,
  `tests/stel_multi_hop_chain.rs`) — a closed circular loop.
- No general `then`-decomposition exists anywhere in `src/` (every `then` hit is a
  comment / stopword).
- `src/stel/status.rs:23` **already declares `multi_step_planner` DEFERRED** —
  "genuinely not-yet-implemented seams."
- The live multi-step execution loop's real general consumer is **find-fusion**
  (a UNION), not dependent multi-hop.

The review also found the proposed binding mechanism independently unsafe
(parse-the-display-body could serve a real-but-wrong CoChange-tier file silently;
a `_bind` strip-miss on the `deny_unknown_fields` `get_file_content` decode is
misclassified `Found` at `tools.rs:242` and rendered as the file body). Those are
moot once we do not build binding, but the classify gap is recorded below.

**Disposition (user-approved):** treat D5/D19 as the fake-success defect it is and
**stop the fake success** — delete `plan_multi_hop_steps`, delete the 3 circular
golden rows + their test scaffold, and let multi-hop queries route honestly
through the existing single-step / find-fusion path. No decomposer, no `_bind`, no
text-parsing. This makes the code match the already-honest `status.rs` signal.

## Objective / non-goals

- **Objective:** remove the only dishonest artifact so multi-step is honestly
  not-implemented; multi-hop queries get best-effort single-step / find-fusion
  routing, never a fabricated 2-step chain.
- **Non-goals:** building any decomposition; changing find-fusion (it stays — the
  real multi-step consumer); changing `status.rs` (already honest).

## Surface (code — must change)

| File | Change |
|------|--------|
| `src/stel/planner.rs` | Delete `plan_multi_hop_steps` (554-612) + call site in `build_plan` (225-227). The 3 queries fall through to `symbol_lookup -> find-fusion -> single-step`. Delete the now-false test `multi_hop_golden_rows_plan_ordered_steps` (1628-1659). |
| `docs/fixtures/routes.golden.jsonl` | Delete the 3 `"chain":"multi"` rows (34-36). |
| `scripts/seed-routes-golden.cjs` | Delete the 3 row definitions (103-121) so regen does not re-add them. |
| `src/stel/golden_replay.rs` | Corpus count 36->33 (`golden_corpus_partitions_all_rows`); drop `MULTI_HOP_GOLDEN_ROW_IDS` + the multi-hop classification handling (`deferred_multi_hop` / `supported_serve`); repoint the synthetic `validate_rejects_failed_step_body` unit test off the deleted row id (it tests the multi-step validator, which find-fusion still needs). |
| `tests/stel_multi_hop_chain.rs` | Delete — executor chain-mechanics coverage (fail-fast, partial body) is already provided by find-fusion execution tests (`stel_find_fusion.rs`, `cochange_fusion.rs`). AC3 verifies that coverage exists before deletion. |
| `tests/stel_param_disposition.rs` | Drop the 2 multi-hop entries (62-63) — no longer a distinct route. |
| `tests/stel_l2_admission.rs`, `src/stel/{executor,mod}.rs` | Review-only: executor chain machinery STAYS (find-fusion shares it); expect comment/import touch-ups at most. |
| `tests/fixtures/stel_multi_hop/` | Delete the 3 fixture projects if no surviving test references them (dead-code + disk discipline); confirm during impl. |
| `docs/reviews/symforge-defect-ledger.md` | D5/D19 -> FIXED (by honest deletion); record the no-consumer rationale + this review. |

Docs/specs (`specs/002-*`, `docs/reviews/*`, `CHANGELOG.md`) are point-in-time
records and are left as-is.

## Sub-decisions

1. **`classify_get_file_content_output` gap (`tools.rs:242`):** after deletion it
   is unreachable (only the deleted multi-hop dispatched a `get_file_content` step
   with bad args; find-fusion dispatches `search_*`, whose classifiers handle
   errors). **Ledger as tracked-minor / defensive; do not fix in this deletion
   PR** (fixing unreachable code here is scope creep).
2. **Ceremony:** short design doc (this file) as the spec artifact, then the
   standard implementation workflow (implement -> adversarial review -> merge);
   no heavyweight writing-plans cycle.

## Acceptance criteria

- **AC1 (honest routing):** each of the 3 ex-multi-hop queries now produces a
  non-empty single-step (or find-fusion) plan via `build_plan` — no panic, no
  empty plan, no fake 2-step chain.
- **AC2 (no fake-success residue):** `rg "plan_multi_hop_steps|MULTI_HOP_GOLDEN_ROW_IDS"`
  -> empty; no `"chain":"multi"` rows in the golden corpus; `status.rs` still lists
  `multi_step_planner` DEFERRED (now strictly true).
- **AC3 (find-fusion intact):** find-fusion planning + execution tests stay green
  (proves the multi-step machinery the executor still needs is covered after
  `stel_multi_hop_chain.rs` deletion).
- **AC4 (gate green):** `cargo fmt --check`, `cargo check`,
  `cargo clippy --all-targets -- -D warnings`,
  `cargo test --all-targets -- --test-threads=1`, `cargo build --release`.

## Evidence trail

- Independent review: workflow `wv1e9m9sb` (REJECT, `no_op_risk_verdict =
  NO_REAL_CONSUMER`).
- Design review that it superseded: workflow `w4x9q9o3q` (the Option-B design;
  its embedded skeptic APPROVED but shared author context and never asked the
  consumer question).
- Self-verified pillars: `status.rs:23` (DEFERRED), `tools.rs:214-243` (classify
  fall-through), `planner.rs:225-227,554-612,1628-1659`,
  `routes.golden.jsonl:34-36`, `golden_replay.rs:443-462`.
