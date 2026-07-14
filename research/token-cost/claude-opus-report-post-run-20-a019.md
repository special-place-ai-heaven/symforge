# Claude Opus independent post-run-20 audit — A019

Reviewer: independent checkpoint session (Fable 5), 2026-07-14.
Scope: read-only audit per `research/token-cost/claude-opus-handoff-post-run-20-a019.md`.
Only this report was written. No product source, harness, manifest, ledger, grader
answer, raw trace, baseline, or prior report was modified. No measured model session
was run, and no grade was written to the ledger.

## Phase 1 — blind oracle re-grade

Procedure followed as ordered: only the `## Frozen tasks and exact oracles` section
of the manifest was read first (section boundaries located by heading scan, stopping
before `## Predetermined run order`), then the 20 redacted grader answers
(`run-01-grader.md` … `run-20-grader.md`), with no arm/surface/token metadata. Task
was inferred from answer content (10 surface-routing answers, 10 CCR answers).

Blind result: **0/20 pass**, matching the recorded grades exactly at pass/fail
level, with the same dominant failure classes:

| Blind class | Runs | Failed criteria (blind) |
|---|---|---|
| S1 answers | 01, 03, 06, 08, 09, 11, 14, 16, 17, 19 | All fail criterion 4 (name production `compact_surface_tools`, never the frozen `compact_probe_tools`) and criterion 5 (no citation inside `surface_probe.rs:253-285`). Applying the frozen text literally — "substitute symbols are not [accepted]" — production-truthful answers still fail. |
| S2 answers | 02, 04, 05, 07, 10, 12, 13, 15, 18, 20 | All fail criterion 5: every answer states the two counter increments and the clone return, but none states the increments occur *before* returning the clone. Several additionally cite `SymforgeRetrieveInput` at `read_tools.rs:394` or `symforge_retrieve` at `tools.rs:10788` — outside the frozen exact ranges (396-399, 10792-10804) — failing criterion 6 where no in-range citation for that symbol also appears. |

Criterion-level agreement with the recorded grades: the ledger's
`oracle_failures` name the same criteria per run (S1: 4+5, plus criterion 2 on the
eight runs that never state that explicit `compact`/`meta` values select their
profiles; S2: 5, plus 6 on runs 02, 04, 07, 10, 12, 18, 20). I independently
confirm runs 05, 13, 15 satisfy the frozen S2 citation ranges and that criterion-2
passes are exactly runs 01 and 14.

**Run 14 equivalence decision (required):** run 14's wording —
"`surface_profile_from_env()` … resolves `Compact`, `Meta`, or default `Full`" plus
the separate statement "The default profile is `Full`" — SATISFIES S1 criterion 2.
The criterion demands (a) default Full and (b) that explicit compact/meta select
their corresponding profiles. Run 14 states (a) explicitly and (b) by naming all
three resolution outcomes of the env read, which under the manifest's
"equivalent prose is accepted" clause is an equivalent statement of the mapping.
Run 14 therefore fails only criteria 4 and 5 — identical to the recorded grade.

Conclusion: recorded grading is deterministic, blind-reproducible, and was not
outcome-driven. Frozen grades must stand.

## Phase 2 — evidence and custody

Independently verified from `shakedown-results.jsonl` (parsed programmatically),
the raw traces, receipts, and the live machine:

- **Run identity/order:** exactly 20 unique run IDs 01–20; task/arm of every record
  matches the predetermined run-order table. Zero exclusions; `oracle_grade_count=1`
  and `record_status` graded on every record.
- **Usage semantics:** every raw trace contains exactly one `turn.completed` usage
  event; `canonical_total_tokens = input_tokens + output_tokens` from that event in
  all 20 runs (re-added from raw traces, not the ledger); `cached_input_tokens` is a
  subset of `input_tokens` in every run and is never re-added.
- **Trace custody:** all 20 `raw_trace` paths exist under the declared evidence
  root; run event traces total 4,865,349 bytes; full raw directory 5,051,709 bytes.
- **Count semantics:** `tool_event_count` = started + completed events (verified
  per run). `symforge_call_count` = completed MCP calls **to the `symforge` server
  only**; run 06 is the instructive case — its 3 completed MCP calls include two
  Codex-host resource calls (`list_mcp_resources`, `list_mcp_resource_templates`,
  server=`codex`), and the ledger correctly counts 1 SymForge call
  (`read_mcp_resource`). Success/error splits match `OutcomeClass` per run.
