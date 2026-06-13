# SymForge v8 — Gap closure plan (binding)

**Status:** PRE-IMPLEMENTATION — **no `src/stel/` until §12A checklist is 100% green**  
Branch: `v8/stel-architecture`  
Supersedes ambiguous items in other docs when they conflict.

**Stack:** [`ideation.md`](ideation.md) → **this file (execution truth)** → [`v8-master-plan.md`](v8-master-plan.md) → [`stel-schema.md`](stel-schema.md) → [`stel-assumptions.md`](stel-assumptions.md)

---

## 0. Hard rule

```text
We do not start STEL implementation until every gap in §3 has a CLOSED or
ACCEPTED-RISK verdict with a pinned artifact, and §12A Pre-flight is all [x].

If a spike hits KILL criteria → pivot per §4 decision tree → re-validate →
only then continue. No “implement anyway and fix later.”
```

**End state (8.1.0):** stdio + **`symforge serve`** (URL + API key), compact STEL, economics gates on 8.0, quality + deploy on 8.1, **committed operator stack (O1–O8, A9)**, **AAP embed path preserved (G-043..G-045)**, sf-bench reproducible on pinned SHAs.

### Paradigm shift (7.x bench vs v8)

```text
sf-bench on SymForge 7.21.1 = informational autopsy of the OLD product only.
It explains why v8 exists (schema tax, trace gaps). It is NOT the v8 scoreboard.

v8 proof = north star gates (H1–H8) measured on the SAME corpus + methodology,
but compared to v8's own pinned baselines after the paradigm ships — not
"beat 7.21.1 on its terms while still carrying 32-tool DNA."

First v8 green battery → pin results-v8-8.0-baseline.json → all regressions
diff against THAT. Optional appendix: 7.x numbers for historical context only.
```

## 1. North star (locked definitions)

| Term | Definition |
|------|------------|
| **Accepted serve row** | Controller `decision=SERVE` **and** judge = `EQUIVALENT` (S vs M evaluated separately — a row can be equivalent yet sGteM; that hurts **H4**, blocked by **H3** on small-file serve rows) |
| **BYPASS row** | Controller returns explicit cheaper path; S = bypass response tokens only; **excluded from H6** numerator/denominator (**A-023**) |
| **`session_net_accepted`** | Σ(M − S) over **accepted serve** rows — **H4 headline** |
| **`session_net_all36`** | Σ(M − S) over all 36 rows — diagnostic only |
| **sGteM** | S ≥ M on a row (schema + payload) |
| **SYMFORGE-LESS** | Equivalence judge: under-answer vs manual load-bearing lines |

**Product promise:** every **accepted serve** call wins or we bypass honestly. Quality (H6) is a **separate program** in 8.1 — not hidden inside token sums.

---

## 2. Release map (no ambiguity)

| Release | Ships | Gates | Transport |
|---------|-------|-------|-----------|
| **8.0.0** | Compact STEL, controller, ledger, stdio MCP | **H1–H5, H7** | stdio (existing) |
| **8.1.0** | Reference quality + unified server + **operator UX** | **H6, H8** + deploy + **O1–O8** | Streamable HTTP `/mcp` + admin `/admin` |

Do **not** tag 8.0 claiming “easy deploy”; do **not** defer H6 fixes silently into 8.0.

---

## 3. Gap register (every gap → closure)

Status: **OPEN** until artifact linked in [`stel-assumptions.md`](stel-assumptions.md) research log.

### 3.1 Measurement & harness

