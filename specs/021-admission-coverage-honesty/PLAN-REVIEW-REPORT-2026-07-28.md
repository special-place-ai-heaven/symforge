# Adversarial plan review — Feature 021 (admission coverage honesty)

**Reviewer:** independent reviewer 2 of 2 (same brief, `PLAN-REVIEW-REQUEST-2026-07-27.md`)
**Date:** 2026-07-28
**Method:** read all four artifacts end-to-end; spot-checked every load-bearing `file:line` citation
against the live source; checked the inherited prerequisite (`sift/tasks.md` Phase 9), the ledger's
deferred-observations section, and PR #479's merge state. Read-only; nothing was edited or built.

**Citation verification summary (all confirmed accurate):**
`knowledge/mod.rs:89-95` (rule, no left boundary, `[^\s"'#]{8,}` value class) ·
`store.rs:3360-3366` + `:3376-3380` (7-variant + 3-variant collapse) + `:3673` + `:3780-3795`
(hardcoded `UnsupportedLanguage`) + `:3394-3405` (lossy reverse map) ·
`domain/index.rs:1424` (`"unsupported language"`) ·
`edit_plan.rs:90` (predicate computed, never consulted in the `:103` match) + `:107` (cascade fed the
full path) + `:133-135` (unconditional `search_symbols` hint) ·
`disambiguation.rs:438-442` (`strip_qualification`, `rsplit_once('.')`) ·
`health_view.rs:275-309` (manifest-first, `files` fallback returns `Normal`/`None`) ·
`handlers.rs:88-93` (no identity in `HealthResponse`) + `:344-346` (`/health`,`/stats` exempt) +
`:901-904` (`None → "policy"`) + `:905` (`/ 1 MiB` at `{:.1}`) + `:918-923` + `:926`/`:935`
(generation computed, one branch prints) + `:958`/`:1113` (both route `Skipped` → policy renderer) +
`:800` (promise) + `:892-894` (disclaimer) ·
`watcher/mod.rs:277-284` (no override parameter) ·
`query.rs:1233-1235` (`all_files` = Tier 1) + `:1243-1256` ·
`format.rs:3061-3100` (dispatch order) + `:3176-3225` (three selectors dropped, whole-file return at
`:3212`) + `:3497-3526` + `:3508` + `:3518-3522` (refusals exist, gated on `&IndexedFile`) +
`:3305` + `:3569`/`:3594` (60 KB cap) + `human_size` use ·
`tools.rs:2184` + `:2191-2198` + `:2336-2365` + `:2410-2421` + `:3316-3323` + `:3351` +
`:8532-8538` + `:8588`/`:8607`/`:8618-8621` ·
`port_file.rs:161`/`:206`/`:234-241`/`:283-289`/`:337-348`/`:495-505` ·
`hook.rs:255` + `:865-868`/`:885` + `:893-901` + `:1041-1058` + `:1083-1091` ·
`discovery/mod.rs:2192-2197` (opt-in env gate) ·
`sift/tasks.md` T062–T082 exist, T066's elimination list contains the exact text T101 amends ·
PR #479: **MERGED 2026-07-27T14:53:35Z**, footprint matches the plan's list file-for-file.

No mis-cited edit site was found. The plan's picture of the code is accurate everywhere I looked —
with one enumeration exception, which is the first finding.

---

## Findings

### [BLOCKER] T137/FR-009 under-enumerate the race-loss `Skipped` sites — SF-DOG-009 stays reproducible on three sibling paths

**Where:** `tasks.md` T137; `spec.md` FR-009; `research.md` §4; source `src/watcher/mod.rs:359, :415, :453, :511, :539, :573, :579`

**Claim under test:** `ReindexResult::Skipped` is returned from "≥8 sites, four of which are
optimistic-concurrency losses" (`:352-359`, `:539`, `:566-573`, `:576-579`); converting those four to
`StaleGeneration` closes the race-vs-admission conflation.

**What I found:** There are **seven** non-admission stale/abort returns in
`read_and_index_with_stable_read`, not four. Every terminal-disposition publish uses the same
three-step pattern (publish → success means genuine admission → `Skipped`; generation moved → retry;
stale → `Skipped`). The stale arms are at:

- `:358-359` — stale metadata-terminal admission (**named** in T137)
- `:415` — stale **generated-output** publication (omitted)
- `:453` — stale **hard-skip** publication (omitted)
- `:511` — stale **content-policy** publication (omitted)
- `:539` — stale hash-skip (**named**)
- `:573` — stale indexed-file publication (**named**)
- `:579` — abort after `MAX_PUBLICATION_ATTEMPTS` (**named**)

