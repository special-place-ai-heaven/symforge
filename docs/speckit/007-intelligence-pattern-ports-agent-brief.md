# Spec Kit Agent Brief — Feature 007: Intelligence Pattern Ports (SoulForge → SymForge)

**Purpose:** Single contextual entry point for a [GitHub Spec Kit](https://github.com/github/spec-kit) agent running the SymForge SDD workflow. Read this file **in full** before executing any `/speckit-*` command.

**Status:** Research complete · Spec not yet created · **Do not implement until `004-v8-operator-serve` spine is merged or explicitly waived**

**Branch (mandatory):** All Spec Kit work and implementation for this feature happen **only** on `007-intelligence-pattern-ports`. Do not commit 007 artifacts to `main`, `review/v8-004-operator-serve`, or any other branch. Open a PR from `007-intelligence-pattern-ports` → `main` when done.

---

## 1. Spec Kit setup in this repo

| Item | Location / value |
|------|------------------|
| Spec Kit version | `0.10.2` (`.specify/integration.json`) |
| Initialized integration | **Claude Code only** (`installed_integrations: ["claude"]`) |
| Skills (slash commands) | `.claude/skills/speckit-*/SKILL.md` |
| Templates | `.specify/templates/` |
| Scripts | `.specify/scripts/powershell/` (`create-new-feature.ps1`, `setup-plan.ps1`, `check-prerequisites.ps1`) |
| Constitution (stub) | `.specify/memory/constitution.md` — **placeholder; must be filled in step 1** |
| Current `feature.json` | Points at `specs/005-v8-harness-onboarding` — **update when 007 is created** |
| Post-specify hook | `agent-context` extension refreshes `CLAUDE.md` managed section (optional, auto) |

**Cursor / other agents:** Spec Kit slash commands are wired for Claude Code. If you are not in Claude Code, follow the **same step order and artifacts** defined below using the skill files as procedure docs and the PowerShell scripts where applicable.

---

## 2. Mandatory workflow order

### Step 0 — Dedicated branch (before anything else)

```powershell
cd E:\project\symforge
git fetch origin main
git checkout main
git pull origin main
git checkout -b 007-intelligence-pattern-ports   # skip if already on this branch
git branch --show-current   # MUST print: 007-intelligence-pattern-ports
```

If you are on `review/v8-004-operator-serve` or any other branch, **stop** and switch. Stash unrelated WIP first; do not mix 005 harness or 004 serve changes into 007 commits.

### Steps 1–6 — Spec Kit SDD

Run in this sequence. Do not skip constitution or clarify unless the operator explicitly waives them.

```text
1. /speckit-constitution   # project principles (SymForge-specific; see §4)
2. /speckit-specify        # baseline spec from §5 feature description
3. /speckit-clarify        # optional but recommended — seed questions in §6
4. /speckit-plan           # implementation plan + research.md + contracts/
5. /speckit-tasks          # dependency-ordered tasks.md
6. /speckit-implement      # execute tasks.md; CI gates in §11
```

**Before step 2:** Create the feature directory (or let `speckit-specify` do it):

```powershell
cd E:\project\symforge
.\.specify\scripts\powershell\create-new-feature.ps1 -Json -ShortName "intelligence-pattern-ports" `
  "Port selective SoulForge intelligence UX patterns onto SymForge LiveIndex + STEL — post-edit impact footers, orientation doctrine, importance-ranked compact repo map, compact find fusion. No SQLite Soul Map, no grep intercept, no terminal agent features."
```

**Expected output directory:** `specs/007-intelligence-pattern-ports/`  
**Expected branch:** `007-intelligence-pattern-ports` (or project hook convention)

**Numbering note:** `005` references `006` as **admin GUI** (out of scope here). This feature is **`007`**.

---

## 3. Executive verdict (research summary)

**Verdict: STAY AND ENHANCE** — keep SymForge's Rust LiveIndex + STEL + MCP architecture. Adopt **thin presentation and routing patterns** from SoulForge ([ProxySoul/soulforge](https://github.com/ProxySoul/soulforge)), not its SQLite Soul Map, grep interception, or full terminal-agent stack.

**SoulForge investigation clone (local, optional):** not committed; re-clone from GitHub if needed for reference.

**Timeline (no theft concern):** SoulForge public v1 **2026-03-01**; SymForge initial commit **2026-03-06**. Zero cross-references between codebases. Convergent product space.

**Dependency:** Ship or branch from **`004-v8-operator-serve`** (`symforge serve`, compact-3 default, STEL ledger). This feature **enhances** intelligence UX on stdio + serve; it must not block or duplicate 004 scope.

---

## 4. Input for `/speckit-constitution`

Replace `.specify/memory/constitution.md` placeholders using **SymForge binding docs** (constitution is currently a template stub).

**Primary sources (read in order):**

1. `AGENTS.md` — mission, local-first, idempotency, recovery, MCP surface rules
2. `CLAUDE.md` — verification gates, architecture map, tool consolidation pattern
3. `docs/v8-gap-closure-plan.md` — v8 binding gates (G-020+, STEL, serve, embed isolation)

**Non-negotiable principles to encode:**

| # | Principle | Rule |
|---|-----------|------|
| I | **Local-first in-process index** | Read path stays in-memory LiveIndex; no second authoritative index (no Soul Map SQLite port) |
| II | **MCP-native surface** | Tools, resources, prompts — not chat-harness injection of repo maps as fake user messages |
| III | **Trust envelopes** | Every response carries machine-readable trust/completeness; truncations disclosed |
| IV | **Determinism & recovery** | Byte-exact persistence; quarantine bad snapshots; idempotency on mutations |
| V | **Frecency invariant** | Discovery/search tools must not bump frecency; commitment paths only |
| VI | **Embed isolation (G-045)** | `cargo check --no-default-features --features embed` stays free of server/network deps |
| VII | **Transport parity** | stdio and `symforge serve` `/mcp` return equivalent tool results for same repo state |
| VIII | **Verification before done** | `cargo fmt --check`, `clippy -D warnings`, `cargo test --all-targets -- --test-threads=1`, `cargo build --release` |

**Governance:** Constitution amendments require version bump in file header; plan-template Constitution Check must reference these principles.

---

## 5. Input for `/speckit-specify`

Paste this block as the feature description (edit only if operator directs):

```text
Feature 007 — Intelligence Pattern Ports (v8 8.1.x)

Port selective code-intelligence UX patterns from competitive research (SoulForge) onto SymForge's existing LiveIndex + STEL stack. SymForge remains an MCP code-intelligence server, not a terminal coding agent.

In scope (phased):
- P1: Post-edit impact footer on successful structural mutations (dependent file count + top co-change partners when git temporal ready), appended to symforge_edit and legacy structural edit tool responses.
- P1: Orientation doctrine in MCP prompts and symforge://repo/map resource — "map orients, tools prove; absence from map ≠ absence from repo."
- P2: Importance-ranked get_repo_map(detail=compact) — replace pure alphabetical ordering with dependent-count + churn-weighted ranking; show path (→N) when N≥2.
- P2: Compact STEL find fusion — multi-word symbol + file fuzzy search with co-change neighbor boost inside symforge find intent (no new public tool).
- P2: Impact intent polish — symforge intent=impact chains dependents + co-changes in one envelope; edit_plan mentions co-change when temporal data exists.

Explicitly out of scope:
- SQLite Soul Map or parallel persistent graph index
- Grep/glob intercept (SymForge does not own client native tools)
- request_tools lazy schema loading (compact-3 STEL already covers this)
- Terminal agent features (TUI, sessions, memory, task router, Neovim, providers)
- LLM-generated symbol summaries in index
- MinHash clone detection (defer to 8.2+)
- Hard 10k file cap (keep fail-closed discovery; optional git-recency subset is 8.2+ only)

Depends on: 004 operator serve spine (transport parity target). Does not duplicate 004 auth/ledger/serve work.

Closes: agent UX gaps identified in SoulForge competitive analysis; advances v8 "singular server" attach experience quality without scope creep.
```

**Suggested user stories for the spec writer:**

1. **Agent sees blast radius after edit** — structural edit success includes compact impact line without extra tool call.
2. **Agent orients before over-reading** — onboard/architecture prompts + repo map resource teach ranked-truncation semantics.
3. **Compact find answers fuzzy concepts** — `symforge` find intent ranks multi-word queries across symbols and paths.
4. **Compact map shows what matters** — `get_repo_map` compact mode surfaces high-fan-in files first within existing token budget.

---

## 6. Seed questions for `/speckit-clarify`

Resolve these into `spec.md` clarifications (recommended defaults in **bold**):

| # | Question | Recommended default |
|---|----------|---------------------|
| Q1 | Should impact footer apply to **all** structural tools or only `symforge_edit`? | **All successful structural mutations** (`replace_symbol_body`, `edit_within_symbol`, `batch_*`, `symforge_edit` apply) |
| Q2 | Footer format: prose vs machine-parseable tag? | **`[impact: N dependents · cochanges: a, b, c]`** plain suffix; trust envelope unchanged |
| Q3 | Compact map ranking: new `detail=ranked` or change default `compact`? | **Change `compact` ordering only**; preserve `full` and `tree` |
| Q4 | Find fusion: new tool vs STEL-only? | **STEL `find` intent only** — no 4th compact tool |
| Q5 | Ship before or after 004 merge? | **After 004 lands** unless operator waives; no serve/auth work in 007 |

---

## 7. Capability matrix (for plan + research.md)

| Capability | SoulForge | SymForge today | Verdict | Effort |
|------------|-----------|----------------|---------|--------|
| Persistent graph index | SQLite Soul Map | LiveIndex + `.symforge/index.bin` | **ALREADY_HAVE** | — |
| Git co-change | Soul Map queries | `git_temporal.rs`, `search_files`, `analyze_file_impact` | **ALREADY_HAVE** | — |
| Importance ranking | PageRank | Caller-weighted `search.rs`; map **alphabetical** | **ADOPT** (compact map) | M |
| Post-edit blast radius | `enrichWithBlastRadius` on every edit | Sidecar hook exists; **MCP edits lack inline footer** | **ADOPT** | S |
| Fuzzy unified find | `soul_find` | Split across search + explore + STEL | **ADOPT** (STEL find) | M |
| Orientation doctrine | `soul-map.ts` usage text | Trust envelopes; weak map semantics in prompts | **ADOPT** | S |
| Grep/glob intercept | `repo-map-intercept.ts` | N/A (MCP server) | **REJECT** | — |
| SQLite Soul Map | Primary index | LiveIndex | **REJECT** | — |
| 10k file cap + recency trim | `applyFileCap` | Discovery fail-closed at high limits | **DEFER** (8.2+) | L |
| MinHash clone detection | Yes | No | **DEFER** | L |
| LLM symbol summaries | In map | Signatures only | **REJECT** | — |

---

## 8. Implementation hints for `/speckit-plan`

**Technical context (pre-filled):**

- **Language:** Rust 2024, single crate `symforge`
- **Touch modules:** `src/protocol/tools.rs`, `src/protocol/format.rs`, `src/protocol/prompts.rs`, `src/protocol/resources.rs`, `src/live_index/query.rs`, `src/stel/planner.rs`, `src/stel/edit_apply.rs`, `src/protocol/smart_query.rs`, `src/protocol/edit_plan.rs`
- **Reuse:** `git_temporal::GitTemporalIndex`, reverse import index, `explore_path_penalty` / `NoisePolicy`, existing sidecar `workflow_post_edit_impact_handler` logic (do not fork behavior)
- **Do not add:** `rusqlite` graph DB, new cargo features beyond existing `server`/`embed` split

**Known code gaps (evidence):**

```text
# Compact repo map sorts alphabetically today — ranking gap
src/live_index/query.rs  capture_repo_outline_view  ~L2295  files.sort_by(alphabetical)

# Co-change exists but opt-in on analyze_file_impact
src/protocol/tools.rs  include_co_changes  ~L4097

# Sidecar post-edit impact exists; MCP structural edits do not inline it
src/sidecar/handlers.rs  workflow_post_edit_impact_handler  ~L737
```

**Contracts to generate in plan phase:**

| Contract file | Contents |
|---------------|----------|
| `contracts/impact-footer.md` | Footer grammar, when omitted, examples |
| `contracts/compact-map-ranking.md` | Scoring formula, `(→N)` display rules, token budget |
| `contracts/stel-find-fusion.md` | Multi-term query behavior, frecency non-bump |
| `contracts/orientation-doctrine.md` | Prompt + resource text requirements |

**Constitution check gates for plan:**

- GATE: No second index / no SQLite graph for queries
- GATE: Frecency invariant preserved in new ranking paths
- GATE: `embed` feature still compiles without server deps
- GATE: Transport parity tests if touching shared protocol formatters

---

## 9. Task seeds for `/speckit-tasks`

```text
Phase 0 — Setup
  [ ] Confirm 004 serve spine available on branch or document waiver
  [ ] Add integration test fixture repo with known dependent edges + git history

Phase 1 — P1 quick wins
  [ ] Shared impact_footer helper (dependents + co-changes)
  [ ] Wire footer into structural edit success paths + symforge_edit apply
  [ ] Tests: footer present/absent cases
  [ ] Orientation doctrine in prompts.rs + resources.rs
  [ ] Tests: prompt/resource content snapshots

Phase 2 — P2 ranking & find
  [ ] Importance-ranked compact repo map view in query.rs + format.rs
  [ ] Tests: high-fan-in files surface first; full/tree unchanged
  [ ] STEL find fusion in planner.rs + smart_query.rs
  [ ] Tests: multi-word query ranking; frecency not bumped
  [ ] Impact intent + edit_plan co-change line

Phase 3 — Polish
  [ ] Golden replay / STEL rows for new intents
  [ ] docs/v8-release-notes.md entry
  [ ] quickstart.md verification scenarios
```

---

## 10. What NOT to build (reject list)

| Pattern | Reason |
|---------|--------|
| SQLite Soul Map as primary index | Duplicates LiveIndex; slower; two truths |
| Grep/glob intercept | SymForge does not control Cursor/Codex native Grep |
| `request_tools` / `release_tools` | Compact-3 + STEL umbrella already solves schema cost |
| Injected Soul Map as user message | MCP product uses tools/resources/prompts |
| LLM summaries in index | Non-deterministic; breaks trust story |
| SoulForge terminal agent features | Different product category |
| Hard 10k cap | SymForge fail-closed is safer; recency subset is separate 8.2 spike |

---

## 11. Verification (implement step)

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets -- --test-threads=1
cargo build --release
```

**Feature-specific tests to add:**

- `tests/impact_footer.rs`
- `tests/compact_map_ranking.rs`
- `tests/stel_find_fusion.rs`

---

## 12. Related specs & docs

| Artifact | Path |
|----------|------|
| Operator serve spine (dependency) | `specs/004-v8-operator-serve/` |
| Harness onboarding (parallel track) | `specs/005-v8-harness-onboarding/` |
| STEL phase 2 | `specs/002-v8-phase2-stel-controller/` |
| v8 gap closure (binding) | `docs/v8-gap-closure-plan.md` |
| Spec Kit upstream | https://github.com/github/spec-kit |

---

## 13. Copy-paste prompts per workflow step

### Step 1 — Constitution

```text
/speckit-constitution

Use docs/speckit/007-intelligence-pattern-ports-agent-brief.md §4 as principle input.
Encode SymForge local-first MCP rules from AGENTS.md and CLAUDE.md.
Fill .specify/memory/constitution.md completely (no placeholder brackets).
```

### Step 2 — Specify

```text
/speckit-specify

Read docs/speckit/007-intelligence-pattern-ports-agent-brief.md in full.
Use §5 as the feature description. Short name: intelligence-pattern-ports.
Feature number: 007. Do not scope 004 serve/auth/ledger work.
```

### Step 3 — Clarify

```text
/speckit-clarify

Use docs/speckit/007-intelligence-pattern-ports-agent-brief.md §6 seed Q&A.
Encode resolved defaults into specs/007-intelligence-pattern-ports/spec.md.
```

### Step 4 — Plan

```text
/speckit-plan

Read specs/007-intelligence-pattern-ports/spec.md and agent brief §7–§8.
Generate research.md, data-model.md, contracts/, quickstart.md.
Respect constitution gates. No SQLite Soul Map.
```

### Step 5 — Tasks

```text
/speckit-tasks

Use agent brief §9 as phase seeds. TDD where tests are listed first.
Mark [P] only for truly parallel tasks.
```

### Step 6 — Implement

```text
/speckit-implement

Execute specs/007-intelligence-pattern-ports/tasks.md.
Verification: agent brief §11. Update tasks.md [X] as you go.
```

---

## 14. Agent checklist before starting

- [ ] Read this entire brief
- [ ] On branch `007-intelligence-pattern-ports` only (`git branch --show-current`)
- [ ] Branch based on `main` (includes this brief @ `docs/speckit/007-intelligence-pattern-ports-agent-brief.md`)
- [ ] Run `create-new-feature.ps1` or verify `specs/007-intelligence-pattern-ports/` exists
- [ ] Update `.specify/feature.json` to point at 007 when specify completes
- [ ] Do not copy SoulForge source code — patterns only
- [ ] Keep diff focused; no unrelated v8 spine changes in 007 PR

---

*Generated: 2026-06-16 · SoulForge competitive investigation + SymForge codebase analysis*
