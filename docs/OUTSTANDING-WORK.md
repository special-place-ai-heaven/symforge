# SymForge — Outstanding Work, Deferred Items & Backlog

**Compiled**: 2026-07-10 · **Branch at time of writing**: `018-dogfood-surface-hardening`

A consolidated ledger of everything not-yet-done: in-flight work, deferred
features, documented residuals, known bugs awaiting triage, environment/infra
issues, and housekeeping. Status tags: 🔴 blocking/now · 🟠 next · 🟡 deferred ·
🔵 investigation · ⚪ housekeeping · ✅ done-this-session (kept for context).

---

## 1. In-flight — needs closing now

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1.1 | **018 verification gate** — `cargo test --all-targets -- --test-threads=1` + `cargo build --release` | 🔴 | fmt ✓ check ✓ clippy `--all-targets` ✓ embed ✓ already green. Test run (Codex `session-mrdisrj5-93olmny6`) was **passing, 0 failures**, on its final worktree-integration group; release build pending. **Harvest the final TEST/RELEASE verdict from that Codex session.** |
| 1.2 | **018 merge** | 🔴 | Once 1.1 is green: present PR for approval, merge with the release-please double-count guard (`gh pr merge N --merge --delete-branch --body "PR #N"`), watch release-please PR, publish. **Not pushed/merged yet — needs explicit user approval.** |
| 1.3 | **018 `tasks.md` status** | 🟠 | Mark T001–T024 done; complete T025 (parity — no shared formatter signature changed, note it), T026 (embed ✓), T027 (full gate — from 1.1), T028 (changelog = per-story `fix:` commit messages, already accurate), T029 (committed on branch). |
| 1.4 | **8.13.9 publish verification** | 🟠 | 017 merged (#444) and release 8.13.9 cut (#445 merged). The publish run `29011261436` (v8.13.9 tag + npm) was **in-flight last seen** — confirm the tag + npm package landed. |
| 1.5 | **`cargo clean`** | ⚪ | After 018 is committed+merged+pushed (§16: cleanup is the last step, gated on work being upstream). Active `target/` on E: is legitimate warm cache until then. |

---

## 2. Major deferred feature — the big one

### 2.1 🔵🟠 Feature 019: Multi-index router / per-session index isolation

**User directive (verbatim intent):** each session/harness should control where its
SymForge indexes; multiple agents using SymForge must work independently with **no
index conflicts and no lock**. The design should be an **in-memory registry of all
live running indexes** + an **intelligent router** that manages requests from all
harnesses (which harness holds which index, timestamps, etc.), so an LLM can
**pick/choose/create** an index at will, independently of other sessions — complete
isolation.

- **User asserts this was the ORIGINAL design** ("no clue what changed") → treat as a
  **regression investigation first**, then spec/implement.
- **Smoking gun (observed this session):** the shared SymForge MCP daemon re-rooted
  itself to `E:\project\testpilot` (a different project), so index queries returned the
  wrong project and subagents had to fall back to Read/Grep. Also observed **5
  uncoordinated `symforge` daemon processes** running at once.
- **Relates to:** Feature 012 (harness-agnostic MCP), `src/daemon.rs` (daemon proxy),
  and Grok report finding #2 (§4.1 below) — likely the same root.
- **Next step:** delegate a Codex `explorer`/`planner` investigation of the current
  daemon/index-root model — how the root is chosen (`SYMFORGE_WORKSPACE_ROOT`,
  `index_folder`), whether a multi-index registry exists or regressed, and what changed
  vs. the "original design" — then spec 019.

---

## 3. Deferred residuals inside shipped/in-progress work

| # | From | Item | Status |
|---|------|------|--------|
| 3.1 | 018 US1 | **Admission-tier demotion of pure-data directories** (Grok finding #6 root; the F5 "JSON-dominated dir" content heuristic). 018 shipped only the *defaulting* fix (source-focused `what_changed`/`detect_impact`); demoting data dirs at index time so JSON keys never become first-class symbols was **deliberately deferred** (broader, riskier — affects search/get_symbol/ranking globally). | 🟡 |
| 3.2 | 018 US4 | **Non-CCR-eligible tools still hard-cut with no `symforge_retrieve` footer**: `get_symbol`, `get_file_context`, `get_symbol_context`, `get_file_content`, `what_changed`, `find_dependents`, `diff_symbols`, `get_repo_map` compact/tree. Out of P4 scope (they're single-target reads, not big ranked payloads; would need `TOOL_OUTPUT_PROFILES` entries). | 🟡 |

---

## 4. Grok dogfood report backlog (`docs/grok_report.md`)

018 fixed findings #1, #3, #4, #5 (and #6's acute symptom via the defaulting fix).
Remaining:

| # | Finding | Status | Notes |
|---|---------|--------|-------|
| 4.1 | **#2 — Multi-project retrieval incomplete** | 🟠 | `get_file_context`/`get_symbol*`/`get_repo_map` ignore secondary projects added via `index_folder(add=true)`; `search_files` schema lacks the project fields. Explicitly **out of scope for 018**; it's a **Feature 012 continuation** and overlaps heavily with the §2.1 multi-index router. Track together. |
| 4.2 | **#6 — data files as first-class symbols** | 🟡 | Acute symptom fixed by 018 US1 defaulting; the durable admission-tier fix remains (= §3.1). |

---

## 5. Environment / infrastructure issues

| # | Item | Status | Notes |
|---|------|--------|-------|
| 5.1 | **SymForge daemon mis-rooted / multi-daemon sprawl** | 🔵 | Root cause of the §2.1 pain. 5 `symforge` daemon processes running uncoordinated; one rooted at `E:\project\testpilot`. |
| 5.2 | **codex-subagents server PATH** | 🟠 | Config now has the **full toolchain PATH** (cargo, go, dotnet, docker, python, node, deno, bun, JDK, uv, gh, rg/fd/bat/jq). **Not live until `/reload-plugins`** (running process holds the old minimal PATH). Until then, Codex dispatches need an in-session PATH prepend (`$env:Path = 'C:\Users\rakovnik\.cargo\bin;' + $env:Path`). |
| 5.3 | **PATH strategy decision** | 🟠 | Snapshot (deterministic) vs. inherit-live (self-maintaining; Codex resolves via `CODEX_SUBAGENTS_CODEX_BIN`, not PATH). **Recommendation: inherit-live** (can't go stale like it just did). User to decide. |
| 5.4 | **Terminal Commander daemon down** | ⚪ | `daemon_unavailable` this session; fell back to plain cargo/Bash. |
| 5.5 | **C: disk under floor** | ⚪ | ~27 GB free on C: (CLAUDE.md §16 floor is 50 GB). Pre-existing system state; SymForge builds target **E:** so they don't add to C:. E: ~18–21 GB free. Watch during heavy builds. |

---

## 6. Housekeeping

| # | Item | Status | Notes |
|---|------|--------|-------|
| 6.1 | `docs/grok_report.md` | ⚪ | Untracked. Decide: commit it (valuable dogfood record) or leave local. |
| 6.2 | `docs/grok-dogfood-prompt.md` | 🟡 | Update the reusable dogfood prompt so future runs **self-disclose testing artifacts** — record index size at start vs. a clean checkout and call out any directories they create (would have caught the `mcps/` self-inflation automatically). Grok offered; endorsed. |
| 6.3 | Stale build dirs | ✅ | Removed `target-wtfix/`, `target-wtfix2/` (~900 MB). No stray worktrees; no `C:/symforge-target`. |
| 6.4 | Grok CLI `init` support (SymForge) | 🟡 | Deferred feature: add `InitClient::Grok` + `register_grok_mcp_server` (clone the Codex TOML path at `~/.grok/config.toml`; native command + `RUST_LOG=off` + `SYMFORGE_WORKSPACE_ROOT`). Optional companion: default quiet stderr on the stdio transport. |

---

## 7. Done this session (context / closure)

- ✅ **017 selector-ranking-fidelity** shipped: P1 `edit_plan` resolves `Type::method`; P2 `explore` concept ranking rebalanced. Merged (#444), released **8.13.9** (#445).
- ✅ **018 implemented** (all four stories committed on the branch, gate all-but-test/release green): US1 source-focused change/impact (+ `include_data` opt-in), US2 browse importance ranking (+ removed the overriding tool re-sort), US3 repo-map root guard, US4 CCR retrieval footer on truncation (fixed the misfiring overflow guard).
- ✅ `mcps/` gitignored (Grok dogfood artifact that had ~doubled the index).
- ✅ Grok dogfood report re-verified (4/4 load-bearing claims confirmed against source).
- ✅ (earlier) Grok TUI twitching fixed, Charlotte MCP fixed, hostinger disabled in Grok; F6 worktree routing fixed & shipped through 8.13.8.

---

## Suggested order of attack (post-018-merge)

1. **Close 018** (harvest gate result → merge → publish → `cargo clean`).
2. **`/reload-plugins`** + confirm Codex full-PATH end-to-end; settle PATH strategy (§5.3).
3. **Feature 019 investigation** (§2.1) — the multi-index router regression; fold in Grok #2 (§4.1). This is the highest-value next feature.
4. Then the smaller deferred items: admission-tier demotion (§3.1), CCR footer for single-target reads (§3.2), Grok `init` (§6.4), dogfood-prompt hardening (§6.2).
