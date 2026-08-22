# CLAUDE.md — SymForge

## Verification (symforge)
- Backend: `cargo fmt --check`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --lib --bins --tests -- --test-threads=1`, `cargo bench --bench observed_refresh_gate_v1 -- --test`, `cargo build --release`
- (as_of 2026-08-19, C7) the suite selects `--lib --bins --tests`, NOT `--all-targets`: the trailing libtest flag is forwarded to every selected binary and the criterion bench target (`observed_refresh_gate_v1`, harness = false) rejects `--test-threads`. The bench is smoked separately in criterion's `--test` mode (one pass per campaign).
- `npm/` only: `cd npm && npm test`
- Mixed: run both before reporting success

**The default-feature gates cannot catch a feature-gated `cfg` mistake.** CI's
`embed-build` job runs `cargo test --no-default-features --features embed --lib --
--test-threads=1`, where the lib tests still compile but every
`#[cfg(feature = "server")]` module (`watcher`, `daemon`, `protocol`, `sidecar`,
`server`, `cli`, …) is absent. A `#[cfg(test)]` item whose only consumer lives behind
`server` is therefore an unused import there, and `-D warnings` fails the build — while
every default-feature gate above passes. Run that exact command before pushing
anything that adds a `#[cfg(test)]` helper. Found the slow way on 2026-08-12: green
locally and on the `rust` job, red on `embed-build`.

When such a helper's consumer is server-only, gate it on
`#[cfg(all(test, feature = "server"))]`. The Feature 020 retirement census
understands cfg predicates: `all(..)` is test-only when any conjunct is, `any(..)`
only when every disjunct is, and `not(..)` never. An earlier amendment matched only a
literal `#[cfg(test)]`, which forced a stacked-attribute workaround; that is no longer
needed and the plain `all` form is the supported spelling.

### Long builds must not run through the Bash tool (as_of 2026-08-12)

The Bash tool's `timeout` maxes at **600000 ms**, and `run_in_background` is subject
to the same ceiling. A cold `cargo test --all-targets` here takes ~25 minutes, so it
is killed at 10 -- **mid-write**. That is not a lost result, it is a corrupted
`target/`: one kill produced `error[E0786]: found invalid metadata files`, the next a
rustc `internal compiler error: no type-dependent def for method call`, the next
`error[E0463]: can't find crate for symforge`. Each looks like a code failure and none
of them is one.

Run anything that can exceed ten minutes through Terminal Commander instead
(`run_and_watch`, then poll `command_status` by `job_id`). Its daemon owns the
process, so the job's lifetime is independent of the tool call. Interleaving feature
sets in one target dir (`--all-targets` then `--no-default-features --features embed`)
causes the same `E0786` without any kill; run them one at a time.

**Windows commit-limit corruption (as_of 2026-08-19).** Two suite builds died
mid-compile to `0xc000012d` (STATUS_COMMITMENT_LIMIT — virtual-memory commit
exhaustion) under default 16-way parallel rustc + linkers, corrupting emitted
artifacts (the E0786 cascade below, plus one rustc ICE and `only metadata stub
found for rlib dependency core/std` on the NEXT cold build). Cap build
parallelism with `-j 8` on this machine for heavy `cargo test`/`bench`/`build`
runs; the capped builds completed where the default died twice.

Recovery, cheapest first: delete `target/debug/incremental`; then
`cargo clean -p symforge`; then a full `cargo clean`. Do not diagnose a rustc ICE or a
missing-crate error as a code defect until the build directory is known clean.

### Windows build cache (disk)