| ID | Gap | Closure action | Artifact | Pass | Pivot | Kill |
|----|-----|----------------|----------|------|-------|------|
| **G-001** | Token method unstable | Phase 0.1: battery 2× | `results-run{1,2}.json` | variance ≤±2% on `session_net_accepted` | Fix harness | Stop — fix ruler first |
| **G-002** | Manual baseline M wrong | Phase 0.2: 6-row spot-check | `docs/research/A-002-manual-spotcheck.md` | 6/6 match | Adjust `lib/manual.js` | Redefine M in spec |
| **G-003** | Equivalence judge untrusted | Phase 0.4: **20** stratified human samples | `docs/research/A-004-equiv-audit.md` | FP+FN ≤10% | Tune judge | Replace judge |
| **G-004** | No v8 baseline yet | **At 8.0 tag:** first green battery → pin `results-v8-8.0-baseline.json` | JSON + SHA | committed | — | — |
| **G-004b** | 7.x bench (informational) | Keep `E:\project\sf-bench\RESULTS.md` as **7.21.1 context only** — do not gate v8 on beating it | appendix | — | — | — |
| **G-005** | No gate automation | Phase 0.6: **`compare-results.js`** | script + CI job | all H* computable | — | — |
| **G-006** | H4 conflated all-36 vs accepted | **DONE** — RESULTS §8.7 + compare-results columns | RESULTS.md | — | — | — |
| **G-027** | Schema ÷50 may lie vs Cursor | Phase 0.10: host measurement **or** conservative mode | `docs/research/A-006-host-schema.md` | documented divisor | Controller uses `max(amortized, full/ session_calls)` | Battery uses full schema per call in “worst case” mode |

### 3.2 L0 surface & schema

| ID | Gap | Closure action | Artifact | Pass | Pivot | Kill |
|----|-----|----------------|----------|------|-------|------|
| **G-019** | compact-3 vs meta-tool unknown | Phase 0.8: A/B on 36 rows | `results-l0-ab.json` | pick winner on **session_net_accepted** + equiv | Use winner | If tie → compact-3 (simpler) |
| **G-005b** | H1 unmeasured | Phase 0.7: stub `list_tools` both surfaces | `docs/research/A-005-schema-bytes.json` | ≤5000 B | Slim JSON Schema (**A-025**) | Merge edit into `symforge` |
| **G-025** | `symforge_edit` schema bloat | Phase 0.7b: measure edit DTO | bytes in A-005 artifact | edit ≤1500 B | `intent=edit` on `symforge` | Resource-first edits |
| **G-017** | 32-tool default | Phase 1: `SYMFORGE_SURFACE=compact` default | code + battery | H1 pass | — | — |

### 3.3 Routing, controller, bypass

| ID | Gap | Closure action | Artifact | Pass | Pivot | Kill |
|----|-----|----------------|----------|------|-------|------|
| **G-012** | Bypass not in battery | Extend sf-bench: **two-hop** mode for bypass rows | `lib/bypass-hop.js` + spec §5.4 | completion proxy passes | H3 = serve-only small rows | — |
| **G-008** | Trajectory ≠ quality | Golden file **`expected_equiv`**, **`expected_decision`** | `routes.golden.jsonl` | A-028 schema | — | — |
| **G-002b** | Golden file missing | Phase 0.5: seed 36 rows | `sf-bench/routes.golden.jsonl` | 36 lines, validated JSONL | — | — |
| **G-009** | Multi-step unproven | Phase 2 spike before full executor | battery A/B T1/T4 | equiv↑ tokens≤ | single-hop + better payload | — |
| **G-011** | Predictor uncalibrated | Phase 3: EMA loop | 3-run trend | error↓ | widen safety margin | disable degrade |
| **G-013** | cache_hit unproven | Phase 2: duplicate-fetch path tests | golden rows + unit path | tokens↓ equiv= | disable cache | — |

### 3.4 Reference quality (H6 — 8.1 program)

| ID | Gap | Closure action | Artifact | Pass | Pivot | Kill |
|----|-----|----------------|----------|------|-------|------|
| **G-029** | T2 0/4 equiv | **Program T2** (§6.1) | per-repo spike docs | ≥2/4 tokio+django **or** policy P-T2 | bypass-only T2 in corpus | shrink T2 corpus with doc |
| **G-030** | T3 0/8 equiv | **Program T3** (§6.2) | outline payload fix + tests | ≥4/8 equiv | degrade+outline policy | exclude T3 small (M=0) from H6 denom |
| **G-031** | Full file 0/4 | **Policy P-FF** (§6.3) | golden `expected_decision=bypass` | always bypass | drop from H6 eligible set | — |
| **G-023** | Bypass vs H6 tension | **A-023** locked in compare-results | code | BYPASS excluded from H6 | — | — |

