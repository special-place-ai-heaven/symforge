# Plan review findings — Cursor (Grok 4.5) — 2026-07-28

Reviewer: Cursor / Grok 4.5
Brief: `PLAN-REVIEW-REQUEST-2026-07-27.md` (read-only; not written)
Artifact: `specs/021-admission-coverage-honesty/` (spec, plan, research, tasks)
Mode: read-only plan review; no source edits.
Output file: this file only (`PLAN-REVIEW-FINDINGS-cursor.md`)

---

## Findings (incremental)

### [HIGH] Phase 4 gates on WS5 work nobody schedules inside 021
**Where:** `tasks.md` Phase 4 (T119–T121); `plan.md` sequencing step 4; sift `tasks.md` Phase 9 T062–T082 (all unchecked)
**Claim under test:** 021 can close SF-DOG-001…005 and unblock ACH-03/04 by gating on the prerequisite.
**What I found:** Phase 4 "adds no implementation" and only confirms T062–T082 landed. Those 21 tasks are still unchecked in `specs/020-repository-knowledge-index/sift/tasks.md`. 021 has no task that says "implement T062–T066 / WS5B–E". A competent engineer building *exactly* the 021 task list stalls at T119 forever, or improvises WS5 outside the plan.
**Why it matters:** SF-DOG-001…005, and transitively ACH-03/04 VERIFY, cannot close from 021 alone. The inherited-by-ID design is sound only if WS5 is an explicit concurrent workstream with an owner and schedule — the plan never names that.
**Suggested fix:** Add an explicit Phase-4 work order: either (a) "implement sift T062–T082 as part of this branch, following sift/tasks.md" with a pointer and ownership note, or (b) a hard external dependency with a named owner / PR / stop condition. Do not leave "GATE: confirm landed" as the only instruction.

### [HIGH] ACH-01 ships a recovery pointer that ACH-04 proves is broken
**Where:** `tasks.md` T115; `plan.md` §D3; ledger SF-DOG-007 expected behavior vs SF-DOG-008
**Claim under test:** Fail-closed path-shaped miss is a complete fix for SF-DOG-007; the pre-registered tension with missing `new_file` mode is resolved.
**What I found:** T115's preferred miss outcome is `file_not_found` / new-file plan **pointing at** `analyze_file_impact(new_file=true)`. ACH-04 (and research §5) shows that seam has no admission override and currently refuses with a false reason. ACH-01 is the MVP and is allowed to ship alone. The plan never requires ACH-01's receipt to state the residual: "recovery advertised is not yet real until ACH-04." Ledger SF-DOG-007 *accepts* recommending `new_file=true`, so this is not inventing scope — but calling SF-DOG-007 closed while re-emitting SF-DOG-002/008's dead recovery is incomplete honesty.
**Why it matters:** An agent that stops after the MVP no longer edits the wrong file (good) but is still handed a coherent, failing recovery loop (bad). The brief's predicted tension is acknowledged only as D3 `file_not_found` vs first-class new-file plan — not as "refusal vs working new-file workflow."
**Suggested fix:** In T115/T117 receipt: explicitly mark SF-DOG-007 closed for *wrong-write only*, with residual "dead recovery pointer" owned by ACH-04. Or make the ACH-01 miss text refuse without naming `new_file=true` until ACH-04 lands.

