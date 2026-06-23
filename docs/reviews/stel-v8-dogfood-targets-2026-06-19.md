# STEL v8 Dogfood — Multi-Target Behavioral QA (2026-06-19)

**Tester role:** Skeptical QA, black-box only (no SymForge source reads).  
**Tool surface:** Compact STEL (`symforge`, `status`, `symforge_edit`) v8.4.0.  
**Method:** Live MCP I/O captured; ground truth from `git` + `rg` on disk.

---

## One-line verdict

**Conditionally trustworthy** for code read/trace/edit when the SymForge process cwd matches the target repo; **not trustworthy** if the Cursor MCP daemon is bound to a stale worktree — queries then answer the wrong project with high confidence. Token economics are labeled heuristics but use fixed per-route buckets; reference tracing misses common usage sites.

---

## Setup

### Candidates surveyed (local disk, no clones)

| Repo | Lang | Files (approx) | LOC ballpark | Git |
|------|------|----------------|--------------|-----|
| `E:\project\rtk` | Rust | 287 | ~46k `.rs` | clean (+ untracked `.symforge/` after tests) |
| `E:\project\justice-compass-app` | TS/JS | 124 | small SPA | clean |
| `E:\project\headroom` | Rust + Python | 1616 | large monorepo | clean |

**Selected:** all three for index truth; **deep matrix on `rtk`** (user moved workspace here; Rust CLI, medium size, edit-safe).

**Excluded:** `symforge` itself (proxy/index quirk per charter).

### How SymForge was invoked

| Channel | Workspace binding | Notes |
|---------|-------------------|-------|
| **Cursor `user-symforge` MCP** | Stale: `project_root=//?/C:/Users/rakovnik/.cursor/worktrees/Agent_Army_Professionals/4fn3` | `status` reported 1439 files / 37242 symbols while Cursor workspace was `rtk` (287 files). `path:` param did not retarget. |
| **Fresh stdio** (`symforge.exe` with **shell cwd** = target repo) | Correct per repo | Used for all scored behavioral tests. |

> **Operational finding:** Dogfooding requires either MCP restart on workspace switch or stdio invocation from the target repo cwd. Cursor MCP alone gave **wrong-repo answers** after workspace move.

---

## Per-hypothesis verdicts

### H-status — index reports empty / not-ready after successful queries

**REFUTED** (when cwd matches target) · **CONFIRMED** (Cursor MCP stale workspace)

**Evidence — rtk, fresh stdio (`cwd=E:\project\rtk`), before queries:**

```
index_ready: true
index_files: 214
project: rtk
index_symbols: 4752
```

Ground truth: 287 non-ignored files (`Get-ChildItem` excl. `.git/target/node_modules`). Gap (~25%) explained by admission/gitignore tiers — **not zero**.

**After 12+ successful `symforge` calls, `status` again:**

```
index_ready: true
index_files: 214
session_tokens: 0
```

Index counts **stable and non-zero**; does not flip to `index_files: 0` / `index_ready: false`.

**Evidence — Cursor MCP while workspace = rtk:**

```
index_ready: true
index_files: 1439
index_symbols: 37242
project: project
```

`symforge://repo/health` resource:

```
project_root=//?/C:/Users/rakovnik/.cursor/worktrees/Agent_Army_Professionals/4fn3
Files: 1439 indexed
```

Orient query returned `crates/aap-agents/...` paths — **wrong repo**, yet `index_ready: true`.

**Cross-target index sanity (stdio, first `status`):**

| Target | `index_files` | Manual file count | `index_ready` |
|--------|---------------|-------------------|---------------|
| rtk | 214 | 287 | true |
| justice-compass-app | 118 | 124 | true |
| headroom | 2636 | 1616 | true |

Headroom over-count vs naive walk likely includes admitted paths under `target`/build artifacts filtered differently — still non-zero and ready.

---

### H-economics — fixed constants vs real measurements

**CONFIRMED** (bucketed heuristics) · **PARTIALLY CONFIRMED** (session accounting gaps)

**Five rtk calls — envelope constants:**

| Query | `output_tokens` | `predicted` | `predicted_net` | `error%` | `schema` / `invoke` |
|-------|-----------------|-------------|-----------------|----------|---------------------|
| find: main CLI entry | 1471 | ~800 | 675 | 83.9% | 45 / 80 |
| find: MinimalFilter | 1636 | ~800 | 675 | 104.5% | 45 / 80 |
| orient: output compression | 667 | ~400 | 275 | 66.8% | 45 / 80 |
| read: src/main.rs outline | 934 | ~400 | 275 | 133.5% | 45 / 80 |
| read: config.rs outline | 656 | ~400 | 275 | 64.0% | 45 / 80 |