T137 converts four and explicitly says "leave the genuine admission and scope/eviction returns
(`:303-314`) as `Skipped`" — but `:415`, `:453`, and `:511` are not genuine admission outcomes; they
are lost races on the generated-output, hard-skip, and content-policy arms. After the phase lands, a
race lost on any of those three paths still returns `Skipped` → `handlers.rs:958`/`:1113` →
`impact_skipped_text` → the byte-for-byte false refusal SF-DOG-009 reports. Research §4's count
("four") is wrong; T132's "which of the ≥8 `Skipped` sites fired" should read ≥12 (the grep shows 12
`Skipped` returns in this function alone). Because `Skipped` remains a valid variant, compiler
exhaustiveness will **not** catch the omission.

**Why it matters:** The plan's exit criterion is "all nine findings closed with receipts." T135's
race test will pass against one of the four converted sites, T174's receipt table will be ticked,
and SF-DOG-009 remains live on three paths. This is the exact failure mode hazard #2 and #4
pre-registered: a receipt that doesn't cover the defect's full surface.

**Suggested fix:** Mechanical, not design — in T137, convert **every** `return
ReindexResult::Skipped` that follows a failed `publish_*_at_generation` retry check (grep
`ReindexResult::Skipped` in `watcher/mod.rs`; they are the stale arms at `:359, :415, :453, :511,
:539, :573` plus the `:579` abort) to `StaleGeneration`. Keep `Skipped` only for publish-success
arms (`:350, :407, :442, :503`) and scope/gitignore eviction (`:313`). Amend FR-009's site list and
research §4's count.

### [MEDIUM] ACH-01's "real reason" disclosure depends on the Phase 4 reason split it declares independence from

**Where:** `tasks.md` T111, T115; `spec.md` US1 acceptance 2; `plan.md` "Implementation order" step 3;
source `src/live_index/query.rs:1243-1256` → `src/live_index/store.rs:3350-3390`

**Claim under test:** ACH-01 "depends on nothing — it does not wait on the admission root cause, on
the honest reason codes, or on any other story," and T115 delivers a `metadata_only` disclosure
"with the real reason … no new API."

**What I found:** `metadata_only_skipped_paths()` projects each manifest entry through
`compatibility_admission_decision` — the exact collapse WS5A (T062–T065) doesn't fix until Phase 4.
So at ACH-01's own VERIFY (Phase 3, before the gate), a `SensitiveContent`-demoted path discloses
`metadata_only` with reason **"unsupported language"** — the SF-DOG-004 lie, inside ACH-01's own
acceptance scenario 2. The end state is fine (T064/T065 make the same accessor truthful, and ACH-01
then inherits honesty for free), and the *guard* genuinely is independent. But "the real reason …
no new API" is only true post-split; pre-split, the true `MetadataOnlyReason` is available only by
reading the manifest entry's `FileDisposition` directly, which the plan explicitly declines to add.

**Why it matters:** The plan's strongest sequencing claim ("fully independent MVP, landable alone")
is overstated for the reason content, and T111's test as written can silently pin the collapsed
reason if the fixture happens to be sensitive-demoted rather than policy-demoted.

**Suggested fix:** Constrain T111's fixture to a policy demotion whose reason survives the collapse
honestly today (lockfile / oversized-data), and add one sentence to T115: the reason string is only
as honest as `compatibility_admission_decision` until T065 lands; reading `FileDisposition` directly
is the alternative if an earlier truthful reason is wanted. Do not reorder Phase 3 — the MVP
sequencing is right.

### [MEDIUM] Phase 5 adds lexical reads of Tier-2 files with no task gating on security dispositions

