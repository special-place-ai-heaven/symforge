# HANDOFF — SymForge trust campaign (resume here)

**As of:** 2026-06-23. **Branch state:** everything below is merged to `main` (released as **v8.8.0**).
**Mission:** make SymForge trustworthy for the LLMs that call it and robust for users — by attacking ROOT causes of its trust defects, not patching symptoms. Engine-first: the facade is a thin honest layer; AAP consumes the engine via the `embed` feature (NOT the MCP server).

## How we work (durable rules — honor these)
- **A defect is a defect, not an "honest gap"** (now in global `~/.claude/CLAUDE.md` §0). Found an issue → name it plainly → add to the ledger → decide on the spot if it's a symptom of a larger root → if so, investigate + attack the root. Don't plug holes endlessly.
- **Deferral policy** (global CLAUDE.md §0): defer *scope/capability* (with a loud, tracked, owned refusal), never *defects*.
- **Never leave open PRs for the user** — process/merge them all yourself via **git-master** with the push mandate; carry through to merge+delete with the release-please guard `gh pr merge <N> --merge --delete-branch --body "PR #<N>"`. (agentmemory `mem_mqpbsq29`.) Still surface (don't execute) destructive ops on another lane's *live* branch.
- **Verify as user**, not "tests passed": the full cargo gate (`fmt --check`, `check`, `clippy --all-targets -D warnings`, `test --all-targets --test-threads=1`, `build --release`, `check --no-default-features --features embed`) AND behavioral/live evidence before claiming done. Full gate green before ANYTHING merges to main.
- **Ultracode** was on: use Workflow for substantive design/investigation; adversarially verify load-bearing work.

## THE source of truth
- **`docs/reviews/symforge-defect-ledger.md`** — every defect, named plainly, ROOT vs SYMPTOM, clustered to 3 culprits + the vetted attack sequence with owners. READ THIS FIRST.
- `specs/012-harness-agnostic-mcp/{spec,research,plan,integration-plan}.md` — the engine-first design + the adversarially-vetted integration plan.
- `docs/reviews/stel-v8-skeptic-audit-2026-06-17.md` + `stel-v8-field-confirmation-2026-06-19.md` — the original audit + 3-agent field evidence (where the defects came from).

## The 3 root culprits
- **A — lossy/fabricating facade**: silently drops caller params + emits unmeasured numbers. Fix = `lossless-or-loud` + `honest-envelope` contracts, enforced by conformance tests.
- **B — engine multi-view search has no per-view derived index / no live rebase**: cross-project search is scoping-less, stale, low-recall.
- **C — `/mcp` is a stateless single-index singleton**: no per-connection session.

## DONE + on main (v8.6.0)
- Engine **base+overlay `IndexView` primitive** (`src/live_index/view.rs`): immutable base shared by `Arc` keyed `(canonical-root, commit)` + per-consumer CoW overlay; O(K) rebase (spike-proven); `WorkingSet` cross-project query w/ source attribution. `embed` contract unchanged.
- **US1 cross-project query**: `index_folder(add:true)` opens projects additively; `search_symbols/search_text/find_references` + `StelRequest` gain `project`/`projects` → `Targets`; single-project path byte-identical. Daemon-only (stdio refuses honestly). Proven by a real daemon-HTTP integration test.
- **C4** wrong-repo fix: per-connection retarget + bound `project_root` in every response.
- **Honesty hardening**: facade refuses cross-project params it can't route; cross-project output bounded+disclosed; `if_match` normalized pre-flight; clean `query`-required error; glossary MCP resource.
- **A1a** — `ParamDisposition` choke point (`src/stel/planner.rs`) + conformance test (`tests/stel_param_disposition.rs`): every `StelRequest` field resolves to an explicit disposition; silent-drop class non-regressable. Zero behavior change.
- **D17** — atomic open (`src/daemon.rs register_session_for_existing_project`): eliminates the open-vs-close TOCTOU; open is fail-never (8-thread stress test asserts it).

## REMAINING — the attack sequence (do in this order)

**B1 — DONE** (implemented, full-gate-green, adversarially reviewed; D11 scoping **FIXED**, D14 ranking **PARTIAL** — per-project ranked+bounded, global cross-project interleave deferred). **A1b — DONE** (PR #358). **C-stopgap — DONE** (012d; D16 silent-wrong half CONTAINED — `/mcp` already refused cross-project via `local_cross_project_refusal`, the gap was a missing regression lock + the message not naming the `/mcp` transport, both now closed). **NEXT = B2/D12.** The original B1 recipe is retained below for reference:
- The empty-overlay fast path `IndexView::search_symbols` (`view.rs:341`) calls the **preset-only** `base_search_symbols(... usize::MAX)` (hardcodes path_scope=Any, language=None). The option-honoring `search_symbols_with_options` / `search_text_with_options` (`search.rs:808/943`) ALREADY EXIST.
- Thread the caller's scoping/limit options through `IndexView`'s search methods → `WorkingSet` cross-project query passes them down → daemon `execute_cross_project_read` builds the options from `SearchSymbolsInput/SearchTextInput/FindReferencesInput` AND **removes `reject_unsupported_cross_project_scoping`** (the honesty-pass guard that loudly refuses path/language/etc.).
- Overlays are EMPTY in US1 (no-overlay-writes invariant) so the empty-overlay path is the cross-project path. Per-overlay derived index for NON-empty overlays is the deferred large item **D-B0**.
- Update the rejection tests (they assert refusal) → assert honoring; add scoping-honored cross-project tests.

Then: **A1b** — DONE (PR #358) · **C-stopgap** — DONE (012d — the `/mcp` refusal was already wired since Phase 3; 012d added the regression lock `tests/serve_http_attach.rs` + named the `/mcp` transport in the refusal) · **B2/D12** — **NEXT** (republish→rebase on HEAD/watcher advance; mechanism `Overlay::rebase` + `StaleOverlay` fence exist).

**Tracked-large (OPEN, owners, blocked-on — NOT deferred-as-acceptable):** D-B0 (per-view derived index, after cross-project writes) · D15 (overlay edits in ordinary reads — Phase 5 read-path flip, ~64 `self.index.read()` + ~20 `capture_*`) · D16 (`/mcp` per-connection daemon session) · D13 (xref recall ~29% — `parsing/xref.rs` qualified-call extraction, NOT a view defect) · D2 (gate decides on estimated economics — owner 013 lane) · serve_port test-fragility (local Docker-Desktop port collisions).

## Working environment + gotchas
- **`E:\project\symforge-012`** worktree is now on **`main`** (clean). Branch the next attack from here. (`feat/012` was merged + deleted.)
- **`E:\project\symforge`** = the **013 predictor-calibration lane's** worktree (branch `013-stel-predictor-calibration-spec`) — has **unpushed commits**; DO NOT disturb. Its remote branch isn't pruned for that reason.
- **`E:\project\symforge-perl`** = perl-grammar lane.
- AAP repo: `E:\project\Agent_Army_Professionals` (consumes symforge via `embed`, git dep on main; diagrams in `docs/`).
- Symforge MCP/daemon can disconnect/flake mid-session; cross-project is **daemon-only**; verify behaviorally via a built binary, not just unit tests.
- Subagent rate limit historically resets ~2pm Europe/Ljubljana.
- A2 (`Figure` provenance enum) was DEMOTED to regression-guard (envelope already honest); don't prioritize it.

## Immediate first action for the next session
A1b (PR #358) and C-stopgap (012d) are landed. **C-stopgap TRACE FINDING** (the prior handoff left this trace unfinished and ASSUMED a silent drop): the `/mcp` cross-project refusal was ALREADY wired since Phase 3 — `local_cross_project_refusal` fires on `/mcp` because that transport's `SymForgeServer::new` leaves `daemon_client=None`, so `proxy_tool_call` short-circuits to `None` before any single-project answer. 012d closed the real gaps: a missing regression lock (`tests/serve_http_attach.rs::cross_project_targeting_is_refused_over_http` — real HTTP transport; all three cross-project tools refuse + name the `/mcp` transport; a no-params control does NOT refuse) and the refusal message now naming the `/mcp` transport. NO new refusal logic. D16's silent-wrong half is CONTAINED; the ROOT (per-connection `/mcp` daemon session) stays tracked-large.

**NEXT:** Read `docs/reviews/symforge-defect-ledger.md`, branch a fresh `feat/012e-b2-rebase` (or similar) off `main` in `E:\project\symforge-012`, and implement **B2/D12** (republish→rebase on HEAD/watcher advance; `Overlay::rebase` + `StaleOverlay` fence already exist) → full gate → PR → merge yourself.
