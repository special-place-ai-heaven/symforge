# STEL v8 — Field Confirmation & Ledger Delta (2026-06-19)

Delta to `stel-v8-skeptic-audit-2026-06-17.md` + `stel-v8-remediation-advice-2026-06-17.md`,
from behavioral dogfooding by three independent agents (Cursor, Mistral, AAP) on
SymForge **8.4.0** across real repos (rtk, justice-compass-app, headroom). Advice only.

## New top finding — promote above everything in the static audit

### C4 — Project binding is single, sticky, and silently un-retargetable (CRITICAL)
- **What happens:** one daemon serves exactly one project, bound to the cwd it was
  spawned in. An MCP client cannot switch the answered repo per call. After a Cursor
  workspace switch to rtk the daemon kept serving an AAP worktree (1439 files,
  `project_root=…Agent_Army_Professionals…`) and answered rtk queries with AAP paths
  while reporting `index_ready: true`. Mistral, bound to the symforge repo, tried the
  `symforge` tool's `path:` param, `serve --help`, `daemon --help`, `init` — found no
  way to retarget, and resorted to spawning a second `symforge serve` on another port
  (the stray foreground console windows seen on this machine are exactly this).
- **Why it's the worst issue:** it yields confident, high-`index_ready` answers about the
  WRONG repository, with no signal to the calling LLM that it is mis-targeted. Silent
  wrong-repo is more dangerous than the empty-index case (C3), and it is hit in normal
  multi-repo agent use, not an edge case. Independently reproduced by 2 agents (Cursor,
  Mistral); a 3rd (AAP) hit the related routing bug below.
- **The trap:** the `symforge` tool exposes a `path:` parameter that reads like a project
  selector but is a within-project filter. It implies a capability it does not have — the
  same "overstated surface" pattern as the rest of the audit, at the param level.
- **Advice (cheapest honest fix first):**
  1. **Make mis-binding visible (S, do now):** surface the bound `project_root` (and file
     count) prominently in EVERY `symforge`/`status` response, not just the health
     resource — so an LLM and user instantly see "this answered AAP, not rtk." This alone
     converts a silent failure into a loud one.
  2. **Document the model (S):** one daemon = one project; switching repos requires a new
     stdio invocation with cwd = target repo (the pattern the Cursor tester used:
     `cd <repo> ; symforge.exe` over stdio). Put this in the MCP onboarding/README. State
     that `path:` is a within-project filter, NOT a project switch.
  3. **Real fix (M-L, the actual ask):** either honor a `project:`/`root:` retarget on the
     tool calls (rebind or fan to the right daemon), or have the MCP client launch/route
     per-repo daemons by cwd automatically. Until then, `path:` should reject or warn when
     given a path outside the bound project rather than silently filtering to nothing.
- **Side effect of fixing C4:** agents stop spawning stray `symforge serve` foreground
  windows, because the legit retarget path will exist.

## Recalibrations to the static audit

- **H1 (if_match TOCTOU) → DOWNGRADE.** Behavioral test: a stale `if_match` after an
  on-disk mutation was correctly refused (`if_match does not match current symbol body`,
  `isError:true`), no clobber; preview left git clean. The pre-flight re-freshens disk and
  catches the realistic stale case. H1 survives only as the NARROW race (a write landing
  between pre-flight check and the actual write) which a manual test cannot trigger. The
  user-facing guarantee holds for normal use. Keep the fix as hardening, not a P0 bug.
- **C3 (status reports empty index) → NARROWER.** Via stdio with cwd = repo, `status` is
  honest (`index_ready: true`, rtk 214 files, `project: rtk`; jc-app 118; headroom 2636).
  The `index_files: 0` / `index_ready: false` is the daemon-PROXY topology only, not
  universal. Real, but topology-specific; C4 is the dominant real-world variant.
- **C1 (economics constants) → LAYER-A PARTLY SHIPPED.** 8.4.0 already relabels the surfaced
  string to `est. … fewer` (the honesty relabel). Confirmed across repos: `predicted_net`
  is only 275 or 675 (route-family bucket), `predicted` ~400/~800, `schema 45`/`invoke 80`
  on every call, unchanged across 656–1636 actual tokens. Layer-B (ground the estimate in
  real index data) still open. Calibration aggregates exist but `predicted` does not track
  `actual` (e.g. 4800 predicted vs 7538 actual over 9 calls).

## New findings beyond the static audit

- **R1 — Symbol parameter ignored; NL word taken as the symbol name (HIGH).**
  `symbol=MinimalFilter` + query "show symbol body" → `get_symbol(name="show")` → not found;
  `symbol=run_err` + "function body" → `get_symbol(name="function")`. The planner parses the
  symbol from the query text and ignores the explicit `symbol` field. Confirmed by Cursor AND
  AAP independently. NOTE: this is exactly what branch `fix/stel-symbol-aware-routing`
  (commits `83af113`, `e12af5e` "honor symbol on read/impact/orient facade routes") targets —
  the 8.4.0 dogfood proves the bug is real; re-dogfood a build of that branch to confirm the
  fix lands. Advice: when `symbol` is provided, it MUST win over query-token extraction.
- **R2 — Reference trace recall ~29% on type/value usages (HIGH).** `find_references` for the
  struct `MinimalFilter` returned 2 definition refs and missed 5 real usage sites
  (rtk `src/core/filter.rs` lines 248/251/318/429/477). Function-call tracing is fine
  (`run_err`, `scrub_sensitive_env_vars` both 100%); type/value usage tracing is the hole.
  Maps to the known A-029 / 8.1 index-recall program — now pinned with line numbers. For a
  trace tool, missing 5 of 7 usages is a real completeness gap an LLM will trust blindly.
- **R3 — Missing symbol silently served the parent file outline (MED).**
  `read TotallyFakeSymbol src/main.rs` returned the full main.rs outline instead of a
  not-found error — same family as R1 (symbol disregarded). Should fail loudly.
- **R4 — Multi-step "then" queries not decomposed (MED, = audit H3).** "find Config struct
  then show who uses it" → single `search_text` with "then" as a search token, no second hop.
  Confirms the planner's general multi-hop is absent (only the 3 hardcoded fixtures decompose).

## What field-testing CONFIRMED works (keep it)
Compact-facade→legacy routing is real; file outlines + large-file truncation honesty; fn-call
tracing precision; edit preview/apply/`if_match` stale-refusal; Tier-2 and out-of-repo errors
are explicit, not hallucinated; per-repo `project:` name when cwd is correct.

## Suggested ledger priority after field data
1. **C4** (project binding / mis-bind visibility) — new #1, silent wrong-repo answers.
2. **R1** (symbol param ignored) — verify the in-flight `fix/stel-symbol-aware-routing` closes it.
3. **C1-B + C2** (ground economics; gate on real signals) — Layer-A relabel already shipped.
4. **R2** (trace recall on value usages) — fold into the 8.1 recall program.
5. **C3** (proxy-mode status) and **H1** (if_match race) — demoted to hardening by field data.

*Sources: Cursor dogfood report (`stel-v8-dogfood-targets-2026-06-19.md`), Mistral retarget
session, AAP migration agent friction tally. All behavioral, SymForge 8.4.0 compact surface.*
