# Independent Post-Run-20 Audit — A019

You are the independent checkpoint reviewer for SymForge's 20-run full-versus-compact token-surface shakedown. Work from repository and retained evidence, not from claims in chat.

## Scope and output

Perform a read-only audit except for one deliverable:

`research/token-cost/claude-opus-report-post-run-20-a019.md`

Do not modify product source, the harness, manifest, results ledger, grader answers, raw traces, baseline, or prior reports. Do not run new measured model sessions. Do not re-grade by writing to the ledger.

Use one final verdict:

- `VERDICT: APPROVE_SHAKEDOWN_CLOSURE` if capture/custody, frozen grades, statistics, error classification, limitations, and next plan are accurate enough to close A019 and proceed to oracle/error-diagnosis design;
- `VERDICT: CHANGES_REQUIRED` if any material evidence, grade, calculation, scope, cleanup, or inference problem must be corrected first;
- `VERDICT: BLOCKED` only if required retained evidence is inaccessible.

List findings by severity with exact file/run evidence and separate:

1. required before A019 closure;
2. required before confirmatory-pilot design;
3. optional hardening.

## Hard safety rules

- Never print a secret value. If one is encountered, name only the variable and file/location.
- Do not dump process environments, Codex configuration, authentication state, or credential-bearing lines.
- Treat `potential_secret_line_count=0` as a claim to verify structurally, not permission to print matched content.
- Do not delete evidence, worktrees, processes, homes, targets, or candidate binaries. Recommend cleanup disposition in the report; the primary agent will execute it after review.

## Phase 1 — blind oracle re-grade first

Before reading the results ledger, run-order table, analysis report, raw traces, prior report, or any arm/surface/token metadata:

1. Read only the two frozen oracle checklists under `## Frozen tasks and exact oracles` in:
   `research/token-cost/token-speed-tool-trust-benchmark-manifest-2026-07-13.md`
2. Stop before `## Predetermined run order`.
3. Read only:
   `research/token-cost/evidence/token-surface-shakedown-a019/run-01-grader.md`
   through
   `research/token-cost/evidence/token-surface-shakedown-a019/run-20-grader.md`
4. Infer S1 versus S2 from answer content. Produce a private run→PASS/FAIL table with exact failed criterion numbers and terse reasons.
5. Apply the frozen text literally:
   - S1 requires `compact_probe_tools` even if the answer reports production truth.
   - S2 criterion 5 requires stating that retrieve count and bytes are incremented before returning a clone.
   - Frozen citation ranges are exact.
6. For run 14, decide explicitly whether “resolves Compact, Meta, or default Full” satisfies S1 criterion 2 and explain the equivalence decision.

Only after fixing that blind table may you inspect the recorded grades and arm assignments.

## Phase 2 — evidence and custody audit

Read:

- `research/token-cost/token-speed-tool-trust-benchmark-manifest-2026-07-13.md`
- `research/token-cost/run-token-surface-shakedown.ps1`
- `research/token-cost/evidence/token-surface-shakedown-a019/semantic-baseline.json`
- `research/token-cost/evidence/token-surface-shakedown-a019/wiring-receipt.json`
- `research/token-cost/evidence/token-surface-shakedown-a019/shakedown-results.jsonl`
- `research/token-cost/claude-opus-report-post-run-01-a019.md`
- `research/token-cost/token-surface-shakedown-report-a019.md`

Independently verify:

- exactly 20 unique run IDs and predetermined task/arm order;
- one real `turn.completed.usage` event per raw trace;
- canonical tokens equal `input_tokens + output_tokens` from that event and cached input is not double-counted;
- raw trace paths exist and correspond to the recorded run;
- `tool_event_count` counts started plus completed events, while `symforge_call_count` counts completed calls;
- completed SymForge success/error counts and first substantive tool match the raw order;
- every record has exit 0, no timeout, semantic readiness `ready`, zero verification mismatches, fixed candidate identity, and baseline-equivalent source/index fingerprints;
- zero exclusions, each grade written once, and recorded failures equal your blind re-grade;
- zero configuration diagnostics, zero potential-secret lines, and no repository change caused by a run, without printing any potentially sensitive line;
- no fixture worktree, candidate process, isolated run home, repository Cargo target, disposable Cargo target, or extra Git worktree remains;
- the retained artifact set and sizes are proportionate to the next diagnostic.
- post-series Amendment C changes only `SelfTest` isolation: confirm the optional `Get-CompactRunArtifactPaths -Root` parameter preserves the production default and the unique temporary root removes the invalid assumption that run 20 is absent.

