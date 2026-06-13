# SymForge MCP Bug Report - Cross-Repo Agent Audit

Date: 2026-06-03

Reporter: Codex audit session

Scope:
- `C:\AI_STUFF\PROGRAMMING\Agent_Army_Professionals`
- `C:\AI_STUFF\PROGRAMMING\testpilot`

Mandate: audit-only, no product source edits.

SymForge runtime observed:
- Version: `7.18.0`
- Runtime mode: `daemon_reused_session`
- Sidecar port: `62260`
- AAP initial index: `1269` files, `34587` symbols
- AAP after re-index during concurrent work: `1331` files, `36037` symbols
- testpilot index: `252` files, `18180` symbols

## Mini-spec

objective:
Create a durable bug report for SymForge MCP issues found during a read-only audit, with enough evidence and acceptance criteria for maintainers to reproduce, fix, and regression-test the defects.

non_goals:
- Do not modify SymForge source code in this task.
- Do not modify product source code in AAP or testpilot.
- Do not clean or revert concurrent-agent changes.

allowed_files_or_area:
- `docs/audit/SYMFORGE_MCP_BUG_REPORT_2026-06-03.md`

contracts_or_interfaces:
- SymForge MCP tools should report trustworthy source authority, parse state, completeness, and write semantics.
- Tools that claim exact dependency/reference behavior should avoid false positives or clearly mark uncertainty.
- Tools exposed to agents should either work in the current runtime mode or clearly report that they are not available before use.

invariants:
- Existing dirty worktree state belongs to another agent and must not be changed.
- Findings must separate confirmed defects from limitations and enhancement requests.
- Reproduction steps must use only read-only or dry-run operations.

acceptance_criteria:
- Report includes environment, tested tools, issue index, per-issue repro, expected behavior, actual behavior, impact, and acceptance criteria.
- Report distinguishes critical correctness defects from usability improvements.
- Report captures cross-check evidence from raw shell where SymForge output was suspicious.

evidence_required:
- SymForge tool outputs from the audit.
- Shell cross-checks for suspected false positives.
- Git status evidence for concurrent worktree state.

stop_conditions:
- If creating the report would require changing source code, stop.
- If another agent changes this report while it is being written, stop and reconcile.

verification_command:
- `git diff --check -- docs/audit/SYMFORGE_MCP_BUG_REPORT_2026-06-03.md`

## Executive summary

SymForge is already valuable as a first-pass code intelligence layer. The strongest tools in this audit were:

- `health` / `health_compact`
- `index_folder`
- `get_repo_map`
- `search_files`
- `search_symbols`
- `search_text`
- `get_file_context`
- `get_file_content`
- `get_symbol`
- `inspect_match`
- `validate_file_syntax`
- dry-run edit tools
- `what_changed`
- `diff_symbols`

The strongest observed use case was compressing large files into actionable outlines. In `testpilot`, `get_file_context` reduced `backend/src/modules/testing/testing.controller.ts`, a 434-line NestJS controller, into a concise outline with imports, consumers, git churn, and co-change hints.

The highest-risk defects are correctness issues in dependency/reference analysis:

1. `find_dependents` produced confirmed false positives in AAP.
2. TypeScript method analysis conflated same-name methods across object boundaries.
3. Parse status reported valid TypeScript syntax as partial.
4. Natural-language `ask` failed a simple compound lookup that direct tools answered.

The tool is usable, but agents should not treat all SymForge dependency, caller/callee, or parse-state outputs as authoritative until the issues below are fixed.

## Test matrix

