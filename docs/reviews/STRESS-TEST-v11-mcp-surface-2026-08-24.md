# SymForge v11.0.5 MCP surface stress test

**For:** an implementing LLM developer agent
**From:** live MCP dogfood on 2026-08-24
**Product under test:** SymForge 11.0.5, full 39-tool surface, daemon session (`session-5`), Windows
**Goal of this document:** fix the defects and token leaks below. Do not rewrite the architecture. Do not expand scope past the ranked work.

This report is self-contained. Reproduce against a daemon with at least two open projects (`index_folder(add:true)`). Do not treat health “token savings” as a session-total percentage.

---

## 0. Instructions for the implementing agent

1. Read this whole file before editing.
2. Honor the reporting invariant in `CLAUDE.md`: a tool may not report success or a project’s facts for an observation it did not make. Silent wrong-project answers are defects, not UX nits.
3. Prefer the smallest diff that closes a ticket. One ticket per commit unless two are the same root cause.
4. Every P0/P1 ticket needs a regression test that would have failed on 2026-08-24.
5. Do not “fix” advertised tool counts, compact-surface filtering, or `repair_index` retirement.
6. Do not edit `.github/workflows/ci.yml` or `release.yml` without updating `WORKFLOW_FINGERPRINTS` in `tests/preventive_runtime_dark_v11.rs`.
7. Run the embed-build gate before claiming unused-item safety:
   `cargo test --no-default-features --features embed --lib -- --test-threads=1`
8. Heavy `cargo test` on this Windows host: cap `-j 8`. Do not run multi-ten-minute suites through a 600s bash timeout.

---

## 1. Executive verdict

SymForge is a working Rust-native code-intelligence MCP. Symbol search, outlines, explore, and dry-run structural edits are correct and fast on Rust, Python, TypeScript, Go, and Java. C++ headers are partial. Multi-project `index_folder(add:true)` correctly keeps the home project active.

The product headline “saves developer tokens while coding” is **true against naive whole-file reads**, **modest against a competent `rg` + windowed-read agent**, and **often inverted on documentation** because `search_text` cannot see markdown/RST while `search_knowledge` payloads are provenance-heavy.

Do not quote 90% as a session number. The only paired end-to-end session measurement on record is **24.8%** (Codex/AAP, 2026-07-13, SymForge 8.14.1), and those agents still used native reads.

Highest-ROI work is not a new tool. It is:

1. Docs on the search path agents already call (`search_text`).
2. `project=` on every remaining tool (and handlers that actually honor it).
3. Compact knowledge discovery payloads.
4. Parser holes that force extra disambiguation turns.

---

## 2. Environment and method

| Item | Value |
|---|---|
| Version | 11.0.5 (`health` / `health_compact`) |
| Runtime | `daemon_reused_session`, sidecar alive |
| Date | 2026-08-24 |
| Home project | `E:/project/symforge` — 1,088 files, 34,833 symbols, load 1,944ms |
| Foreign corpus | shallow clones at `E:/tmp/symforge-stress/{flask,cobra,zod,gson,fmt}` |
| Open method | `index_folder(path, add:true)` for each foreign root |
| Mutations | dry_run / `apply=false` only |
| Coverage | 32 of 39 advertised tools fired; four parallel dogfood agents plus the parent session |

### Indexed working set (end of session)

| Project name | Language | Files | Symbols | Snapshot |
|---|---|---:|---:|---|
| symforge (home, active) | Rust | 1,088 | 34,833 | present |
| flask | Python | 226 | 2,296 | absent |
| zod | TypeScript | 606 | 11,934 | absent |
| gson | Java | 311 | 5,856 | absent |
| fmt | C++ | 144 | 6,390 | absent |
| cobra | Go | 64 | 1,152 | absent |

Confirm inventory with `status(detail="projects")`.

### What was not measured

- HTTP transport (stdio/daemon MCP only)
- Applied (non-dry-run) edits on foreign repos
- Compact-3 surface (`SYMFORGE_SURFACE=compact`)
- Paid paired Claude/Codex session rerun
- Perl / C# / Ruby as primary dogfood languages (home repo has fixtures only)

---

## 3. Token economics (do not mix these numbers)

Three baselines were used. Mixing them is how the 90% claim gets oversold.

### 3.1 Per-call measurements (this session)

