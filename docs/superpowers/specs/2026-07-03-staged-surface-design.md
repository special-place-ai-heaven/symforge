# Staged Tool Surface — Design (8.11.0)

**Status:** DRAFT for review · **Date:** 2026-07-03 · **Owner:** surface redesign (post-dogfood campaign)
**Supersedes:** compact-3 as default (`SYMFORGE_SURFACE` default flips from `compact` to `staged`).
**Evidence base:** waves 1–3 of the 2026-07 dogfood campaign (D21–D23, PR #397/#399/#401), A-017 (OPEN — compact-routing premise never validated), A-019 (VALIDATED only for a harness-relay byte-parity A/B, not discoverability), field failure of 8.10.0 compact-3 in production.

## 1. How LLMs actually use MCP tools (the behavior model this design serves)

Every choice below traces to one of these observations. If a future change can't be traced to one, it doesn't belong in the surface.

- **B1 — Agents arrive with intent, not curiosity.** An LLM never browses tools for fun; it holds a task ("fix this bug", "map this repo") and wants the shortest path from intent to evidence. Tools named after task verbs get used; tools named after implementation internals get ignored.
- **B2 — Names are read before schemas, schemas often never.** Tool selection is dominated by name semantics and the first sentence of the description. A familiar-looking name beats a better-suited unfamiliar one.
- **B3 — First-call success decides adoption.** If the first call to a tool errors or returns something confusing, the agent abandons the tool for the rest of the session (field-observed: 8.10.0 compact refusals → "the LLM simply could not use its tools"). Deterministic, forgiving defaults are load-bearing.
- **B4 — Agents imitate their own recent successes.** One working call pattern gets repeated verbatim. Whatever the entry tools teach on turn one becomes the session's dialect.
- **B5 — In-band result text steers harder than tool lists.** A `Tip:` line naming a tool inside a result reliably triggers use of that tool — even when the agent never read its schema. Protocol-level notifications (`tools/listChanged`) are honored by the client but not "noticed" by the model unless the text says it too.
- **B6 — Parameter ceremony is failure surface.** Every required parameter multiplies first-call failure odds. Optional-with-good-defaults wins (proven: wave-1 name-only `get_symbol`).
- **B7 — Errors are read once and acted on literally.** An error naming the exact corrective call gets followed; a generic error causes blind retries or abandonment. An error naming a tool the caller cannot invoke is worse than no hint (P0-1, D23).
- **B8 — Agents budget context and avoid tools that flooded them once.** Caps + truthful pagination keep a tool in play (proven: wave-1 detect_impact 54 MB → 67 KB).
- **B9 — Session memory fades.** Compaction erases early teaching; discoverability must be continuous (tips on every result), not a one-time menu.
- **B10 — Strict clients cannot call names they have not seen.** Most harnesses refuse to invoke a tool absent from their current `tools/list`. Server-side permissiveness is necessary but not sufficient; the reveal loop must actually update the client's list.

## 2. Principles

1. **Advertise progressively, allow always.** Staging shapes *attention*, never *capability*. Any real tool called by name serves (and auto-reveals its group). There is no policy gate anywhere in this design. (Anti-goal: rebuilding the compact-3 refusal wall.)
2. **The intelligence lives in disclosure, never in silent routing.** Entry tools have ONE deterministic documented behavior each. No intent-guessing planner sits between the agent and a real tool (Culprit A lesson; D5/D-A0 lineage).
3. **Disclosure rides the results.** Every reveal is stated in-band in the result text; `tools/listChanged` is fired for the client, text is written for the model (B5).
4. **Every leaf tool stays real.** The staged surface exposes the same canonical tools (36 as of 8.10.2) with their real names and schemas; nothing is renamed, wrapped, or degraded.

## 3. The surface

### 3.1 Initial `tools/list` (staged mode): 7 entry verbs, 1:1 with `tool_catalog_groups()`

| Entry tool | Group (existing) | Flagship behavior (deterministic) | Reveals |
|---|---|---|---|
| `orient` | orientation | repo map / overview; `query="what can you do"` → the catalog (groups + blurbs) | `get_repo_map`, `get_file_context`, `explore`, `ask`, `conventions` |
| `find` | search | find-fusion (the existing honest multi-surface fan-out, merged plan) | `search_symbols`, `search_text`, `search_files`, `inspect_match` |
| `read` | symbol-context | `get_symbol` semantics (name-only supported) | `get_symbol`, `get_symbol_context`, `find_references`, `get_file_content` |
| `impact` | impact-analysis | `detect_impact` (capped, per-list pagination, origin/main-preferring base) | `find_dependents`, `what_changed`, `diff_symbols`, `analyze_file_impact`, `find_references`, `get_symbol_context` |
| `edit` | dry-run-edits | `edit_plan` (plans; NEVER applies) | all 8 edit tools |
| `project` | project-switching | index a repo (or several) / switch the active repo | `index_folder`, `checkpoint_now` |
| `diagnose` | diagnostics | status/health readout (connection-surface honest, per D22) | `health`, `health_compact`, `validate_file_syntax`, `context_inventory`, `investigation_suggest` |

Notes:
- The seven names are task verbs (B1/B2). Descriptions front-load the flagship action in the first sentence and end with "using this reveals: <tool names>".
- `diagnose` carries the trust readout from turn one — the trust surface is never hidden.
- The compat aliases `symforge`, `symforge_edit`, `status` remain callable (not advertised in staged mode) for existing configs and CLAUDE.md files. `status` maps to `diagnose`'s flagship; facade dispatch is unchanged for legacy callers.
- Entry-verb params mirror their flagship tool's params (same names, same optionality — B6). No new vocabulary. One documented superset: `project` with no args lists the indexed projects (the switch/index behaviors take `index_folder`'s params unchanged).