### 3.5 Deploy & transport (8.1)

| ID | Gap | Closure action | Artifact | Pass | Pivot | Kill |
|----|-----|----------------|----------|------|-------|------|
| **G-020** | No Streamable HTTP | Phase 4.2: rmcp feature + `/mcp` | `Cargo.toml` + `serve.rs` | A-020 battery parity | stdio-only 8.1 **cancelled** | — |
| **G-021** | sidecar/local sprawl | Phase 4.3: merge routes | single axum router | A-021 no regression | keep sidecar loopback only | — |
| **G-022** | HTTP proxy hop | In-process dispatch in server | benchmark p99 | A-022 | keep proxy if multi-process required | — |
| **G-030b** | init templates missing | Phase 4.4: `init --url` | JSON for Cursor + Claude Code | manual smoke | — | — |
| **G-032** | Governor not universal | Shared `ToolExecutor` used by stdio, daemon proxy, sidecar, HTTP | design + code | same write-gate all paths | loopback-only sidecar interim | — |
| **G-033** | Sidecar HTTP unauthenticated | Loopback bind default; non-loopback requires Bearer or disabled | code + test | no open bind without auth | retire standalone sidecar in 8.1 | block 0.0.0.0 in prod |
| **G-034** | No transport-agnostic runtime | `ServerRuntime` owns index + STEL + governor + auth; transports thin | `src/server/` or refactor plan | single tool dispatch table | keep daemon string-map with tests | — |
| **G-035** | Structured BYPASS missing | Machine-readable bypass body + `do_not_retry_symforge_same_target` | `stel-schema.md` + harness | two-hop A-012 passes | prose-only bypass | — |
| **G-036** | init 32-tool allowlist undermines compact | Version-aware init: compact hosts get 3 tools only | `init.rs` + docs | post-8.0 smoke | — | — |

### 3.6 Documentation & product copy

| ID | Gap | Closure action | Artifact |
|----|-----|----------------|----------|
| **G-README** | “70–95%” claims | Phase 3.6: README aligned to battery | README.md |
| **G-030c** | Phase doc drift | A-030 on every phase edit | README crosswalk |
| **G-POLY** | PolyForm NC | ideation Q6 note + README license | not blocking 8.0 |

### 3.7 Admin UI & operator surface (8.1)

| ID | Gap | Closure action | Artifact | Pass | Pivot | Kill |
|----|-----|----------------|----------|------|-------|------|
| **G-037** | No operator web UI | Phase 4.7: admin SPA + `/api/v1/*` on `symforge serve` | [`v8-admin-ui.md`](v8-admin-ui.md) | O1,O4 pass | — | **8.1 blocked** |
| **G-038** | No STEL ledger SQLite | Phase 3 L4: `stel_ledger_events` migration | schema + store | dashboard + H4 query | export JSON only | **8.1 blocked** |
| **G-039** | No product API-key store | Hashed keys in server DB; rotate via admin | `server.db` + admin API | O3,O7 pass | — | **8.1 blocked** |
| **G-040** | No first-run / post-update onboarding | CLI URL banner + browser open + wizard | onboarding in server DB | O2,O3 pass | — | **8.1 blocked** |
| **G-041** | No harness scan + config apply | `HarnessRegistry`; scan/apply API + CLI `--scan` | `src/harness/` | O5–O8 pass | — | **8.1 blocked** |
| **G-042** | No ops telemetry in admin UI | System resources + symforge/harness PIDs | `/api/v1/system` | O4 pass | — | **8.1 blocked** |

Detail: [`v8-admin-ui.md`](v8-admin-ui.md) — **O1–O8 required for 8.1.0 tag**. Depends on **G-020**, **G-034**, **G-033**.

### 3.8 AAP embed integration (Agent Army Professionals)

