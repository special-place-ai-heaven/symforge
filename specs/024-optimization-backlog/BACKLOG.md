# Optimization / speedup backlog — /autoresearch campaign list

as_of 2026-08-03. Owner scope ruling: "anything involving efficiency, speed,
token saving is fair game." Every item here is a candidate for its own
`/autoresearch` run. Measured items carry real numbers from the phase
instrumentation that landed in PR #488 (`79053ab`, released in v8.16.9);
unmeasured items must be measured BEFORE optimizing — never tune against a
guess (that mistake is what PR #488 exists to prevent).

Baseline: symforge repo, ~910 files / ~26k symbols, debug build on the dev
box. Release is ~4-5x faster across the board; both matter (debug bounds the
test-suite and dev loop, release bounds operators).

## Measured — ready to fire, ranked by effort-to-win

| # | target | measured cost | where | why this ranking |
|---|--------|--------------|-------|------------------|
| 1 | **Index publication — knowledge bridge + manifest + authority** | **12.1 s** debug / ~2-3 s release per cold start | `src/live_index/store.rs` `new_with_scout_plan_and_code_signals` | Rebuilds ~5,400 bridge cards + 20k forward links from scratch on EVERY start. "Recompute everything on boot" is a persistence problem; the snapshot/checkpoint system already exists to anchor a cached or incremental bridge. Best effort-to-win. Constraint: published data must stay byte-equivalent (suite + golden replay define equivalence). |
| 2 | **Admission + parse** | **12.9 s** debug | `admit_and_parse_entries`, `src/live_index/store.rs` | The biggest number and the hardest surgery: parser pipeline, secret scan, tree-sitter. Candidates: per-file parse cache keyed by content hash (a snapshot restore path already exists — why does fresh-load never reuse it?), parallelism tuning. Second campaign, not first. |
| 3 | **Serve runtime construction** | **7.9 s** debug | `build_serve_runtime` → `SymForgeServer::new_with_state_placement` (tool_router/prompt_router) | Suspect: schema generation for the 39-tool surface at startup. Candidates: lazy/once-per-process router, precomputed schemas. Affects every serve start incl. `symforge admin` start-on-demand. |
| 4 | **Trigram index build** | **4.2 s** debug | `TrigramIndex::build_from_files` | Full-content scan after parse. Candidates: build during the parse pass (content is already in cache-warm memory there), parallelize, or persist alongside the snapshot. |

## Measured — infrastructure speed (not runtime code)

| # | target | measured cost | notes |
|---|--------|--------------|-------|
| 5 | **CI rust job** | **~26 min** per run (compile-dominated; suite itself is ~3.5 min since b5776c5) | Candidates: cargo caching (rust-cache/sccache), splitting build from test, dropping the release build from PR CI (it gates nothing a PR needs). Every PR pays this twice (push + PR event?) — verify triggers first. |
| 6 | **Cold worktree builds in dev** | **6-20 min** per worktree (deps from scratch each time; 3 worktrees today = ~3 rebuilds of identical deps) | Per-worktree `target/` is the CLAUDE.md disk rule, but a shared sccache would keep the isolation while killing the redundancy. Measure sccache hit-rate on this workload before adopting. |

## Consequences already tied to the above

- Production `symforge admin`/`setup` start-on-demand carries a 60 s deadline
  covering a full cold index+publication+runtime build (`ADMIN_SERVE_START_DEADLINE`,
  `src/cli/admin.rs`). A large cold monorepo can exceed it for real operators
  (Kimi's b5776c5 commit note). Items 1-4 attack this at the root; if they land
  well, the deadline stops being scary and needs no separate fix.

## Token efficiency — fair game per owner ruling 2026-08-03

The measuring instrument already exists: the per-session efficiency tracker
(health's "Token Savings" / "Session Efficiency" sections, baselined against
competent-manual windowed reads). A token campaign that cannot cite those
counters, or a byte measurement of the surface in question, does not start.

| # | target | status | notes |
|---|--------|--------|-------|
| 7 | **`tools/list` schema payload** (39-tool full surface) | unmeasured | Paid once per MCP session by EVERY client. Measure serialized bytes of the full surface vs compact-3; candidates: tighter descriptions, deduplicated shared param docs, schema `$ref` reuse. The compact surface exists — the question is what the DEFAULT surface costs and what's shaveable without losing routing quality. |
| 8 | **`health` / `status` output size** | unmeasured (visibly large) | The full health dump is the single biggest tool response in routine use. `health_compact` exists; measure how often the full form is actually needed vs habit, and whether sections can be demand-driven. |
| 9 | **Hook-injected context** (PostToolUse impact notes, prompt-context signals, session-start recall) | unmeasured | Fires on every tool call / prompt in every Claude session — small × thousands/day. Measure injected bytes per hook type per day from a real session log; candidates: suppress no-signal injections (e.g. "Prompt-context signal: none" lines still cost tokens). |
| 10 | **Top-tool response formats** (`search_text`, `get_file_context`, `get_repo_map` at high detail) | partially measured (per-session savings counters exist) | The savings counters prove wins vs raw reads; the open question is headroom WITHIN the formats — evidence lines, scope banners, and truncation notices repeated per response. Measure the fixed-overhead bytes per response shape. |
| 11 | **Knowledge bridge card text** | unmeasured | 5,384 cards feed both memory (item 1's startup cost) and query-time response size. A leaner card representation pays twice. Couple with item 1's campaign. |

## Candidates — NOT yet measured (measure first, then promote)

| target | suspicion | how to measure |
|--------|-----------|----------------|
| Git temporal / coupling cold build | health reports ~1.7 s for 500 commits/90d; coupling is lazy-on-request already | time `git_temporal` init on a large-history repo |
| Hook latency (`symforge hook` per PostToolUse/UserPromptSubmit call) | fires on every tool call in every Claude session; even 100 ms × thousands of calls/day is real | time 100 sequential hook invocations |
| `search_text`/`search_symbols` p95 on large repos | no complaints, no data | benchmark battery against a big corpus |
| Snapshot load vs fresh load crossover | snapshot restore exists (`load_snapshot`); when is it actually used, and is it faster than fresh load post-#488 timers? | compare the two paths' phase logs |

## Separate campaign — rmcp 2.2 → 3.0 migration (owner: "dearly needed")

Not an optimization but a WANTED migration (owner ruling 2026-08-03), tracked
here so it is not lost: MCP spec 2026-07-28 — sessionless Streamable HTTP,
stateless lifecycle with `server/discover`, MRTR result enums across the
manual `ServerHandler` impl (14 files / 76 use-sites), MSRV 1.88. Upstream
migration guide: rust-sdk discussion #969; note 3.1.0 is already out, so the
migration should target the latest 3.x, not 3.0.1 blindly. Needs its own
spec'd campaign; do NOT let dependabot #492 auto-merge — the bump without the
migration is a broken build or, worse, silently changed transport semantics.

## Protocol for each campaign (learned this week, binding)

1. Metric first: the phase logs from #488 are the instrument; a campaign that
   cannot point at a log line or benchmark number does not start.
2. Iterate in debug, CLAIM in release — LTO reshapes profiles.
3. Behavior equivalence is part of the acceptance criteria: full suite,
   tripwire, golden replay. For item 1, published-data byte-equivalence.
4. One campaign, one worktree, one PR; cleanup on merge (targets + worktree
   + branch both ends).

## Research result — serve snapshot restore (as_of 2026-08-04, /autoresearch pass)

Measured with the installed release 8.17.0 on this repo (916 files, snapshot
`.symforge/index.bin` 20 MB):

- **Local stdio ALREADY restores**: `load_source=SnapshotRestore`, sidecar
  listening ~3 s after process start; the ~2 min to a "Ready" status label is
  the BACKGROUND snapshot verification (`snapshot_verify_state=Pending` →
  verified), not the load — tools serve during it with honest trust labels.
- **`symforge serve` NEVER calls `load_snapshot`** (`src/server/serve.rs` has
  no `persist::` call site) — every serve start pays the full cold pipeline
  (release: parse 4.60 s + runtime 3.59 s + publication 2.67 s + trigram
  0.61 s ≈ 9.9 s to listening).
- **Verdict**: wiring `persist::load_snapshot` (staleness-gated, the exact
  daemon/stdio path) into serve startup converts ~9.9 s cold to ~3 s warm and
  removes the 60 s-deadline fragility; the machinery, staleness checks, and
  verify-in-background flow all exist and are proven on the stdio path. This
  is the item-8/item-9 lever; parse-parallelism/mmap work only pays on the
  genuinely-cold first index of a project.
- Next: implementation speckit (spec 026) per the item-by-item protocol.