- `predicted_net` is **only 275 or 675** (route-family bucket), not derived from response size.
- `predicted` is **~400 or ~800** per family — unchanged across 656–1636 actual tokens.
- `schema_tokens: 45` and `invoke_tokens: 80` **identical on every call**.
- Per-call `session_tokens_served` stayed **0** on all stdio invocations despite prior calls in same pipe.

**Cursor MCP persistent session `status` (after 9 calls):**

```
session_tokens: 16471
predicted_response_tokens: 4800
actual_response_tokens: 7538
predicted_net_total: 937
```

Calibration block tracks aggregates but `predicted_*` does not track `actual_response_tokens` closely (7538 actual vs 4800 predicted).

**"Saved vs manual"** strings always `est. 275 fewer` or `est. 675 fewer` — same buckets as `predicted_net`, not recomputed from served bytes.

---

### H-routing — NL misroute / no multi-step decomposition

**CONFIRMED** (multi-step) · **PARTIALLY CONFIRMED** (symbol read phrasing)

**Multi-step query:**

```
query: "find Config struct then show who uses it"
plan: find → search_files → search_text (inferred)
```

Step 2 terms: `["find","Config","struct","then","show","who","uses"]` — **no second hop** to `find_references` / read. "then" treated as search token.

**Symbol read misparsed:**

```
query: "show symbol body"  symbol=MinimalFilter  path=src/core/filter.rs
→ get_symbol invocation: {"name":"show"}
→ File not found: .
```

```
query: "function body"  symbol=run_err
→ get_symbol invocation: {"name":"function"}
```

**Find by symbol name** routes to `search_files+search_text`, not `search_symbols`:

```
query: "search_symbols name=MinimalFilter"
→ search_files: No indexed source files matching ...
→ search_text terms: ["search_symbols","name=MinimalFilter"]
```

Did eventually list `struct MinimalFilter` in `src/core/filter.rs` via text search — **partial**, not direct symbol lookup.

**Orient vs find:** `"how does output compression work"` → `explore` (git diff symbols) — **wrong domain** for RTK filter compression (ground truth: `src/core/filter.rs`, `MinimalFilter`).

---

### H-edit — `if_match` not enforced on write

**REFUTED**

| Step | Query / args | Observed |
|------|--------------|----------|
| Preview | `symforge_edit` `limits` replace, no `apply` | `[DRY RUN] Would replace ...` — **git clean** |
| Apply | `apply=true`, new body with `// dogfood-test-marker` | `replaced fn limits (94 → 122 bytes)` — correct diff |
| Stale `if_match` | `if_match` = pre-edit body after file mutated | `if_match does not match current symbol body` **isError:true** — **no clobber** |

Restored with `git checkout -- src/core/config.rs`; final `git status` clean for tracked files.

---

## Test matrix — `rtk` (primary)

### A. Index truth

See H-status. **Pass** on correct cwd.

### B. Find / orient (5 queries)

| # | Query | Route | Grade | Ground truth |
|---|-------|-------|-------|--------------|
| 1 | main CLI entry point | search_files+search_text | **partial** | `src/main.rs:1206 fn main` in results but buried in 200 text hits |
| 2 | MinimalFilter struct | search_files+search_text | **correct** | `src/core/filter.rs:156` in top symbols |
| 3 | cargo test filter command | search_files+search_text | **partial** | `cargo_cmd.rs`, `runner.rs` present; noisy docs |
| 4 | how does output compression work | explore | **wrong** | Top hit `run_diff` in git.rs, not filter pipeline |
| 5 | repo overview what is RTK | search_files+search_text | **partial** | README/FEATURES in text hits; no cohesive overview |

### C. Read (3 symbols + 1 outline)

| Request | Result | Grade |
|---------|--------|-------|
| outline `src/main.rs` | 58-symbol outline, large-file truncation noted | **correct** |
| symbol `MinimalFilter` + path | misrouted to `get_symbol(name="show")` | **wrong** |
| outline `src/core/config.rs` | full 29-symbol outline + consumers | **correct** |
| symbol `run_err` | misrouted to `get_symbol(name="function")` | **wrong** |

Truncation honesty on `main.rs`: **good** — explicit `Large file summary: outline+imports only`.

### D. Trace (recall vs `rg`)

| Symbol | SymForge refs | Manual `rg` call sites | Recall |
|--------|---------------|------------------------|--------|
| `run_err` | 1 (`src/main.rs:1453`) | 1 | **100%** |
| `MinimalFilter` | 2 (defn lines 156, 163 only) | 5 usage sites (248, 251, 318, 429, 477) | **~29%** (missed value usages) |
| `scrub_sensitive_env_vars` | 6 (all in binlog.rs) | 6 (4 calls + 2 tests) | **100%** |