| ID | Gap | Closure action | Artifact | Pass | Pivot | Kill |
|----|-----|----------------|----------|------|-------|------|
| **G-043** | Embed contract not release-gated | E1/E2: embed tests + AAP sibling build in CI/docs | `src/embed.rs` contract + CHANGELOG | green on tag | manual AAP gate doc | **8.0 blocked for embed consumers** |
| **G-044** | No AAP operator convenience | Admin AAP panel + harness presets (E6–E9) | [`v8-aap-integration.md`](v8-aap-integration.md) | AAP panel smoke | generic MCP scan only | **8.1 convenience** |
| **G-045** | STEL leaks into embed build | `server` feature audit; embed CI without axum/rmcp | CI job | embed build clean | split crate | **8.0 blocked** |

AAP repo: `E:\project\Agent_Army_Professionals` · Primary integration: **`aap-code-intel`** + `symforge` **`embed`** feature (not MCP config).

---

## 4. Decision trees (when spikes fail)

### 4.1 A-019 meta-tool vs compact-3

```text
Run battery ×3 surfaces (full 32, compact-3, meta-1/2)
  → winner = max session_net_accepted s.t. equiv_count ≥ baseline
  → if meta wins: Phase 1 ships meta (update stel-schema L0)
  → if compact wins: ship compact-3
  → if all lose vs north star gates: STOP — fix L3 payloads before L0
```

### 4.2 A-029 T2 spike

```text
Spike: sidecar-parity refs (markdown + benches + imports)
  PASS: ≥2/4 equiv on tokio+django T2
  PIVOT: register policy P-T2 — T2 tasks are bypass-only (grep path in envelope)
         → remove T2 from H6 eligible denominator (4 rows)
  KILL: cannot achieve PIVOT without breaking north star → expand corpus research
```

### 4.3 A-005 H1 budget

```text
Measure list_tools bytes
  PASS: ≤5000
  PIVOT-A: slim symforge_edit → merge intent=edit
  PIVOT-B: drop symforge_edit from list; edits via symforge only
  KILL: cannot fit edit + read + status → 2-tool surface + resources (document in A-005)
```

### 4.4 A-020 transport

```text
Implement Streamable HTTP; run full battery
  PASS: S,M,equiv within ±1% of stdio
  PIVOT: ship 8.1 stdio-only + documented SSH tunnel (ideation deploy § fallback)
  KILL: rmcp blocker → fork minimal HTTP JSON-RPC shim (spike before Phase 4 start)
```

### 4.5 H6 at 50% unreachable after T2/T3 program

```text
After §6 complete:
  PASS: ≥18/36 eligible equiv
  PIVOT: H6 → 35% (13/36) for 8.1.0 + 8.2 roadmap for 50% (document, do not silently lower)
  KILL: do not tag 8.1 — extend program with dated milestone
```

---

## 5. Harness specifications (implement in Phase 0)

### 5.1 `compare-results.js`

**Input:** `baseline.json`, `candidate.json` (sf-bench output schema)

**Must emit per gate:**

| Gate | Computation |
|------|-------------|
| H1 | `schemaBytes` from candidate setup |
| H2 | replay `routes.golden.jsonl` → pass rate |
| H3 | rows matching `*_small` AND `decision=serve` → count sGteM (must be 0) |
| H4 | `session_net_accepted` ≥ 0 |
| H5 | per task: `mcpCalls` ≤ 1 where golden `chain=single` |
| H6 | `equiv / eligible` ≥ 0.50; BYPASS rows excluded |
| H7 | \|net_accepted_run1 − run2\| / run1 ≤ 0.02 |
| H8 | per-language accepted serve net vs baseline |

**Row classification (required fields in results.json per task):**

```json
{
  "equivalence": "EQUIVALENT|SYMFORGE-LESS|SYMFORGE-MORE|BYPASS",
  "acceptedServe": true,
  "sGteM": false,
  "decision": "serve|bypass|degrade|cache_hit",
  "mcpCalls": 1,
  "eligibleH6": true
}
```

Exit code: 0 iff all gates for target release pass.

**Preflight mode (`--preflight`):** Before `results-v8-8.0-baseline.json` exists, §12A only requires that the script **runs and computes** gate columns on shakedown JSON. Accept either: (a) `--baseline` and `--candidate` pointing at the same shakedown file, or (b) a synthetic minimal fixture checked into `sf-bench/fixtures/`. Full baseline-vs-candidate regression is required for **8.0 tag**, not for unlocking `src/stel/`.