| Area | AAP result | testpilot result | Notes |
|---|---:|---:|---|
| Indexing | PASS | PASS | Both repos indexed quickly; no failed files. |
| Watcher health | PASS | PASS | Watchers active, no overflow. |
| Symbol search | PASS | PASS | Strong for exact symbol names. |
| Text search | PASS with caveats | PASS with caveats | Good headers for truncation and trust. |
| File context | PASS | PASS | Best tool family in the audit. |
| Raw content reads | PASS | PASS | Line/symbol reads worked. |
| Syntax validation | PASS with caveats | PASS with caveats | Some valid frontend syntax reported partial. |
| Reference lookup | PASS with caveats | PASS with caveats | Symbol references generally usable; OO same-name ambiguity observed. |
| File dependents | FAIL in AAP | PASS in sampled testpilot case | AAP false positives confirmed by shell. |
| Dry-run edits | PASS | PASS | No writes performed; previews were clear. |
| Batch dry-run edits | PASS | PASS | Failed closed on bad selector, succeeded on corrected selector. |
| Git change detection | PASS | PASS | `what_changed` matched shell status. |
| Symbol diff | PASS | PASS | `testpilot` code-only empty result was correct because last commit was docs/config. |
| Co-change ranking | PARTIAL | PARTIAL | Co-change data exists, but `search_files` ranking fallback looked inconsistent. |
| Natural-language ask | PARTIAL | FAIL for compound query | Direct tools were better. |
| Checkpoint | FAIL | FAIL | Unavailable in daemon-proxy mode. |
| PATH diagnosis | PASS with usability caveat | PASS with usability caveat | Correctly detected shadowing but suggested a weak fix for Windows users. |

## Issue index

| ID | Severity | Category | Title |
|---|---|---|---|
| SF-001 | Critical | Correctness | `find_dependents` reports unrelated AAP files as dependents. |
| SF-002 | High | Correctness | TypeScript caller/callee analysis conflates same-name methods across object boundaries. |
| SF-003 | High | Parser correctness | Valid TypeScript `import("rxjs").Subscription[]` type is reported as partial syntax error. |
| SF-004 | Medium | Parser coverage | Angular template control-flow syntax is reported as partial HTML parse. |
| SF-005 | Medium | Agent UX | `ask` fails compound lookup queries that direct tools can answer. |
| SF-006 | Medium | Ranking correctness | Co-change ranking falls back even when known co-change partners are in scope. |
| SF-007 | Medium | Runtime/tool contract | `checkpoint_now` is exposed but unavailable in daemon-proxy mode. |
| SF-008 | Medium | Install diagnostics | PATH shadow warning is correct but the remediation is not Windows-native. |
| SF-009 | Medium | Index hygiene | Re-index during concurrent work admitted root scratch/probe files into AAP index. |
| SF-010 | Low | Tool discoverability | Important tools are lazily exposed and easy for agents to miss. |
| SF-011 | Low | Convention analysis | `conventions` gives shallow or misleading summaries for TypeScript projects. |

## SF-001 - `find_dependents` reports unrelated AAP files as dependents

Severity: Critical

Status: confirmed defect

Tool:
- `find_dependents`
- `get_symbol_context` when it embeds file-level dependency graph output

Repo:
- `C:\AI_STUFF\PROGRAMMING\Agent_Army_Professionals`

Repro:

```text
mcp__symforge.find_dependents({
  "path": "crates/aap-db/src/stores/work_item.rs",
  "compact": true,
  "limit": 12
})
```

Observed actual:

SymForge reported:

```text
File-level dependency graph: 488 files depend on crates/aap-db/src/stores/work_item.rs
  crates/aap-agents/src/actor_runtime/actor.rs  (7 refs: call)
  crates/aap-agents/src/actor_runtime/spawn.rs  (7 refs: call)
  crates/aap-agents/src/actors/bus_actor.rs  (7 refs: call)
  crates/aap-agents/src/actors/coder_actor.rs  (138 refs: call)
  ...
```

Shell cross-check:

```powershell
rg -n "work_item|WorkItem|WorkState|claim_one|aap_db" `
  crates/aap-agents/src/actor_runtime/actor.rs `
  crates/aap-agents/src/actor_runtime/spawn.rs `
  crates/aap-agents/src/actors/coder_actor.rs
