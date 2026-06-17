# SymForge v8 (STEL) — Skeptical Senior-Engineer Audit

- **Date:** 2026-06-17
- **Target:** SymForge `v8.0.0` (commit `3df5210`), the "STEL" rework — compact L0->L1 MCP facade (`symforge`, `status`, `symforge_edit`) plus a token-economics ledger.
- **Mandate:** Find overstated code surfaces, dishonest status, claim-vs-reality gaps, and technical fallacies that make the tool untrustworthy to the LLMs that call it or fragile for the users who run it.
- **Method:** Dogfooded the *live installed* v8.0.0 MCP binary, plus 5 parallel read-only specialist audits (economics, status/index, ledger, planner/routing, security), then independently re-verified every load-bearing claim against source.
- **Not done (honest scope):** No `cargo build`/`test`/`clippy` was run (read-only, parallel-audit constraint). Findings are from source reading + live MCP dogfooding, not from a fresh build or the full test suite. Live observations are from the deployed daemon-proxy topology; a same-process `serve`/stdio run may differ where noted.

---

## Bottom line

**SymForge's *internal* honesty is good — its *LLM-facing* honesty is not.**

The engineering bones are real: a working SQLite ledger, clean L2/L3 separation, constant-time auth, secure-default startup, genuine path-traversal guards, a candid assumptions register that marks most economics claims `OPEN`, and a `status` `deferred:` list that names unfinished work. The team is not lying to *itself*.

But the three surfaces an LLM actually reads on every call — the **trust envelope**, the **status banner**, and the **`symforge_edit` "guarded apply" contract** — each claim more than the code delivers:

1. The "token economics" that is the entire thesis of v8 is **two hardcoded constants** (`400`, `800`) dressed in measurement vocabulary. The live envelope advertised **"275 saved vs manual"** on a call the tool's own calibration then recorded as a **141-token loss**.
2. `status` reports a **fully-working index as empty and not-ready** (`index_files: 0`, `index_ready: false`) in the default deployment, because it reads the wrong process.
3. The advertised **`if_match` write-guard is never enforced at the write** — a real (if race-gated) path to silently clobbering user code.

None of these are fatal, and none require a rewrite. They require either wiring the surfaces to reality, or relabeling them honestly. Until then, an LLM that *trusts* SymForge's self-reported numbers is being misled, which is the precise opposite of what a code-intelligence tool for LLMs needs.

---

## Trust scorecard

| Surface | What it tells the LLM | What the code does | Verdict |
|---|---|---|---|
| Trust envelope `…saved vs manual` | A realized token saving | `manual` & `response` are fixed constants (`800`/`400`); realized net was **negative** live | **OVERSTATED / mislabeled** |
| Envelope `predicted · error %` | A calibration accuracy signal | Estimate-vs-estimate readout, fed into nothing | **OVERSTATED (dead readout)** |
| Envelope `session_net_vs_manual` | Net savings this session | A monotonically-rising gross token counter; no "manual" term subtracted | **DISHONEST (mislabeled field)** |
| `status` `index_ready/files/symbols` | Index health | Reads empty front-end proxy shell, not the daemon index that serves queries | **DISHONEST / BUG** |
| `status` `l1..l4: active`, `handler_*: active` | Subsystem liveness | Unconditional literal strings; never probe real wiring | **OVERSTATED** |
| `status` `l4_ledger: active` + `durable_ledger: unavailable` | Active durable ledger | In-memory ledger works; durable store only wired in `serve` mode | **OVERSTATED (self-contradicting)** |
| `symforge_edit` "guarded apply" (`if_match`) | Optimistic-concurrency safety | `if_match` checked pre-flight only; write path never re-checks it | **REAL RISK (data integrity)** |
| "L1 planner" | Intelligent planning/routing | Deterministic keyword/intent dispatch table | **OVERSTATED naming (code is fine)** |
| Multi-hop chaining ("A-009 VALIDATED") | Validated multi-step planning | Fires only on 3 hardcoded exact-match query strings | **OVERSTATED verdict** |
| Golden replay / `expected_equiv` | Routing-accuracy / equivalence measurement | Asserts tool-name shape only; `expected_equiv` is never asserted | **OVERSTATED (unmeasured)** |
| Auth / secrets / path safety | Secure | Constant-time auth, secure defaults, redacted secrets, traversal guards | **GENUINELY SOLID** |
| In-memory ledger correctness | Records decisions | Confirmed live: `ledger_events` 0 -> 1 after a serve | **HONEST / works** |