| Call | Naive whole-file (tokens) | Competent ~50-line window (tokens) | SymForge served (tokens) | Win vs naive | Win vs competent |
|---|---:|---:|---:|---|---|
| `get_symbol` `LiveIndex` in `src/live_index/store.rs` | 92,489 | ~400 | 905 | ~99% | **loss or wash** |
| `get_file_context` outline `src/flask/app.py` | ~16,000 | ~1,400 | ~529 | ~97% | win |
| `get_file_context` outline `include/fmt/base.h` | ~41,315 | ~400 | ~175 | ~99% | win |
| `get_file_context` outline `Gson.java` | ~14,515 | ~400 | ~261 | ~98% | win |
| `search_knowledge` Flask “application factory” (6 hits) | ~2,500 (rg+file) | ~500 (`rg -C3`) | ~3,000 | **loss** | **loss** |

`estimate=true` on `get_symbol(LiveIndex)` reported: symbol body ~905, raw file ~92,489.

Early-session `health` billed one `search_symbols` at 17.4× vs competent-manual (125 served, 2,055 saved). That is the honest **named-symbol lookup** win.

### 3.2 Session counters (this session, not a %)

| Counter | Value |
|---|---|
| MCP tool calls recorded | 143 across 32 tools |
| Health “tokens saved” | ~49,916 across 11 hook fires |
| Hook workflows routed | 6/44 (14%) at first health snapshot |
| Fail-open (no sidecar) | 38 of 44 at first snapshot |
| Context inventory | ~6,904–7,767 tokens tracked in session cache |

Health savings only count **routed** work. Cursor `Read`/`Grep` that never hit the sidecar are invisible. 14% hook adoption means the health counter is not the user’s bill.

### 3.3 Paired session evidence (historical, still the best session number)

From `research/token-cost/end-to-end-feature-benchmark-2026-07-13.md`:

| Trial | Native total | SymForge total | Saved | % |
|---|---:|---:|---:|---:|
| 1 | 2,085,089 | 1,330,244 | 754,845 | 36.2% |
| 2 | 3,092,305 | 2,564,159 | 528,146 | 17.1% |
| Combined | 5,177,394 | 3,894,403 | 1,282,991 | **24.8%** |

Causal limitation in that writeup: both SymForge-enabled agents still emitted native command/file events. Do not claim the full 24.8% is “because of symbol-aware retrieval.”

### 3.4 What is actually actionable

**Use the MCP when the agent already knows a name.** `search_symbols` → `get_symbol` / `get_file_context(sections=["outline"])` / `edit_within_symbol` beats opening the file.

**Do not treat it as a docs grep.** Until markdown/RST is on `search_text` (or knowledge is compact and ranked), a competent agent with ripgrep wins mixed code+docs tasks.

---

## 4. Per-tool scorecard

Legend: **OK** correct on the home project and on a foreign `project=` selector when the schema has one. **HOME-ONLY** schema or handler ignores foreign projects. **DOCS-BLIND** misses markdown/RST. **VERBOSE** correct but token-negative vs `rg`.