- Artifacts: repo `.cargo/config.toml` sets `target-dir = "target"`, i.e. artifacts land **beside the checkout, on whatever drive the repo is on** (gitignored). The comment in that file and this line both used to say "on **E:**", which was only ever true while the checkout lived there; as_of 2026-08-12 the repo is on **C:** with no E: drive present, and `target/` had reached **180 GB** on C:. Do **not** pass `CARGO_TARGET_DIR=...` on the command line (older handoffs suggested `C:/symforge-target` — that filled **C:**).
- **Agent discipline**: OK to run full `cargo` gates locally; **clean up after yourself** — when you finish a heavy local session (`test --all-targets`, `build --release`), run `cargo clean` before ending so debug artifacts do not accumulate. If `target/debug` is already large before your gate, `cargo clean` first.

## CI Gates

- PR CI runs version sync, `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, the full Rust test suite,
  `cargo build --release`, and npm tests.
- The `rust` job is compile-dominated (~25 min wall as_of 2026-08-05; the suite
  itself is ~3.5 min). **Do not re-add the four-key `Swatinem/rust-cache`
  configuration** — it was tried and MEASURED SLOWER, then reverted. Warm
  restore: `rust` 24m57s -> 25m36s, `darwin-serve-port` 2m59s -> 4m13s,
  `embed-build` 2m43s -> 2m52s, `embed-musl` flat. Four jobs, all
  neutral-to-worse.
  **The measurement stands; the explanation originally written here did not.**
  It claimed archive I/O simply exceeds the compile it avoids. The action's own
  README says caches are capped at **10 GB repo-wide, and exceeding that evicts
  older entries** — and the reverted config used FOUR distinct `shared-key`s
  (`symforge-rust`, `-embed`, `-embed-musl`, `-darwin`). Four dependency caches
  for this dep tree plausibly blew that cap, so every run paid the save cost
  while often getting a miss. That fits the data better than the original claim.
  Also worth knowing before re-testing: rust-cache does **not** cache workspace
  crates (it prunes them and sets `CARGO_INCREMENTAL=0` itself), so it caches
  only dependencies — which here is the expensive part (25 tree-sitter C
  grammars, vendored libgit2 via cmake, bundled sqlite).
  **Untested and possibly viable:** a single shared-key across the ubuntu jobs,
  or `cache-targets: false` (registry only). Neither was measured.
- CI runs `clippy --all-targets` WITHOUT a preceding `cargo check`: clippy is a
  strict superset (same rustc checks, more lints, more targets) and cannot reuse
  a `cargo check` pass, so running both compiled the graph twice for one answer.
- `cargo build --release` is NOT droppable from PR CI: the tool-correctness
  harness runs `verify-tools.cjs --bin target/release/symforge`.
- One CI run per PR (`pull_request`). **`push` never triggers `ci.yml` at all** —
  not on branches, not on `main` (as_of 2026-08-22). All-time counts:
  `gh api "repos/:owner/:repo/actions/workflows/ci.yml/runs?event=push"` returns
  `total_count` **0**, against **666** for `event=pull_request`. The earlier note
  here said push runs "fire only on `main` after a merge"; that was wrong. The 37
  push runs visible in `gh run list` all belong to `release.yml`, which declares
  `on: push: branches: [main]`.

  **The cause is `ci.yml`'s own trigger.** It declares `push:` with only
  `tags-ignore: ["v*"]`. When a push trigger carries *only* tag filters, GitHub
  does not run it for branch pushes at all — so this filter means "non-`v*` tag
  pushes only", and the only tags this repo pushes are `v*`. The trigger is
  therefore unreachable in both directions.

  **What that costs:** `ci.yml`'s `Validate pushed commit subjects` step
  (`conventional_commits.py check-push-range`) has never executed, ever. It is
  the gate meant to stop a non-conventional subject reaching `main` outside a PR
  — the exact failure that reddened Release run #938. Adding `branches: ['**']`
  to the push trigger would arm it, at the cost of a full CI run on every merge
  to `main`. That is a live cost/benefit call, not a typo fix; do not flip it
  casually. Note also that arming it is what would make a per-run concurrency
  group for `main` meaningful — while push CI stays unreachable, any such change
  guards a queue that has no entrants.
- Scheduled and manual CI additionally run ignored performance smoke coverage:
  `test_load_perf_1000_files` and `calibrate_current_repo_smoke`.
- Full real-repo coupling calibration is operator-triggered with
  `SYMFORGE_CALIBRATION_REPOS`; standard CI must not depend on local paths.

## Merging PRs (release-please visibility — verified 2026-07-26)

Verified directly against `googleapis/release-please-action@v5` source
(`manifest.ts`, `github.ts`, `commit.ts`) and live CI logs on this repo
(`prepare-release` job). The previous guidance in this section was wrong and
had made several real, merged, CI-green `fix:` commits invisible to
release-please for a full day (stuck at 8.16.3; see agentmemory `[symforge]`
for the incident writeup).

**What release-please actually does:** its commit walker reads `main`'s
history backward from HEAD via GitHub's GraphQL `history` connection,
stopping the instant it reaches the SHA already recorded as the last release
for this package. Every commit it visits before that cutoff — merge commits
*and*, depending on git-graph shape, a merged branch's own inner commits —
becomes a parse candidate. For each candidate, it parses that commit's own
raw git message (subject + body as committed, not the PR description) and
`git log --no-merges` at the CI gate doesn't change what release-please
sees — that flag only governs `execution/conventional_commits.py`'s own
subject-format validation, a separate, unrelated check.

Two failure modes, both confirmed from real history on this repo:

- **Double-count (the original real bug):** GitHub's *default* `--merge`
  commit body is the PR title (already conventional here). release-please's
  parser splits a commit's message on any blank-line boundary where the next
  paragraph itself looks conventional, and counts that paragraph as an
  independent commit *attributed to the merge commit's own SHA*. If the
  underlying branch's own inner commit (same conventional message) is *also*
  swept into that release cycle's candidate window — which happens whenever
  the previous release boundary sits far enough back in history — you get
  the exact same changelog line twice, once per SHA. This really happened:
  8.16.2's CHANGELOG has duplicate "bind upserts to reviewed actions" and
  "reauthorize pending replay writes" entries (one via the merge commit,
  one via the inner commit).
- **Total invisibility (the regression the old guidance caused):**
  overriding the merge commit's *body* to non-conventional text
  (`--body "PR #<N>"`) while leaving the *subject* as GitHub's generic
  `Merge pull request #N from ...` means NEITHER the subject NOR the body is
  parseable. There is no other fallback. Once the underlying branch's inner
  commits also miss the candidate window (graph-shape dependent, and NOT
  reliable — it silently failed here), the whole PR contributes zero
  changelog entries and zero version bump, forever, with no error raised.
  This is what actually happened to PRs #470, #471, #472, #475.