---

## Live evidence (dogfooded v8.0.0)

Three sequential calls to the installed MCP server, same session:

**Call 2 — `symforge` explore** returned accurate symbols + cross-refs from `src/stel/controller.rs`, with this envelope:
```
tokens: 816 served · 275 saved vs manual · schema 45 · invoke 80
predicted: 400 · error: 104.0%
```

**Call 3 — `status detail=full`**, issued *after* that successful explore:
```
ledger_events: 1            <- the process DID record the explore
session_tokens: 944
last_ledger_route: explore
index_ready: false          <- ...yet the index it serves from reports empty
index_files: 0
index_symbols: 0
durable_ledger: unavailable
predicted_response_tokens: 400
actual_response_tokens: 816   <- 2x the prediction
predicted_net_total: -141     <- realized a LOSS vs assumed manual baseline
```

Two contradictions, observed live, not theorized:
- The envelope said **+275 saved**; the calibration ledger said **-141 net** (a loss) for the same work.
- The same process that recorded the explore in its ledger reports its serving index as **empty / not ready**.

---

## Findings (by severity)

### CRITICAL

**C1 — The "token economics" is two hardcoded constants wearing a lab coat.**
`src/stel/planner.rs:51-53` stamps **every** plan step with `est_response_tokens: 400`, `est_manual_tokens: 800`, and `index_refs: vec![]` (the one field — `IndexRef.raw_chars`, `types.rs:152-155` — designed to ground prediction in real file size is *never populated*). `estimate_economics` (`controller.rs:333-349`) merely sums those constants, so a single-step serve **always** yields `predicted_net_vs_manual = 800 - (400+45+80) = 275`, regardless of query, file, tool, or symbol. The live "275 saved / predicted 400" are not data points; they are the only values this code can emit. A-011 ("`raw_chars` + lines predict response tokens within +/-20%") is `OPEN` in the register precisely because no predictor exists to validate. *Classification: OVERSTATED/DISHONEST.* **Fix:** populate `index_refs.raw_chars` from the live index and compute estimates from real bytes/lines, OR rename these `default_*_heuristic` and drop measurement vocabulary until A-011 validates.