- **Uniform gates:** all 20 records: exit 0, no timeout, readiness `ready`,
  `snapshot_verify_mismatches=0`, candidate `8.14.1` /
  `6C4176E03299B768793ACB64012FDD95783476B6AE59662FC4AD7B8C310FFC3B`, tree
  `30704cf80723d4c40a0ac6bb65faf8aeaef50ea6`, 851-file manifest
  `8CB1C3…60C7`, outline `006DEB…2ADD`, 726/720/4/2 files, 21,830 symbols —
  identical to `semantic-baseline.json`. Materialized byte hashes vary and are
  treated only as informational, as required.
- **Hygiene:** `configuration_diagnostic_count=0` and
  `potential_secret_line_count=0` in all records (verified structurally as claims;
  no matched content printed); `repository_clean=true` and no `unexpected_changes`
  in any run.
- **Cleanup state (live):** no fixture worktree at the declared temp path; `git
  worktree list` shows only the main worktree; no disposable Cargo target
  (`symforge-token-trust-target-8.14.1-a019` absent); no repo `target/`; no isolated
  `.codex-home-*` run homes; no candidate-binary process running (only this dev
  session's npm-global SymForge servers, a different binary path).
- **Retained sizes match the handoff:** golden a019 state 17,260,173 B (16.46 MiB);
  raw evidence 5,051,709 B; compact evidence 389,958 B (0.37 MiB); pinned candidate
  60,908,544 B (58.09 MiB).
- **Amendment C:** `Get-CompactRunArtifactPaths` takes optional `-Root` defaulting
  to `$script:EvidenceRoot` (production call site at line 978 uses the default);
  the SelfTest assertion now supplies a unique GUID-suffixed nonexistent temp root,
  removing the invalid "run 20 artifacts are absent" assumption. Harness-test-only,
  as claimed.

One custody item the report's retained-size list omits (not material to A019
validity): the **pre-restart invalidated baseline pair** still exists —
`symforge-token-shakedown-evidence-a10ff102` (110,632 B) and
`symforge-token-shakedown-golden-a10ff102` (17,260,173 B). Amendment A requires
retaining the invalidated traces separately; disposition should still be stated at
cleanup (see recommendation).

## Phase 3 — independent statistics

Every cell recomputed from the ledger; **all figures in the analysis report's
per-cell tables, run-order receipt, and route/trust table reproduce exactly**,
including: token medians 476,423 / 314,223 / 925,568 / 1,038,580; MADs 69,785 /
64,483 / 145,865 / 182,854; wall medians 100.483 / 90.778 / 217.209 / 232.785 s
with matching MADs; zero-call runs 0 in all cells; first substantive tool relevant
in all 20 runs (`search_symbols`/`search_text` on full, `symforge` on compact,
one `read_mcp_resource` in run 06); SymForge completed calls 43/21/176/28 with
successes 43/14/176/18 and errors 0/7/0/10; native fallback runs 5/5 in every
cell with command counts 45/46/20/186; immediate retries 7 (S1-compact) + 5
(S2-compact) = 12 under the strict definition (next *completed* event of any
channel is another call to the same facade); citation-error runs 5/5, 5/5, 3/5
(04, 10, 20), 4/5 (02, 07, 12, 18). The 20-row run-order receipt matches the
ledger row-for-row.

Native fallback classification (command strings inspected privately, not
reproduced): 189 search-class and 108 read-class completed native commands; zero
build/test/package commands across all 20 runs.

**Admissibility ruling:** with 0/5 frozen-oracle passes in every cell, no
success-conditioned token or speed comparison is admissible from this series. The
descriptive per-cell dispersion is valid for harness validation and for
timeout/storage planning only. The report draws exactly this line and does not
cross it (no pooling, no headline saving, no significance test, no causal claim).
Correct.

## Phase 4 — compact error anatomy

Verified call-by-call from the raw traces (structured item extraction, not text
grep):

- **17 failed compact SymForge calls; full has zero.** Confirmed: 17 items with
  `status=failed`, server `symforge`, all in the two compact cells; zero failed
  MCP calls in any full run.
- **Four pre-dispatch decode failures.** Confirmed: result text
  `failed to deserialize parameters: unknown variant …, expected one of 'orient',
  'find', 'read', 'trace', 'impact', 'edit', 'meta', 'auto'` with free-form intents
  (`repository code investigation`, `retrieve`, `investigate code`, `investigate`)
  in runs 09, 13, 16, 19. These carry no `symforge/result_status` metadata —
  consistent with serde rejecting the closed `IntentBucket` enum
  (`src/stel_core/types.rs:13-22`, `#[serde(rename_all="snake_case")]`, and
  `StelRequest.intent: Option<IntentBucket>` at line 83) before facade dispatch.
