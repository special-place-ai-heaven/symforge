# INSTRUCTIONS — SymForge Token-Cost Optimization Loop

**Locked to the AI. Edited only by the human.**

## Goal

Shrink the token cost of SymForge MCP tool *output* (the text a caller pays
for per response) without losing information content or violating the trust
contract (`specs/010-v8-trust-remediation/`). This is **portfolio
optimization**, not single-file evolution: rank formatter output paths in
`src/protocol/format.rs` (and siblings) by token cost on fixed fixtures, and
attack the current worst offender each round.

Feature `011-ccr-output-compression` already shipped cross-call savings
(session cache-hits, ranked search compaction, CCR retrieve, dedup hints).
**Do not re-solve those.** This loop targets *per-call* formatter verbosity —
the shape of a single fresh, uncached response.

## Rules

1. **Asset**: files under `src/protocol/` (formatters), plus
   `src/sidecar/handlers.rs::repo_map_text` (the real formatter behind
   `get_repo_map`'s compact mode — widened into scope at Round 2 once it
   turned out to be outside `src/protocol/`). Never touch
   `research/token-cost/score.py` or this instructions file.
2. **One hypothesis, one change, one round.** Pick the current worst-scoring
   fixture/tool pair, form one hypothesis, make one change, rescore only that
   pair.
3. **Keep or revert**: if the round's score beats its own baseline (strictly
   lower tokens, same or better information content — see "What counts as a
   win" below), keep the change and it becomes the new baseline for that
   fixture. If not, `git checkout --` the touched file(s) back to the prior
   commit and try a different hypothesis next round.
4. **Never touch trust-contract prose.** Anything backing
   `specs/010-v8-trust-remediation/` honesty/envelope guarantees (parse
   resilience wording, STEL envelope fields, admission/degraded-mode
   disclosures) is off-limits for deletion. Compressing *wording* is fine;
   deleting *disclosures* is not.
5. **What counts as a win**: strictly fewer real tokens (see score.py) on the
   frozen fixture, with no loss of distinct information (same set of facts
   recoverable from the output — renaming/shortening labels is fine,
   dropping a field that has no other source is not).
6. **No moving the goalposts**: scoring logic lives in `score.py` only and is
   locked. If a fixture or metric turns out to be wrong, tell the human and
   wait — don't quietly patch the scorer to make a change look better.
7. Run in ~5-minute rounds, overnight, indefinitely, until the human stops it
   or the ranked worst-offender list stops improving for 2 consecutive full
   passes (diminishing returns = pause and report, don't grind forever).

## Scope note (why not all 32+ tools at once)

The scoring file freezes a small fixture set (see `score.py`) covering the
formatters the initial survey flagged as highest-cost:
`get_symbol` (full body dump), `search_text` (default view), `get_repo_map`
(outline rows), `get_symbol_context` (callers/callees padding),
`find_references` (verbose per-hit view). Expand the fixture set only with
explicit human approval — adding fixtures silently changes what "biggest
win" means mid-run.

## Cross-repo validation (blocked, revisit later)

Ideally fixtures would also run against a large external project
(`C:\AI_STUFF\PROGRAMMING\Agent_Army_Professionals` was proposed) to catch
formatter changes overfit to SymForge's own code shape. Current MCP session
is hard-scoped to one `project_root` at connect time; the `project`/`projects`
params exist in the protocol (feature 012) but are documented as
surface-parity-only on the compact facade — not yet routed. Revisit by
opening a second MCP-connected session pointed at that project and running
the same fixture *shapes* (large function, common search term, repo map,
referenced symbol, well-used type) there for comparison, once 012 wires
cross-project routing through or a second session is available.

## Verification path

`cargo build --release` is buildable in this environment (release binary
confirmed at v8.10.0, matching `main`). Prefer scoring via the already-connected
live MCP session (`mcp__symforge__*` tools) — it needs no extra harness and
reflects the exact running server. Rebuild + rerun `cargo test` after each kept
change to catch regressions the fixture set wouldn't show.

## Log

Every round appends one line to `research/token-cost/log.md`:
`round #, fixture/tool, hypothesis, tokens before -> after, kept/reverted`.