**C2 — The serve/degrade/bypass gate decides on the fiction, and most branches are dead.**
`evaluate_plan_with_session` (`controller.rs:40-135`) branches on `net = predicted_net_vs_manual`: `<=0` bypass, `<=50` degrade, else serve. Because `net` is a positive constant (>=275) for every plan the real planner emits, **degrade and economics-bypass are unreachable** outside the separate P-FF policy path and cache-hit. So the headline "economics gate" is, for normal queries, a decorative computation that always returns `serve`. Worse, where the economics-bypass *could* fire (it can't, in practice), it tells the host to read lines 1-80 of a file *instead of* running the requested tool (`controller.rs:221-235`) — silent capability loss sold as a token win, gated on an unvalidated constant whose live error was 104%. *Classification: OVERSTATED/DISHONEST + latent BUG (unreachable branch).* **Fix:** gate on a validated predictor or cheap real signals (indexed file size, result count); do not bypass on synthetic economics.

**C3 — `status` reports a working index as empty because it reads the wrong process.**
In the default production topology the MCP server runs as a **daemon-proxy front-end** (`main.rs:267`, `new_daemon_proxy`). Code tools proxy to the warm daemon index (`explore` -> `self.proxy_tool_call("explore", …)`, `tools.rs:7284`; daemon serves it, `daemon.rs:2389`). But `status_stel_tool` (`tools.rs:8529-8557`) reads the front-end's **own** `self.index` directly with **no proxy**, and that index is empty by design in proxy mode (only ever filled by `ensure_local_index` on daemon failure, `protocol/mod.rs:587-643`). The daemon dispatch has no `status` arm anyway (`daemon.rs:2435` bails "unknown tool"). So `status` and `explore` read two different indices; `status`'s zeros measure the wrong process. Live-confirmed: the index stayed `0/not-ready` even after a successful, ledgered explore. The commit that claims to fix status honesty (`e494fe4`, "honest fusion-empty status P3-7") touches `planner.rs`/`tools.rs`, **not** `status.rs`/the index fields — this contradiction was never addressed. *Classification: DISHONEST/BUG.* **Fix:** proxy `status`'s index/ledger facts to the daemon (add a `status` arm to `daemon.rs::execute_tool_call` and `proxy_tool_call("status", …)` first, mirroring `explore`), or source the counts from the already-proxied `health`. Add a daemon-mode regression test asserting `status` index counts match what `explore` sees.

### HIGH

**H1 — `if_match` guarded-apply is pre-flight-only; the write never re-checks it (TOCTOU).**
`symforge_edit {apply:true, if_match:X}` validates `if_match` against the *indexed* body inside `run_pre_apply_gates` (`edit_apply.rs:73-79`) under a `read()` lock that is **released when that function returns**. The mutation then runs separately via `replace_symbol_body` (`tools.rs:8458` -> `edit_tools.rs:517`), which re-freshens from disk, re-resolves the symbol, and writes — and **never receives or re-checks `if_match`** (confirmed: the token exists only in `types.rs` + `edit_apply.rs`, nowhere in the write path). Between the two freshens, a concurrent writer (another agent, an editor, `git checkout`, a second `symforge_edit`) can change the file; the second freshen splices against the *new* bytes the caller never validated, the write lands, and the response still reports a successful guarded apply. *Classification: REAL RISK (data integrity), HIGH (race-gated, not one-shot).* **Fix:** re-verify the resolved body byte-for-byte against `if_match` immediately before `atomic_write_file`, in the same critical section as the splice — or have the STEL handler perform the splice+write itself against the bytes it validated rather than delegating to an independent re-resolve.

**H2 — `session_net_vs_manual` is a mislabeled gross counter.**
The envelope's `session_net_vs_manual` (`envelope.rs:47`) is fed `session_context.snapshot().total_tokens` (`tools.rs:8142`), which only ever *increases* (`session.rs` `total_tokens += tokens`). No "manual" term is ever subtracted. It is gross tokens served, labeled as net savings — so the number grows the more SymForge is used, and A-015 ("matches L4 ledger within +/-1%") cannot hold because the value is not a net at all. *Classification: DISHONEST (mislabeled).* **Fix:** subtract an accumulated manual baseline to produce a true net, or rename to `session_tokens_served`.

**H3 — Multi-hop "VALIDATED" is three magic strings.**
`plan_multi_hop_steps` (`planner.rs:104-161`) emits a multi-step plan only when the query is *exactly* `"search then fetch cfg_if body"`, `"outline then find connection refs"`, or `"find test.js then read it"` — with hardcoded tool args (`json!({"path":"src/lib.rs","name":"cfg_if"})`). Change one word and it falls through to single-step. The production handler genuinely executes multi-step plans end-to-end (`tools.rs:8193-8217`), so the *execution* is real — but the *planning* is a 3-entry lookup, and `docs/phase2-stel-checkpoint.md:120` marks A-009 `VALIDATED` ("multi-hop internal chain on 3 golden rows"). Replaying 3 pre-baked strings against their pre-baked plans demonstrates a hardcoded path; it does not validate a multi-hop *planner*. The honest `status` line (`multi_step_planner` in `deferred:`) tells the truth; the doc verdict does not. *Classification: OVERSTATED verdict.* **Fix:** demote A-009 to PARTIAL/DEMONSTRATED ("3 fixed fixtures replay; general decomposition deferred").

**H4 — Golden replay never grades equivalence; `expected_equiv` is dead data.**
Every row in `fixtures/routes.golden.jsonl` carries `"expected_equiv":true`, and the struct has the field (`types.rs:313`), but **no code path asserts it.** `validate_serve_replay_output` (`golden_replay.rs:244-310`) checks that the right tool *name* appears and produced a non-error body — route *shape*, not answer correctness. So the corpus claims "STEL's route is equivalent to the manual answer" on every row while nothing measures equivalence. This is exactly the gap A-008 admits ("95% trajectory metric not numerically measured"). Several "accuracy" tests also feed fixture queries back through `build_plan` and assert they match the fixtures — a change-detector/tautology, not a correctness check. *Classification: OVERSTATED (unmeasured).* **Fix:** assert `expected_equiv` against captured golden bodies, or strip the field so it stops implying a measurement that never runs. Any external "95% trajectory" claim is currently unsupported by code.

### MEDIUM

**M1 — `l1..l4: active` / `handler_*: active` are unconditional literals.** `status.rs:110-116` pushes fixed strings that never probe real wiring; `l4_ledger: active` prints even while `durable_ledger: unavailable` and `ledger_persistence` sit in `deferred:` on the same surface. *Fix:* derive labels from real state (`in-memory` vs `durable` vs `unavailable`); reserve "active" for the wired case.

**M2 — `calibration:` envelope field is hardcoded `"pending"`.** `handler.rs:44` always passes `calibration: "pending"`; it is not a live read of the (honestly observational) calibration module. *Fix:* thread the real tuning state, or rename to a static `mode:`.

**M3 — `error: %` is an estimate-vs-estimate dead readout.** `predict_error_pct` (`handler.rs:71-76`) compares the constant `400` against `body.len()/4` (itself a heuristic, not a tokenizer). The live 104% is the honest tell that the predictor is a constant; the value feeds nothing (no margin, no gate, no tuning). *Fix:* mark informational, or wire a real tokenizer + feedback.

**M4 — Ledger `actual_response_tokens` / `manual_baseline_tokens` carry estimates under "actual"/"baseline" names.** `ledger.rs:84-125`: `actual_response_tokens` is `body.len()/4`; `manual_baseline_tokens` is the constant `800`. The persisted ledger — the thing A-016 calibration would learn from — is seeded with constants it can never learn anything real from. The *shape* is right (predicted and actual stored separately); the *labels* overstate. *Fix:* rename to `estimated_*`/`assumed_*` until a real tokenizer/baseline lands.

**M5 — Durable ledger writes are fire-and-forget with two silent-swallow layers.** `protocol/mod.rs:338-341` spawns the write and drops the result; `ledger_store.rs:210-216` logs-and-continues on insert error; `open` failure degrades to `Disabled` (a no-op) with only a `warn`. An operator can't distinguish "ledger working" from "silently disabled" except via the `durable_ledger:` line. Intentional (FR-011 "never fail the request"), but a real durability gap if ever treated as an audit log. *Fix:* surface `durable_ledger: disabled (open failed)` distinct from `unavailable (not wired)`.

**M6 — Durable ledger table has no retention.** Self-documented at `ledger_store.rs:39-41` ("grows unbounded — no TTL/prune"). Honest, deferred; on a long-lived `serve` host the DB grows one row per invocation forever. *Fix (deferred per their note):* TTL/cap + operator maintenance note.

### LOW

- **L1 — In-memory `SessionLedger` panics on lock poison** (`ledger.rs:26-46` `.expect(...)`) while the durable store deliberately recovers poison — a divergence from the stated never-panic invariant. *Fix:* `unwrap_or_else(|e| e.into_inner())` for parity.
- **L2 — Tee "backup" is best-effort, not a transactional undo** (`edit.rs:161-166`, `edit_safety/tee.rs`): snapshot failure is non-fatal, files >1 MiB are skipped, snapshots prune after 200 files/7 days, restore is a textual hint. Acceptable as defense-in-depth; should not be presented as a guaranteed rollback for H1's clobber. *Fix:* fail closed when a small-file snapshot can't be written for `apply:true`; document as best-effort.
- **L3 — Keyless loopback `serve` leaves `/api/v1/keys` mint/rotate/revoke unauthenticated** (`auth.rs:150-152`). Intentional secure-default (Origin gate blocks browser CSRF), but on a shared host any local process can mint a key. *Fix:* always gate mutating key routes, or document the exposure.
- **L4 — `deferred:` list is a frozen string constant** (`status.rs:15-16`) that lists `ledger_persistence` as deferred even though the durable store ships in `serve` mode — stale literal, misreports shipped capability. *Fix:* compute the list from real feature flags.

---

## What is genuinely good (don't lose this in the noise)

- **The assumptions register (`docs/stel-assumptions.md`) is exemplary.** It marks the economics predictor (A-011), trust-envelope/calibration (A-015/A-016), and the founding premise (A-017) as `OPEN`, and refuses to "raise the threshold to make it pass." The dishonesty is in the shipped surfaces, **not** in the team's record of what's proven.
- **The ledger is real engineering**, not a Potemkin facade: SQLite with WAL + busy_timeout, idempotent migration, control-char sanitization + field caps (injection/DoS hygiene), poison-recovery on every lock, off-hot-path `spawn_blocking` writes, a bounded shutdown drain that can't hang the server, and a file-reopen test proving rows survive. In-memory recording is confirmed working live (`ledger_events` 0 -> 1).
- **Security fundamentals are solid:** constant-time bearer auth with length-fold + regression test, refuse-to-start without a key off-loopback, refuse inline `--api-key` (argv leak), fail-closed Origin gating, SHA-256 key store with one-time reveal, redacted AAP preset secret on the surfaced path, and genuine two-layer path-traversal guards (lexical reject of `..`/absolute/scheme, then canonicalized containment check).
- **Idempotency is genuinely enforcing**, not advisory: `create_dir` atomic create-once reservation, request-hash replay, hard `Conflict` on key+different-request, per-project store (no cross-session bleed), dry-runs don't reserve.
- **L2/L3 separation and find-fusion semantics show taste:** an empty UNION is correctly downgraded to `EmptyResult` rather than a lying `Found`, and a failed inner multi-hop step becomes `reject` with a failure footer, not a fake serve. The error paths most teams skip are tested.
- **The planner-is-a-dispatch-table is arguably the right call** for a token gateway — small, deterministic, auditable. The problem is only the word "planner" and the unmeasured accuracy verdicts, not the design.

---

## The deepest caveat

The entire reason-for-being of the v8 compact surface — "LLM tool-selection accuracy degrades past ~30-50 exposed tools, so replace 32 tools with 3" — is **A-017, recorded as `OPEN`, "cited, not reproduced."** Every overstated surface above sits on top of a foundational premise the project itself has not validated on its own corpus. That doesn't make the premise wrong (it's a reasonable industry prior), but it means the whole rework should be framed as a *bet under test*, not a *proven win* — which is exactly how the assumptions register frames it, and exactly how the README/envelope/status do **not**.