- **Thirteen facade failures with `symforge/result_status.outcome_class=
  invalid_request`.** Confirmed; their envelopes show planner routes ending in
  `reject` against leaves `search_files`, `search_text`, `search_symbols`,
  `find_references`, `get_file_context` (route confidence mostly `inferred`, one
  `fallback`). Note one *additional* completed (non-failed) run-13 call whose
  payload also contains the `invalid_request` string — text matching alone
  overcounts to 14; the item-status classification is the correct one.
- **No `error_class` or `retryable` metadata on any of the thirteen.** Confirmed
  per call, and structurally: the `ResultStatus` contract
  (`src/protocol/result_status.rs`) defines only `outcome_class` (+ version); no
  such fields exist to populate.
- **12 immediate same-facade retries** across the two compact cells under the
  strict adjacent-completed-event definition (runs 7, 9×2, 12×2, 13×2, 16×3,
  19×2). Confirmed.
- **Native commands are read/search only; compact S2 186 vs full S2 20.**
  Confirmed.

Source corroboration (SymForge-indexed inspection): `IntentBucket` closed enum as
above; compact schema built from `schema_object::<StelRequest>()` in
`compact_surface_tools` (`src/stel/surface_list.rs`); compact decode/dispatch via
`call_statused!(symforge_facade_tool, SymforgeCallInput)` in
`src/protocol/mod.rs` with `invalid_request_result` minting the
`OutcomeClass::InvalidRequest` envelope on decode failure.

**Root-cause discipline:** the aggregate categories do NOT yet license a fix. The
report's own restraint is correct. Before assigning the thirteen routed rejects to
schema, planner selection, argument construction, leaf behavior, classification,
or envelope policy, extract per call: the exact request JSON shape class, the
planner's chosen leaf and constructed leaf arguments, the leaf's actual return
(empty/not-found vs malformed), which validation produced the reject, the exact
envelope text served, and what the model did next (retry argument delta vs native
fallback). In particular, distinguish "valid query the planner mis-armed" from
"valid leaf response misclassified as invalid_request" — the retained traces
contain enough to decide this without new runs.

## Phase 5 — product scope and next plan

- **Product diff:** `git diff src/stel/surface_list.rs` contains exactly the
  authorized change — `read_only_hint=Some(true)`, `open_world_hint=Some(false)`
  on compact `symforge` only, plus the `compact_surface_annotations_are_honest`
  regression test asserting `symforge_edit` and `status` are NOT read-only. The
  wiring receipt agrees (`symforge_edit_read_only=null`, `status_read_only=null`).
  No schema, description, routing, or admission change. In scope.
- **Next-plan challenge:** the report's eight commitments are evidence-proportionate
  with two flags:
  1. *Slight overreach:* "keep provider/host deferred discovery … as the leading
     confirmatory design" (Decision item 5, Checkpoint 4). A019 produced no
     observation about deferred discovery; the preference is imported from the
     reconnaissance, not from this data. Acceptable as a design default, but the
     report should not imply A019 evidence ranks it above alternatives. The
     client-neutral allowlist arm mitigates this.
  2. *Wording nit:* "referenced sensible leaf families (… `explore` …)" — no failed
     call *chose* `explore` as its leaf; it appears only inside envelope
     alternative-route text of 5 failures. Harmless but imprecise.
  All other elements — never re-grade A019; replace the production-inaccurate S1
  oracle; either prompt for the S2 mutation-before-clone order explicitly or drop
  it as an incidental wording trap (I recommend dropping it — it grades
  implementation trivia the prompt does not solicit); commit the annotation before
  building the confirmatory candidate; diagnose all 13 invalid requests before any
  schema-only rescue; keep compact-3 a separate rescue arm, not the default;
  postpone power planning until revised tasks yield successful outcomes — are
  correct and appropriately conservative.

## Findings by severity

### Required before A019 closure

None. All material claims verified; no grade, calculation, custody, or scope
problem was found. (The verdict below closes A019.)

### Required before confirmatory-pilot design

1. **S1 oracle production mismatch (carried from run-01 audit, now 10× confirmed):
   rewrite S1 to grade `ServerHandler::list_tools` → `compact_surface_tools`;**
   reserve `compact_probe_tools` for an explicitly probe-focused task. Frozen A019
   grades stand untouched.
2. **S2 criterion 5:** remove the mutation-before-clone ordering requirement or
   ask for it explicitly in the prompt; as frozen it failed 10/10 answers that were
   otherwise substantively correct.
3. **Citation-range brittleness:** pin symbol identity + candidate tree, and grade
   "line inside the symbol at the pinned tree" rather than a hand-frozen numeric
   range; 7/10 S2 answers failed criterion 6 on 1–4-line drift (e.g. 10788 vs
   10792, 394 vs 396) while naming the correct symbol.
