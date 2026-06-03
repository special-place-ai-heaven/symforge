# SymForge — Independent Whole-Codebase Review & Test Brief

You are an independent senior reviewer. Your job is an **honest, methodical, evidence-based audit of the entire SymForge codebase** — not a rubber stamp, not a skim of the latest commits. Assume the authors are competent and the code mostly works; your value is in the **specific, reproducible problems** you can prove, and in the **honest map of what you did and did not verify**.

You have a checked-out repository, a shell, and the ability to build, run, and write files. **Use them. Review by running the code, not by reading it alone.** A claim you have not executed is a hypothesis, and you must label it as one.

---

## 0. Honesty contract (read first, non-negotiable)

1. **Code is the source of truth.** Comments, docs, commit messages, and this brief can be wrong or stale. When code and prose disagree, trust the code and report the discrepancy.
2. **Reproduce before you claim.** Every "bug" must come with a concrete reproduction: a failing test you wrote, a command + observed output, or an exact execution trace through cited lines. If you cannot reproduce it, file it as "suspected, unverified" with your reasoning — clearly separated from confirmed findings.
3. **Cite `file:line` for everything.** No finding without a location.
4. **Distinguish severity classes and do not inflate.** A real bug (wrong output, crash, data loss, security hole) is not the same as a smell (maintainability) which is not the same as a style nit. Report all three but never dress a nit as a defect to pad the report.
5. **State confidence.** For each finding: `confirmed` (you reproduced it), `high` (traced it but couldn't run), `speculative` (pattern-matched). No false certainty.
6. **No invented issues. No hallucinated APIs.** Do not claim a function, flag, or behavior exists without opening it. If you're unsure a method exists, grep/open it first.
7. **Report coverage honestly.** End with what you reviewed, what you skipped, and what could not be verified on your platform. Silence is not coverage. "I read all of `tools.rs`" must be true if you write it.
8. **Negative results are valuable.** If an area you expected to be fragile is actually solid, say so — that's signal too.

A premature "looks good" is worse than an honest "here is what I could not check." Optimism that isn't backed by execution is the failure mode we are paying you to avoid.

---

## 1. What SymForge is (orientation)

SymForge is a **Rust MCP (Model Context Protocol) server that gives LLM coding agents symbol-aware code navigation and editing** over a repository: search, symbol extraction, references/dependents, structural edits, and an MCP `tools/list` surface of ~32 canonical tools. It parses ~12 languages via tree-sitter, maintains a live in-memory index with a file watcher, and runs as a daemon with a sidecar plus an in-process index, fronted by an npm launcher that ships per-OS native binaries.

**Why correctness matters more than usual here:** the consumer is an LLM that *trusts these tool outputs to navigate and edit code*. The worst failure mode is **confidently wrong output** — a search that silently returns results from the wrong project, a `find_references` that misses real uses, a dependent graph that conflates two languages. Wrong-but-silent is far more dangerous than a loud error. Weight your review toward **trust violations**: anywhere a tool can return plausible, well-formatted, *incorrect* data without signaling uncertainty.

Scale: ~172 Rust files, ~104k LOC under `src/`, ~10.5k indexed symbols, ~2,256 test attributes. Some files are very large (see hotspots).

---

## 2. Environment, build, and the verification gate

- **Toolchain:** Rust pinned (see `rust-toolchain.toml` / CI; currently 1.95.0), **edition 2024**.
- **Hard compiler constraints (in `Cargo.toml [lints]`):** `unsafe_code = "deny"` and `warnings = "deny"`. Every `unsafe` site carries an explicit `#[allow(unsafe_code)]`; **every warning fails the build.** A change that merely warns is a red finding.
- **Full gate (run all; this is what CI enforces):**
  ```
  cargo fmt --check
  cargo check
  cargo clippy --all-targets -- -D warnings
  cargo test --all-targets -- --test-threads=1
  cargo build --release
  cd npm && npm test
  ```
  **`--test-threads=1` is required** — tests share process/port/filesystem state and will flake or false-fail under parallelism. If you see nondeterministic failures, re-run single-threaded before reporting them as bugs (but DO report genuine test-isolation defects).
- **Engine-only build (library embedders, no server/daemon):**
  ```
  cargo build --no-default-features --features embed
  cargo test  --no-default-features --features embed --lib -- --test-threads=1
  ```
  The `embed` feature must compile and pass with the server surface (daemon, sidecar, protocol server, cli, axum, rmcp) entirely gated out. **Feature-gating leaks are a known defect class** — verify nothing server-only leaks into the engine modules.
- **Ignored perf smokes (run if probing performance):** `cargo test -- --ignored` includes `test_load_perf_1000_files` and `calibrate_current_repo_smoke`.
- **Baseline first:** before any analysis, run the full gate on a clean checkout and record the result. If anything is red on `main`, that is finding #1 with the exact output.

**Cross-platform caveat:** the code has `#[cfg(unix)]` and `#[cfg(windows)]` branches (process/PID handling, path logic, launcher). You can likely only *compile* one OS. For the OS you cannot build, **reason explicitly about the cfg-gated code and flag it as "needs other-OS verification."** Do not assume a branch you can't compile is correct — this is a real historical bug source here.

---

## 3. Method — work in phases, record as you go

**Phase 0 — Baseline.** Clean build + full gate (incl. embed build). Record green/red verbatim. Note build time and any warning suppressed by `-A`.

**Phase 1 — Map the system.** Produce your own short architecture model before judging anything:
- Module responsibilities under `src/` (`protocol/`, `live_index/`, `parsing/`, `sidecar/`, `daemon.rs`, `cli/`, `version_registry`, `watcher/`, `domain/`, `embed`).
- The **request lifecycle**: an MCP tool call → daemon proxy vs in-process local index vs sidecar → result formatting → response. Identify exactly *which index serves which tool* and *when fallbacks trigger*. State-routing is the #1 risk area (see §5).
- The **trust envelope**: how results signal authority/staleness/completeness (result-status metadata, "source authority", "completeness" fields). Where can a tool emit confident output without that signaling?
- The npm launcher → native binary → daemon/sidecar lifecycle (process spawn, PID files, version registry, durable install).

**Phase 2 — Dimension sweeps.** Go area by area (don't free-associate). For each, read the actual code paths and form testable hypotheses. Dimensions in §4.

**Phase 3 — Adversarial testing.** This is where you earn the review. For each credible hypothesis, **write a new test or run a real scenario** that tries to break it. Prefer:
- New `#[test]`/`#[tokio::test]` cases and fixtures that target suspected gaps (parsing edge cases, cross-language collisions, empty/loading/error states).
- End-to-end scenarios: switch projects mid-session, restart the daemon, kill the sidecar, concurrent tool calls, a file changing under the watcher, a malformed/over-large input, a path-traversal attempt, a symlink, a non-UTF-8 file, a huge file.
- Property/fuzz-style inputs for the parsers and the edit engine (does a structural edit ever corrupt a file? round-trip?).
- For performance claims: measure with a command, report p50/p95 or before/after — no benchmark, no perf claim.

**Phase 4 — Synthesize.** Severity-ranked findings with evidence, plus an honest coverage map. Output format in §7.

---

## 4. Review dimensions (cover each; note any you skip)

1. **Correctness & logic.** Wrong results, off-by-one, edge cases (empty input, single element, unicode, CRLF/LF, BOM, very large files, deeply nested code), intent-vs-implementation mismatches. Especially: search, symbol extraction, references/dependents, ranking, formatters.
2. **State & concurrency.** The daemon / sidecar / in-process index triad. Index switching and invalidation. Watcher races, debounce, reconciliation. Generation/fencing of in-flight mutations. Recycled PIDs. Can two tools in one session observe **different** repository state? Can a stale index survive a project switch?
3. **The trust envelope / silent-wrong-output.** Anywhere a tool returns formatted data that could be from the wrong project, stale, truncated-without-saying-so, or missing real matches while reporting success. This is the highest-value category.
4. **Security.** MCP input handling (untrusted tool arguments), path traversal / escaping the repo root, symlink following, command/argument injection in the launcher and any spawned process, the npm package surface (no install scripts? exact-pinned optional deps?), file read/write bounds, resource exhaustion (unbounded read/index of hostile input), and any place secrets/tokens flow (CI release scripts under `execution/`, `.github/workflows/`).
5. **Error handling & panic surface.** There are ~2,300 `unwrap()/expect()/panic!()` sites. Which are reachable from untrusted input or normal operation? A panic in a tool handler is a denial of service for the agent. Distinguish "provably safe unwrap" from "reachable panic." Check error propagation: are failures swallowed, logged-and-ignored, or surfaced as fake success?
6. **Cross-platform (`cfg`).** Every `#[cfg(unix)]`/`#[cfg(windows)]` branch — does the *other* OS path compile and behave? Process termination, PID liveness, path normalization (`\\?\`, UNC, WSL `/mnt/...`), case sensitivity, executable extension. (Historical bug: unix-only `unsafe` code that broke `warnings=deny` only on Linux CI, invisible on a Windows dev box.)
7. **Parsing correctness across languages.** tree-sitter queries per language for symbols and xrefs. Are reference/import/type captures **language-scoped**? Do bare value uses, re-exports, aliases, macros, generics, and qualified paths resolve correctly? Cross-language name collisions. Missing node kinds. Test each supported language at least minimally.
8. **Public/MCP contract.** `tools/list` shape, tool input schemas, backward-compat aliases (`src/daemon.rs`), result-status metadata, resources/prompts surfaces. Does documented behavior match actual? Are removed tools still aliased? Schema/registration drift (the allowlist of tool names vs actually-registered tools).
9. **Feature gating (`embed`).** Confirm the engine builds and tests with `--no-default-features --features embed`. Hunt for server-only types/deps leaking into engine modules (`parsing`, `live_index`, `query`, `git`, `domain`, the `embed` facade). Dead-code-under-embed warnings.
10. **Tests quality.** Are tests asserting behavior or just exercising code? Weak assertions, tests coupled to implementation detail, missing edge/negative cases, missing failure-mode tests, flaky/order-dependent tests, fixtures that don't represent reality. Coverage of the hotspots in §5.
11. **Build / release / CI integrity.** `.github/workflows/`, `execution/*.py`. Version sync across `Cargo.toml` + npm packages. Release automation correctness (checkout refs pinned to the release tag, idempotent publish, matrix coverage). Supply-chain surface of the npm packages.
12. **Maintainability (report, don't inflate).** The 20k-line `tools.rs` and other large files — genuine structural risk vs working-but-big. Premature abstraction, duplication, dead seams. Keep these clearly separated from correctness findings.

---

## 5. Risk hotspots (start here — but do not stop here)

Grounded by size and by where defects have actually occurred. Use as entry points; coverage must still be whole-codebase.

| Area | Files | Why it's risky |
|---|---|---|
| MCP tool handlers | `src/protocol/tools.rs` (~20k LOC), `src/protocol/edit.rs`, `src/protocol/edit_tools.rs`, `src/protocol/format.rs` | Largest surface; every tool's logic + formatting; trust-envelope lives here |
| Index query & routing | `src/live_index/query.rs`, `search.rs`, `store.rs`, `persist.rs` | References/dependents, ranking, index state, persistence, project switching |
| Daemon / sidecar lifecycle | `src/daemon.rs`, `src/sidecar/handlers.rs`, `src/sidecar/*` | Proxy-vs-local routing, PID/process handling, fallback, state consistency |
| Parsing / xref | `src/parsing/xref.rs`, `src/parsing/*` | Per-language tree-sitter queries; language-scoping; capture completeness |
| Install / launcher / versioning | `src/cli/init.rs`, `src/cli/hook.rs`, `version_registry`, `npm/` | Cross-platform spawn, durable install, version sync, supply chain |
| Watcher | `src/watcher/`, watcher_state | File events, debounce, reconciliation, overflow, races |
| Release/CI | `.github/workflows/*.yml`, `execution/*.py` | Publish correctness, version sync, secrets |

---

## 6. Known recent defect-classes — hunt for more of each

These are real bugs found and fixed in this codebase. They tell you the *shape* of defects that slip through here. **For each, look for siblings that weren't fixed.** Do not assume the single fix closed the whole class.

1. **Silent cross-project / stale-index serving.** A project switch updated some tools' view but not others (one tool family kept serving the old project from a frozen local index because a fallback path never invalidated it). → Are there *other* paths that mutate "current root" without invalidating every index/cache a tool can read? Can any tool serve stale state after a switch, reload, or watcher event?
2. **Language-blind matching.** Reference/import/dependent matching that keyed on a bare name across languages (a Python `import gguf` matched a Rust `gguf.rs`). → Audit *all* cross-file/symbol matching for missing language scoping.
3. **Incomplete tree-sitter capture.** A reference kind simply wasn't captured (bare-value `const`/`static` uses), so `find_references` under-reported with no error. → Check every language's xref/symbol query for missing node kinds (value uses, re-exports, aliases, macro bodies, trait/impl, generic args).
4. **`cfg`-blind-spot.** OS-gated code that compiles/passes on the author's OS but breaks the *other* OS (and `warnings=deny` made it fatal only on the CI OS). → Re-examine every `#[cfg(...)]` branch for the un-built OS.
5. **Offline / failure-path false success.** Update/install logic that silently "passed" when a probe failed (treated unreachable as success; read only stdout and missed a failing launcher; killed a recycled PID; deleted an orphan before re-registration). → Hunt for failure paths that degrade to silent success instead of loud unavailability.
6. **Release-automation drift.** A publish job checked out the triggering SHA instead of the pinned release tag, risking publishing the wrong tree / re-publishing an existing version. → Verify every release job uses the correct ref and is idempotent; verify version-sync guards.
7. **Schema/registration drift.** The list of advertised tool names diverging from actually-registered handlers/aliases. → Cross-check `tools/list`, the init allowlist, daemon aliases, and real handlers.

---

## 7. Output format

Write your report to a file named `review-findings-<your-tool>.md` (e.g. `review-findings-codex.md` / `review-findings-cursor.md`) at repo root, and also summarize inline. Structure:

```
# SymForge Review — <tool/model>, <date>

## Baseline
- Full gate result (verbatim pass/fail per command), build time, platform/OS, toolchain.
- Embed build result.

## Summary
- N findings: <critical> / <high> / <medium> / <low> / <nit>
- Top 3 risks in one sentence each.
- Overall honest assessment (2-4 sentences).

## Findings (ranked by severity, then confidence)
### [SEV-1] <title>
- Severity: critical | high | medium | low | nit
- Confidence: confirmed | high | speculative
- Location: path:line (+ all related sites)
- What's wrong: precise description of the incorrect behavior.
- Reproduction: the test you wrote / command + observed output / traced execution. Paste real output.
- Impact: who/what breaks, and how it manifests to the LLM consumer.
- Suggested fix: minimal, specific (a diff sketch is ideal). Note risks of the fix.
- Class: maps to a §6 defect-class? new class?

## Tests I added
- List new tests/fixtures, what each proves, pass/fail.

## Coverage map (honesty section — required)
- Reviewed thoroughly: <areas/files>
- Reviewed lightly: <areas/files>
- Not reviewed: <areas/files> and why
- Could not verify on my platform: <cfg-gated code, other-OS paths, anything needing infra you lacked>
- Assumptions made.

## Things that are solid
- Areas you tried to break and couldn't (with how you tried).
```

Severity guide: **critical** = data loss / security hole / silent-wrong-output on a common path; **high** = crash/incorrect result on a realistic path; **medium** = wrong on an edge case or degraded UX; **low** = minor/cosmetic-but-real; **nit** = style/preference.

---

## 8. Hard DON'Ts

- Don't approve without running the gate. "Compiles" and "tests pass" are the floor, not the verdict.
- Don't report style nits as defects, and don't pad. Ten real, reproduced findings beat fifty speculative ones.
- Don't trust comments/docs/this brief over the code.
- Don't claim an API/behavior exists without opening it. No hallucinated function names.
- Don't say "works" for anything you didn't execute. Label hypotheses as hypotheses.
- Don't limit yourself to the latest commits — this is a **whole-codebase** audit.
- Don't assume a `#[cfg]` branch you couldn't compile is fine.
- Don't fix in place silently — your job is to *find, prove, and propose*. (Small reproduction tests are welcome; production fixes only if explicitly asked.)

Begin with Phase 0. Take the time to be right. Honest and incomplete beats confident and wrong.