### 5.2 `routes.golden.jsonl` (one line per sf-bench task)

**Canonical path:** `sf-bench/routes.golden.jsonl` (repo sibling or submodule). §12A accepts a copy under `symforge/docs/fixtures/` only if kept in sync via CI check.

```json
{
  "id": "tokio/t2_find_references",
  "query": "...",
  "must_call": ["find_references"],
  "must_not_call": [],
  "expected_decision": "serve|bypass",
  "expected_equiv": true,
  "chain": "single|multi",
  "eligible_h6": true,
  "notes": "T2 reference trace"
}
```

Phase 0.5: generate 36 rows from existing battery task defs; human review `expected_*` for 10 rows minimum.

### 5.3 Schema token accounting

| Mode | S tokens include |
|------|------------------|
| **Battery default** | payload + ceil(schemaBytes/4)/50 |
| **Battery worst-case** | payload + ceil(schemaBytes/4) per call (flags `--schema-per-call`) |
| **Controller production** | `max(amortized, per_call)` until A-006 validated |

Both modes reported in RESULTS.md after Phase 0.10.

### 5.4 Bypass two-hop harness

For rows where SymForge returns `decision=bypass`:

1. Parse bypass hint (path + line range).
2. Simulate host `Read` on range → tokens `R`.
3. Re-run equivalence on combined intent (or mark `BYPASS_COMPLETE` if manual says grep-only).
4. **H3 serve-only** path: bypass rows skip sGteM check; **economics** track `S_bypass` ≪ M.

---

## 6. Reference quality program (8.1 — planned in full)

### 6.1 Program T2 (find references)

**Root cause hypothesis:** index refs miss markdown, benches, cross-file text matches (sf-bench SYMFORGE-LESS lists).

| Step | Work | Done when |
|------|------|-----------|
| T2.1 | Audit tokio T2 missing sites vs `find_references` + sidecar | gap taxonomy doc |
| T2.2 | Implement missing source classes (markdown paths, bench imports) | tokio T2 equiv |
| T2.3 | Repeat django T2 | django T2 equiv |
| T2.4 | Battery T2 all repos | ≥2/4 min (**A-029**) or **P-T2** registered |

**P-T2 (pivot):** T2 tasks become mandatory **bypass** with envelope `grep -r …` + line window; `eligible_h6=false` in golden file.

### 6.2 Program T3 (outline)

**Root cause:** outline responses omit load-bearing symbols (0/8 equiv).

| Step | Work | Done when |
|------|------|-----------|
| T3.1 | Fix outline formatter / section selection | fmt T3 large equiv (worst row: S=3718 M=540) |
| T3.2 | Small T3 where M=0 → **mandatory bypass** (not serve) | 0 sGteM on T3 small |
| T3.3 | Battery T3 | ≥4/8 equiv or bypass policy for T3 small |

### 6.3 Policy P-FF (full file review)

Tasks designed to tie/lose: **always bypass** with “use Read whole file” — `expected_decision=bypass`, `eligible_h6=false` (4 rows). Document in golden file; removes false H6 pressure.

### 6.4 H6 eligible set

```text
eligible_h6 = 36 − BYPASS-policy rows (P-FF: 4, optional P-T2: 4, optional T3-small: 4)
Target: equiv / eligible ≥ 50%
Report both raw 36 and eligible counts in RESULTS.md
```

---

## 7. Implementation phases (complete checklist)

### Phase 0 — Pre-implementation (§12 must be green)

| Step | Deliverable | Assumption |
|------|-------------|------------|
| 0.1 | Battery 2× | A-001 |
| 0.2 | Manual spot-check | A-002 |
| 0.3 | Harness shakedown on v8 branch binary | A-003 |
| 0.4 | Equiv audit n=20 | A-004 |
| 0.5 | `routes.golden.jsonl` | A-028 |
| 0.6 | `compare-results.js` | G-005 |
| 0.7 | Schema stubs + bytes | A-005, A-025 |
| 0.8 | L0 A/B battery | A-019 |
| 0.9 | Schema amortization study | A-006, A-027 |
| 0.10 | Bypass two-hop in harness | A-012 |
| 0.11 | Document P-FF + eligible H6 rules | G-031 |
| 0.12 | rmcp compile spike doc | A-031 |

