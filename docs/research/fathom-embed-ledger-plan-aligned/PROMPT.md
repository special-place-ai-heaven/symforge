# Plan-aligned ledger: paste-ready /unlazy prompt and priming

Drafted 2026-09-15 by unlazy-master for the SymForge owner, for the session that
executes docs/plans/2026-09-15-fathom-embed-alignment.md on
`feature/fathom-embed-contract`. It supersedes docs/research/fathom-embed-ledger/,
which stays untouched and must not be used. Nothing here has been executed: no
CHECK ran, nothing is approved.

## Paste-ready prompt

```
/unlazy tree 1 Deliver plan docs/plans/2026-09-15-fathom-embed-alignment.md Tasks 1-7 on feature/fathom-embed-contract (CR-0, CR-1, CR-2, CR-3, F-1, F-2, embedded search_knowledge, selector, path prefix, one release) — do not stop until every gate in docs/research/fathom-embed-ledger-plan-aligned/GATES.md and all its child ledgers is met.

Unlazy runtime facts for this session:
- First: read the plan docs/plans/2026-09-15-fathom-embed-alignment.md and this ledger (docs/research/fathom-embed-ledger-plan-aligned/PLAN.md, PROMPT.md, GATES.md, gates/*.md, oracles/ledger-oracle.mjs). Do not read or use docs/research/fathom-embed-ledger/; it is superseded and must stay untouched.
- Skill dir: C:/Users/rakovnik/.claude/skills/unlazy   (checker: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs, linter: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs)
- Mode: orchestrated, sequential fallback (scope fathom-embed-plan-aligned, in-tree, not .unlazy/). One task at a time on this branch; one cargo target dir; feature sets never build concurrently. No --claim, no dispatch wave, no --log.
- Ledger(s): DRAFTED at docs/research/fathom-embed-ledger-plan-aligned/{PLAN.md, GATES.md, gates/cells.md, gates/leaf-1.1.md ... leaf-1.7.md} by unlazy-master, with the oracle oracles/ledger-oracle.mjs. Treat as INHERITED: run --status, read every CHECK and the whole oracle script, lint, then ask me before --approve. Never approve on my behalf.
- Platform/shell: win32; the checker's default shell is cmd.exe via ComSpec. Every CHECK is `node …`; no pipes, grep, tail or &&.
- Oracle scripts the work must author before the first check: none of the scripts (the oracle exists). The work authors the mutation patches named in each ledger under docs/research/fathom-embed-ledger-plan-aligned/mutations/ and the three baselines under docs/research/fathom-embed-ledger-plan-aligned/baselines/ (PROMPT.md step 3).
- Contract inventory rows: PLAN.md C1..C29, all ACTIVE. Reconcile every row before completion.
- Verification: parent re-verification is --reverify, never --status. Report with qualified ids (leaf-1.2:G3) and MEASURED met/unmet/abandoned counts.
- Stop hook: not wanted (it discovers .unlazy/<scope> and a root GATES.md, so it cannot see this in-tree ledger).
- Out of scope / non-goals: anything outside plan Tasks 1-7 and the named C3 companion; stringified server tool output for embed callers; a second index of Markdown bodies; removing or renaming any existing public item, field, signature, re-export or contract atom; any OperationKind variant other than the accepted IndexCensus and SearchKnowledge; a new dependency; widening what search_text scans or changing search_knowledge retrieval; distinguishing path rule from content rule in withheld reports; relocating the plan's tests.
- Safety: no push, pull request, merge, tag, release or CI-run cancellation without my explicit instruction; commit only locally on feature/fathom-embed-contract; do not edit the plan, the brief or docs/research/fathom-embed-ledger/; test repositories are disposable temp dirs, the checkout is precious.
```

## Priming: how to run it

1. **Read the plan and this ledger**, then the brief sections each task cites.
   Confirm `git rev-list -n 1 v11.2.0` prints b549aa7 and the branch is
   `feature/fathom-embed-contract`.

2. **Owner decisions (revision 2, 2026-09-15; stronger options only):**
   - `OperationKind` gains exactly `IndexCensus` and `SearchKnowledge`. Major
     accepted. Reusing `SearchSymbols` / `SearchText` is forbidden. Do not add
     `IndexProgress` as a kind. API gates use
     `--extend OperationKind=IndexCensus,SearchKnowledge` and do not freeze the
     enum.
   - Fixture (leaf-1.3 G16): this checkout, real Loading, full reload ≥10 s,
     twenty timed closes, unrelated open inside the one-second bound. The 32-file
     hold is a unit latch for cooperative-cancel tests, not G16. If a debug
     reload of this checkout is under 10 s, enlarge the fixture until the floor
     is real; do not lower G16. Replace the 50 ms unrelated-open sleep with a
     join-entered signal.
   - Names stay. Pin C28 (progress meanings) and C29 (knowledge result is evidence-
     complete, not a `TextSearchResult` clone).
   - G8: `.github/workflows/ci.yml` and `tests/preventive_runtime_dark_v11.rs`
     belong to Task 3 Step 5. The 200-character description files
     (`src/protocol/tools.rs`, `src/protocol/edit_tools.rs`, `tests/conformance.rs`,
     `src/stel/surface_list.rs`) are a named C3 companion, isolated as their own
     commit, not dropped and not unlabeled extras.

