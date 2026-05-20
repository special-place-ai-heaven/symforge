---
goal_id: SFB06
title: Make current partial-parse hygiene distinguish expected vendor noise from unexpected repo partials
chain_id: symforge-live-code-backlog
phase: Phase 1 - test hardening and diagnostics
status: "Completed"
depends_on: []
target_branch: "backlog-implementation"
prohibited_branches: ["main", "master"]
worktree_hint: ".worktrees/backlog-implementation"
created_at: "2026-05-19"
started_at: "2026-05-20T11:56:50.0234073+02:00"
completed_at: "2026-05-20T12:31:00.3111914+02:00"
completion_commit: "37d7918"
blocked_reason: ""
gate: "decision-gated"
risk_level: "medium"
source_refs:
  - "docs/live-code-backlog.md#6"
---
# SFB06 - Make current partial-parse hygiene distinguish expected vendor noise from unexpected repo partials

Use this file directly with `/goal`:

```text
/goal .agent/goals/symforge-live-code-backlog/SFB06-make-current-partial-parse-hygiene-distinguish-expected-vendor-noise-from-unexpected-repo-partials.md
```

## Goal File Workflow

0. Treat this markdown file as the whole prompt. Do not ask the user for extra instructions. If the task cannot be completed safely, mark it `Blocked` and explain exactly why in the final report.
1. Run the Branch Guard before editing this file, source code, tests, npm files, docs, generated artifacts, or Cargo metadata.
2. After Branch Guard passes, update this file's frontmatter: set `status` to `In progress` and set `started_at` to an ISO-8601 timestamp.
3. Execute only this goal's mini-spec. Keep changes inside `allowed_files_or_area`. Do not expand into adjacent backlog items unless this file explicitly says so.
4. If a stop condition is hit, stop implementation, set `status` to `Blocked`, set `blocked_reason`, leave `completion_commit` empty, and commit the status update if committing is safe.
5. When acceptance criteria pass, run the verification command exactly as written unless the command is impossible for a documented pre-existing reason.
6. Commit the verified implementation work first. Then update this file: set `status` to `Completed`, set `completed_at`, and set `completion_commit` to the exact verified work commit hash.
7. Commit the goal-status update as a separate commit.

## Branch Guard

This goal belongs only to branch `backlog-implementation`.

Before making any change, run:

```bash
git branch --show-current
git status --short
```

If the branch is `backlog-implementation`, continue only if the working tree is clean or contains only this goal's already-started changes.

If the branch is `main`, `master`, or any other branch, do not edit there. Use or create the dedicated worktree:

```bash
if [ -d ".worktrees/backlog-implementation/.git" ] || [ -f ".worktrees/backlog-implementation/.git" ]; then
  cd .worktrees/backlog-implementation
else
  git fetch origin
  git worktree add -b backlog-implementation .worktrees/backlog-implementation origin/main || git worktree add .worktrees/backlog-implementation backlog-implementation
  cd .worktrees/backlog-implementation
fi
mkdir -p .agent/goals/symforge-live-code-backlog
```

If this goal file is not present in the worktree, copy it from the original checkout into `.agent/goals/symforge-live-code-backlog/` before updating frontmatter. Rerun the branch/status check in the worktree. Stop if the target worktree is unavailable, dirty with unrelated work, or still not on `backlog-implementation`.

## SymForge Goal Discipline

- Work from current code, not historical plans. Do not revive deleted historical docs, ADRs, RTK plans, old reports, or planning directories.
- Do not invent unrelated product features.
- Prefer small, reviewable Rust changes with focused tests.
- Preserve existing MCP behavior, public output contracts, npm packaging, CLI flags, tool names, schemas, and daemon behavior unless this goal explicitly changes one.
- Keep SymForge local-first: in-process `LiveIndex` and `.symforge/` local state remain the source of runtime truth.
- Maintain byte-exact source handling. Do not normalize line endings, rewrite source bytes casually, or serve stale spans silently.
- Never turn mock, stale, degraded, disabled, blocked, unavailable, or unknown state into success.
- If the target is already implemented, strengthen tests/evidence instead of duplicating code.
- If a public contract changes, add tests that pin the contract and note whether npm/client setup is affected.

## Dependency Guard

If `depends_on` is not empty, inspect the referenced goal file(s) under `.agent/goals/symforge-live-code-backlog/` when present. If a dependency is absent or not marked `Completed`, continue only if the code already contains the dependency's acceptance artifacts. Otherwise mark this goal `Blocked` with evidence.


## Mini-Spec

objective:
- Decide and implement how SymForge reports the remaining vendored SCSS parser C/header partial parses so health shows zero unexpected partials or clearly marks vendor partials as expected noise.

non_goals:
- Do not reopen the old Rust `&raw` parser issue unless current code reproduces it.
- Do not broadly suppress all vendor files.
- Do not weaken parser diagnostics for project-owned source.

