# SymForge Defect Ledger

Plain-named defects (no euphemisms — a missing/broken/wrong feature is a DEFECT, not an "honest gap"). Each is tagged ROOT or SYMPTOM and clustered to its culprit. "Loudly refused / disclosed" means we stopped a *silent* wrong answer — it does NOT mean the defect is resolved.

Status: OPEN | IN-PROGRESS | FIXED. Owner: 012 (this lane) | 013 (predictor lane) | new.
Last updated: 2026-06-22.

## CULPRITS (root causes — these get attacked, not their symptoms)

- **CULPRIT A — the STEL facade is a lossy, fabricating router.** It (1) emits numbers it never measured, (2) silently drops caller params it didn't curate, (3) makes claims it never validated. Every facade trust defect below is a symptom of this one design.
- **CULPRIT B — the engine's multi-view search has no per-view live derived indices and no live rebase.** `WorkingSet`/`IndexView` search runs base-only + overlay post-filter, so cross-project/overlay search is scoping-less, stale, and low-recall.
- **CULPRIT C — `/mcp` is a stateless single-index singleton.** No per-connection session state on the HTTP transport.

## Defects — CULPRIT A (lossy/fabricating facade)

| ID | Defect (plain) | Root/Symptom | Status | Owner |
|----|----------------|--------------|--------|-------|
| D-A0 | The facade routes a curated subset of each tool's params and silently drops the rest; decorates output with values it didn't measure. | **ROOT (Culprit A)** | OPEN | new |
| D1 | Economics numbers (`saved`/`predicted`/`net`) are hardcoded constants, not measured. The tool reports savings it cannot substantiate. | SYMPTOM(A) | IN-PROGRESS | 013 |
| D2 | serve/degrade/bypass gate decides on the fabricated economics; degrade/bypass branches are unreachable for real plans. | SYMPTOM(A) | OPEN | 013 |
| D3 | `session_net_vs_manual` is a rising gross token counter mislabeled as net savings. | SYMPTOM(A) | OPEN | new |
| D4 | `status` reports an empty/zero index in daemon-proxy mode (reads the wrong process). project_root now surfaced (C4), but the proxy index/ledger numbers still read the front shell. | SYMPTOM(A) | PARTIAL | new |
| D5 | Multi-hop "A-009 VALIDATED" is 3 hardcoded query strings; the validation claim is false. | SYMPTOM(A) | OPEN | new |
| D6 | Golden replay asserts route *shape*, never answer equivalence; `expected_equiv` is dead; any "95% trajectory" claim is unsupported. | SYMPTOM(A) | OPEN | new |
| D7 | `symbol` param ignored on read/impact/orient (query token taken as the symbol name). | SYMPTOM(A) | VERIFY | 012/branch |
| D8 | `path:` reads as a project selector but is a within-project filter; vocabulary trap. Glossary + error added (loud), facade vocabulary still lossy. | SYMPTOM(A) | LOUD-ONLY | new |
| D9 | The `symforge` facade silently dropped `project`/`projects`. Now **loudly refused** — but the defect (facade does not route cross-project) is unfixed. | SYMPTOM(A) | LOUD-ONLY | new |

## Defects — CULPRIT B (engine multi-view search: no per-view derived index, no live rebase)

| ID | Defect (plain) | Root/Symptom | Status | Owner |
|----|----------------|--------------|--------|-------|
| D-B0 | `WorkingSet`/`IndexView` `search_text`/`find_references` run base-only + overlay post-filter; no per-view derived (trigram/reverse) index; no live rebase on change. | **ROOT (Culprit B)** | OPEN | new |
| D11 | Cross-project scoping (`path`/`language`/`symbol_kind`/`direction`/`structural`) is broken — currently **loudly refused**, not honored. | SYMPTOM(B) | LOUD-ONLY | 012 |
| D12 | Cross-project base is a frozen snapshot from open time; results go stale after ANY watched change (not just commits). No republish→rebase. | SYMPTOM(B) | OPEN | 012 |
| D13 | Reference trace recall ~29% on type/value usages (`find_references` misses value sites). | SYMPTOM(B) | OPEN | new |
| D14 | Cross-project output ranking is working-set/tier order, not real relevance ranking (capped, but not ranked). | SYMPTOM(B) | LOUD-ONLY | 012 |
| D15 | Single-project overlay edits are NOT visible in ordinary reads (read path uses `self.index`, not `IndexView`). | SYMPTOM(B) | OPEN | 012 |

## Defects — CULPRIT C (transport) + independent

