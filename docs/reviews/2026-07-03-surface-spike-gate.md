# Surface spike gate — measurements and verdict (2026-07-03)

Executes phase 2 of the phased decision in
`docs/superpowers/specs/2026-07-03-staged-surface-design.md` §8a. Three
measurements were run by independent agents; raw artifacts are referenced at
the end. This document records the numbers and the gate verdict they force.

## Measurement (a) — full `tools/list` acceptance per harness

Payload, measured over a real MCP stdio handshake against the shipped
`symforge.exe` 8.10.3 (protocol 2025-06-18), not estimated:

| Surface | Tools | Result bytes | Tokens (o200k_base) |
|---|---|---|---|
| `full` | 36 | 71,437 | 16,153 |
| `compact` | 3 | 4,603 | 1,135 |
| names-only (what a deferred harness pays) | 36 | — | 119 |

The spec's "~62 KB" estimate was 15% low; actual is 71.4 KB. Heaviest schema:
`search_text` (5,362 B); per-tool median 1,846 B.

Per-harness acceptance (live-tested without touching any persistent config):

| Harness | Schema handling | Full-surface cost | Accepts full 36? |
|---|---|---|---|
| Claude Code 2.1.199 | deferred (native) — observed | ~119 tokens names-only | **MEASURED: YES** |
| Codex CLI 0.142.2 | deferred via undocumented `tool_search` — observed (docs still claim full-injection) | names/deferred until searched | **MEASURED: YES** (live `status` call succeeded) |
| Gemini CLI 0.37.1 | full-injection — official docs, no cap/deferral | ~16k tokens per request | **MEASURED: YES** (all 36 enumerated, exit 0) |
| Kilo CLI 7.1.19 | full-injection — observed | ~16k tokens per request | **MEASURED: YES** (all 36 enumerated) |
| Kilo VS Code ext 7.2.52 | full-injection into system prompt — official docs | ≥16k tokens per request | INFERRED: YES (no documented cap; not live-tested, GUI-only) |
| Claude Desktop 1.18286 | hybrid — default loads all, official opt-in "On demand" mode | ~16k default / ~0 on-demand | INFERRED: YES (community ~100/server cap ≫ 36; live test forbidden: persistent config + running app) |
| Cursor 3.10.2 | in transition — historical hard 40-tool cap removed from docs; community reports cap lifted | UNKNOWN | UNKNOWN (36 fits even the old cap alone; cumulative cap with other servers unknown) |

**No harness rejected or errored on the full 36-tool list.** Degradation is a
per-turn token cost (~16k) on full-injection harnesses, not a refusal.

## Measurement (b) — tips-follow-rate (behavior-model claim B5)

Mined from 4,154 real symforge MCP calls across 138 sessions
(2026-03-23 → 2026-07-03):

- 63% of all full-surface results carry a `Tip:` line — tips are boilerplate,
  not signal.
- Raw loose follow rate 39.1%, strict 27.7% — but with a base-rate (lift)
  control the effect disappears: for most tipped tools, agents were equally or
  MORE likely to call the tool when it was **not** tipped (lift ≤ 1 for
  `edit_within_symbol` 0.46, `get_file_content` 0.57, `search_symbols` 0.83,
  `search_text` 0.84, `replace_symbol_body` 0.73; `diff_symbols` 0/11).
- The only positive steering came from rare, contextual tips
  (`what_changed`: ~21× lift, n=3).

**Verdict on B5: killed in its strong form.** Always-on tips do not measurably
steer tool choice; saturation destroyed the channel. This independently
undermines the staged design's reveal-discovery mechanism (B9 leaned on tips)
and flags the existing always-on tip machinery as unpaid context spend
(follow-up: make tips rare and contextual).

## Measurement (c) — misroute / turns-to-first-evidence, full vs compact

**Unanswerable from this corpus**: compact-facade routing is 0.3% of calls
(~5 organic data points; the production compact failures happened in project
directories outside the mined corpus). Reported for the record: full-surface
misroute ≤ 6.7% (upper bound; dominated by honest zero-match answers to
speculative searches), median 1 symforge call to first substantive evidence
(2 when diagnostics readouts are excluded), 59% of failures self-recover on
the immediate retry. The full surface's own record is strong; no causal
full-vs-compact claim is made.

## Gate verdict (per §8a phase 3 rule)

The §8a rule: *"if no harness rejects `full` and selection quality holds →
`full` becomes the default, `compact` stays as the escape hatch, and §3 is
SHELVED as design-on-file. Staged is built only if the spike finds a genuinely
constrained consumer."*

- No harness rejects full — condition met (measured on 4 harnesses, inferred
  on 2, unknown only for Cursor's undocumented cumulative cap).
- Selection quality holds — condition met (≤6.7% misroute, first evidence at
  call 1–2).
- Genuinely constrained consumer — **not found**. Gemini CLI, Kilo, and
  default-mode Claude Desktop PAY (~16k tokens/turn) but none REFUSE, and the
  proven-unusable alternative was compact-3 (the 8.10.0 field failure).
  `compact` remains the documented escape hatch for token-sensitive setups.
- Additional strike against staged: its discovery mechanism (tips, B5) is
  now evidenced not to steer.

**VERDICT: `full` becomes the default surface; `compact` stays as the escape
hatch; the staged 7-verb surface stays SHELVED as design-on-file** (§3 of the
design spec), to be revisited only if a harness with a hard cap or genuine
refusal shows up.

Implementation consequences (follow-up wave, separate from the G-036 fix):

1. Flip the env-absent server default from `Compact` to `Full`
   (`surface_profile_from_env` default arm in `src/protocol/surface_probe.rs`)
   plus the test suite that pins the compact default
   (`tests/surface_default_compact.rs`), prompts/docs that state the default,
   and the D22/D23 surface-honesty strings. Note: this retroactively makes the
   G-036-era field configs (37-name allowlist, no env) coherent.
2. Decide per-harness init values for full-injection harnesses
   (Gemini/Kilo/Desktop/Cursor currently written as explicit `compact` by the
   G-036 fix): keep compact-by-init for the ~16k/turn harnesses, or flip to
   full. Operator decision; the G-036 preserve semantics make either safe to
   change later.
3. Tips overhaul (rare + contextual) tracked as a separate backlog item.

## Incidental findings worth tracking

- Codex CLI 0.142.2 has an undocumented `tool_search` deferred layer — docs
  lag the binary; both Anthropic and OpenAI CLIs now defer schema loading.
- Gemini CLI 0.37.1 oauth-personal auth is dead (`IneligibleTierError`,
  "migrate to Antigravity") — `symforge init` guidance for Gemini should
  assume API-key auth.
- The `status` tool's readout opens with the internal codename banner
  (`── stel status ──`) — visible verbatim in third-party harnesses; consider
  a product-name banner.

## Raw artifacts

Session scratchpad (`C:\Users\rakovnik\AppData\Local\Temp\claude\E--project-symforge\6a067a28-d117-45c5-a205-9bf6ac08eb7d\scratchpad\`):
`tools_list_full.json`, `tools_list_compact.json`, `measure_tools_list.py`
(payload measurement); `mine_surface.py`, `mine_pass2.py`, `results.json`,
`results2.json` (transcript mining). Harness live-test configs were scratch-dir
only; no persistent config was modified during measurement.
