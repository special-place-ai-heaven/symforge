# Adversarial review request — symforge `health` output economics (backlog item #8)

**Requested of:** an independent reviewer (Codex CLI) working directly against the
repository.
**Requested by:** the Claude session that ran the 2026-08-05 optimization campaign.
**Date:** 2026-08-05. **Repo:** `E:\project\symforge`, branch `main`.

Read this file end to end before touching anything. It is written to be
self-contained: you are not expected to have any prior context from the session
that produced it.

---

## 0. Why you are being asked

Backlog item #8 is *"Shrink `health` / `status` output tokens."* A measurement
pass produced numbers, a verification pass checked them, and I then made a
judgement call and **closed the item without shipping any change**.

That judgement is the thing under review. I may be wrong in either direction:

- wrong to close it (there are safe wins I dismissed too readily), or
- wrong in *how* I framed what remains (my replacement framing already failed
  one test — see §4).

During this campaign, **seven separate claims did not survive contact with the
code** — two from the backlog, three from measurement subagents, one from a
change I shipped and then reverted on the evidence, and one from my own
reasoning. That track record is why #8 is being sent out for an independent
look rather than accepted.

**Do not try to agree with me.** Your value here is refutation.

---

## 1. What `health` is

`health` is a symforge MCP tool: an operator/agent diagnostic that reports index
status, file/symbol counts, project + session identity, load time, watcher
state, token-savings telemetry, hook adoption metrics, git-temporal status, a
parse/span quarantine registry, and repository-knowledge (Feature 020) state.

Related surfaces:

- `health_compact` — an existing, deliberately smaller variant.
- `status` — trust envelope + index health summary; has `detail` levels
  (`compact` (default), `full`, `projects`).

Primary renderer: `src/protocol/format.rs::health_report_from_stats_windowed`
(~lines 2533-2807).

---

## 2. How to reproduce every number (do this yourself)

Do **not** trust the figures in §3. Re-derive them.

The installed release binary speaks MCP JSON-RPC over stdio when invoked with no
subcommand:

```
C:\Users\rakovnik\.npm-global\node_modules\symforge-windows-x64\bin\symforge.exe
```

Run it with `cwd = E:\project\symforge` and env
`SYMFORGE_WORKSPACE_ROOT=E:\project\symforge`. Do **not** set `SYMFORGE_SURFACE`
(that selects an opt-in 3-tool surface; `health` is on the default 39-tool one).