---

## Prioritized remediation

**Trust-critical (do before promoting v8 as "token-economical"):**
1. **C1/C2** — Stop surfacing constant-derived numbers as measured savings. Either ground the predictor in real index data or relabel everything `est.`/`assumed`/`heuristic`. Fix the `session_net_vs_manual` mislabel (H2). This is the #1 LLM-trust issue: the tool currently advertises savings it can post-hoc contradict by 416 tokens (+275 claimed vs -141 realized).
2. **C3** — Make `status` proxy to the daemon so a working index doesn't report empty; add the daemon-mode regression test.
3. **H1** — Enforce `if_match` at the write, in the same critical section as the splice. This is a code-mutation safety guarantee that is currently advertised but not kept.

**Honesty cleanup (cheap, high signal-to-noise):**
4. **M1/M2/M4/L4** — Derive `l1..l4`/`handler_*`/`calibration:`/`deferred:` from real state instead of literals; rename ledger `actual_/manual_` fields.
5. **H3/H4** — Demote A-009 to PARTIAL; either assert `expected_equiv` or remove it; purge any "95% trajectory" claim from docs/README until measured.

**Hardening (deferred-OK, but track):**
6. **M5/M6/L1/L2/L3** — distinguish `disabled` vs `unavailable` durable ledger; add ledger retention; panic-harden the in-memory ledger; document tee as best-effort; gate keyless-loopback key routes.

---

## What I could NOT verify

- **No fresh build / full test run** (read-only audit). Whether the suite is currently green is unverified; assertions were read, not executed.
- **Live topology assumption:** the `index_files: 0` + `durable_ledger: unavailable` cluster is the exact fingerprint of `new_daemon_proxy` (`main.rs:267`) and was reproduced live, but I did not instrument the binary to *prove* daemon mode vs a stdio/`serve` run; a same-process `serve` run would change the durable-ledger and possibly index observations (the C3 status/index split is structural to proxy mode regardless).
- **Symlink/junction TOCTOU on the write target** (Windows): lexical guards + canonicalize make it low-likelihood, but a post-canonicalize symlink swap before `persist` was not dynamically tested (no live exploitation).
- **`smart_query::classify_intent` internals** (the `Auto` fallback) were out of scope; likely also rule-based, unconfirmed.
- The exact wall-clock width of the H1 race window (structural defect confirmed regardless of size).

---

*Audit conducted by dogfooding the live v8.0.0 MCP server and 5 parallel specialist code audits, with every CRITICAL/HIGH claim re-verified against source by the lead. Treat specialist findings as inputs verified here, not as independent gospel.*