### Phase 1 — L0 + H1

All of S2–S4 in [`stel-schema.md`](stel-schema.md). Exit: **H1 PASS**, 5 golden rows replay.

### Phase 2 — L1 + L2

S5–S6. T2 spike start (§6.1). Exit: **H3, H4, H5** on compact surface.

### Phase 3 — L4 + 8.0.0

S7. Exit: **H1–H5, H7**; README honest copy.

### Phase 4 — Quality + 8.1.0

§6 complete + §3.5 deploy. Exit: **H6, H8**, `symforge serve`, init templates, A-020..A-022.

---

## 8. rmcp / `symforge serve` (specified before Phase 4)

**Cargo.toml additions (spike in Phase 0.13 doc only — implement Phase 4):**

```toml
rmcp = { version = "1.1.0", features = [
  "transport-io",
  "transport-streamable-http-server",
  "server",
], optional = true }
```

**CLI:**

```bash
symforge serve --listen 127.0.0.1:8787 --api-key sf_… [--tls-cert …]
```

**Routes:** `POST /mcp` (Streamable HTTP), `GET /health` (no secret), existing daemon project/session APIs internal or merged.

**Phase 0.12 deliverable:** `docs/research/A-031-rmcp-spike.md` — compile proof + hello InitializeRequest (no full STEL). *(A-020 is stdio-vs-HTTP battery parity at 8.1.)*

---

## 9. Assumption dependency DAG (summary)

```text
Phase 0: A-001..004 → A-019 → A-005,A-025 → A-006,A-027 → A-012
Phase 3 exit: A-024 pin results-v8-8.0-baseline.json at tag
Phase 1: (all above VALIDATED)
Phase 2: A-008..014, A-029 spike
Phase 3: A-015, A-016
Phase 4: A-020..022, A-023, Programs T2/T3, P-FF
```

No phase starts if any **blocking** assumption for that phase is OPEN.

---

## 10. Risk register (accepted only with artifact)

| Risk | Mitigation | Accept only if |
|------|------------|----------------|
| H6 50% too hard | §6 program + P-T2/P-FF pivots | Pivot documented before 8.1 tag |
| Cursor schema not amortized | worst-case battery mode + controller max() | A-006 OPEN with conservative path |
| rmcp immature | Phase 0.13 spike | spike PASS before Phase 4 code |
| Equivalence judge wrong | 20-sample audit | A-004 VALIDATED |
| Branch baseline drift | pin JSON + SHA in CI | every battery compares to pin |

---

## 11. What we explicitly do NOT do before 8.1

- OAuth / SSO / multi-tenant
- Semantic/vector tier (see `semantic-tier-roadmap.md` — post-8.1)
- Remote indexing without server-host repo
- Raise H1 threshold
- Tag 8.0 with `symforge serve` marketing
- Start `src/stel/` with OPEN Phase 0 assumptions

---

## 12. Pre-flight checklists (binary — see split below)

**Hard rule:** `src/stel/` starts only when **§12A** is 100% `[x]`. **§12B** blocks Phase 4 / 8.1 only — not Phase 1.

**Progress (2026-06-13):** A-019 **VALIDATED** (compact-3). B-A019 **closed**. **NO-GO** — independent sign-off not obtained; review **ready to solicit** (B-SIGNOFF latent).

### §12A — Before first `src/stel/` commit (Phase 1 pre-flight)

**Measurement**