Protocol, one JSON object per line on stdin:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2026-07-28","capabilities":{},"clientInfo":{"name":"review","version":"1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}
{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"health","arguments":{}}}
```

Measure three different things and keep them distinct — conflating them is the
single easiest way to get this wrong:

| figure | how |
|---|---|
| **text bytes** | UTF-8 length of the concatenated `result.content[].text` |
| **result bytes** | minified JSON of the whole `result` object |
| **wire bytes** | the raw stdout line |

Working reference scripts (read them, don't assume they're right):

- `<scratch>/verify_health.py` — health / health_compact / status, two sessions
- `<scratch>/verify_health_growth.py` — health before vs after 8 varied tool calls

where `<scratch>` is
`C:\Users\rakovnik\AppData\Local\Temp\claude\E--project-symforge\dd98000a-d58d-438a-bad6-d4e5ae466f7d\scratchpad`.

Binary version at time of writing: **8.22.0**. `main` has since advanced
(PRs #510, #513, #515, and possibly #517); none of those touch health rendering,
but **verify that assumption** rather than taking it from me — building `main`
and re-measuring is a legitimate part of this review.

---

## 3. Measurements

### 3a. Verified by me, first-hand, this session

| surface | text bytes | lines |
|---|---|---|
| `health` | **5,950** | 71 |
| `health_compact` | **1,735** | 13 |
| `status` (default) | **455** | 19 |

`health` / `health_compact` = **3.43×**. `health` / `status` = **13.08×**.

Two independent fresh processes running the identical call sequence produced
**byte-identical** output (delta +0 on all three).

### 3b. From the earlier measurement pass — treat as UNVERIFIED

Its verifier reproduced ~90% of the structure byte-exactly but found the
headline stale. **My own re-measurement disagrees with its headline too**
(5,950 B vs its 6,255 B; 3.43× vs its 3.52×). Section attribution below is
therefore indicative only:

| section | bytes | share |
|---|---|---|
| parse/span quarantine registry | 1,416 | 22.6% |
| repository knowledge (Feature 020) | 1,123 | 18.0% |
| hook adoption | 724 | 11.6% |
| runtime identity line | 397 | 6.3% |
| git temporal | 378 | 6.0% |
| session efficiency + per-tool breakdown | 359 | 5.7% |
| partial parse summary | 258 | 4.1% |
| parse-resilience advisory (constant prose) | 245 | 3.9% |
| **index core (Status/Files/Symbols/Loaded in)** | **101** | **1.6%** |
| `_meta symforge/project_evidence` (on EVERY tool response) | 294 | — |

If that attribution holds, the arresting fact is the last-but-one row: **the
actual index-health answer is ~1.6% of the `health` response.**

### 3c. An unexplained variance — please chase this

Across my own two scripts, in the same repo, same binary, same session-start
conditions, `health` measured:

- **5,950 B / 71 lines** (script A)
- **6,208 B / 80 lines** (script B)

258 bytes and **9 lines** apart. Within each script the output was perfectly
deterministic (+0 across repeats). I did **not** isolate the cause. Candidates I
did not eliminate: differing tool-call sequence before the `health` call, a
daemon being alive vs not, watcher/generation state, quarantine registry
contents, or knowledge-publication timing.

**This matters more than any byte count.** If `health` output varies by ~4%
between runs for reasons nobody can name, then every byte target for this item
is being set against a moving baseline, and any "we saved N bytes" claim is
unfalsifiable. Identifying the cause is the highest-value thing you could do.

---

## 4. The claims to attack

### CLAIM 1 — "#8 has no genuinely free wins." (mine; the basis for closing it)

Sub-claims:

**1a. Suppressing zero-valued counters is not free.**
`src/protocol/format.rs::header_counts` (~lines 364-386) always emits 12 fields,
including 7 category counters that were zero in the observed run. Its own doc
comment states it *"Always reports the TRUE category totals … so the header
never overstates what is on screen."* It is a machine-parseable line and
~30 references in `tests/health_parse_quarantine.rs` assert it. I judged
suppression to be a change to a documented guarantee, for ~250 B (~4%).

*Attack surface:* is "absence ⇒ zero" actually a weaker contract than printing
`=0`? Is the doc comment describing a real consumer requirement or just
intent? Does any consumer parse that line positionally?

**1b. Removing `runtime_state=` is not free.**
`format_runtime_status` (`format.rs:597-613`) emits both `mode={}` and
`runtime_state={}` filled from *the same expression* (`status.mode.label()`).
`RuntimeStatus` has no distinct runtime-state field, so it is pure duplication.
`format_runtime_status_compact` omits it entirely. **But**
`src/protocol/tools.rs:19280` asserts `full.contains("runtime_state=daemon_reused_session")`
and `:12060` fixtures it — so it is a deliberate full-vs-compact distinction.

*Attack surface:* is a test asserting a field's presence the same as a
contract? Is the right fix to delete it, or to make it carry real distinct
state (i.e. is the duplication a latent bug where the wrong expression was
passed)?

**1c. The big levers are design decisions, not cleanups.**
Demand-driven quarantine entry list (~1,141 B), rendering the Feature-020
knowledge block via the existing `format_repository_knowledge_health_compact`
(~870 B), moving session telemetry to `status` (~525 B), dropping three constant
advisory strings (~479 B).

*Attack surface:* is any of these actually safe and uncontroversial? Is the
"advisory prose re-sent verbatim on every call" one a genuine freebie I
wrongly lumped in with the risky ones?

### CLAIM 2 — "The real issue is unbounded growth." (mine; **already falsified once**)

I replaced the byte framing with: *health output grows with session length, so
the problem is unbounded growth rather than absolute size.*

**I tested this and it did not reproduce.** Same process, `health` called before
and after 8 varied tool calls (`search_text`, `search_symbols`, `get_repo_map`,
`search_files`, `conventions`, `get_file_context`, `find_references`,
`what_changed`):

```
health BEFORE 8 varied tool calls : 6208 B (80 lines)
health AFTER  8 varied tool calls : 6208 B (80 lines)
delta                             :   +0 B
```

*Attack surface:* is there a real growth mechanism I failed to trigger (more
distinct tools? edits? a daemon session? longer uptime? multiple projects?), or
is the growth claim simply false? Note the per-tool breakdown supposedly scales
with *distinct tools used* (3 observed of 39 possible) — 8 distinct tools should
have moved it and did not. Either the section does not work as described, or my
test did not reach it.

### CLAIM 3 — the surrounding verdicts (lower priority, but fair game)

- `health_compact` already exists, so the fix for "health is too big" may simply
  be *use the compact tool*, making #8 a documentation/routing issue rather than
  a rendering one. I did not seriously evaluate this.
- The 294 B `_meta symforge/project_evidence` block rides on **every** tool
  response, not just health. If it is redundant with content already in the
  response body, it is a far larger aggregate win than anything inside health.
  I did not investigate it.

---

## 5. Constraints you must respect

These are real and were verified during the campaign:

1. **Honesty specs are binding.** `specs/010-v8-trust-remediation` and
   `specs/021-admission-coverage-honesty` own the trust/coverage claims. A
   change that makes a degraded, stale, partial or truncated state *quieter* is
   a regression, not an optimization — regardless of bytes saved. A closely
   related item (#10) was closed for exactly this reason: its proposed "always
   use the compact envelope" would have hidden degraded results.
2. **`SF-STRESS-011` wording is deliberate.** `src/protocol/tools.rs:1058-1063`
   documents that "filter active (heuristic)" replaced "vendor filtered"
   because the latter overstated a heuristic path filter as a guaranteed
   outcome. Do not propose shortenings that re-introduce overstatement.
3. **Test blast radius is real.** ~34 literal references to health section
   headers/prefixes across 5 files, including 30 in
   `tests/health_parse_quarantine.rs` and 24 in `capability_status_integration.rs`.
4. **Verification gates** (repo `CLAUDE.md`): `cargo fmt --check`,
   `cargo clippy --all-targets -- -D warnings`,
   `cargo test --all-targets -- --test-threads=1`, `cargo build --release`.
   The full serial suite takes ~16-19 min. The CI `rust` job is ~24 min.
5. **Do not add `Swatinem/rust-cache`** to CI. It was tried this session,
   measured slower, and reverted (see `CLAUDE.md` → CI Gates).

---

## 6. Deliverable

A single markdown file: `specs/024-optimization-backlog/HEALTH-REVIEW-FINDINGS-codex.md`.

Structure it as:

### 6.1 Verdict on each claim
For CLAIM 1 (incl. 1a/1b/1c), CLAIM 2, CLAIM 3 — one of
**CONFIRMED / PARTIALLY WRONG / REFUTED**, each with the evidence that decided
it (file:line, or a command + its output). "I agree" without independent
evidence is worth nothing here.

### 6.2 The variance question (§3c)
Either identify the cause of the 5,950 B / 71-line vs 6,208 B / 80-line
difference, or state precisely what you ruled out and what remains open. If
`health` output is not deterministic given a stated set of preconditions, say so
plainly — that reframes the whole item.

### 6.3 Your own measurements
A table of what YOU measured, with the method, so a third party can re-run it.
Flag every place your numbers disagree with §3a.

### 6.4 A ranked recommendation table

| # | change | est. bytes | safety | contract risk | test churn | recommend? |
|---|---|---|---|---|---|---|

`safety` ∈ {free, judgment-call, costs-honesty}. Be strict: this campaign found
that *most* things labelled "free" were not. If your honest answer is
"close #8, ship nothing", say that — it is a perfectly good outcome and matches
my current position, but only if you reached it independently.

### 6.5 What you would do first
One concrete next action, sized, with its acceptance criterion.

---

## 7. Ground rules

- **Read the code before concluding.** Every wrong claim this campaign produced
  came from reasoning about behaviour instead of reading the implementation.
- **Measure, don't estimate.** Where you cannot measure, say "not measured".
- **Do not modify the repository.** No commits, no branches. This is a review;
  the deliverable is the findings file. (Writing that one file is the sole
  exception.)
- **Do not run `cargo` builds concurrently with other work** — a full suite is
  ~16-19 min and the machine may be busy. If you need a build, say so in your
  findings rather than assuming it ran.
- Distinguish clearly between *what you verified*, *what you inferred*, and
  *what you are guessing*. Mixing those three is the failure mode this brief
  exists to catch.