**The fix — default to squash-merge, and ALWAYS pass an explicit safe body
(as_of 2026-08-17, measured on #582):**

```
gh pr merge <N> --squash --delete-branch --subject "<conventional PR title> (#<N>)" --body "<one short paragraph, no parentheses, no colon-bearing prose lines>"
```

`gh`'s default squash subject is the PR title + `(#N)`; this repo's own CI
(`.github/workflows/ci.yml` `conventional-commits` job) already enforces
that every PR title is conventional before merge, so the resulting squash
commit's subject is automatically valid. A squash commit has no second
parent and no reachable inner commits at all, so there is nothing left for
either failure mode above to act on — deterministically, not by graph-shape
luck. This does trade away per-commit granularity/bisectability inside a
single PR on `main`; the full inner-commit history remains visible on the
(closed) PR itself via the GitHub API/UI indefinitely.

**Why the explicit `--body` is mandatory, not hygiene:** gh's DEFAULT squash
BODY is the concatenated list of every inner commit message. release-please
parses the whole squash message, and any inner-body prose line shaped like
`word: text` starts a new conventional-parse unit — on #582 the line
`hook:PreTool is still ingress - PreToolUse ran and the process returned
Ok(())...` impersonated a header and its `Ok(())` broke the parser
(`unexpected token '(' at 287:4`). ONE parse error makes the ENTIRE squash
commit invisible: release-please reported "No user facing commits found",
the owed 10.6.1 release PR was never created, and the workflow's "Prove
that no release was owed" gate (correctly) went red. PR3's identical-shape
default body survived by luck, not by design. The changelog line for a
parsed-away commit is recoverable only by landing a new parseable `fix:`
commit that names it.

If a PR's inner-commit history must stay reachable on `main` (rare — e.g. a
deliberately staged multi-commit landing), use `--merge` but give the merge
commit's own SUBJECT the real conventional message (not just the body):