4. **Commit the annotation prerequisite** (`src/stel/surface_list.rs`) before
   building the confirmatory candidate so fixture source and candidate binary share
   custody (currently divergent, symmetric, disclosed).
5. **Complete the Checkpoint-3 call-level diagnostic** of all 17 failures (fields
   listed in Phase 4 above) before deciding whether a schema-only rescue arm is
   justified.

### Optional hardening

6. State a disposition for the pre-restart invalidated baseline pair
   (`…-evidence-a10ff102`, `…-golden-a10ff102`; ~16.6 MiB combined) — retain the
   110 KB invalidated traces with the A019 archive, delete the pre-restart golden
   state.
7. Fix the two report wording nits: `explore` was never a failed call's chosen
   leaf; and note in the error-anatomy section that raw-text matching finds an 18th
   (completed) call containing the `invalid_request` string, so future tooling must
   classify by item status.
8. Record the `symforge_call_count` counting rule ("completed calls to the
   `symforge` server; host resource calls excluded") next to the
   `tool_event_count` caveat in the report, using run 06 as the worked example.

## Cleanup recommendation (primary agent executes after approval)

Deletable immediately after this approval:

- **Golden a019 state** (16.46 MiB): its purpose (per-run fixture seeding) is
  finished; the semantic baseline JSON fully identifies it.
- **Pre-restart golden state** `…-golden-a10ff102` (16.46 MiB).
- **Wiring quarantine** under the raw evidence root, after confirming the compact
  wiring receipt (already in-repo) is the surviving record.

Must remain until the compact call-level diagnosis (Checkpoint 3) completes:

- **All 20 raw event traces + stderr logs** (4.82 MiB) — they are the sole source
  for the per-call failure extraction.
- **Compact in-repo evidence** (0.37 MiB) — permanent.
- **Pinned candidate binary** (58.09 MiB) — retain until the annotation commit
  lands and a confirmatory candidate supersedes it; it is the only executable
  matching the frozen SHA if any trace question requires replaying a tool call.
- **Pre-restart invalidated traces** `…-evidence-a10ff102` (0.11 MiB) — cheap,
  historically required by Amendment A.

## Required ending

VERDICT: APPROVE_SHAKEDOWN_CLOSURE

**What A019 establishes.** A019 establishes that the harness is valid: 20/20 runs
captured complete, custody-clean records with one real usage event each, canonical
token totals reproducible from raw traces, deterministic blind-reproducible oracle
grading, zero exclusions, and full teardown. Substantively it establishes that the
Amendment A annotation makes the compact read facade callable and initially adopted
(first tool relevant in 20/20 runs), while compact *continuation* is untrustworthy
in this configuration: 17 of 49 compact SymForge calls failed (4 pre-dispatch
free-form-intent decode failures against the closed `IntentBucket` enum; 13 facade
`invalid_request` envelopes exposing no typed recovery metadata), driving 12
immediate retries and 186 native read/search fallbacks in compact S2 versus 20 in
full S2 — whereas the full surface produced zero tool errors in 219 completed
calls. It also establishes that both frozen oracles are unusable for confirmatory
gating (S1 grades a probe-only symbol against production truth; S2 grades an
unsolicited implementation-order sentence), and that within-cell variance is large
and measurable.

**What A019 does not establish.** It does not establish that any arm is a token,
speed, or quality winner; with 0/20 oracle passes there are no successful-task
observations, so no success-conditioned efficiency comparison, headline saving, or
sample-size estimate is admissible. It cannot attribute any full-versus-compact
difference to schema exposure, because the toggle simultaneously changes catalog
size and read routing. It does not establish the root cause of the 13 routed
invalid requests (planner, argument construction, leaf behavior, classification,
or envelope policy remain undecided), does not prove the compact facade
irredeemable, and does not show that the 93.7% schema-byte reduction predicts any
end-to-end saving.

**Required next actions before any product code change:**

1. Complete the call-level diagnostic of all 17 compact failures from the retained
   raw traces (request shape, chosen leaf, constructed arguments, leaf return,
   rejecting validator, envelope text, model's next action).
2. Author a new versioned manifest with repaired oracles: production-true S1;
   S2 without the mutation-before-clone trap (or with it explicitly prompted);
   symbol-identity-pinned citations; two independent graders agreeing on a blind
   replay before freezing.
3. Commit the truthful compact-annotation change (with its regression test) so the
   confirmatory candidate and fixture source share custody.
4. Execute the cleanup disposition above; retain traces and candidate through the
   diagnostic.
5. Only then design the confirmatory arms (canonical catalog / deferred discovery
   / client-neutral allowlist / compact-3 rescue) and defer power planning until
   revised tasks produce successful equivalent outcomes.