| Tool | Result | Notes |
|---|---|---|
| `health` / `health_compact` | OK | Honest quarantine, knowledge degraded flags, project list on compact |
| `status(detail="projects")` | OK | Six open projects, home=active after `add:true` |
| `index_folder(add:true)` | OK | Foreign roots added; home stayed active. Fresh clones reported `checkpoint=degraded` |
| `get_repo_map` | OK | Truncates ~1000 tokens; doctrine footer is correct |
| `search_symbols` | OK | Flask/Command/ZodType/Gson/vformat resolved. Comma-lists fail as a literal. `terms` alone rejected |
| `search_text` | DOCS-BLIND | Excellent on source + enclosing symbol. Zero hits on `.md`/`.rst` |
| `search_files(resolve=true)` | OK | Flask `app.py` → 3 candidates, honest ambiguity |
| `get_symbol` | OK | Default ~1000-token cap cuts large class/struct bodies mid-way |
| `get_file_context` | OK / fragile | Outline is the highest-ROI call. Default/cache on Zod omitted outline until `sections=["outline"]` + `force_refresh` |
| `get_file_content` | OK | `estimate=true` and `around_symbol` worked |
| `get_symbol_context` | OK | `verbosity="signature"` useful. Callers sometimes heuristic vs `find_references` |
| `find_references` | leaky | Results were in-project but banner said “across projects”; `path=` + `project=` rejected |
| `find_dependents` | OK | `format.rs` → 23 dependents; compact form good |
| `inspect_match` | HOME-ONLY | **No `project` in schema.** `path=src/flask/app.py` → `File not found` |
| `explore` | OK | Concept clusters were the right symbols |
| `ask` | router-only | Routes to `search_symbols`/`explore`/`find_references`. Does not answer “how does X work” |
| `conventions` | HOME-ONLY | **Schema `{}`.** Always returned this repo’s Rust stats while dogfooding Flask/Zod/fmt |
| `edit_plan` | OK | `project=` honored; 809 Cobra `Command` refs counted |
| `context_inventory` | OK | Tracked bodies vs search-only entries |
| `investigation_suggest` | OK | Suggested loading `LiveIndex` impls not yet fetched |
| `what_changed` | OK | Honored `project=`. Default `code_only=true` hid `.gitignore` on fresh clones |
| `diff_symbols` | leaky | Schema has `project`. Cobra agent observed home-repo diffs / empty when history existed |
| `detect_impact` | HOME-ONLY | **No `project` in schema.** Cobra `since=HEAD~1` returned `Cargo.toml` / `src/cli/...` |
| `analyze_file_impact` | noisy | Reindexed `format.rs`; default payload truncated at ~150 tokens with 764 more lines |
| `validate_file_syntax` | OK | Malformed JSON fixture and Flask `pyproject.toml` both honest |
| `checkpoint_now` | HOME-ONLY | **No `project` in schema.** Cobra call wrote ~29MB into home `.symforge/index.bin`; cobra stayed `snapshot=absent` |
| `search_knowledge` | VERBOSE / degraded | Recall on docs is real. Bridges mostly `broken_anchor`/`missing`. CHANGELOG outranked current docs |
| `review_knowledge` | noisy | `mode=summary` → `shown_dossiers=0`. `mode=remediation` dumped action hashes (CCR) |
| `curate_knowledge` | blocked | Honest guard errors, then `capability=unavailable reason=atomic_durability_unavailable` |
| `replace_symbol_body` dry_run | OK | Flask `Flask` 63347→22 bytes preview; no write |
| `edit_within_symbol` dry_run | OK | Flask `wsgi_app` 1 replacement |
| `insert_symbol` dry_run | OK | |
| `delete_symbol` dry_run | OK | Missing-symbol path honest |
| `batch_edit` / `batch_insert` dry_run | OK | |
| `batch_rename` dry_run | false positives | Cobra `Command`→`Cmd`: 809 sites including `exec.Command` |
| `symforge_edit` preview | OK | Needs flat `symbol`+`op`+`path`, not nested intent map |
| `symforge_retrieve` | not re-hit | CCR hashes were emitted (`ade80137d7a2` on remediation) |

---

## 5. Language parser notes

| Language | Quality | Keep | Fix |
|---|---|---|---|
| Rust | Excellent | struct/impl/fn, reverse index | — |
| Python | Strong | `class Flask` + nested methods, docstrings | — |
| TypeScript | Strong with holes | v3 `class ZodType` vs v4 interface+factory | Barrel/`export { z }` files report 0 symbols. `z.object` does not resolve to `fn object` |
| Go | Good | `struct Command`, methods as `fn` | Type aliases (`type CompletionFunc = func(...)`) not indexed. YAML keys indexed as symbols |
| Java | Good | classes/interfaces | Constructors classified as `fn` with the class name |
| C++ | Partial | Free functions in headers found | `include/fmt/base.h` parse status partial; template syntax error near `` `T` ``; whole `basic_string_view` dumped as one `fn` |
| Markdown/RST | Knowledge-only | Knowledge search finds them | `search_text` returns zero |

---

## 6. Knowledge feature (Feature 020) — current truth

Observed on home project via `review_knowledge(mode="summary")` and `health`:

