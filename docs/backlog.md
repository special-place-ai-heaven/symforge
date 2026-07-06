# SymForge backlog — written 2026-07-06, for continuation

State of the world when this was written, then the ranked queue. Everything
here assumes the 2026-07-06 dogfood fix campaign (waves 1–5) — canonical
record in `docs/reviews/2026-07-06-dogfood-findings.md` (findings + Fix log).

## Where things stand (2026-07-06 evening)

| Wave | PR | Status | What |
|---|---|---|---|
| 1 trust honesty | #416 | merged | #1 Tier-2 disclosure sweep, #2 root-caused, #5a multi-word hint |
| 2 daemon correctness | #418 | merged | #7 impact-path admission gate + 4MB code threshold, #6 root-named NotFound |
| 3 noise + granularity | #420 | merged | #8/#5b hook noise cut, #4 `occurrence`/`near_line` on edit_within_symbol |
| 4 recall + routing | #421 | merged | #3 `macro-generated` symbol kind, #2 closed via #3, FR-006b added to spec 012 |
| 5 root guard + system paths | #423 | **in CI at time of writing** | #6 hook half (caller_root → 409 → daemon by-root fallback), raw-input system-path refusal |

Releases cut today: 8.12.0, 8.12.1. The Wave-4 feat and Wave-5 fixes should
land as **8.13.0** — see "Release watch" below.

## Immediate queue (mechanical, do first)

1. **Merge PR #423** when CI is green. Merge rule (release-please
   double-count guard, from CLAUDE.md):
   `gh pr merge 423 --merge --delete-branch --body "PR #423"`.