### [HIGH] ACH-01 silently overlaps WS5C on `edit_plan.rs` without amending it
**Where:** `tasks.md` T111/T115 (ACH-01); sift `tasks.md` T072–T074 (WS5C / SF-DOG-002); hazard 5 claimed mitigation
**Claim under test:** 021 references WS5 by ID with exactly one amendment (T101); no silent duplication.
**What I found:** ACH-01 adds Tier-2 `metadata_only` disclosure and kills the fuzzy miss path in `edit_plan.rs` *before* Phase 4. WS5C T072–T074 still plans to make the planner admission-aware on the same file for SF-DOG-002. Hazard 5's mitigation ("exactly one amendment") is false for behavioral overlap: T101 only amends T066's elimination list. There is no amendment telling WS5C to preserve ACH-01's cascade veto / miss outcomes.
**Why it matters:** WS5C landing after ACH-01 can rewrite or dilute the MVP fix; or ACH-01 partially closes SF-DOG-002 without updating the exit ledger, so T174 can double-count or miss the residual.
**Suggested fix:** Amend WS5C (like T101) to say ACH-01 already owns path-shaped miss + Tier-2 disclosure; WS5C owns only "recovery tools named must actually work on metadata-only targets" (or merge WS5C into ACH-01's VERIFY). Update the exit table accordingly.

### [MEDIUM] ACH-03 does not actually need honest SkipReasons for its core fixes
**Where:** `plan.md` sequencing step 4/6; `tasks.md` "What unblocks what" §1; brief §4c causal claim
**Claim under test:** SF-DOG-004 (LOW) must gate SF-DOG-009 (HIGH) because one shared string makes the cause unassertable.
**What I found:** Source confirms `handlers.rs:901-904` maps `reason: None` → `"policy"`, and `health_view.rs:275-309` prefers manifest over `files`. Those are the SF-DOG-009 mechanisms. Tests T133/T134/T137/T140 can fail today *without* distinct SkipReasons: assert `Normal`+`None` never prints `reason: policy`, assert oracle agreement, assert typed race variant. Honest codes *are* required for ACH-04's same-reason receipt (SC-009) and for distinguishing real exclusions in diagnostics — not for the ACH-03 rendering/oracle/race core.
**Why it matters:** The causal claim is overstated. Sequencing ACH-03 after Phase 4 is conservative, not necessary. If WS5 stalls (Finding 1), ACH-03 is blocked for a wrong reason.
**Suggested fix:** Split ACH-03: rendering + StaleGeneration + oracle reorder can proceed after Phase 2; only "same honest reason string in exclusion diagnostics" waits on T062–T065. Soften the "004 gates two HIGHs" language accordingly.

### [MEDIUM] SF-DOG-008 VERIFY may go vacuous after T066 — plan caught this class for ACH-02 only
**Where:** `tasks.md` T153; research §5 / §7 Q1; plan ACH-02 vacuity mitigation (T123)
**Claim under test:** ACH-02's vacuity risk was caught; look for others missed.
**What I found:** Ledger SF-DOG-008's refused file renders as Tier 2 `unsupported language` — i.e. admission *saw* it and collapsed the reason (research §1 / store.rs:3360-3366, verified). If T143 finds `SensitiveContent` (or the false-positive rule), T066 alone can admit clean fixtures and make T153's "identifier found after one full index" pass **without** T149–T152. The plan correctly forces ACH-02 fixtures to be Tier-2-by-policy after the gate; it does not require ACH-04 fixtures to remain unsearchable *after T066* and *before* ACH-04 code, nor a RED recorded against that ordering.
**Why it matters:** T174 can tick SF-DOG-008 closed on demotion cleanup while `new_file` contract lies and untracked-match discard (tools.rs:2336-3351) remains.
**Suggested fix:** Mirror ACH-02: T145/T153 must use fixtures that stay excluded or unindexed for a *deliberate* non-secret reason after T066 (or assert the `new_file` promise/schema contradiction and the discard-to-prose path independently of "identifier found").

### [MEDIUM] T108 cites wrong `human_size` line; helper may be unreachable as claimed
**Where:** `tasks.md` T108; `plan.md` FR-013 / shared size; source `format.rs:1591` vs claimed `:3668`
**Claim under test:** One shared size fix at handlers.rs:905; reuse existing `human_size`.
**What I found:** `size_mb` at `handlers.rs:905` and `"policy"` default at `:901-904` are correct (verified). `human_size` lives at `src/protocol/format.rs:1591` (private `fn`), not `:3668`. T108 says "if not reachable from handlers.rs, note that" — good escape hatch — but the cited line is wrong and will waste implementer time.
**Why it matters:** Low correctness risk (escape hatch exists); accuracy defect in an otherwise load-bearing early phase.
**Suggested fix:** Point T108 at `format.rs:1591`; expect a local equivalent or `pub(crate)` promotion.

### [LOW] Line-number drift on health guard citation
**Where:** plan/spec cite `handlers.rs:344-346`; source has the exemption starting at `:345` with comment at `:342-343`
**Claim under test:** Load-bearing citations are checkable.
**What I found:** Behavior is correctly described (`/health` and `/stats` exempt). Off-by-one vs current source is cosmetic.
**Why it matters:** Does not block building.
**Suggested fix:** Optional cite refresh; not required to start.

### [LOW] Index-identity adoption is justified, not creep
**Where:** brief §4e; `plan.md` ACH-05; research §6
**Claim under test:** Adopting a tenth defect is scope creep.
**What I found:** Live two-curl reproduction is coherent; `/health` exemption and identity-less `HealthResponse` (`handlers.rs:88-93`) verified. Measurement channel binding to MCP `status` is the right interim control. Not creep.
**Why it matters:** N/A — anti-finding for the scope lens.
**Suggested fix:** None.

---

## Hazard register (§5) — tested

| # | Hazard | Verdict |
|---|---|---|
| 1 | undetermined dressed as determined | **Pass.** T103/T132/T143/T155 require recorded output before fixes. No fix task I found silently assumes those answers. |
| 2 | VERIFY steps that cannot fail | **Mostly pass**, with MEDIUM vacuity risk on T153 after T066 (above). ACH-02 vacuity handling is real. T110/T117 `Constant ts` absent is a genuine failing assertion (cascade at `edit_plan.rs:107-113` verified). |
| 3 | false negative → wrong answer | **Pass for selectors** if D3/T110/T112 hold (`Type.Method` / `Foo::bar`). **Partial fail for recovery:** ACH-01 avoids wrong write but can still recommend a non-working `new_file=true` path. |
| 4 | sequencing that doesn't unblock | **Partial fail.** Honest codes do unblock ACH-04 same-reason checks; they do **not** strictly unblock ACH-03's core. WS5-not-scheduled is the larger sequencing hole. |
| 5 | silent WS5 duplication | **Fail.** T101 is real for T066's elimination list; ACH-01↔WS5C overlap is unamended duplication on `edit_plan.rs`. |
| 6 | `0.0 MB` as three bugs | **Pass.** One precision bug at `handlers.rs:905` `{:.1}` MiB; appears in four findings. |

Pre-registered tension (007 fail-closed vs no new-file mode): **not resolved.** Plan chooses refuse + pointer to broken recovery and treats 007 as closable at ACH-01 checkpoint.

---

## Completeness map (ten defects)

| Defect | Closing work | Orphan? |
|---|---|---|
| SF-DOG-001 | WS5B T068–T071 + T168 | No task *implements* WS5B inside 021 — orphaned unless Finding 1 fixed |
| SF-DOG-002 | WS5C T072–T074 + size | Same; also overlapped by ACH-01 |
| SF-DOG-003 | WS5D T075–T077 | Same orphan class |
| SF-DOG-004 | WS5A T062–T066 + T101/T119/T120 | Same; T101 amendment is correct and evidence-backed |
| SF-DOG-005 | WS5E T078–T080 | Same orphan class |
| SF-DOG-006 | ACH-02 T122–T131 | Covered; fixture vacuity handled |
| SF-DOG-007 | ACH-01 T110–T118 | Covered for wrong-write; residual recovery honesty deferred undocumented |
| SF-DOG-008 | ACH-04 T143–T154 | Covered in plan; VERIFY vacuity risk after T066 |
| SF-DOG-009 | ACH-03 T132–T142 + size | Covered; over-gated on WS5 |
| index identity | ACH-05 T155–T167 | Covered; justified adoption |

T174's receipt table is strong *if* filled with failing assertions — but it can be ticked for 001–005 by pointing at WS5 tests that never ran on this branch. Gate that: receipt must name a command that was executed on the closing commit.

---

## Source citation spot-checks (load-bearing)

| Claim | Result |
|---|---|
| `edit_plan.rs:90` path_shaped; `:107` cascade with full path | **Confirmed** |
| Comment `:104-106` sanctions fall-through | **Confirmed** |
| `secret.context-assignment` over-broad regex `knowledge/mod.rs:89-95` | **Confirmed** (lines 90–94) |
| `is_placeholder` misses `{canary}` / code RHS | **Confirmed** (`:130-157`) |
| store collapse seven reasons → UnsupportedLanguage `:3360-3366` | **Confirmed** |
| `health_view` manifest-first `:280-296` then files `:297-306` | **Confirmed** |
| handlers `None`→`"policy"` `:901-904`; size MiB `:905` | **Confirmed** |
| `/health` exemption comment `:342-343` | **Confirmed** (cite off-by-one) |
| `HealthResponse` no identity `:88-93` | **Confirmed** |
| `metadata_only_skipped_paths` `query.rs:1243` | **Confirmed** |
| `render_file_content_bytes` no around_match; fallthrough `:3212` | **Confirmed** |
| Untracked exclusion default OFF `discovery:2192-2203` / `SkipReason` docs | **Confirmed** |
| PR #479 merged | **Confirmed** (`mergedAt` 2026-07-27T14:53:35Z) — ACH-04 gate clear |
| `human_size` at `format.rs:3668` | **False** — at `:1591` |

Security invariant claim: tightening the rule removes false positives rather than relaxing Sensitive* reads — **holds**, provided T121's canary/placeholder distinction is enforced in WS5 T066 (not specified in 021 beyond the gate text).

---

## Verdict

VERDICT: FIX-FIRST (2 blockers)

Blockers to clear before build-as-written:

1. **Phase 4 gates on WS5 work nobody schedules inside 021** — 001–005 (and ACH-03/04 as currently sequenced) cannot close from the 021 task list alone.
2. **ACH-01↔WS5C silent overlap on `edit_plan.rs`** — hazard 5 mitigation is incomplete; MVP and SF-DOG-002 will thrash or double-own without an amendment.

(The ACH-01 broken-recovery pointer is HIGH but not a build-stopper if residual is explicit; treat as required receipt text, not a third blocker.)

**Crux:** The plan's strongest idea — inherit WS5 by ID — is also its failure mode: without a scheduled implementer for T062–T082, "until all is green" is a checklist over a missing workstream.

**Confidence and its limits:**
- Checked: all four artifacts; dogfood ledger SF-DOG-007/008 sections; load-bearing sites in `edit_plan.rs`, `knowledge/mod.rs`, `store.rs`, `health_view.rs`, `handlers.rs`, `format.rs`, `discovery/mod.rs`, `query.rs`, `domain/index.rs`; PR #479 state; sift Phase 9 task text.
- Did not: re-run the 29/29 demotion probe; index testpilot to settle T103/T143; enumerate every `render_file_content_bytes` caller; execute any cargo gate; read full ledger observations 613–881 beyond the deferred list.
- Did not manufacture LOWs for line-noise; source citations were mostly accurate where load-bearing.