| Metric | Home (symforge) | Flask clone |
|---|---|---|
| Dossiers | 6,624 | 124 |
| lifecycle=unknown | 6,494 (98%) | 124 (100%) |
| broken_anchor | 2,521 | 63 |
| review_due | 4,103 | 61 |
| shown_dossiers in summary | 0 | 0 |
| policy_digest | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` (SHA-256 of empty) | same empty |
| curation | `atomic_durability_unavailable` | n/a |
| freshness | degraded, `last_valid_content_generation: 2`, `ObservationFailed`, `ReconciliationPending` | current on the clone |
| bridge | truncated (tens of thousands omitted / 18k ambiguous samples) | truncated (320 ambiguous) |

`search_knowledge` on this repo for `"token savings"` ranked `CHANGELOG.md` v0.4.0 / v0.14.0 / v0.15.0 above `docs/ideation.md` (“Token savings as sole product headline”).

`search_knowledge` does not accept `path` (only `path_prefix`). Passing `path` deserializes as unknown field.

Doc→code bridges on Flask `docs/quickstart.rst`: 21/22 missing or ambiguous; 1 exact (`jsonify`).

**Do not paper over this with a greener banner.** The degraded flags are honest. The work is ranking, compact payloads, search_text coverage, and durability so curation can exist.

---

## 7. Ranked work

Each ticket is written so a developer agent can implement and test it without this chat.

Priority:

- **P0** — silent wrong answer or false-zero that burns retries
- **P1** — correctness in a common path, or token-negative by default
- **P2** — quality / parser / ergonomics

---

### P0-1. `search_text` is blind to markdown and RST

**Symptom.** Agents following AGENTS.md (“prefer `search_text` over grep”) conclude docs do not exist.

**Repro (home project, 512 markdown files indexed):**

```
search_text(query="token savings", glob="**/*.md", limit=5)
```

Actual: no matches, with a suggestion to try `search_symbols`.

```
search_knowledge(query="token savings", limit=5)
```

Actual: hits in `CHANGELOG.md` and `docs/ideation.md`.

**Repro (flask, `project="flask"`):**

```
search_text(query="application factory", glob="docs/**", project="flask")
```

Actual: no matches.

```
search_knowledge(query="application factory", project="flask", limit=6)
```

Actual: `docs/api.rst:184`, `docs/extensiondev.rst:72`, `docs/patterns/celery.rst:98`, tutorial RST files.

**Expected.** Either:

- A: `search_text` searches the same admitted knowledge/text bytes as knowledge search for `.md`, `.mdx`, `.rst`, `.txt`; or
- B: zero-hit on a docs glob/path auto-falls back to knowledge with an explicit banner: `search_text does not index this language; routed to search_knowledge`.

False zero with no fallback is the bug. Do not leave AGENTS.md recommending a tool that cannot see 512 files the index already holds.

**Suggested implementation.** A is better (one tool, one mental model). If the text index currently skips non-code by design, that skip is now a product defect. At minimum emit a structured “index disposition: knowledge-only” footer naming `search_knowledge` and the hit count, instead of “No matches.”

**Acceptance.**

1. Home: `search_text(query="token savings", glob="**/*.md")` returns at least one `docs/` or `CHANGELOG.md` or `AGENTS.md` hit, **or** a fallback banner plus knowledge hits in the same response.
2. Flask: `search_text(query="application factory", project="flask")` is not a silent zero.
3. Existing code-search tests for `src/` still exclude tests by default.

**Likely files.** `src/live_index/search.rs`, `src/protocol/search_tools.rs`, `src/protocol/tools.rs`, knowledge bridge admission, AGENTS.md only if the contract changes.

---

### P0-2. Thread `project=` through tools that still hit the home index

Silent wrong-project answers violate the reporting invariant.

#### P0-2a. `conventions` — schema is `{}`

**Repro.** Daemon with flask open and home=symforge:

```
conventions()
```

There is no `project` parameter to pass.

Actual: `Language: Rust`, `anyhow` / `thiserror`, `1087–1088 files`, imports `crate`/`symforge`.

**Expected.** `conventions(project="flask")` reports Python (or an explicit refusal: “conventions is home-project-only”). Silent Rust stats during a Flask session is a wrong answer.

**Acceptance.** Schema includes `project` (same selector contract as `search_symbols`). Test: open flask additively, call `conventions(project="flask")`, assert language is Python and file count is ~226 not ~1088. Call without `project` still targets the active project (today’s behavior, documented).

#### P0-2b. `inspect_match` — no `project`

**Repro.**

```
inspect_match(path="src/flask/app.py", line=110)
```

Actual: `File not found: src/flask/app.py`

**Expected.** `inspect_match(..., project="flask")` returns enclosing `class Flask`. Without `project`, active-project miss is fine **if** the error names the active project and suggests `project=`.

**Acceptance.** Schema + handler honor `project`. Test as above. Error on miss includes active `project_name`.

#### P0-2c. `checkpoint_now` — no `project`; writes the home snapshot

**Repro.** Cobra in the working set, `snapshot=absent`. Call `checkpoint_now(verify_after_write=true)` intending cobra.

Actual (2026-08-24): first attempt quarantined a version-mismatch; second wrote ~29MB to `E:/project/symforge/.symforge/index.bin`. Cobra stayed `snapshot=absent`.

**Expected.** Either require `project=` and write that project’s `.symforge/index.bin`, or refuse when the session has multiple open projects and no selector. Never write project A’s snapshot while the caller is working in project B.

**Acceptance.** Multi-project session: `checkpoint_now(project="cobra", verify_after_write=true)` creates `E:/tmp/symforge-stress/cobra/.symforge/index.bin` (or the cobra root’s `.symforge/`) and does not change home snapshot identity. Omitting `project` with multiple opens: explicit error naming the selector.

#### P0-2d. `detect_impact` — no `project`

**Repro.** `detect_impact(since="HEAD~1", project="cobra")` is not even valid — `project` is not in the schema. Cobra agent called it anyway / without a selector and received `Cargo.toml`, `src/cli/harness.rs` (home repo).

**Expected.** Same selector contract as `what_changed`. Git is run in the selected project’s root.

**Acceptance.** Open cobra+symforge. `detect_impact(since="HEAD~1", project="cobra", scope="files")` returns zero `src/` Rust paths. If cobra is a depth-1 clone and `HEAD~1` is invalid, the error is an invalid git ref for **cobra**, not a home-repo diff.

#### P0-2e. `diff_symbols` — schema has `project`; handler leak suspected

**Repro.** `diff_symbols(compact=true, project="cobra")` and `diff_symbols` against `HEAD~3` on cobra reported empty or home-shaped results.

**Expected.** Git diff is computed in the selected root. Empty result is only valid when that repo’s refs really have no symbol delta.

**Acceptance.** Same multi-project test as detect_impact. Prefer a fixture with two commits, not a `--depth 1` clone.

**Likely files for P0-2.** `src/protocol/tools.rs` input structs, `src/cli/init.rs` if schemas are generated from rust, each handler, `tests/` multi-project daemon tests (there are existing Feature 012 tests — extend them). Mirror the `project` field docs from `SearchSymbolsInput`.

---

### P0-3. `find_references` claims cross-project and refuses path scope when `project=` is set

**Repro.** `find_references(name="toJson", compact=true, project="gson")` reported 761 refs “across projects” (results were gson tests). Adding `path=` was rejected: path scoping not supported with cross-project targeting.

**Expected.** Single `project=` is single-project. Path scoping works. Banner must not say “across projects”. Default-excluding tests (or documenting that refs include tests, unlike `search_text`) should be consistent with other discovery tools.

**Acceptance.** `project="gson"` + `path="gson/src/main/java/com/google/gson/Gson.java"` returns in-file / in-project refs and does not error. Envelope `Scope:` line names gson only.

---

### P1-1. Compact `search_knowledge` (and stop ranking changelog as current)

Discovery hits currently include `content_hash`, `publication`, `voice`, `bridge_previews`, `provenance_ids`, `finding_ids` per row. That is why knowledge loses to `rg -C3` on token cost.

**Expected default (or `compact=true`, default true for agents):**

```
path:line  heading
one-line snippet
```

Keep full provenance on `review_knowledge` and on `compact=false`.

**Ranking.** `authority_scope=default` should prefer current docs / AGENTS / specs over `CHANGELOG.md` historical_record for queries that are not explicitly historical.

**Acceptance.**

1. `search_knowledge(query="token savings", compact=true, limit=5)` payload is smaller than today’s six-hit dump by a large factor (assert char/token cap).
2. Top hit for `"token savings"` on this repo is not a 0.4.0 changelog line when `docs/ideation.md` and `research/token-cost/` exist.
3. Full provenance still available via `review_knowledge(mode="document", path=...)`.

**Likely files.** `src/protocol/knowledge_search.rs`, `src/protocol/knowledge_model.rs`, ranking/authority.

---

### P1-2. `review_knowledge(mode="summary")` shows zero dossiers

**Actual.** `total_dossiers=6624 shown_dossiers=0 overflow=0` — counts only, no samples.

**Expected.** Summary includes a bounded sample (e.g. top 10 by review_due / broken_anchor) **or** the tool description says summary is counts-only and names `mode=remediation` / `document`. Today the description implies dossiers are returned (`limit` = “Maximum number of complete dossiers”).

**Acceptance.** Either `shown_dossiers>0` with `limit=10`, or schema description + a one-line “counts only; use mode=document|remediation” so agents do not retry.

`mode=remediation` on `path_prefix="docs/"` produced a CCR-compressed hash dump (`ade80137d7a2`). Add `max_tokens` respect and a compact index (path + action_id only) before the hash list.

---

### P1-3. `curate_knowledge` blocked: `atomic_durability_unavailable`

Not a banner bug. Curation cannot apply. `health` reports the reason honestly.

**For this cycle:** document in the tool description that apply is unavailable until atomic durability is wired; preview/guards may still run. Do not pretend apply works.

**Follow-up (only if already in-flight in Feature 020):** wire atomic durability so `.symforge-knowledge.toml` writes are possible. Out of scope unless a current spec slice already owns it. Do not invent a durability design in this ticket.

---

### P1-4. `batch_rename` false positives (Go `exec.Command`)

**Repro.** `batch_rename(path="command.go", name="Command", new_name="Cmd", dry_run=true, project="cobra")`

Actual: 809 sites / 35 files including `exec.Command("shellcheck", ...)` in tests.

**Expected.** Type-aware / qualified-ident matching. `exec.Command` is package `os/exec`, not `cobra.Command`. Uncertain matches already have a “manual review” path — `exec.Command` must be uncertain or excluded, not confident.

**Acceptance.** Dry-run on cobra `Command`→`Cmd` does not list `exec.Command` under confident matches. Paginate dry-run output (default cap + “N more sites”).

---

### P1-5. `get_file_context` default/cache can omit the outline

**Repro (zod).** First `get_file_context(path=packages/zod/src/v3/types.ts, project="zod")` and a cache hit returned knowledge trust header only (~111 tokens), no outline. `sections=["outline"]` + `force_refresh=true` returned 706 symbols (623 omitted by budget).

**Expected.** Default / empty `sections` always includes `outline`. Cache hits must return the same sections as a miss, not a metadata stub.

**Acceptance.** Test: `get_file_context(path=..., project=...)` without `sections` contains a symbol outline. Second identical call (session cache) still contains the outline unless `force_refresh` is the only difference.

This is the highest-ROI tool when it works. A metadata-only default is a token and correctness defect.

---

### P1-6. `analyze_file_impact` default truncation hides the answer

**Repro.** `analyze_file_impact(path="src/protocol/format.rs")` truncated at ~150 tokens, “764 additional output line(s) not shown.”

**Expected.** Lead with counts (added/removed/changed) and a bounded list, then CCR the rest. Do not spend the budget on a wall of `[Changed] fn default` lines before the summary.

**Acceptance.** First 2k chars of the response contain added/removed/changed **counts**. Full list retrievable via higher `max_tokens` or CCR hash.

---

### P1-7. `ask` is a router, not an answerer

**Repro.** `ask(query="how does Flask routing work", project="flask")` routed to `explore("flask routing")` and dumped symbols (`raise_routing_exception`, `dispatch_request`) with no narrative.

**Expected for “how” questions:** 5–15 line answer citing 2–5 symbols/paths, then the routed tool dump. For “where is X defined” the current router is fine.

If synthesis is out of scope for a non-LLM server, change the tool description: “Router only; does not generate explanations.” Agents currently call it as an oracle.

**Acceptance.** Either a short synthesized paragraph in the response, or the description and “Route confidence” header say router-only so agents skip it for “how” questions.

---

### P1-8. Cross-project result caps can drop a whole project

**Repro.** `search_symbols(query="format", projects=["gson","fmt"], limit=20)` returned fmt-only hits in the capped window. Gson has `format` symbols (`ISO8601Utils`, etc.) that did not appear.

**Expected.** Round-robin or per-project quotas so `projects=[A,B]` cannot be 100% A. Footer should say `fmt: 20, gson: 0 of N` if gson was omitted.

**Acceptance.** Test with two fixtures each containing a unique symbol name plus a shared name; capped results include at least one hit from each project or an explicit `omitted_projects` list.

---

### P2-1. TypeScript barrels and namespace APIs

- `packages/zod/src/v4/classic/index.ts` / `external.ts`: 0 symbols (re-export only).
- `ask("where is z.object defined")` / `search_symbols("z.object")`: miss. Actual factory: `fn object` in `packages/zod/src/v4/classic/schemas.ts`.

**Expected.** Index `export { x }` / `import * as z` chains so `z.object` resolves, or teach `ask`/`search_symbols` to split dotted names (`z.object` → `object` kind=fn).

**Acceptance.** `search_symbols(query="z.object", project="zod")` or `get_symbol(name="object", path=".../schemas.ts")` is reachable from the dotted query in one call.

---

### P2-2. Go type aliases not indexed

`type CompletionFunc = func(...)` in cobra `completions.go` is not a symbol. `kind=type` + `CompletionFunc` → no matches.

**Acceptance.** `search_symbols(query="CompletionFunc", project="cobra")` returns a type-alias (or `type`) symbol at the alias line.

---

### P2-3. Java constructors classified as `fn`

`Gson` constructors at `Gson.java` L243/L247 appear as `fn Gson`. Methods also `fn`. Outlines cannot distinguish constructor vs method.

**Expected.** Kind `constructor` (or `fn` with a constructor tag in outline). Do not break existing `kind=fn` filters without a compat note.

---

### P2-4. C++ header partial parse / span corruption

`get_file_context` on `include/fmt/base.h`: parse status partial, syntax error near `` `T` ``, 395 symbols, 312 omitted by budget, `basic_string_view` body as one `fn`. Nested `fn other` wrapping `class basic_memory_buffer` in `format.h`.

**Acceptance.** At least: partial-parse files do not emit a multi-hundred-line span as a single `fn`. Prefer dropping the bad span (quarantine) over serving a blob. Template/`T` recovery is bonus.

---

### P2-5. YAML keys as symbols

Cobra `.github/labeler.yml` keys appear in `search_symbols(query="Command")`.

**Expected.** Config keys either not in symbol search by default, or behind `include_generated` / a config language filter. Code-symbol queries should not be dominated by CI YAML.

---

### P2-6. `search_symbols` comma lists and `terms`

`query="Command, Execute, AddCommand"` → no matches (literal). `terms=[...]` without `query`/`kind`/`path_prefix` rejected.

**Expected.** Document that query is a single substring, **or** split on commas. If `terms` is in the schema, it must be sufficient to search.

---

### P2-7. `get_symbol` default 1000-token cap

Flask `class Flask` (~15.8k) and Cobra `struct Command` (~2.1k) truncated mid-body. Footer exists. Fine if `max_tokens` is documented in the tool description as default ~1000. Today agents do not pass it and think they have the whole class.

**Expected.** Description states the default cap. For `class`/`struct` larger than cap, lead with signature + method outline, not a mid-field cut of the first N tokens.

---

### P2-8. `what_changed` default hides non-code

Fresh clones: `code_only=true` (default) → “No uncommitted changes” + “1 path filtered” (`.gitignore` for `/.symforge/`). Easy to misread as clean.

**Expected.** Keep the default, but the first line should be `0 code paths; 1 non-code filtered` without requiring a second call. Already almost there — make the filtered count impossible to miss.

---

### P2-9. Dry-run increments worktree-misuse

After flask/cobra dry_runs without `working_directory`, `health_compact` showed `Worktree misuse/hour: 5`. Dry-run is not a write.

**Expected.** Dry-run does not increment worktree-misuse. Misuse is for committed writes to the indexed copy when a worktree was in play.

---

### P2-10. Watcher reconcile repairs during multi-project add

End of session: `Watcher: events 714, overflows 0, reconcile repairs 2839`. Investigate whether `index_folder(add:true)` storms the home watcher. If expected, do not count foreign-project publication as home reconcile repairs. If not, it is a perf/correctness smell (index generation ran to 577 in one session).

Not a user-facing tool bug. Add a metric test or a log line that names **which root** was repaired.

---

## 8. Suggested implementation order

Do these in order. Later items depend on selector honesty.

| Step | Tickets | Why first |
|---|---|---|
| 1 | P0-2a, P0-2b, P0-2c, P0-2d, P0-2e | Stop lying about which project you observed |
| 2 | P0-3 | Same class of bug (`project=` not actually single-project) |
| 3 | P0-1 | Stops false-zero retries; biggest token leak for agents |
| 4 | P1-5, P1-6 | Outline and impact are the high-ROI read tools |
| 5 | P1-1, P1-2 | Knowledge usable without a token tax |
| 6 | P1-4, P2-1, P2-2, P2-3, P2-4 | Parser / rename quality |
| 7 | P1-7, P1-8, P2-* remainder | Ergonomics |

Skip P1-3 durability design unless Feature 020 already has a slice owner.

---

## 9. Tests to add (minimum)

Prefer daemon multi-project tests over stdio-only. Pattern: open home fixture + foreign fixture with `add:true`, then call the tool with `project=<foreign>`.

| Test name (suggested) | Asserts |
|---|---|
| `conventions_honors_project_selector` | Flask → Python, not Rust |
| `inspect_match_honors_project_selector` | `src/flask/app.py:110` → `class Flask` |
| `checkpoint_now_writes_selected_project_snapshot` | cobra `.symforge/index.bin` exists; home snapshot identity unchanged |
| `checkpoint_now_refuses_ambiguous_multi_project` | no `project` + 2 opens → error |
| `detect_impact_git_root_is_selected_project` | no `Cargo.toml` when project=cobra |
| `diff_symbols_git_root_is_selected_project` | same |
| `find_references_single_project_allows_path_scope` | gson + path does not error; banner not “across projects” |
| `search_text_markdown_or_honest_fallback` | home `**/*.md` “token savings” not silent zero |
| `search_text_rst_or_honest_fallback` | flask “application factory” not silent zero |
| `get_file_context_default_includes_outline` | no `sections` still has outline; cache hit too |
| `batch_rename_does_not_confident_match_exec_Command` | cobra dry_run |
| `search_knowledge_compact_omits_hash_wall` | compact payload size bound |
| `cross_project_search_quotas_both_projects` | shared name, both projects represented or omitted list |

Run `cargo test --lib --bins --tests -- --test-threads=1` for the new tests, not `--all-targets`. Then embed-build as in §0.

---

## 10. What not to change

- Advertised 39-tool full surface vs 40 registered (`symforge` facade filtered from full profile).
- Compact-3 opt-in via `SYMFORGE_SURFACE=compact`.
- `repair_index` / `get_index_run` / `cancel_index_run` retirement phrases in AGENTS.md + README.md (pinned by `tests/conformance.rs`).
- Frecency: discovery tools must not bump.
- Byte-exact persistence / no newline translation.
- Dry-run remaining dry-run. Do not apply stress-test edits to `E:/tmp/symforge-stress` or this repo as part of the fix.
- Do not lower knowledge `coverage=degraded` flags to look greener.

---

## 11. Reproduction corpus

Foreign clones used on 2026-08-24 (shallow `--depth 1`):

| Dir | Upstream |
|---|---|
| `E:/tmp/symforge-stress/flask` | https://github.com/pallets/flask.git |
| `E:/tmp/symforge-stress/cobra` | https://github.com/spf13/cobra.git |
| `E:/tmp/symforge-stress/zod` | https://github.com/colinhacks/zod.git |
| `E:/tmp/symforge-stress/gson` | https://github.com/google/gson.git |
| `E:/tmp/symforge-stress/fmt` | https://github.com/fmtlib/fmt.git |

Re-clone if missing. Official SFBENCH pins (deeper history) live in `research/full-surface-benchmark/corpus.lock.json` if a two-commit git fixture is needed for P0-2d/e.

Open them with:

```
index_folder(path="<abs>/flask", add=true)
index_folder(path="<abs>/cobra", add=true)
# ...
```

Confirm with `status(detail="projects")` that `symforge` remains `home=yes active=yes`.

---

## 12. Appendix — first-party schema gaps (verified 2026-08-24)

These input schemas have **no** `project` / `projects` field:

| Tool | Schema properties |
|---|---|
| `conventions` | `{}` |
| `inspect_match` | path, line, context, sibling_limit, max_tokens, estimate |
| `checkpoint_now` | export_artifact, verify_after_write |
| `detect_impact` | base_branch, since, depth, scope, include_untracked, include_data |

Tools that **have** `project` and were still leaky in dogfood (handler bugs, not schema): `find_references`, `diff_symbols` (suspected), possibly `search_text` banner “across projects” even with `project=flask`.

`search_knowledge` accepts `path_prefix` not `path`. `review_knowledge` accepts both (`path` required for `mode=document`).

---

## 13. Appendix — health excerpt (end of session)

```
Status: Ready | Files: 1088 indexed (1082 parsed, 4 partial, 2 failed) | Symbols: 34833 | Loaded: 1944ms
Watcher: active (events: 714, overflows: 0, repairs: 2839)
Token savings: ~49916 tokens saved across 11 hook fires
Tool calls: 143 recorded across 32 tools
Knowledge curation: capability=unavailable reason=atomic_durability_unavailable
Knowledge: manifest=degraded ... bridge=truncated(31980 omitted)
```

Parse quarantine (home): 2 failed fixtures (`malformed.json`, `unclosed_step.yml`) + 4 expected vendor partials under `vendor/tree-sitter-scss/`. Not product bugs.

---

End of report. Implement P0 before anything else. A green suite that still returns Rust conventions for Flask is not done.