2. **Release watch**: after the merge, the Release workflow should open a
   `chore(main): release 8.13.0` PR containing the Wave-4 feat + Wave-5
   fixes. Merge it (same body rule) to cut the release.
   - Known quirk (observed today, run 28822434928): right after a merge +
     branch delete, release-please's commit collection can race the PR
     association and see ONLY the merge commit ("Splitting 1 commits …
     Considering: 0") → no release PR. It self-heals on the next run because
     collection is "commits since all latest releases". If no release PR
     appears after #423's Release run either, re-run the Release workflow
     (`gh run rerun <id>` or push any commit) before assuming breakage.
3. **Upgrade this machine**: `symforge update` (installed binary was 8.11.1;
   the long-running daemon predates ALL five waves). Restart the daemon /
   MCP sessions so the fixes are actually in the loop. Until then the old
   bugs (tools.rs eviction, retarget hijack, hook noise) still reproduce
   locally — they are fixed in main, not in the running process.
4. **`cargo clean`** after any heavy local gate session (project rule;
   freed 64.9GiB once today).

## Ranked backlog

### 1. Dogfood pass on 8.13.0 (cheap, high signal)

Re-run real work on the upgraded binary to field-verify the campaign:

- External (non-symforge) edits to `src/protocol/tools.rs` (>1MB) must NOT
  evict it — watcher and impact path agree under the 4MB code threshold.
- A subagent running `index_folder` on a scratchpad clone must NOT poison
  the main repo's hooks — expect 409 + daemon by-root fallback, no false
  "not found" alarms.
- Conversational prompts and regex Greps must produce ≤1 line of hook
  context.
- `search_symbols(query="<macro-declared name>")` on a repo using
  `define_id_type!`-style macros must return the `macro-generated` symbol.

Anything that still misbehaves goes into the findings doc as a new numbered
finding (same discipline as 2026-07-06).

### 2. FR-006b proper — per-session/connection-scoped project binding

The one deliberately-open design item from the campaign. Wave 5 closed the
observable symptom (hooks can no longer be answered from the wrong project);
the shared mutable binding remains.

- Spec: `specs/012-harness-agnostic-mcp/spec.md`, FR-006b (added 2026-07-06,
  progress note appended) plus FR-006/007/008 and the base+overlay working-set
  model (FR-001…005) it builds on.
- Reality today: the daemon already keys sessions (`SessionRecord.
  active_project_id`, `set_active_project`, `index_folder_for_session` in
  `src/daemon.rs`) — but one MCP connection = one session shared by the main
  agent AND its subagents, and the non-additive `index_folder` retargets that
  shared session. The local (stdio) server is worse: one process-global index.
- Direction: make retarget additive-by-default at the session level (working
  set + explicit active project per CALLER context), or pin hook/facade
  queries to a declared root per request (Wave 5 did this for hooks only).
  Spec work first, then implementation — the council's standing objection is
  that warnings are bandages; the ticket closes only when the binding is
  per-session.
- Size: the largest item here; treat as its own spec-driven effort.

### 3. Wording debt: Tier-2 disclosure says ">1MB"

`tier2_reference_disclosure` (src/protocol/tools.rs, Wave 1) still describes
the demotion threshold as ">1MB"; since Wave 2, code languages demote at 4MB
(`METADATA_ONLY_CODE_BYTES`), data formats at 1MB. One-line message fix +
test-string touch-ups. Check `impact_admission_refusal` (sidecar/handlers.rs)
and `tests/impact_admission.rs` assertions (one asserts the ">1MB" wording)
while in there.

### 4. Macro-generated symbols: extend beyond Rust module level

Wave 4 shipped the cheap heuristic for Rust (`macro_invocation` idents at
module scope, capped 8, dedup). Known non-goals that may deserve follow-ups
if dogfood asks for them:

- `paste!`/concat-style generated names (compound identifiers) are invisible
  by construction — would need heuristic name synthesis; probably YAGNI.
- Other macro-heavy languages (Elixir `defmacro` use sites, C preprocessor)
  have the same blind spot; wait for a field report before generalizing.
- search_symbols could surface a one-line trust note when results INCLUDE
  macro-generated hits ("declared by macro; body synthesized at compile
  time") — today the kind label alone carries this.

### 5. Verify-tools harness: cover the new surfaces

`scripts/verify-tools.cjs` (2 fixtures, 19 cases) predates waves 3–5. Worth
adding when touching it next:

- an `edit_within_symbol` case with `occurrence:`/`near_line:` (exact-output
  write snapshot),
- a `search_symbols` case for a `macro-generated` symbol,
- (optional) a hook-level case pinning the caller_root 409 contract.
Remember the gotcha: any output-format change breaks snapshots — delete the
stale `.snap` files and re-run with `--update` (harness defaults to
`target/debug/symforge.exe` on Windows; `--bin` for release).

### 6. Spec 016 Perl hardening — remaining optional items

From the 2026-07-06 handoff, everything requested was done (full-surface
init flip for Codex/Gemini/Kilo, SUPER/coderef/use-parent fixtures, CPAN
benchmark at 98.4% raw clean). The three ts-parser-perl gaps recorded in
`docs/research/perl/cpan-benchmark-2026-07-06.md` are upstream parser
limitations — revisit only if Perl work resumes.

## Standing discipline (so tomorrow's session doesn't relearn it)

- Gates: `cargo fmt --check` · `cargo clippy --all-targets --features server
  -- -D warnings` · `cargo test --features server --all-targets --
  --test-threads=1` (single-threaded is a CORRECTNESS gate) ·
  `cargo build --release --features server`. Run suite + release build
  concurrently when the machine is idle; `cargo clean` when done.
- Merge PRs with `--body "PR #<N>"` (release-please double-count guard).
- Wave discipline: one branch/PR per wave, findings-doc Fix log updated in
  the same PR, memory file
  (`~/.claude/projects/.../memory/project_dogfood_findings_2026_07_06.md`)
  updated after merges.
- If symforge's own editing tools report "File not found" on a file you just
  edited externally: that's the pre-8.13 daemon (old binary) — run
  `analyze_file_impact` to re-admit, and upgrade per the immediate queue.