3. **Record the three baselines at b549aa7 in a detached temporary worktree**, so
   the branch's uncommitted work is not disturbed. Through Terminal Commander
   `run_and_watch`, one command at a time:
   - `git -C E:/project/symforge worktree add --detach E:/project/symforge-baseline-v11.2.0 b549aa7`
   - with cwd `E:/project/symforge-baseline-v11.2.0`:
     - `node E:/project/symforge/docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs baseline --out E:/project/symforge/docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-default.json -- test -j 8 --no-fail-fast --lib --bins --tests -- --test-threads=1`
     - `node E:/project/symforge/docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs baseline --out E:/project/symforge/docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-embed-lib.json -- test -j 8 --no-fail-fast --no-default-features --features embed --lib -- --test-threads=1`
     - `node E:/project/symforge/docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs baseline --out E:/project/symforge/docs/research/fathom-embed-ledger-plan-aligned/baselines/suite-embed-integration.json -- test -j 8 --no-fail-fast --no-default-features --features embed --test embed_bound_index -- --test-threads=1`
   - then `cargo clean` in that worktree, `git -C E:/project/symforge worktree remove E:/project/symforge-baseline-v11.2.0`, `git -C E:/project/symforge worktree prune`.
   A cold build there is slow and large; check free space on E: first. If any
   run prints `BASELINE FAILURE`, stop and report those tests to the owner.

4. **Commit the work in progress locally** on `feature/fathom-embed-contract`
   before any mutant or `committed` gate: mutant gates refuse to touch uncommitted
   files, and they restore bytes exactly. Keep commits local.

5. **Inspect, then stop for approval.** Non-executing:
   - `node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --status docs/research/fathom-embed-ledger-plan-aligned/GATES.md docs/research/fathom-embed-ledger-plan-aligned/gates/cells.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.1.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.2.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.3.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.4.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.5.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.6.md docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.7.md`
     (`--status` rejects `--cwd` and `--timeout`.)
   - `node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs <each ledger>`
   - read `oracles/ledger-oracle.mjs` in full.
   `--approve` runs what it approves. Approvals bind command, expectation, working
   directory, shell, timeout and full PATH, so approve from the launcher that
   will run every later check (Terminal Commander): either the owner runs it
   there, or the owner writes an explicit instruction naming the ledger files and
   the session runs exactly that. Approve `gates/cells.md` before any leaf.

6. **Run every gate-check through Terminal Commander** from the repository root:
   argv `node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-check.mjs --root . --cwd . --timeout 10800 --reverify <ledger>`
   (node 1: `--timeout 43200`). `wait_ms` at most 60000, then `wait` with
   `bucket_id` and `cursor`. Watch rules: `ALL MET`, `UNMET`,
   `HANDOFF REQUIRED`, `APPROVAL REQUIRED`, `LEDGER-ORACLE RED`,
   `Blocking waiting for file lock`. Never the Bash tool for cargo, never
   sleep-polling, never `| tail` or `> file` plus grep. SymForge's CLAUDE.md
   binds: `-j 8` on heavy builds, one feature set at a time per target dir, clear
   `target/debug/incremental` first if artifacts look corrupted.

7. **Per task, in plan order**: re-read citations (G1); write failing tests and
   record each red observation with command and failure line; implement; make
   green; author each mutation patch (hand-remove the named guard, `git diff >`
   the patch, `git checkout -- <file>`), regenerate a patch when a later task
   moves its guarded code; four passes (implement, expert reread, defect hunt,
   polish); `--reverify` the leaf until ALL MET; report; do not push. If a mutant
   run is interrupted: `node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs restore`.

8. **Release (leaf 1.7)** only on the owner's instruction: CI inventory, squash
   subject naming every identifier, tag after all gates pass, task-ledger record,
   then Fathom updates its pinned revision and lockfile.

9. **Report per task**: citation table; each red observation; each green gate
   result; each mutation (patch, test that went red, restored); the measured
   `CLOSE-LATENCY` line; the gate-check summary with measured met/unmet/abandoned
   counts and qualified ids; every ABANDON with its reason; owner questions.