| ID | Defect (plain) | Root/Symptom | Status | Owner |
|----|----------------|--------------|--------|-------|
| D16 | `/mcp` is a stateless single-`SymForgeServer`-over-one-index; remote multi-tenant/multi-project impossible. | **ROOT (Culprit C)** | OPEN | new |
| D17 | open-vs-close TOCTOU race in the daemon (fail-loud, pre-existing on main). | ROOT (independent) | OPEN | new |
| D18 | Reading a missing symbol silently returns the parent file outline instead of a not-found error. | SYMPTOM(A) | OPEN | new |
| D19 | No multi-step query decomposition (only the 3 hardcoded multi-hop strings). | SYMPTOM(A) | OPEN | new |

## FIXED this engagement (012 lane, verified green)

- C4 wrong-repo binding → per-connection retarget + bound-root visibility (real daemon test).
- if_match TOCTOU/over-strict → normalized pre-flight compare, write-time byte guard preserved.
- Opaque `query`-missing error → clean validation.
- Glossary MCP resource added.
- Overlay-search correctness (noise/scope gate, caps, truncation, determinism, coherent counts).
- close_session multi-project leak → reaper + regression test.
- Strict-MCP-client schema rejection of Phase 3 `projects` fields → `schemars(with=...)`.
- Base+overlay engine primitive + cross-project query (US1) — live-verified.

## Attack plan (roots, not holes)

- **Attack Culprit A:** a structurally-enforced facade contract — **lossless-or-loud** (every caller param is routed, forwarded, or explicitly refused — never silently dropped) + **honest-envelope** (only measured or explicitly-`est.`-labeled values reach the wire), with a conformance test so the class cannot regress. Dissolves D-A0, D3, D5, D6, D8, D9, D18, D19 and removes the fabrication behind D1/D2.
- **Attack Culprit B:** per-`IndexView` derived indices + republish→rebase in the engine, so cross-project/overlay search honors scoping (D11), stays fresh (D12), ranks (D14), recalls value usages (D13), and ordinary reads see overlay edits (D15).
- **Attack Culprit C:** per-connection session dimension on `/mcp` (stateful mode / per-connection proxy server).
- Independent: D17 race hardening.

## Attack sequence — vetted 2026-06-22 (adversarial critique wf_981b5b87, verdict REVISE→PROCEED w/ A1a)

This ledger lives on `feat/012` (worktree); A1/A2/B1 all land here (the superset branch). The 013 lane keeps its own `013-findings-ledger.md` (root D3 = storage/transport coupling via `cfg(feature="server")`) — coordinate, out of this lane's scope but tracked.

Sequence by (defects-killed ÷ effort), gated by file independence:
1. **A1a** — `ParamDisposition` choke point in `build_plan_from_steps` + conformance test. Every `StelRequest` field resolves to `Routed|Forwarded|Refused|NotApplicable`; silent-absent is asserted-against. **Zero behavior change — `routes.golden.jsonl` does NOT move.** Erects the non-regressable guard against the silent-drop class (D-A0) at zero risk. **ATTACK FIRST.** Owner 012.
2. **D17** — collapse the open-vs-close TOCTOU (single `projects.write()` entry). S/LOW, isolated. Owner 012.
3. **B1** — thread caller options through the empty-overlay search path (`search_*_with_options` already exist) → honors D11 scoping + D14 ranking on the cross-project read path. S-M/LOW. Owner 012.
4. **A1b** — gated per-tool forwarding (`max_tokens`→args, `path`→`path_prefix`); golden re-baselined. Owner 012.
5. **C-stopgap** — `/mcp` loudly refuses `project`/`projects` (contain D16's silent-wrong half). Owner 012.
6. **B2** — republish→rebase on HEAD/watcher advance (D12). Owner 012.

A2 (`Figure` provenance enum) = DEMOTED to regression-guard (envelope already honest); low priority, owner 012.

Tracked-LARGE (OPEN, real owner + blocked-on — NOT euphemized):
- **D-B0** per-view derived index for non-empty overlays (K-delta trigram merge) — owner 012, blocked-on: cross-project-write track.
- **D15** overlay edits in ordinary reads (Phase 5: ~20 `capture_*` port + read-path flip) — owner 012, blocked-on: read-path migration.
- **D16** `/mcp` per-connection daemon session — owner 012, blocked-on: stateful-mode substrate + parity re-test.
- **D13** xref recall ~29% (now: `parsing/xref.rs` extraction defect) — owner: recall/8.1 program.
- **D2** gate decides on estimated economics for non-read routes — owner 013, blocked-on: grounding extension to search routes.
- **D5/D6** false "VALIDATED"/"95% trajectory" claims — doc demotion, owner 012.