### 3.2 Reveal mechanics

A group is revealed when ANY of:
1. its entry verb is called (flagship runs; group tools appended to `tools/list`);
2. any member tool is called directly by name (serves immediately; group revealed as side effect — principle 1);
3. the agent asks the catalog to open it (`orient` with a "show me X tools" query).

On reveal, the server:
- appends the group's real tools to the advertised list and fires `tools/listChanged` (B10);
- prepends one line to the triggering result: `Revealed: dry-run-edits — 8 tools now available: replace_symbol_body, edit_within_symbol, …` (B5);
- reveals are additive and session-sticky; nothing is ever hidden again mid-session.

`Tip:` lines in results may name not-yet-revealed tools; a tip naming a hidden tool is itself served by rule 2 the moment the agent follows it (tips remain the continuous-discovery channel per B9 — no change to existing tip machinery).

### 3.3 First-call success requirements (B3)

- Each entry verb's flagship must succeed on a bare call in a ready index (`orient` with no args → repo map; `find` requires only `query`; `read` requires only `name`; `impact` with no args → uncommitted-changes impact; `edit` requires target+goal; `project` with no args → list indexed projects; `diagnose` with no args → status readout).
- On an empty index, every entry verb returns the D23-fixed connection-surface-aware recovery hint (B7) — and the hint's named recovery action (`project` / `index_folder`) is ALWAYS in the initial advertised surface, killing the P0-1 loop class structurally.

## 4. Modes and configuration

`SYMFORGE_SURFACE` values:

| Value | Behavior | Who it's for |
|---|---|---|
| `staged` (**new default**) | §3 above | harnesses without native deferred tool loading; the general case |
| `full` | all 36 tools advertised from turn one | harnesses with native deferred loading (Claude Code) — the client is already the drill-down; hiding names there only hurts (B10) |
| `compact` | legacy 3-tool facade | byte-constrained legacy configs; documented escape hatch, no longer default |

**Init/update coherence (folds in the G-036 root fix — this ships in the same release or the design fails in the field):**
- `symforge init` detects the harness (Claude Code / Codex / Gemini CLI / unknown) and writes an EXPLICIT `SYMFORGE_SURFACE` env plus an allowlist that matches the surface it wrote. A config in which the allowlist and surface disagree must be impossible to generate.
- `symforge update` / re-registration PRESERVES an existing user-set env and allowlist; it never regenerates the MCP entry from scratch when one exists (field evidence: the 8.10.1 update wiped `SYMFORGE_SURFACE=full` and reproduced G-036 in production, 2026-07-02).
- Client `tools/listChanged` support is verified per target harness during implementation ([V] gate below); harnesses that ignore it get `full` or `compact` from init, never `staged`.

## 5. Honesty requirements (Principle VII carried forward)

- `diagnose`/`status` reports the ACTIVE mode and the currently-advertised tool count (`surface: staged (12/36 revealed)`), connection-surface honest per D22/D23.
- Reveal lines never overstate: only tools actually appended to the list are named.
- The entry verbs' descriptions state their flagship mapping explicitly ("runs detect_impact") — no pretense of intelligence beyond disclosure.
- Version-skew: an old client config calling `symforge`/`status`/`symforge_edit` works unchanged; a new staged server behind an old all-35 allowlist serves every name (allow-always), so the G-036-era config class degrades gracefully instead of refusing.

## 6. Testing strategy

- Unit: reveal-state machine (initial 7, additive reveals, idempotent re-reveal, session-stickiness); entry-verb → flagship arg mapping is deterministic (table-driven).
- Conformance: staged-mode `tools/list` = exactly the 7 verbs (+ nothing hidden-but-refused: calling every one of the 36 real names on a staged server serves and reveals).
- The strict-client schema compat suite runs against all three modes.
- Golden replay: entry-verb flagship routes are pinned like any other route.
- Live [V] gate before ship: drive a real staged session from Claude Code AND one listChanged-limited harness; verify the reveal loop end-to-end (the in-process test gap documented in D23 applies here too — live verification is mandatory, not optional).

## 7. Explicitly out of scope

- Any intent-classification / NL routing inside entry verbs (Culprit A; permanently out).
- Policy gating (capability restriction) of any tool group.
- Renaming/removing any of the 36 canonical tools.
- The `standard` static 15-tool tier (obsoleted by staged; the groups ARE the tier, disclosed dynamically).
- Server-side per-connection surface negotiation via `initialize` clientInfo sniffing (future option; init-time detection covers today's need).

## 8. Open questions (to resolve in planning)

1. Does Claude Code re-read `tools/list` promptly on `listChanged` in current builds? (Determines whether `staged` is also viable there, or `full` remains its init default. Verify empirically.)
2. Reveal-state residency: per MCP connection (adapter process) vs per daemon session — must reveals survive an adapter restart mid-session? (Lean: per-connection, stateless daemon; consistent with D22/D23 threading.)
3. Should `orient`'s catalog answer also be an MCP *resource* (it nearly is: `tools_catalog_resource` exists) so harnesses can pin it in context?
