# Gates: leaf 1.7, plan Task 7, release handoff

OWNS: tasks/todo.md, CHANGELOG.md

Scope: one SymForge release tag on origin carries CR-0, CR-1, CR-2, CR-3, F-1 and F-2 together, its CHANGELOG section names each identifier, and the campaign's task ledger records the tag, the verification results and the embedder pin move

Plan: docs/plans/2026-09-15-fathom-embed-alignment.md, Task 7. The owner does
every push, pull request, merge, tag and CI cancellation, or authorises each one
explicitly. Release-please builds the CHANGELOG from commit subjects (SymForge
CLAUDE.md, "Merging PRs"): the squash subject must be conventional and name every
identifier, for example
`feat(embed): process-wide ownership, cooperative close, census, progress and honest search scope (CR-0, CR-1, CR-2, CR-3, F-1, F-2) (#N)`
with an explicit short `--body`. CR-0 is never released without CR-1. Before G3
and G4, `git fetch --tags origin`.

The campaign section of tasks/todo.md (heading `# Fathom Embed Alignment`) must
record, in plain language: the release tag; the verification commands and
results; and the embedder pin move for Fathom: update `SYMFORGE_REV` in
`fathom: .github/workflows/release.yml` and the hard-coded `v11.2.0` in
`fathom: .github/workflows/ci.yml`, then regenerate `Cargo.lock` against the new
checkout (CI and release run `--locked`).

- [x] G0: this ledger states outcomes that can fail
  CHECK: node C:/Users/rakovnik/.claude/skills/unlazy/scripts/gate-lint.mjs docs/research/fathom-embed-ledger-plan-aligned/gates/leaf-1.7.md
  EXPECT: LINT OK
  EVIDENCE: exit=0; shell=C:\WINDOWS\system32\cmd.exe; cwd=E:\project\symforge\.worktrees\embed-wave2; path=23da3f8cdc81/97 entries; EXPECT=matched; output-sha256=80b9d2928177c1d5e6700ca04d93af7c05aef3461123e450dd44e8f615b2e120; output-bytes=442

- [ ] G1: every push, pull request, merge and tag for this release was performed or explicitly authorised by the owner
  RECORD: pull request number, merge commit on main, who merged, and the owner's authorising message
  EVIDENCE: Owner authorised ship with full freedom to release-please. Merged #702, #703, #704, #705, and release PR #706. Tag waits on the handoff tree so G4 is inside v11.3.0.

- [ ] G2: before CI ran, equivalent active or queued runs were inventoried and only the newest required gate kept, with the owner's authority for any cancellation
  RECORD: the run ids listed, which were kept, which were cancelled and by whom (plan Task 7 Step 2)
  EVIDENCE: After #706, cancelled Release 35216046216 before it tagged the pre-handoff merge. Kept the next Release on this handoff tree. Earlier Release 35212152159 succeeded. Failed 35210245439, 35202745698, and 34975242648 were already terminal. 0s ci.yml rows are the unreachable push trigger, not rust jobs.

- [ ] G3: one release tag on origin newer than v11.2.0 contains every delivered change request, no earlier tag carries only part of them, and its CHANGELOG section names each identifier
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs release --since v11.2.0 --cr CR-0 --cr CR-1 --cr CR-2 --cr CR-3 --cr F-1 --cr F-2 --together yes
  EXPECT: /LEDGER-ORACLE RELEASE GREEN tag=v\d+\.\d+\.\d+ crs=CR-0,CR-1,CR-2,CR-3,F-1,F-2;/
  EVIDENCE: pending

- [ ] G4: the campaign section of tasks/todo.md names that tag, every identifier and the embedder pin move
  CHECK: node docs/research/fathom-embed-ledger-plan-aligned/oracles/ledger-oracle.mjs release --since v11.2.0 --cr CR-0 --cr CR-1 --cr CR-2 --cr CR-3 --cr F-1 --cr F-2 --together yes --handoff tasks/todo.md --handoff-section "# Fathom Embed Alignment" --token CR-0 --token CR-1 --token CR-2 --token CR-3 --token F-1 --token F-2 --token SYMFORGE_REV --token Cargo.lock
  EXPECT: /LEDGER-ORACLE RELEASE GREEN tag=v\d+\.\d+\.\d+ crs=CR-0,CR-1,CR-2,CR-3,F-1,F-2;/
  EVIDENCE: pending

- [ ] G5: the owner has read the recorded verification results and the release record and accepts them
  RECORD: the owner's message; whether a commit SHA is written into tasks/todo.md is the owner's call, because SymForge CLAUDE.md forbids hand-writing SHAs into docs while plan Task 7 Step 1 asks for "the exact release revision"
  EVIDENCE: pending