### E. Economics

See H-economics table.

### F. Edit safety

See H-edit. Preview, apply, stale guard all behaved.

### G. Failure modes

| Case | Query | Expected | Observed |
|------|-------|----------|----------|
| Missing symbol | `read` `TotallyFakeSymbol` `src/main.rs` | loud not-found | **wrong** — served full `src/main.rs` outline (ignored fake symbol) |
| Tier-2 file | `read` `Cargo.lock` | honest non-indexed | **correct** — `Not indexed: Cargo.lock is Tier 2 (metadata only)` |
| Outside repo | `read` `../symforge/Cargo.toml` | reject | **correct** — `decision: reject` (path outside repo) |
| Large file | `read` `src/main.rs` full content | truncate or cap | **correct** — outline only, ~21007 tokens vs whole-file noted |
| Meta index | `index_folder reset...` | index op | **partial** — routed to `context_inventory` fallback |

---

## Cross-language notes

| Lang | Target | Index | Find/trace quality |
|------|--------|-------|-------------------|
| Rust | rtk | Good (214/287) | Trace strong for fns, weak for struct **value** usage |
| TS | justice-compass | Good (118/124) | `trace App` → `find_references` **reject** (ambiguous/common name) |
| Rust+Py | headroom | Loaded (2636 files) | Index-only this run; no deep matrix |

No evidence Rust indexes worse than TS; **reference kind** (fn call vs type value) matters more than language.

---

## What worked well

- **Compact facade routing** to legacy tools is real — envelopes show actual `search_text`, `get_file_context`, `find_references`, `replace_symbol_body` invocations.
- **File outlines** (`get_file_context`) accurate and fast on rtk; large-file truncation honestly disclosed.
- **Function call tracing** (`run_err`) precise with file:line and enclosing symbol.
- **Edit preview** leaves disk untouched; **apply** produces correct structural diff; **impact dependents** listed.
- **`if_match` optimistic concurrency** refuses stale writes.
- **Tier-2 / out-of-repo** failures explicit, not hallucinated content.
- **Per-repo project name** in `status` when cwd correct (`project: rtk`).

---

## Findings table

| Sev | Surface | Query | Expected | Observed | Verdict |
|-----|---------|-------|----------|----------|---------|
| **critical** | Cursor MCP | any query after workspace → rtk | Index/query rtk | Still indexed AAP worktree (1439 files); `src/main.rs` not found | **wrong repo** |
| **high** | `symforge` read | `symbol=MinimalFilter` + NL body query | Symbol source | `get_symbol(name="show")` not found | **misroute** |
| **high** | `symforge` trace | `who calls MinimalFilter` | All usages | Only 2 defn refs; missed 5 value sites | **low recall** |
| **high** | `symforge` find | `how does output compression work` | Filter/compress code | `explore` → git diff helpers | **wrong** |
| **med** | envelope | varied output sizes | Scaled `predicted` | Fixed ~400/~800 buckets | **heuristic** |
| **med** | `status` / envelope | after N calls (stdio) | `session_tokens` > 0 | Per-call `session_tokens_served: 0`; status `session_tokens: 0` | **accounting gap** |
| **med** | `symforge` find | `... then show who uses it` | 2-step plan | Single text search with "then" as term | **no decompose** |
| **med** | `symforge` read | missing symbol | Error | Returned parent file outline | **silent ignore** |
| **low** | `status` | file count vs disk | Rough match | 214 vs 287 (admission) | **acceptable** |
| **low** | `symforge_edit` | stale `if_match` | Refuse | Refused correctly | **pass** |
| **low** | `symforge_edit` | preview | No write | git clean | **pass** |

---

## What we could not test / UNVERIFIED

- **Full matrix on justice-compass-app and headroom** — index + spot checks only (time/cwd harness).
- **Facade economics on wrong-repo Cursor MCP** — numbers emitted but semantically meaningless.
- **`multi_step_planner`** — listed under `deferred` in every `status` output.
- **Binary file read** — not attempted (no large binary in rtk root).
- **Hang/crash** — none observed in ~40 tool calls.

---

## Reproduction notes

```powershell
# Correct-repo invocation pattern used for scored tests:
cd E:\project\rtk
$sf = "$env:USERPROFILE\.npm-global\node_modules\symforge-windows-x64\bin\symforge.exe"
# JSON-RPC initialize + tools/call piped to $sf
```

Cursor MCP config (`~/.cursor/mcp.json`) launches `symforge.exe` **without `cwd`** — relies on daemon session binding; **rebind on workspace switch not verified**.

---

*Dogfood session: 2026-06-19. SymForge 8.4.0 compact surface. Primary target: `rtk`.*
