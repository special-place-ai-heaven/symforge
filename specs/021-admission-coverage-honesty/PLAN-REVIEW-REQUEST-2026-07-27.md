# Adversarial plan review request — Feature 021 (admission coverage honesty)

**Date:** 2026-07-27
**Repo:** `E:\project\symforge` (Rust, SymForge MCP server) — branch `feat/knowledge-llm-sift`
**Artifact under review:** `specs/021-admission-coverage-honesty/` — `spec.md` (494 lines), `plan.md` (328), `research.md` (394), `tasks.md` (740)
**Status:** untracked, uncommitted, no source changed. Nothing has been built from it yet.
**You are one of two independent reviewers** working from this same brief. You will not see the other's findings. Do not try to guess them.

---

## 1. What you are reviewing

A SpecKit plan that claims it will close **nine reproducible SymForge defects** plus one adopted defect, "until all is green."

You are reviewing **the plan**, not the code. The question is not "is SymForge broken" — that is established. The question is: **if a competent engineer built exactly this plan, would all ten defects actually end up closed, verifiably, without new breakage?**

Read all four artifacts. Read the source files they cite — every `file:line` in the plan is checkable and you should check the load-bearing ones.

**Do not edit anything.** Read-only. Findings go in §7 of this document.

---

## 2. The defects being closed

Source ledger (read-only, in a different project): `E:\project\testpilot\.scratch\symforge-dogfood-issues-2026-07-27.md`

| ID | Severity | One-line |
|---|---|---|
| SF-DOG-001 | HIGH | exact text search silently misses metadata-only source files — unqualified "no matches" |
| SF-DOG-002 | HIGH | `edit_plan` dead-ends and recommends tools with the same blind spot |
| SF-DOG-003 | MEDIUM | `what_changed` default contradicts its own schema wording |
| SF-DOG-004 | LOW | admission diagnostic reports "unsupported language" for a supported language |
| SF-DOG-005 | LOW | compact health admission counts are ambiguous |
| SF-DOG-006 | MEDIUM | Tier-2 `around_match`/`around_symbol` ignore the request and return the file from line 1 |
| SF-DOG-007 | **HIGH** | `edit_plan` resolves a path-shaped target by fuzzy symbol match and recommends destructive edits on an unrelated file |
| SF-DOG-008 | HIGH | a successful full index leaves eligible untracked source unsearchable; `new_file=true` does not admit it |
| SF-DOG-009 | HIGH | context says "Tier 1, parsed"; impact says "not indexed, Tier 1, policy, 0.0 MB" — same file, same generation |
| (adopted) | — | index identity: two call paths simultaneously reported different projects' index figures |

---

## 3. Context you need, so you do not re-derive it

### 3a. The measured root cause of the demotion

Most of the ledger is downstream of **one over-broad regex**. Rule `secret.context-assignment` at `src/knowledge/mod.rs:89-95` has no left word boundary and a value class (`[^\s"'#]{8,}`) that ordinary code satisfies. So `let token = token.to_lowercase();` is classified as a secret. One finding anywhere in a file demotes the **whole file** to metadata-only; symbols are dropped and the byte buffer discarded before parsing.

This was verified behaviourally, 29/29: 19 predicted-sensitive files are all absent from the live index, 10 predicted-clean are all present — including the counter-intuitive cases (309 KB `store.rs` demoted, 97 KB `knowledge_authority.rs` admitted, 1.26 MB `tools.rs` demoted, 250 KB `format.rs` admitted). **Zero genuine secrets among all 29 findings.** This is why size, parse failure, and CRLF were all correctly eliminated and the cause still went unfound.

The true reason is then erased twice: hardcoded to `SkipReason::UnsupportedLanguage` at `src/live_index/store.rs:3780-3795`, and independently collapsed from seven sibling variants at `:3360-3366`. It renders as "unsupported language" (`src/domain/index.rs:1424`) with a size appended (`src/protocol/format.rs:3658-3686`) — a language claim and a size, both false.

### 3b. What already exists and must NOT be rewritten

`SIFT-WS5` — `specs/020-repository-knowledge-index/sift/spec.md` User Story 6 (~line 166) and `sift/tasks.md` Phase 9, tasks **T062–T082** — already covers **SF-DOG-001…005** with real targets. Feature 021 deliberately **references it by ID** rather than restating it, and makes exactly **one** amendment (T101). Duplication here is a defect; check for it.

### 3c. Non-negotiable constraints