**Where:** `tasks.md` T122–T131 (absence); `spec.md` "Security invariant" + edge cases ("any raw read
added by this feature MUST respect the frozen security invariant"); `plan.md` §D1 ("`ACH-02`'s …
fallback scope … depend on this answer")

**Claim under test:** The frozen invariant governs every raw read this feature adds.

**What I found:** No Phase 5 task refuses `SensitivePath`/`SensitiveContent`-demoted files in the
new `around_match`/`chunk_index` dispatch in `render_file_content_bytes`, and no RED test asserts
that refusal. Plan.md D1 correctly records that ACH-02's fallback scope depends on T104's ruling —
but the dependency never materializes as a task. Mitigating fact, checked: the existing whole-file
raw-disk fallback (`tools.rs:8588-8607`) already serves those bytes today, so this is not a *new*
exposure class; around_match is a bounded view of an already-readable file. Related edge: T152's
option A merges proven match *locations* from untracked files into `result.files`; untracked files
carry no recorded exclusion, so a would-be-sensitive untracked file's match locations would surface
without any detector consultation (today only the path name leaks).

**Why it matters:** An implementer following tasks.md literally ships code that violates the spec's
own restated security invariant, with FR-026 providing no test to catch it. The ruling the plan
defers to (T104) lands in Phase 1; the consuming gate was never tasked.

**Suggested fix:** Add one task in Phase 5 (or fold into T129): the new selector branches consult
the manifest disposition and refuse `SensitivePath`/`SensitiveContent` files, with a RED test. For
T152 option A, either run the detector over untracked candidates before merging locations or record
in T104 that deliberate named-file reads are out of invariant scope — explicitly, not by silence.

### [MEDIUM] SF-DOG-001…005 closure is gated on work no 021 task schedules

**Where:** `tasks.md` Phase 4 (T119–T121), exit-criteria table; `specs/020-repository-knowledge-index/sift/tasks.md` Phase 9

**Claim under test:** "All nine ledger findings closed with receipts" by building this plan.

**What I found:** T119–T121 are gates, not implementations. The implementing tasks (T062–T082) sit
**unchecked** in the sift slice's task list, and no 021 task — and no stated owner anywhere in the
four artifacts — schedules their execution. Referencing rather than duplicating is the right call
(the brief's §6 records it as deliberate), and the gates fail loudly rather than lying, so this is
not hidden. But the plan's own goal ("until all is green") has an unowned prerequisite: an engineer
who builds exactly 021's tasks.md cannot close five of the ten exit-criterion findings.

**Why it matters:** Orphaned exit criteria. The brief's completeness lens asks "does every defect
have at least one task that would actually close it" — for 001…005 the answer is "yes, in another
slice nobody is scheduled to run."

**Suggested fix:** One sentence in tasks.md Phase 4 stating the execution intent — e.g. that
T062–T082 are built first, on the same branch, as part of this feature's delivery (with T101's
amendment applied), or that 021's exit criterion is explicitly conditioned on the sift slice
shipping. Either is honest; silence is not.

### [LOW] The "SF-DOG-004 gates two HIGHs" causal claim is true for ACH-04, overstated for ACH-03

**Where:** `plan.md` step 4 / "What unblocks what" #1; `spec.md` "The one thing that makes
everything else observable"

**What I found:** For ACH-04 the dependency is real — SC-009 requires the index receipt and the
search response to name the *same* reason, which is untestable while ≥11 causes print one string.
For ACH-03 it is weaker than claimed: T137's variant, T139's oracle reorder, and T140's render fixes
(drop the `None → "policy"` default, treat `Normal`+`None` as internal inconsistency) are all
implementable and testable *without* distinct `SkipReason` variants — none of T133–T136's RED
assertions needs the split to be meaningful. The sequencing is harmless (conservative, and T141's
honesty assertions benefit), so this is accuracy, not structure: the claim "ACH-03 **cannot**
distinguish a real policy exclusion from `reason: None`" is true of the *current* code but not of
the post-T140 code.

**Suggested fix:** Reword the dependency as "ACH-03's *final truthfulness assertions* presume the
split; its structural fixes do not." No resequencing needed.

### [LOW] FR-008's "truthful with no `tools.rs` edit" holds for the honor path, not the refusal path

**Where:** `tasks.md` T131; `spec.md` FR-008; `plan.md` §D4 consequence; source
`src/protocol/tools.rs:8532-8538, :8618-8621`

**What I found:** The mode annotation is built from the *request* and prepended in `tools.rs` before
the renderer runs. When T129 honors a selector, the annotation becomes truthful, as claimed. When
the renderer *refuses* (absent literal; `around_symbol` per the recommended D2 refusal), the header
still asserts `── mode: match (explicit) ──` / `── mode: symbol (explicit) ──` — a mode the response
did not service — and only the body contradicts it. FR-008 says the annotation "MUST become
truthful," and the plan's own mechanism cannot fully deliver that for refusals without the
`tools.rs` edit it routes around. T131's assertion (annotation + first line number in one test)
covers only the honor path. Note the routing constraint has expired anyway: PR #479 **merged**
2026-07-27T14:53:35Z (verified via `gh`), so `tools.rs` is editable; the spec's "Blocked on" language
in US4 and the T144 gate are now stale-but-harmless.

**Suggested fix:** Extend T131's test to the refusal case (annotation must not assert a mode the
body refuses — simplest truthful form: suppress the annotation when the renderer's first line is a
structured refusal), and take the now-free `tools.rs` edit if that is cleaner than renderer-side
gymnastics. Also refresh the US4 "Blocked on" note and T144 to record that the gate has passed.