```

Result:

```text
no matches
```

Expected:

`find_dependents(path="crates/aap-db/src/stores/work_item.rs")` should list only files that import, re-export, directly reference, or otherwise depend on symbols from that file. Unrelated actor runtime files should not appear.

Impact:

This is the most serious finding. It can make agents overestimate blast radius, avoid safe changes, produce false architecture conclusions, or waste large amounts of review time. Because `get_symbol_context` also embeds this file dependency graph, the false positives contaminate otherwise useful edit-prep output.

Likely failure mode:

The dependency graph may be collapsing broad crate/module usage, shared names, or generic call references into a per-file dependency edge. The reported `call` refs against a data-store file suggest the graph is not limited to imports or resolved symbol references.

Acceptance criteria:

- Add a regression fixture where `a.rs` exports `WorkItemStore`, `b.rs` imports it, and `c.rs` has unrelated functions with common names. `find_dependents(a.rs)` returns `b.rs` only.
- For AAP, `find_dependents(crates/aap-db/src/stores/work_item.rs)` must not list files that contain no direct import/reference to `work_item`, `WorkItem*`, or exported symbols from that file.
- If the graph is intentionally approximate, output must be labeled approximate and must separate direct imports, direct symbol references, crate-level dependents, and unresolved heuristic edges.
- `get_symbol_context` should not embed high-volume approximate file dependency output without making uncertainty explicit.

## SF-002 - TypeScript caller/callee analysis conflates same-name methods across object boundaries

Severity: High

Status: confirmed limitation or defect

Tool:
- `get_symbol_context`
- likely `find_references`
- likely `edit_plan` reference counts

Repo:
- `C:\AI_STUFF\PROGRAMMING\testpilot`

Target file:
- `backend/src/modules/testing/testing.controller.ts`

Repro:

```text
mcp__symforge.get_symbol_context({
  "name": "startExploration",
  "path": "backend/src/modules/testing/testing.controller.ts",
  "symbol_kind": "fn",
  "symbol_line": 44,
  "verbosity": "signature",
  "sections": ["dependents", "siblings", "git"]
})
```

Observed actual:

For controller method:

```ts
async startExploration(@Body() dto: StartExplorationDto) {
  return this.testingService.startExploration(dto.applicationId, {
    maxDepth: dto.maxDepth,
    maxPages: dto.maxPages,
  });
}
```

SymForge reported:

```text
Callers (1):
  startExploration backend/src/modules/testing/testing.controller.ts:45 in fn startExploration
Callees:
  startExploration backend/src/modules/testing/testing.controller.ts:45 in fn startExploration