Candidate identity expected by the frozen series:

- version `8.14.1`
- SHA-256 `6C4176E03299B768793ACB64012FDD95783476B6AE59662FC4AD7B8C310FFC3B`

Semantic baseline expected:

- tree `30704cf80723d4c40a0ac6bb65faf8aeaef50ea6`
- 851 tracked files
- source manifest `8CB1C3E40C4EB8C2B84FC3C46F61503A6E8B2E4BACC55C166EA795E8FFF660C7`
- repository outline `006DEB8BC310E03332E796C5ED606295D72264539D471274A448C25E435D2ADD`
- 726 indexed, 720 parsed, 4 partial, 2 failed, 21,830 symbols
- zero snapshot verification mismatches

Materialized snapshot byte hashes are informational only and must not be treated as semantic equality keys.

## Phase 3 — independent statistics

Recompute each task/arm cell separately from `shakedown-results.jsonl`:

- passes out of five;
- all five token totals, median, min–max range, and median absolute deviation;
- all five model-session wall times, median, min–max range, and MAD;
- zero-call runs;
- first-tool-correct runs;
- completed SymForge calls, successes, and errors;
- native read/search fallback runs and command count;
- immediate retries;
- citation-error runs.

Verify the 20-row predetermined run-order receipt in the report. Do not pool tasks, calculate a headline saving, run significance tests, or claim causality. Decide whether any token/speed comparison is admissible when every cell has 0/5 frozen-oracle passes.

For native fallback classification, inspect command strings privately and report only categories/counts. Do not reproduce command arguments.

## Phase 4 — compact error anatomy

Independently inspect every failed compact call in the raw traces and verify or refute:

- 17 compact SymForge errors total; full has zero;
- four failures occur before facade dispatch because a descriptive free-form string does not decode as the closed `IntentBucket` enum;
- thirteen facade results carry `symforge/result_status.outcome_class=invalid_request`;
- those thirteen expose neither `error_class` nor `retryable` metadata;
- 12 failures are followed immediately by another call to the same compact facade across the two compact cells;
- every completed native command is a read/search fallback and no build/test command occurred;
- compact S2 has 186 native fallback commands versus 20 for full S2.

Use SymForge for source-code inspection. Check at least:

- `IntentBucket`, `src/stel_core/types.rs`;
- compact tool schema construction in `src/stel/surface_list.rs`;
- compact request decode/dispatch in `src/protocol/mod.rs`;
- compact planning/execution/classification in `src/stel/planner.rs`, `src/stel/executor.rs`, and `src/protocol/tools.rs`.

Do not propose a code patch from aggregate categories alone. State which call-level facts must be extracted before assigning root cause to schema, planner, argument construction, leaf behavior, classification, or envelope policy.

## Phase 5 — product scope and next-plan review

Verify the product diff remains limited to the authorized truthful annotation/test in `src/stel/surface_list.rs`, and that `symforge_edit` and `status` were not made read-only.

Challenge the report's next plan:

- preserve A019 grades and never outcome-driven re-grade;
- replace the production-inaccurate S1 oracle before confirmatory use;
- either explicitly prompt for S2 mutation-before-clone order or remove it as an incidental wording trap;
- align fixture source and candidate binary custody by committing the annotation before the confirmatory pilot;
- diagnose all compact invalid requests before deciding on a schema-only rescue;
- prioritize the canonical semantic catalog with host/provider deferred discovery, plus a client-neutral semantic allowlist for unsupported hosts;
- keep compact-3 as a separate rescue arm rather than the default surface until it demonstrates equivalent task success;
- postpone power planning until revised tasks produce successful equivalent outcomes.

Explicitly flag any recommendation that overreaches the evidence.

## Cleanup recommendation

The current retained sizes are approximately:

- golden state: 16.46 MiB;
- complete raw-evidence directory: 5,051,709 bytes (4.818 MiB), including 20 run event traces totaling 4,865,349 bytes (4.640 MiB);
- compact evidence: 0.37 MiB;
- pinned candidate: 58.09 MiB.

Recommend, by artifact class, what may be deleted immediately after your approval and what must remain through the compact call-level diagnosis. Do not perform cleanup yourself.

## Required report ending

End with:

1. the exact verdict line;
2. a one-paragraph statement of what A019 establishes;
3. a one-paragraph statement of what it does not establish;
4. a checklist of required next actions before any product code change.