```
gh pr merge <N> --merge --delete-branch --subject "fix(scope): description (#<N>)" --body "PR #<N>"
```

This is parsed directly off the header with no split needed, so it's
reliable — but it does not fully eliminate the double-count risk above if
history later happens to also sweep in that PR's inner commits, so prefer
squash unless there's a specific reason not to.

Release PRs themselves stay `--merge` (the workflow's auto-merge). With
`skip-github-release: true`, release-please then aborts with "untagged,
merged release PRs outstanding" until this workflow creates the tag. The
"Prove that no release was owed" step must not treat that abort as a silent
parse-away: counting feat/fix since the *previous* tag blocks tagging.
That is how v10.2.0 never landed after #561 merged (as_of 2026-08-13).
If HEAD has already moved past the untagged release commit, create `vX.Y.Z`
at that merge SHA and label the PR `autorelease: tagged` — do not tag HEAD.
Helper: `execution/prove_release_owed.py`.

## Architecture

Rust MCP server providing symbol-aware code and repository-knowledge navigation, review, curation, and editing tools. The **default** MCP `tools/list` surface **advertises 39 tools** while **40 are registered** (as_of 2026-08-07). Registration count: `#[tool(` attribute sites — 33 in `tools.rs` + 7 in `edit_tools.rs` — equal to the 40 names in `SYMFORGE_TOOL_NAMES` (including `health_compact`, `search_knowledge`, `review_knowledge`, and `curate_knowledge`), pinned by `test_client_allow_lists_match_registered_tool_surface`. The advertised count is one lower because `list_tools_for_profile` filters the compact-only `symforge` facade out of the full profile (`src/protocol/surface_probe.rs:173`) — `symforge_retrieve` is its full-surface equivalent. Do not "fix" a client reporting 39; that is correct. the compact-3 surface (`symforge`, `symforge_edit`, `status`) is a documented opt-in escape hatch via `SYMFORGE_SURFACE=compact`, with backward-compat aliases for removed tools in `src/daemon.rs`. Resources and prompts are first-class protocol surfaces, not side notes.

Protocol (as_of 2026-08-04, spec 025): rmcp 3.1.0 serving MCP 2026-07-28 alongside every legacy revision; the advertised set is a frozen allow-list in `supported_protocol_versions()` (`src/protocol/mod.rs`) — extend deliberately, never let a dependency bump widen it. Static list surfaces carry SEP-2549 cache hints (1h, `Public`); `resources/read` is pinned `ttl_ms=0`/`Private` (INV-4). **Mixed-surface deployment warning (FR-311c)**: `CacheScope::Public` on `tools/list` assumes ONE surface configuration per HTTP origin — operators fronting a full-surface and a compact-surface symforge under one origin must not share a public cache, or full-surface tool schemas leak to compact-deployment clients (the dispatch gate blocks the calls, not the schema disclosure). Legacy roots binding (`bind_workspace_from_client_roots`) is on a deletion clock: when rmcp removes the deprecated Roots API (`Peer::list_roots`), that function is deleted and clients are directed to `index_folder`.

Key source files:
- `src/protocol/tools.rs` — Tool handlers, input structs, tests
- `src/protocol/read_gate.rs` — The single admission gate for raw-disk content
  reads. Every lane that reopens a file from disk (rather than serving bytes
  already in the in-memory index) routes through `admit_disk_read`, which owns
  the read and returns the buffer only on a permit verdict.
- `src/protocol/format.rs` — Output formatters
- `src/daemon.rs` — Daemon proxy with backward-compat aliases
- `src/cli/init.rs` — Tool name list for client init
- `src/live_index/query.rs` — Index query functions
- `src/protocol/resources.rs` — MCP resource handlers
- `src/protocol/prompts.rs` — MCP prompt handlers
- `src/protocol/result_status.rs` — Machine-readable outcome metadata

## Reporting invariant (as_of 2026-08-07 — binding)

**A component may not report success for an operation whose completion it did
not observe.** Not "attempted", not "usually works", not "the code path that
does it was called" — observed.

This is not style. Six defects fixed on 2026-08-06 were one defect wearing
different clothes, and every one of them shipped green:

- `stop_incompatible_recorded_daemon_at` deleted a live daemon's port/pid
  records on every branch — including the ones where the safety gate *refuses*
  to terminate and where `terminate_process` fails. The survivor kept serving
  its own index, undiscoverable and unstoppable.
- Both re-parse loops did `let _ = admit_and_index_single_path(...)`, discarding
  a result that can be `Skipped`/`ReadError`/`NotFound`, then asserted in a
  comment that those files "were re-parsed above, so they are correctly not
  mismatches".
- `format_search_envelope` collapsed to the compact `Trust:` banner on a *string
  comparison* against `"current index"` that every code-navigation caller passed
  as a literal — asserting currency it never measured.
- `index_folder(add:true)` advertised a `project_name` that no selector could
  resolve.

The recurring shape: **the thing that reports is not the thing that knows.** A
caller that discards a return value, a formatter handed a literal instead of a
measurement, a cleanup that runs on the refusal branches too — each produces a
confident answer with nothing behind it. None of these are caught by tests that
assert on the reported value, because the reported value is exactly what is
wrong.

When adding any status line, banner, envelope, or success return, answer in the
PR: *what did this observe, and what does it emit when the observation fails?*
If the answer is "it cannot fail", that is the claim under review.

symforge's product is being trustworthy about what it knows. A wrong answer is
recoverable; a confidently wrong answer is not.

## Tool Consolidation Pattern

When merging tools A into B:
1. Add new params to B's input struct (with `#[serde(default)]`)
2. Add mode branch in B's handler
3. Remove `#[tool]` attribute from A (keep the method for internal use)
4. Add backward-compat alias in `src/daemon.rs` `execute_tool_call`
5. Remove A from `SYMFORGE_TOOL_NAMES` in `src/cli/init.rs`
6. Update cross-reference descriptions in other tools
7. Update tests: add new field initializers, add mode-specific tests

<!-- SPECKIT START -->
For additional context about technologies to be used, project structure,
shell commands, and other important information, read the current plan
at specs/028-preventive-activation-cut/plan.md
<!-- SPECKIT END -->

## Documentation hygiene (as_of 2026-07-13 — binding)

This file is the ONLY live-truth document in this repo; volatile claims carry an
`as_of` stamp. Doc map (enforced by user-level hooks — new .md files prompt the owner):

- `CLAUDE.md` — live state + rules. UPDATE it when a change falsifies a claim here.
- `specs/NNN-*/` — feature lifecycle (spec-kit), if used.
- `docs/solutions/` — compounded learnings (`/ce-compound` files them).
- `docs/archive/` — historical; never trust, never update, only append moves.
- Other docs are legacy pending triage — verify against code before believing them.
- Durable decisions/lessons go to agentmemory with the `[symforge]` content prefix;
  session-start recall injects them automatically.

### README.md and AGENTS.md are pinned by a test (as_of 2026-08-07 — binding)

`repair_lifecycle_retirement_contract_documents_checkpoint_snapshot_workflow`
(`tests/conformance.rs`) reads `AGENTS.md` and `README.md`, joins them, and asserts
that seven exact phrases appear across the pair:

```
`repair_index` is intentionally retired
`get_index_run` and `cancel_index_run` remain retired
`checkpoint_now(verify_after_write=true)`
`health` or `health_compact`
`index_folder` reset
`.symforge/quarantine/index-snapshots/`
No durable run IDs are exposed
```

The test does not care WHICH of the two files carries a phrase, only that the joined
text contains it. So moving a sentence between them is safe; deleting it is not, and
neither is paraphrasing it.

**Before restructuring either file, grep `tests/` for it.** This is not discoverable
from the documents themselves — nothing in either file says it is load-bearing. The
2026-08-07 README rewrite folded the Recovery section into Configuration and dropped
"No durable run IDs are exposed", which lived only in the README; six of the seven
phrases survived in `AGENTS.md`. CI went red 25 minutes later, on `main`.

A docs edit is a code change when a test reads the docs. Run the suite, or at minimum
replicate the assertion — the check is pure string containment and takes seconds.

### CI workflows are pinned by a test too (as_of 2026-08-22 — binding)

`tests/preventive_runtime_dark_v11.rs` carries `WORKFLOW_FINGERPRINTS`, a
whole-file `<fnv1a-64>:<bytes>` seal over **`.github/workflows/ci.yml` and
`.github/workflows/release.yml`**. Any byte of either workflow moving fails the
suite. The file states its own purpose: *"no workflow byte may change without a
human revisiting the judgement that no gate builds doctests."* It is a change
detector, not a security boundary — FNV-1a is not collision-resistant and
whoever edits a workflow can edit the pin. What it buys is that the edit is
never silent.

**So every CI workflow edit is a two-file change**: the workflow, and its
fingerprint in the same commit. Nothing in either workflow file says it is
load-bearing; the only thing that tells you is a red `rust` job. PR #612 — a
four-line `concurrency:` block — went red on exactly this before anyone noticed
the seal existed.

Recompute is arithmetic, not judgement: FNV-1a-64 over the LF-normalized file,
`:` , then the normalized byte length. The `rust` job is the observing authority;
a pin refreshed any other way is plausible rather than verified. Same-file
neighbours `CARGO_LINES` pin the occurrence count of each cargo invocation, so
adding, rewording, or duplicating a cargo gate needs that reconciled too — and
reconciled deliberately, never by loosening the pin.

This is the same trap as the README/AGENTS.md phrase pinning above, in different
clothes: **a workflow edit is a code change when a test reads the workflow.**
Before touching either workflow, grep `tests/` for its filename.

### Never hand-write volatile state (binding, as_of 2026-08-05)

Handover and campaign docs rot because volatile facts get TYPED into them. Reading
`specs/024-optimization-backlog/HANDOVER-2026-08-04.md` one day after it was written
turned up three already-stale claims: `origin/main is at 506fe30` (it was `78adcf3`),
`aap-embedder-reverse-asks.md is UNCOMMITTED` (it had landed), and a worktree list
that was true only at the moment of writing.

Those are not documentation failures. They are facts that should never have been
typed by hand. So:

- **Do not write these into any doc**: git SHAs, branch/worktree lists, "X is
  uncommitted", open PR numbers or their CI state, the current version.
- **Generate them instead**: `pwsh scripts/campaign-state.ps1` (add `-Json` for a
  machine-readable bootstrap). It reads git, `gh`, and `Cargo.toml` on
  `origin/main`, and filters untracked files that are byte-identical to `main` —
  stale-branch noise that otherwise reads as pending work.
- **Docs keep the durable half**: protocol, task roster, measurements with their
  method, gotchas, decisions. Cite the command for anything that moves.