allowed_files_or_area:
- src/parsing/**
- src/live_index/**
- src/protocol/format.rs
- src/protocol/tools.rs
- vendor/tree-sitter-scss/** only if fixing parser fixture metadata, not changing vendored parser code casually
- tests/**

forbidden_files:
- Cargo.toml except if a parser dependency check proves required; prefer separate SFB18 for Rust parser upgrade
- npm/**
- docs/**
- plans/**
- .planning/**
- openspec/**

contracts_or_interfaces:
- Unexpected partials remain visible in health.
- Expected vendor partials are either suppressed through an explicit vendor-noise classifier or surfaced with a clear expected/vendor label.
- Repo-owned Rust source partials remain a failure signal.

invariants:
- No silent hiding of project-owned parse failures.
- No vendored source rewrite unless the decision explicitly chooses fix-over-suppress and tests justify it.

acceptance_criteria:
- Decision is recorded as FIX_VENDOR, SUPPRESS_VENDOR_NOISE, or LABEL_EXPECTED_VENDOR_LIMITATION.
- Health output reports zero unexpected partials or clearly labels vendor partials.
- Tests cover vendor SCSS parser files and a project-owned partial file separately.

evidence_required:
- Decision paragraph.
- Before/after health partial-parse output.
- Parser/health test output.
- Default verification output.

stop_conditions:
- Vendor partials are caused by corrupted vendored files; stop and decide whether to update vendor source separately.
- Suppressing vendor noise also suppresses project-owned partials; stop and narrow classifier.

verification_command:

```bash
cargo fmt --check
cargo check
cargo test --all-targets -- --test-threads=1
rg "partial|vendor|scss" src tests
```

Default full verification, when task-specific verification passes and time permits:

```bash
git branch --show-current
git diff --check
cargo fmt --check
cargo check
cargo test --all-targets -- --test-threads=1
cargo build --release
```

If this goal changes `npm/**`, also run:

```bash
cd npm && npm test
```


reviewer_checklist:
- Gate type is `decision-gated` and was handled honestly.
- Branch evidence shows `backlog-implementation`.
- Changes stayed inside allowed files/areas.
- Forbidden historical docs/plans were not revived.
- Public MCP, CLI, npm, daemon, and output contracts did not regress unless this goal explicitly changed and tested them.
- Verification output is included in the final report.

## Task Prompt

Run only this goal. Follow the Branch Guard, update this file before and after work, keep edits inside the allowed files/areas, satisfy the mini-spec, run verification, commit verified work, then commit the status update. Report blockers instead of guessing.

## Completion Evidence

Decision:
- LABEL_EXPECTED_VENDOR_LIMITATION. The remaining current partial parses are vendored `tree-sitter-scss` C/header parser sources, so health now classifies only `vendor/tree-sitter-scss/src/**/*.c|h` partial parses as expected vendor parser limitations when the file is also classified as vendor and parsed as C/C++. Repo-owned partial parses and unrelated vendor partial parses remain unexpected health signals.

Before health partial-parse output:

```text
Files:  191 indexed (187 parsed, 4 partial, 0 failed)
Partial parse files (4):
  1. vendor/tree-sitter-scss/src/parser.c
  2. vendor/tree-sitter-scss/src/tree_sitter/alloc.h
  3. vendor/tree-sitter-scss/src/tree_sitter/array.h
  4. vendor/tree-sitter-scss/src/tree_sitter/parser.h
```

After health partial-parse output from rebuilt `target/release/symforge.exe`:

```text
Files:  190 indexed (186 parsed, 4 partial, 0 failed)
Partial parse summary: 0 unexpected, 4 expected vendor
Parse resilience: expected vendor partial files kept best-effort symbols; they are labeled as vendor parser noise below.
Expected vendor partial parse noise (4):
  1. vendor/tree-sitter-scss/src/parser.c [expected vendor: tree-sitter-scss C/header parser limitation]
  2. vendor/tree-sitter-scss/src/tree_sitter/alloc.h [expected vendor: tree-sitter-scss C/header parser limitation]
  3. vendor/tree-sitter-scss/src/tree_sitter/array.h [expected vendor: tree-sitter-scss C/header parser limitation]
  4. vendor/tree-sitter-scss/src/tree_sitter/parser.h [expected vendor: tree-sitter-scss C/header parser limitation]
```

Parser/health test output:
- `cargo test partial_parse -- --test-threads=1` passed.
- `cargo test expected_vendor -- --test-threads=1` passed.
- `cargo test mark_all_vendor -- --test-threads=1` passed.
- `cargo test project_owned -- --test-threads=1` passed.
- `cargo test --all-targets -- --test-threads=1` passed.

Default verification output:
- `git branch --show-current` returned `backlog-implementation`.
- `git diff --check` exited 0.
- `cargo fmt --check` exited 0.
- `cargo check` exited 0.
- `rg "partial|vendor|scss" src tests` exited 0.
- `cargo build --release` exited 0.

## Final Report Format

Objective:
- <repeat this goal's objective>
Gate:
- <implementation-ready | evidence-gated | decision-gated>
Changes:
- <focused list of implementation changes>
Files changed:
- <paths>
Acceptance criteria:
- PASS/FAIL: <criterion> — <evidence>
Verification:
- PASS/FAIL: `<command>` — <summary>
Evidence:
- <branch evidence, test output summaries, rg output, before/after notes, status/output examples>
Commit:
- Verified work commit: `<hash>`
Known gaps / blockers:
- <none or explicit blocker with reason>
Next goal:
- SFB07 - Pin search_text usage grouping behavior for doc comments and markdown