### [LOW] T142's watcher-event race reproduction is timing-sensitive with no flake budget

**Where:** `tasks.md` T142, T135

**What I found:** "Repeat under an active watcher event … confirm the stale-generation response
succeeds after one refresh" is a real assertion against a real race — which means it is inherently
timing-dependent, and the plan gives no retry/witness strategy (e.g. barrier on the `trace!` at
`watcher/mod.rs:572`, or a forced generation bump via a second writer) to make the collision
deterministic. Every other VERIFY in the plan is deterministic; this one can pass by never actually
losing the race.

**Suggested fix:** In T135/T142, require the test to *witness* the loss (assert the
`StaleGeneration` variant was observed at least once across N forced-collision iterations, with the
collision driven by a controlled second publisher rather than wall-clock luck).

---

## Hazard scorecard (§5 of the brief)

| # | Hazard | Plan's claim | My assessment |
|---|---|---|---|
| 1 | undetermined dressed as determined | 4 investigation-first tasks | **Held.** T103/T132/T143/T155 all require recorded outputs; stop conditions are explicit. No fix task I found silently assumes Q1–Q3. |
| 2 | VERIFY steps that cannot fail | T174 receipt table | **Mostly held.** Every VERIFY names command + failing assertion; I found none that passes regardless. Two partial gaps: T135's race test covers only 4 of 7 race paths (BLOCKER above), and T142's race reproduction can pass without witnessing the race (LOW). |
| 3 | trading a false negative for a wrong answer | T121 guards it | **Held.** T121 explicitly distinguishes "recognized as placeholder" from "no longer matched," and requires existing detector tests green. The detector fix adds a boundary rather than deleting a rule. |
| 4 | sequencing that doesn't unblock | one shared string ⇒ untestable | **Half held.** True for ACH-04 (SC-009), overstated for ACH-03 (LOW above). Sequencing itself is safe. |
| 5 | silent duplication of WS5 | referenced by ID, one amendment | **Held.** T101 is the only edit; the amended text exists verbatim at `sift/tasks.md` T066. But see the MEDIUM on WS5 being unscheduled. |
| 6 | `0.0 MB` = one bug or three | one precision bug at `handlers.rs:905` | **Held.** Verified: `byte_len` is populated (`health_view.rs:301`), divided by 1 MiB, rendered `{:.1}` — one bug, one task is correct. |

**Pre-registered tension (SF-DOG-007 fail-closed vs. no `new_file` mode): resolved.** The plan does
not ship a refusal and call it a fix. FR-003/T115 emit `file_not_found` **plus** a pointer to
`analyze_file_impact(new_file=true)` (research §7 D3 records the ruling), and ACH-04/T149
independently forces the `new_file` contract to be real (override or honest schema). The pointer is
valid precisely for the eligible-new-file case; ACH-04 covers the excluded-file case. This is the
correct decomposition.

---

## Verdict

VERDICT: FIX-FIRST (1 blocker)

Blocker: **T137/FR-009 under-enumerate the race-loss `Skipped` sites** (`watcher/mod.rs:415, :453,
:511` unconverted) — as written, SF-DOG-009's receipt can be produced while the defect remains
reproducible on three sibling paths, so the plan cannot meet its own exit criterion. The fix is
mechanical (three more arms, same pattern), not structural; nothing else about the plan's shape
needs to change.

**Crux:** the plan's enumerations of behavior-altering sites are exhaustive — falsified exactly once
(race sites: research said ≥8/four-race, source shows 12/seven-race), and every other fix rests on
the same "we listed all the arms" trust. The BLOCKER is worth fixing partly because re-running that
grep is cheap insurance for the other phases.

**Confidence and its limits:**
- I verified every load-bearing citation I list above against the live source, plus the sift
  prerequisite's existence and T066's amended text, plus PR #479's merge state. I did **not**
  re-run the 29-file demotion measurement or the two-curl reproduction (read-only review; the
  research evidence is internally consistent and matches the code I read).
- I did not execute any test or build; RED/VERIFY feasibility is assessed from the cited fixtures
  and APIs, all of which exist.
- I skimmed rather than exhaustively re-derived the ledger's deferred observations (lines 613–881):
  they are recurrences of 001/004 plus the five items the plan names as deferred; nothing
  load-bearing appeared to be dropped, but I did not trace each of the ~15 entries to a root cause.
- Q1–Q3 are open by design; whether the investigation tasks can actually reach their recorded
  answers (T103/T143 need a testpilot-bound index) is assumed feasible, not proven.