1. **Frozen security invariant (Feature 020):** a bounded lexical fallback must NEVER read files excluded for `SensitivePath` or `SensitiveContent`. The plan claims fixing the over-broad rule removes *false positives* rather than relaxing this invariant — test that claim.
2. Repo gates: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release`, `cargo check --no-default-features --features embed`, `cd npm && npm test`.
3. Task IDs must not collide with `T001–T082` (the sift slice). 021 starts at T100.

---

## 4. What to attack

Prioritise these. They are the failure modes that would actually make the plan fail if built as written.

### 4a. Completeness
- Does **every** one of the ten defects have at least one task that would actually close it? Name any orphan.
- Does the exit criterion (T174's receipt table) actually establish closure, or is it a checklist that could be ticked without proof?

### 4b. Verification integrity — the highest-value lens
- **Does any VERIFY task pass whether or not the fix landed?** A check that cannot fail is not evidence. Name every one you find.
- Where a task claims a fixture exercises a path, would that fixture still exercise it *after the earlier phases land*? (The plan claims it caught one such vacuity risk in ACH-02 — verify that reasoning, and look for others it missed.)
- Does each VERIFY name a runnable command, and an assertion that would fail in the fix's absence?

### 4c. Sequencing
- The plan asserts a LOW-rated finding (SF-DOG-004, honest reason codes) **gates two HIGHs** (008, 009), because while all causes print one string no test can assert which fired. **Is that causal claim true, or merely plausible?**
- Does any 021 task depend on a WS5 task (T062–T082) that nobody has scheduled?
- Is anything sequenced *later* than its dependents?

### 4d. Correctness of the fixes as specified
- For each named edit site: is that actually where the behaviour lives? Cite `file:line` when you disagree.
- Would the specified change produce the claimed behaviour, or only appear to?
- **Does any fix trade a false negative for a wrong answer?** That is the dangerous direction. Specifically: does the SF-DOG-007 fail-closed guard break legitimate targets (`Type.Method`, `Foo::bar`) or legitimate new-file planning?
- Does correcting the detector rule risk a false **negative** — a real secret now admitted and indexed?

### 4e. Scope
- 75 tasks across 9 phases. **Is this right-sized, or is there scope creep?**
- One defect (index identity) was **adopted** — it is not one of the nine. Justified, or creep?
- The plan defers most of the ledger's "Observations awaiting isolated reproduction" (ledger lines 613-881). Is anything load-bearing deferred?

### 4f. Undetermined vs determined
Four findings have genuinely unknown causes and get investigation-first tasks (T103, T132, T143, T155). **Check that no fix task elsewhere silently assumes a cause that is still open.** A plan that pretends to know an unknown cause is worse than one that names it.

---

## 5. Six hazards pre-registered before the plan was read

These were written down *before* anyone saw the draft, precisely so they could not be rationalised away afterwards. The plan claims to address all six. **Test each claim rather than accepting it** — and say plainly if a claimed mitigation is cosmetic.

| # | Hazard | The plan's claimed handling |
|---|---|---|
| 1 | undetermined dressed as determined | 4 investigation-first tasks before any fix |
| 2 | VERIFY steps that cannot fail | T174 receipt table requires the failing assertion |
| 3 | a fix trading a false negative for a wrong answer | T121 guards the false-negative direction |
| 4 | sequencing that doesn't unblock what it claims | causal argument: one shared string ⇒ no test can assert which cause fired |
| 5 | silent duplication of WS5's T062–T082 | referenced by ID; exactly one amendment (T101) |
| 6 | `0.0 MB` treated as one bug when it may be three | one bug — `sidecar/handlers.rs:905` divides by 1 MiB at `{:.1}` |

One tension was also predicted in advance and you should judge whether the plan resolves it: **SF-DOG-007's fail-closed fix may collide with the ledger's separate complaint that `edit_plan` has no `new_file` mode at all.** Failing closed converts a wrong answer into a hard refusal — strictly better, but the new-file workflow still has no path forward. Does the plan say which of those it is solving, or does it ship a refusal and call it a fix?

---

## 6. Already acknowledged — do not spend effort here

These are known and recorded. Flagging them again is not a finding.

- The plan does not rewrite WS5; that is deliberate.
- `data-model.md` and `contracts/` were deliberately not created, with stated reasons.
- The git-worktree index-identity case is unreproduced and recorded as such.
- Per-range secret redaction (vs full-file demotion) is deferred to an explicit owner decision (T104).
- PR #479 was a blocker for ACH-04; it merged at 2026-07-27T14:53:35Z, so that gate is clear.

---

## 7. Findings — APPEND AS YOU GO, TO YOUR OWN FILE

> **Do NOT write into this file.** Three independent reviewers are working from this brief
> concurrently. If you all append here, your writes race and findings are silently lost.
>
> **Create and append to your own file, in this same directory:**
>
> ```
> specs/021-admission-coverage-honesty/PLAN-REVIEW-FINDINGS-<your-name>.md
> ```
>
> Use whatever short name identifies you (e.g. `-codex.md`, `-kimi.md`, `-cursor.md`).
> Start it with a one-line header naming yourself and the date. This file below is the
> format spec and the shared input — treat it as read-only.

**Write each finding the moment you have it. Do not batch them to the end.** If you stop early — context exhausted, budget exhausted, interrupted — whatever is on disk is still useful. A half-finished review that was written incrementally beats a complete one that was never saved.

Format per finding:

```
### [SEVERITY] Short title
**Where:** file:line (in the plan, and/or in the source it cites)
**Claim under test:** what the plan asserts
**What I found:** the specific problem
**Why it matters:** what breaks if built as written
**Suggested fix:** concrete, or "unclear — needs owner decision"
```

Severity: `BLOCKER` (plan cannot be built as written) · `HIGH` (a defect would remain open or a new one introduced) · `MEDIUM` (real gap, workaroundable) · `LOW` (accuracy/clarity).

<!-- Append findings below this line. -->



---

## 8. Verdict — fill in last

One of exactly these, on its own line:

- `VERDICT: BUILD-AS-IS` — no blocking defects; any findings are improvements.
- `VERDICT: FIX-FIRST (n blockers)` — list the blocker titles.
- `VERDICT: RESTRUCTURE` — the plan's shape or sequencing is wrong, not just its details.

Then:

- **Crux:** the single thing whose falsity would most damage this plan. One sentence.
- **Confidence and its limits:** what you could NOT check, and why.

**If the plan is sound, say so.** Do not manufacture findings to appear thorough — a padded review costs more than a short one, because every false finding consumes real work to disprove. An honest "I found two MEDIUMs and nothing worse, here is what I could not verify" is a better result than ten invented LOWs.

Disagreeing with the plan's author, or with the reasoning in §3 and §5 of this brief, is explicitly in scope. The root cause in §3a is measured, but the plan built on it is not.