- [x] A-001 VALIDATED (2× battery) — [`docs/research/A-001-measurement-repeatability.md`](research/A-001-measurement-repeatability.md)
- [x] A-002 VALIDATED (manual spot-check) — [`docs/research/A-002-manual-spotcheck.md`](research/A-002-manual-spotcheck.md)
- [x] A-003 VALIDATED (harness runs on v8 branch binary) — MCP shakedown [`A-003-mcp-shakedown.jsonl`](research/A-003-mcp-shakedown.jsonl); battery rows OPEN
- [x] A-004 VALIDATED (equiv audit) — [`docs/research/A-004-equiv-audit.md`](research/A-004-equiv-audit.md)
- [x] `compare-results.js` runs on harness shakedown JSON (**`--preflight` mode**) — in-repo [`G-005-inrepo-preflight.json`](research/G-005-inrepo-preflight.json) (H1/H7 diagnostic)
- [x] `routes.golden.jsonl` 36 rows + schema validated — [`docs/fixtures/routes.golden.jsonl`](fixtures/routes.golden.jsonl) + [`A-028-golden-routes.md`](research/A-028-golden-routes.md)
- [ ] RESULTS.md §8.7 + compare-results columns live *(v8 runs only)*
- [x] **No requirement** to beat or pin `results-7.21.1-baseline.json` — [`docs/research/phase0-12a-scope-boundary.md`](research/phase0-12a-scope-boundary.md)

**Surface choice**

- [x] A-005 VALIDATED (H1 feasible) — compact probe 891 B [`A-005-schema-bytes-summary.md`](research/A-005-schema-bytes-summary.md)
- [x] A-025 VALIDATED (edit budget or pivot documented) — unit test PASS [`surface_probe.rs`](../src/protocol/surface_probe.rs)
- [x] A-019 L0 surface locked — compact-3 wins L0 A/B [`A-019-l0-surface-choice.md`](research/A-019-l0-surface-choice.md) + [`A-019-l0-ab-results.json`](research/A-019-l0-ab-results.json)
- [x] A-006/A-027 documented (amortization policy) — [`docs/research/A-006-host-schema.md`](research/A-006-host-schema.md)

**Bypass harness (serve economics trust)**

- [x] A-012 two-hop harness spec implemented **or** H3 scoped to serve-only in compare-results until implemented — [`docs/research/A-012-bypass-policy.md`](research/A-012-bypass-policy.md) (serve-only interim)

**Process**

- [x] P-FF + eligible H6 rules **documented** in golden-file README (implementation of 4 bypass rows may wait for §12B) — [`docs/research/A-012-bypass-policy.md`](research/A-012-bypass-policy.md)
- [x] Phase crosswalk reviewed (A-030) — [`docs/research/A-030-phase-crosswalk.md`](research/A-030-phase-crosswalk.md)
- [x] Decision log updated in ideation.md — 2026-06-13 Phase 0 §12A entry
- [ ] No OPEN assumption blocks Phase 1 per §9 — A-019 closed; **B-SIGNOFF** remains — see [`phase0-12a-review-signoff.md`](research/phase0-12a-review-signoff.md)

**Phase 0 blockers (2026-06-13):** **B-SIGNOFF** (latent — independent review ready to solicit). B-A019 **closed**. B-SFBENCH **closed**. RESULTS.md §8.7 **deferred**.

**Only when every §12A box is checked → first commit in `src/stel/`.**

### §12B — Before Phase 4 / 8.1.0 (quality + deploy pre-flight)

**Not required before `src/stel/`.**

- [ ] P-FF policy enforced in `routes.golden.jsonl` (4 rows) + battery
- [ ] H6 eligible set validated in compare-results output
- [ ] A-023 reflected in compare-results (BYPASS excluded from H6 denominator)
- [ ] A-031 rmcp compile spike doc (`docs/research/A-031-rmcp-spike.md`)
- [ ] A-020..A-022 validated (stdio vs HTTP battery parity + deploy acceptance)
- [ ] **O1–O8** operator acceptance ([`v8-admin-ui.md`](v8-admin-ui.md)) — admin, onboarding, harness hub on 2 hosts

---

## 13. Success criteria (final)

| Milestone | Objective proof |
|-----------|-----------------|
| **8.0.0** | `compare-results.js candidate baseline` → H1–H5,H7 PASS; footer = `session_net_accepted` |
| **8.1.0** | H6,H8 PASS; `symforge serve` + paste JSON on 2 hosts; battery stdio vs HTTP ±1%; **O1–O8 PASS** (admin, onboarding, harness scan/apply) |

---

*Last updated: 2026-06-12 — amend when any gap closes (link artifact in research log).*