```

Expected:

`this.testingService.startExploration(...)` is a call to `TestingService.startExploration`, not a caller of `TestingController.startExploration`. The controller method should not be listed as its own caller because it invokes a same-name service method.

Impact:

This can produce false refactor plans and incorrect call graphs in common TypeScript/NestJS/Angular patterns where controllers delegate to services with identical method names.

Acceptance criteria:

- Add a TypeScript fixture with `Controller.start()` delegating to `this.service.start()`. Querying `Controller.start` must not count `this.service.start()` as a self-reference or caller.
- If type resolution is unavailable, report these as `unresolved_same_name_member_call` rather than exact caller/callee edges.
- `edit_plan` should not compute "References: 1 call sites" from same-symbol lexical matches unless it can resolve the receiver type.

## SF-003 - Valid TypeScript dynamic import type is reported as partial syntax error

Severity: High

Status: confirmed parser false positive

Tool:
- `health`
- `get_file_context`
- `validate_file_syntax`

Repo:
- `C:\AI_STUFF\PROGRAMMING\testpilot`

Target file:
- `frontend/src/app/features/workflows/workflow-builder.component.ts`

Repro:

```text
mcp__symforge.validate_file_syntax({
  "path": "frontend/src/app/features/workflows/workflow-builder.component.ts"
})
```

Observed actual:

SymForge reported:

```text
Status: partial
Diagnostic: tree-sitter: syntax error near `import('rxjs').Subscription` (line 462, column 17)
```

Raw source around the line:

```ts
export class WorkflowBuilderComponent implements OnInit, OnDestroy {
  private api = inject(ApiService);
  private route = inject(ActivatedRoute);
  private router = inject(Router);
  private ws = inject(WebSocketService);
  private subs: import('rxjs').Subscription[] = [];
```

Expected:

`import('rxjs').Subscription[]` is valid TypeScript type syntax and should parse without marking the file partial.

Impact:

Agents may treat valid source as malformed, over-prioritize nonexistent syntax bugs, or distrust symbol extraction in Angular/TypeScript files. The file still produced a useful outline, but the parse-state signal is wrong.

Acceptance criteria:

- Add a TypeScript parser fixture containing `private subs: import("rxjs").Subscription[] = [];`.
- `validate_file_syntax` returns `Status: ok`.
- `health` does not include this file in unexpected partials.
- If the configured tree-sitter grammar cannot support this syntax, classify the diagnostic as parser limitation, not repo-owned syntax error.

## SF-004 - Angular template control-flow syntax is reported as partial HTML parse

Severity: Medium

Status: parser coverage limitation

Tool:
- `health`
- `get_file_context`

Repo:
- `C:\AI_STUFF\PROGRAMMING\testpilot`

Target file:
- `frontend/src/app/app.html`

Repro:

```text
mcp__symforge.get_file_context({
  "path": "frontend/src/app/app.html",
  "sections": ["outline"]
})
```

Observed actual:

SymForge reported:

```text
Parse status: partial
Diagnostic: tree-sitter: syntax error near `0) {` (line 14, column 38)
```

The outline included Angular control-flow constructs:

```text
mod @if
mod @for
router-outlet
```

Expected:

Angular template control-flow syntax such as `@if (...) { ... }` and `@for (...) { ... }` should either parse cleanly or be classified as an expected framework-specific partial, not an unexpected repo-owned parse problem.

Impact:

Frontend-heavy Angular repos will show noisy parse-health warnings even when templates are valid.

Acceptance criteria:

- Add an Angular template fixture using `@if` and `@for` syntax.
- Either parse it cleanly or categorize it as `expected_framework_partial`.
- Health output should separate "valid framework syntax unsupported by parser" from "likely malformed file."

## SF-005 - `ask` fails compound lookup queries that direct tools can answer

Severity: Medium

Status: confirmed agent UX defect

Tool:
- `ask`

Repo:
- `C:\AI_STUFF\PROGRAMMING\testpilot`

Repro:

```text
mcp__symforge.ask({
  "query": "Where is TestingController defined and what module imports it?"
})
```

Observed actual:

SymForge routed to:

```text
search_symbols(query="TestingController defined and what module imports it?")
```

Then returned:

```text
No symbols matching 'TestingController defined and what module imports it?'.
```

Direct calls that worked:

```text
search_symbols(query="TestingController", language="TypeScript")
find_references(name="TestingController", path="backend/src/modules/testing/testing.controller.ts", symbol_kind="class", symbol_line=36)
```

Expected:

`ask` should decompose the compound query into:

1. Find `TestingController`.
2. Fetch references or dependents for that symbol/file.
3. Return both definition and importing module.

Impact:

Agents often use natural-language tools when unsure which tool to call. This failure mode gives a false "not found" result for a simple valid question.

Acceptance criteria:

- Add a routing test for "Where is X defined and what imports it?"
- `ask` should not pass the whole sentence as a symbol query.
- If query decomposition is uncertain, `ask` should return a suggested direct tool sequence instead of a false negative.

## SF-006 - Co-change ranking falls back even when known co-change partners are in scope

Severity: Medium

Status: suspected ranking defect

Tools:
- `analyze_file_impact`
- `search_files`

Repos:
- AAP
- testpilot

AAP repro:

```text
analyze_file_impact({
  "path": "crates/aap-db/src/stores/work_item.rs",
  "include_co_changes": true
})
```

Observed co-change output:

```text
crates/aap-db/tests/work_item_store.rs coupling: 0.500
crates/aap-db/src/stores/mod.rs coupling: 0.182
```

Then:

```text
search_files({
  "query": "work_item",
  "rank_by": "path+cochange",
  "anchor_path": "crates/aap-db/src/stores/work_item.rs",
  "debug_ranking": true
})
```

Observed actual:

`crates/aap-db/tests/work_item_store.rs` appeared in the returned files, but SymForge still reported:

```text
co-change ranking fallback used - anchor_path=... loaded 20 usable coupling partner(s), but none matched returned candidates or passed rank gates
```

testpilot showed the same pattern with:

```text
backend/src/modules/testing/testing.controller.ts
backend/src/modules/testing/services/testing.service.ts
```

Expected:

If `analyze_file_impact` says a file is a co-change partner and `search_files` returns that same file, `rank_by="path+cochange"` should either apply the co-change score or explain the precise rank gate that rejected it.

Impact:

The tool advertises co-change ranking, but agents cannot tell whether fallback is legitimate or a bug.

Acceptance criteria:

- Add a fixture with known coupling `a.ts <-> a.test.ts`.
- `search_files(query="a", rank_by="path+cochange", anchor_path="a.ts")` ranks `a.test.ts` with visible co-change contribution.
- Debug output must name the exact rejection reason when a known partner "does not pass rank gates."

## SF-007 - `checkpoint_now` is exposed but unavailable in daemon-proxy mode

Severity: Medium

Status: runtime/tool-contract issue

Tool:
- `checkpoint_now`

Repos:
- AAP
- testpilot

Repro:

```text
mcp__symforge.checkpoint_now({ "verify_after_write": true })
```

Observed actual:

```text
Checkpoint failed: checkpoint_now is unavailable in daemon-proxy mode; restart with SYMFORGE_NO_DAEMON=1 for local in-process checkpointing.
```

Expected:

One of:

- The tool should be hidden when unavailable in the current runtime mode.
- The daemon proxy should forward checkpoint requests to the daemon.
- `health` should prominently report `checkpoint_now=unavailable` before agents call it.

Impact:

Agents may believe they can persist an index checkpoint but cannot. This is especially relevant before switching projects with `index_folder`.

Acceptance criteria:

- `health_compact` includes checkpoint availability.
- Tool discovery marks `checkpoint_now` unavailable in daemon-proxy mode, or the tool works by forwarding to the daemon.
- Error message includes a Windows-native and shell-native command example for restarting in local mode.

## SF-008 - PATH shadow warning is correct but remediation is not Windows-native

Severity: Medium

Status: confirmed usability issue

Tool:
- `health`
- `health_compact`

Environment:
- Windows PowerShell

Observed actual:

SymForge warning:

```text
bare `symforge` runs C:\Program Files\nodejs\symforge (version unknown),
not your install C:\Users\poslj\AppData\Roaming\nvm\v22.20.0\node_modules\symforge-windows-x64\bin\symforge.exe (7.18.0)
Fix:
  add to ~/.profile: export PATH="C:\Users\poslj\AppData\Roaming\nvm\v22.20.0\node_modules\symforge-windows-x64\bin:$PATH"
```

Shell cross-check:

```powershell
Get-Command symforge -All
```

Output:

```text
C:\Program Files\nodejs\symforge.ps1
C:\Program Files\nodejs\symforge.cmd
C:\Program Files\nodejs\symforge
```

Expected:

The warning should tailor remediation to the active shell/OS. For PowerShell, suggest checking `$env:Path`, NVM path ordering, or using `setx PATH` with caution. For bash/WSL, suggest profile edits.

Impact:

The diagnosis is valuable, but the fix is easy for a Windows user or agent to misapply.

Acceptance criteria:

- Health detects current shell/OS and emits Windows-native remediation under PowerShell.
- Include `Get-Command symforge -All` as the suggested verification command on Windows.
- Keep POSIX `~/.profile` guidance only for POSIX shells.

## SF-009 - Re-index during concurrent work admitted root scratch/probe files into AAP index

Severity: Medium

Status: confirmed hygiene issue

Tool:
- `index_folder`
- `health_compact`
- `what_changed`

Repo:
- AAP

Context:

The user intentionally made the audit harder by noting another agent was working in the project. The AAP worktree was dirty with one tracked docs file and multiple untracked root scratch/status files:

```text
.diff_flags.txt
.diff_sprint.txt
.dr.txt
.grep_aapcommon.txt
.orch_grep.txt
.probe.txt
.stash.txt
.supargs.txt
.tinfo.txt
.unexpected_diff.txt
.verify.txt
.verify2.txt
.wsl.txt
.wslcargo.txt
.wt2.txt
.wtdiff.txt
.wtstatus.txt
```

Observed actual:

Initial AAP index:

```text
1269 files, 34587 symbols
```

After switching to `testpilot` and re-indexing AAP:

```text
1331 files, 36037 symbols
```

The higher file/symbol count is consistent with untracked scratch/probe files being admitted into the source index.

Expected:

Root-level temporary/probe/status files should either be excluded by default, admitted as metadata-only, or explicitly surfaced as noisy untracked files. At minimum, `health` should make it obvious that untracked files changed the index shape.

Impact:

Concurrent-agent scratch files can pollute repo maps, symbol counts, parse warnings, search results, and token-savings metrics.

Acceptance criteria:

- Add an admission policy test with untracked root `.probe.txt`, `.diff_flags.txt`, and `.verify.txt` files.
- Default repo map and symbol search should not treat these as product source.
- Health should report "indexed untracked files: N" or "noise-filtered untracked files: N."
- Provide a setting to include/exclude untracked files deliberately.

## SF-010 - Important tools are lazily exposed and easy for agents to miss

Severity: Low

Status: agent UX improvement

Tooling surface:
- Tool discovery
- Deferred MCP metadata

Observed actual:

The first tool discovery query exposed core tools such as `health`, `search_text`, `get_file_context`, and edit tools. A second query was needed to expose important tools such as:

- `find_references`
- `find_dependents`
- `get_symbol_context`
- `batch_rename`
- `batch_edit`
- `delete_symbol`
- `analyze_file_impact`
- `index_folder`
- `context_inventory`
- `investigation_suggest`

Expected:

For an agent-facing code intelligence MCP, common workflows should be discoverable as groups:

- orientation
- search
- symbol context
- impact analysis
- dry-run edits
- project switching
- diagnostics

Impact:

Agents may under-test or under-use SymForge because they do not know key tools exist.

Acceptance criteria:

- Add a `tool_catalog` or improve `health` with grouped tool recommendations.
- `ask("what tools can I use for impact analysis?")` should return `find_references`, `find_dependents`, `get_symbol_context`, `analyze_file_impact`, `what_changed`, and `diff_symbols`.
- Tool discovery metadata should include workflow tags.

## SF-011 - `conventions` gives shallow or misleading summaries for TypeScript projects

Severity: Low

Status: quality improvement

Tool:
- `conventions`

Repo:
- `testpilot`

Observed actual:

```text
Error handling: Result-based: 3 files return Result, 0 unwrap()s, 0 expect()s
Naming: Functions: 0% snake_case (0/1237). Types: 100% CamelCase (290/290).
Tests: 1 test files, 0 inline test modules, 0 test functions
```

Expected:

For a TypeScript/NestJS/Angular project, conventions should summarize:

- NestJS modules/controllers/services pattern
- decorators and DTO validation style
- Angular component/template/service pattern
- signal usage if Angular signals are prevalent
- test framework and test scarcity, if detected
- error handling via Nest exceptions, thrown `Error`, HTTP exceptions, RxJS, or promise flows

Impact:

The current output is Rust-biased and not actionable enough for TypeScript work.

Acceptance criteria:

- Add language-aware convention detectors for TypeScript/NestJS/Angular.
- Report common decorators, module layering, DTO validation, and frontend component style.
- Avoid "Result-based" language unless the project actually uses a Rust-like `Result` abstraction.

## Positive findings to preserve

These behaviors worked well and should be kept while fixing the bugs:

1. `health` and `health_compact` provide useful project identity, watcher state, parse issues, git temporal status, and PATH diagnostics.
2. `get_file_context` is highly effective for large-file orientation and should remain the default first read.
3. `search_text` headers clearly report trust, parse state, scope, and truncation.
4. `search_files(resolve=true)` correctly refused to over-resolve ambiguous `README` in `testpilot`.
5. Dry-run edit tools clearly reported write semantics and did not modify files.
6. Bad edit selectors fail closed with a useful message.
7. `what_changed` matched shell `git status` for both repos.
8. `diff_symbols(code_only=true)` correctly returned no code changes for a docs/config-only `testpilot` HEAD commit.
9. `context_inventory` and `investigation_suggest` are useful for context hygiene.

## Suggested repair order

1. Fix `find_dependents` false positives and add direct regression tests.
2. Fix or downgrade TypeScript same-name method caller/callee edges.
3. Fix TypeScript dynamic import type parsing or classify it as parser limitation.
4. Add Angular template syntax classification.
5. Improve `ask` query decomposition for common compound questions.
6. Fix co-change ranking or expose exact rank-gate diagnostics.
7. Hide, forward, or pre-declare `checkpoint_now` availability in daemon-proxy mode.
8. Improve Windows PATH remediation.
9. Add untracked/noise file admission controls.
10. Add workflow-oriented tool discovery and language-aware conventions.

## Regression test proposals

### Dependency graph fixture

Files:

```text
src/store/work_item.rs
src/uses_store.rs
src/unrelated_actor.rs
```

Expected:

```text
find_dependents(src/store/work_item.rs) == [src/uses_store.rs]
```

### TypeScript same-name method fixture

Files:

```ts
class Service {
  start() {}
}

class Controller {
  constructor(private service: Service) {}
  start() {
    return this.service.start();
  }
}
```

Expected:

Querying `Controller.start` must not report `this.service.start()` as a caller of `Controller.start`.

### Dynamic import type fixture

Source:

```ts
class Example {
  private subs: import("rxjs").Subscription[] = [];
}
```

Expected:

`validate_file_syntax` returns `Status: ok`.

### Angular control-flow template fixture

Source:

```html
@if (items.length > 0) {
  @for (item of items; track item.id) {
    <div>{{ item.name }}</div>
  }
}
```

Expected:

Either parse cleanly or classify as expected Angular template syntax limitation.

### Ask decomposition fixture

Query:

```text
Where is TestingController defined and what module imports it?
```

Expected:

Tool route:

```text
search_symbols("TestingController") -> find_references("TestingController")
```

### Co-change ranking fixture

Setup:

Create git history where `src/a.ts` and `src/a.test.ts` co-change repeatedly.

Expected:

```text
search_files(query="a", rank_by="path+cochange", anchor_path="src/a.ts")
```

shows `src/a.test.ts` with nonzero co-change contribution.

### Daemon checkpoint fixture

Modes:

- daemon proxy
- in-process

Expected:

- In daemon proxy mode, tool is hidden, forwarded, or health marks it unavailable.
- In in-process mode, checkpoint writes and verifies successfully.

## Tool coverage from audit

Exercised at least once:

- `health`
- `health_compact`
- `index_folder`
- `get_repo_map`
- `search_files`
- `search_symbols`
- `search_text`
- `get_file_context`
- `get_file_content`
- `get_symbol`
- `get_symbol_context`
- `inspect_match`
- `find_references`
- `find_dependents`
- `explore`
- `ask`
- `conventions`
- `edit_plan`
- `edit_within_symbol` with `dry_run=true`
- `replace_symbol_body` with `dry_run=true`
- `insert_symbol` with `dry_run=true`
- `delete_symbol` with `dry_run=true`
- `batch_insert` with `dry_run=true`
- `batch_edit` with `dry_run=true`
- `batch_rename` with `dry_run=true`
- `analyze_file_impact`
- `what_changed`
- `diff_symbols`
- `validate_file_syntax`
- `context_inventory`
- `investigation_suggest`
- `checkpoint_now`

Not exercised with live writes:

- All mutation tools were dry-run only by design.

## Final assessment

SymForge is strong enough to be useful in daily agent work, but it needs correctness hardening before agents should treat dependency graphs, caller/callee traces, or parser diagnostics as authoritative.

The recommended policy until fixes land:

- Use SymForge first for orientation and narrowing.
- Cross-check `find_dependents`, common-name TypeScript references, and parser errors before acting.
- Prefer dry-run edit tools and verify with `git diff`, typecheck, lint, and tests.
- Treat `ask` as a convenience hint, not a source of truth.
