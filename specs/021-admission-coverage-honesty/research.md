# Phase 0 Research: Admission Coverage Honesty

**Feature**: `specs/021-admission-coverage-honesty/` | **Date**: 2026-07-28

This file exists because the investigation produced **measured** evidence that (a) contradicts an
elimination recorded in the prerequisite slice, and (b) is too specific to fit in a plan. Everything
here was measured against `E:/project/symforge` with the live index confirmed bound
(`status` → `project_root: //?/E:/project/symforge`, `index_ready: true`, `index_files: 891`,
`index_symbols: 25050`).

Disk gate observed throughout: `df -h /e` reported 44 GB free before and 54 GB free after. No
`cargo build --release` and no `cargo test --all-targets` were run inside the repository during the
investigation. `git status --porcelain` was empty.

---

## 1. The demotion root cause — MEASURED, and it invalidates `T066`'s elimination list

### What was done

The secret detector was extracted **verbatim** from `src/knowledge/mod.rs:31-207`
(`compile_secret_rules`, `is_placeholder`, `contains_ascii_case_insensitive`, `scan_secret_bytes`,
plus `SECRET_SCAN_MAX_BYTES = METADATA_ONLY_CODE_BYTES = 4 MiB` from `src/domain/index.rs:1482`) into
a standalone scratchpad crate depending on the same `regex = "1.11"`, and run against the real files.
The repository tree was never modified.

### Result

```text
SECRET_POLICY_VERSION=1 SECRET_SCAN_MAX_BYTES=4194304

src/protocol/knowledge_search.rs  (82750 bytes)
  => Sensitive -> MetadataOnlyReason::SensitiveContent { rule_ids: ["secret.context-assignment"], finding_count: 2 }
     match secret.context-assignment @ line 1328: "token = token.to_lowercase();"
     match secret.context-assignment @ line 2138: "token={canary}"

src/knowledge/mod.rs  (21751 bytes)
  => Sensitive -> ... finding_count: 2   (canary fixtures @ 531, 555)

src/live_index/knowledge_authority.rs  (99678 bytes)
  => Clean -> StableContentAdmission::Admitted

src/live_index/store.rs  (316761 bytes)
  => Sensitive -> ... finding_count: 3   (canary fixtures @ 7543/7544/7589)

src/protocol/tools.rs  (1293416 bytes)
  => Sensitive -> ... finding_count: 5
     match ... @ line 15927: "token = Arc::clone(&watcher.stop_token);"

src/protocol/format.rs  (256408 bytes)
  => Clean -> StableContentAdmission::Admitted
```

**Zero real secrets among all 29 findings measured.**
`src/protocol/knowledge_search.rs:1326-1330` is an ordinary tokenizer loop:
`let token = token.to_lowercase();`. `src/protocol/tools.rs:15927` is
`let original_stop_token = Arc::clone(&watcher.stop_token);` — a cancellation token.

### Behavioral confirmation — 29/29, no misses in either direction

The probe predicts 19 of 167 `src/**/*.rs` files `Sensitive`. Cross-checked against the live index:

- **All 19 predicted-Sensitive are ABSENT from the index.** `get_symbol` batch over 14 of them
  (`src/cli/admin.rs`, `src/cli/serve.rs`, `src/daemon.rs`,
  `src/live_index/{knowledge_bridge,local_ref_scout,query,rank_signals}.rs`, `src/parsing/mod.rs`,
  `src/protocol/{prompts,tools}.rs`, `src/server/{admin/api_v1,auth}.rs`,
  `src/stel/edit_planner.rs`, `src/watcher/mod.rs`) returned `File not found` for every one.
  `search_symbols("AnalyticsMode")` (defined in `src/analytics/store.rs`) → no symbol, text-path
  fallback only. `get_file_context` → "Tier 2 (metadata only) — reason: unsupported language,
  size 21 KB" for `src/knowledge/mod.rs`.
- **All 10 predicted-Clean controls ARE indexed** and returned real bodies: `src/worktree.rs`,
  `src/version_registry.rs`, `src/stel_core/{ledger_store,calibration}.rs`,
  `src/stel/{planner,status}.rs`, `src/protocol/{format,search_tools}.rs`,
  `src/live_index/knowledge_authority.rs`, `src/domain/index.rs`.

Including the counter-intuitive cases: 309 KB `store.rs` demoted, 97 KB `knowledge_authority.rs`
admitted, 1.26 MB `tools.rs` demoted while 250 KB `format.rs` is admitted. **Size does not explain
it.** A testpilot cross-check with the same binary confirms the same: a 10 KB spec and a 439 KB
service fire identically.

### The mechanism

`classify_stable_content(relative_path, targets, bytes)` is a **pure function of the file's own
bytes**. It is the single gate all three ingest paths share (`src/live_index/store.rs:3780` cold
load / full reindex, `src/watcher/mod.rs:493-494` incremental,
`src/live_index/local_ref_scout.rs:329` ref-blob), so the demotion reproduces identically and
deterministically on every reindex. It is **not** stateful — no ordering, budget, generation, or
circuit-breaker involvement.

The offending rule, `src/knowledge/mod.rs:89-95`:

```text
(?i)(?:api[_-]?key|secret|token|password|passwd|pwd|client[_-]?secret)[ \t]*[:=][ \t]*["']?([^\s"'#]{8,})
```

Two defects compound:

1. **No left word boundary.** The alternation matches *inside* any longer identifier: `stop_token`,
   `original_stop_token`, `csrfToken`, `cache_key`, `sort_key` all contain a keyword immediately
   followed by `=`/`:`.
2. **The value class `[^\s"'#]{8,}` is satisfied by ordinary code.** Any RHS expression with 8+
   non-space, non-quote, non-`#` characters qualifies: `token.to_lowercase();` (20 ch),
   `Arc::clone(&watcher.stop_token);` (31 ch), `page.locator(passwordSel).first();`,
   `Symbol(sessionId),`, `literal,`.

`is_placeholder` (`src/knowledge/mod.rs:130-157`) cannot save them: it whitelists a closed set of
literal words plus `${…}` and `{{…}}` only, so a single-brace format placeholder like
`token={canary}` — the repository's **own** detector test fixtures — and every code expression pass
through as real findings.

One finding anywhere ⇒ `SecretScan::Sensitive` ⇒ `MetadataOnlyReason::SensitiveContent`
(`src/knowledge/mod.rs:318-328`). The whole file's symbols are dropped and the owned byte buffer is
discarded before parsing. Full-file granularity, no per-range redaction.

Then the truth is thrown away twice:

- `src/live_index/store.rs:3780-3795` hardcodes `SkipReason::UnsupportedLanguage` into the
  `AdmissionDecision` while storing the true `FileDisposition::MetadataOnly { reason }`. The same
  hardcode appears on the `Unreadable`/`UnstableDuringRead` arms (~`:3742-3770`) and the
  missing-ingest-plan arm (~`:3673`).
- Independently, `compatibility_admission_decision` (`src/live_index/store.rs:3360-3366`) collapses
  `SensitiveContent` and six siblings back into `UnsupportedLanguage` — so even if the first hardcode
  were fixed, the reverse map still erases it. Verified in source; the round trip at `:3394-3405`
  maps `UnsupportedLanguage | SizeCeiling | None` → `UnsupportedTextEncoding`, so it is not even
  lossless in the other direction.
- `src/domain/index.rs:1424` renders it `"unsupported language"`;
  `src/protocol/format.rs:3676-3679` appends `", size {n} KB"`.

**That is exactly `SF-DOG-004`'s "conflates language support and file size", and exactly why the
size / parse-failure / CRLF eliminations all came back negative.**

### Two aggravating specifics worth naming

- The demotion is largely **self-inflicted**: the repository's own detector-test canaries
  (`password={canary}`, `token={canary}` in `src/knowledge/mod.rs`, `src/live_index/store.rs`,
  `src/protocol/tools.rs`) demote the very modules that implement admission.
- It has **hollowed out SymForge's self-view**: `src/protocol/tools.rs` (1.26 MB, the whole tool
  surface), `src/daemon.rs`, `src/live_index/query.rs`, `src/live_index/store.rs`,
  `src/watcher/mod.rs` are all invisible to `search_symbols`/`search_text`/`get_symbol` right now.
  `stable_read_with_retries` (`src/live_index/store.rs:445`) is unfindable via SymForge for exactly
  this reason.

### The amendment `T066` needs (task **T101**)

`specs/020-repository-knowledge-index/sift/tasks.md:231` records
*"secret-pattern content (zero detector literals in `knowledge_search.rs`)"* as a ruled-out cause.
**That elimination is invalid.** The rule needs no secret literal —
`let token = token.to_lowercase();` at `src/protocol/knowledge_search.rs:1328` is sufficient.
Whoever wrote it checked for secrets, not for the rule's actual match surface.

---

## 2. `SF-DOG-006` — the Tier-2 read path is a different renderer

`get_file_content` builds a full `ContentContext` (mode + `around_match`/`around_symbol`/
`chunk_index`) and then **forks on index membership**:

- Tier 1 → `format::file_content_from_indexed_file_with_context`
  (`src/protocol/format.rs:3061-3100`), which dispatches chunk (`:3065`), `around_symbol` (`:3078`),
  `around_match` (`:3088`), bytes (`:3099`).
- Tier 2 / non-indexed → `format::render_file_content_bytes` (`src/protocol/format.rs:3176-3225`)
  via `src/protocol/tools.rs:8607`.

`capture_shared_file_for_scope` (`src/protocol/tools.rs:8539-8541`) is Tier-1-only —
`src/live_index/query.rs:1259-1261` is `self.files.get(relative_path).cloned()`, and Tier-2 paths live
in `manifest_entries` (`query.rs:1243`), so a Tier-2 request takes the `None =>` arm at
`tools.rs:8588`. The handler's own comment at `tools.rs:8475-8477` confirms the routing.

`render_file_content_bytes` handles **only** `start_line`, `around_line` (`:3195`),
`show_line_numbers`, and `header` — verified by reading it. The other three selectors are silently
dropped, so execution reaches `:3210-3214`:

```rust
if !context.show_line_numbers && !context.header {
    return match (context.start_line, context.end_line) {
        (None, None) => content.into_owned(),
```

That is the whole file from line 1, then `cap_file_content_output` (`format.rs:3594`) truncates at
`GET_FILE_CONTENT_MAX_BYTES = 60_000` (`:3569`) with a footer that ironically advises
"use … around_match, or around_symbol to read a smaller window" (`:3602`).

`around_line` is the one mode implemented in **both** renderers — which is precisely why the ledger
reports `around_line` works and the other three do not.

**The structured refusals the ledger asks for already exist and are merely unreachable.**
`format.rs:3497-3526` already returns `not_found_file_match` on an empty candidate set (`:3508`) and
an occurrence-not-found message (`:3518-3522`). Both are gated behind `file: &IndexedFile`. Same
coupling on `render_numbered_chunk_excerpt` (`:3305`).

The misleading header is the `mode_annotation` prepended at `src/protocol/tools.rs:8618-8621` (built
at `:8532-8538` as `── mode: match (explicit) ──`). **Once the renderer honors or refuses the
selector, that annotation stops lying with no `tools.rs` edit** — which is what keeps `ACH-02` clear
of PR #479's footprint.

---

## 3. `SF-DOG-007` — one seam, two symptoms

`plan_edit` (`src/protocol/edit_plan.rs:32`) resolves a bare target as exact path → path-suffix match
→ symbol cascade. It computes the right predicate at `:90`:

```rust
let path_shaped = target.contains('/') || target.contains('.');
```

…and then **never consults it in the match at `:103`**. The `0 =>` arm at `:107` runs the symbol
cascade with the FULL PATH as the selector. The comment at `:104-106` states the fall-through as
deliberate intent, so it must be rewritten, not just bypassed.

The collapse is then mechanical: `collect_selector_hits` (`:233`) → `resolve_symbol_selector` →
`find_candidates_cascade` (`src/live_index/disambiguation.rs:319`) strategy 4 (`:362-367`) →
`strip_qualification` (`:438-442`), whose `rsplit_once('.')` on
`backend/src/modules/testing/services/generated-auth-global-setup.ts` yields exactly `ts`.

Both ledger halves share this one seam. `index.all_files()` (`src/live_index/query.rs:1233-1235`)
iterates `self.files` — **Tier 1 only**. So a VALID-but-Tier-2 path misses `exact` (`:84-86`) *and*
misses `suffix_matches` (`:91-99`), hits the same `0 =>` arm, and gets the same fuzzy `ts`.
Nonexistent path and valid-Tier-2 path are the same code path.

The prior fix (SF-AAP-001, `edit_plan.rs:68-83` + `tests/edit_plan_literal_path_precedence.rs`)
hardened only the case where the path **does** exist in Tier 1, and documents this exact hazard at
`:70-72` — but left the miss case open.

The fail-closed guard already has its data source: `LiveIndex::metadata_only_skipped_paths()`
(`src/live_index/query.rs:1243-1256`) is on the same `impl` `plan_edit` already borrows `all_files()`
from, and returns `(path, reason)` — enough to distinguish `metadata_only` from `file_not_found`
**without a new API**.

**`strip_qualification` must not be changed.** `rsplit_once('.')` → `ts` is the proximate cause, but
the behavior is correct for its intended input (`Type.Method`, `Foo::bar`); constraining it would
break Go/C++ selector resolution.

---

## 4. `SF-DOG-009` — a non-authoritative outcome rendered by an authoritative formatter

`ReindexResult::Skipped` is returned from ≥8 sites in `read_and_index_with_stable_read`, and **four
are not admission outcomes**: `src/watcher/mod.rs:358-359` (stale metadata-terminal admission
rejected), `:539` (stale hash-skip publication), `:566-573` (stale indexed-file publication
rejected), `:576-579` (abort after `MAX_PUBLICATION_ATTEMPTS` = 4). Scope/gitignore eviction also
returns it at `:303-314`.

Both impact entry points funnel that single variant into the policy renderer:
`src/sidecar/handlers.rs:958` (new-file) and `:1113` (edit) → `impact_skipped_text`.

`impact_skipped_text` (`src/sidecar/handlers.rs:886-939`, read and verified) then **default-fills the
facts it lacks**:

- `:901-904` — `view.reason.map(…).unwrap_or_else(|| "policy".to_string())`. **`reason: None` prints
  as `"policy"`.**
- `:918-923` — `"Not indexed: {path} is {tier_label} — reason: {reason}, size {size_mb:.1} MB"`,
  producing the literal Tier-1-and-not-indexed contradiction.
- `:905` — `view.size.unwrap_or(0) as f64 / (1024.0 * 1024.0)` at `{:.1}` ⇒ **every file under
  ~51 KB prints `0.0 MB`**, and `size` is populated (`health_view.rs:301` sets
  `Some(file.byte_len)`). It is a precision bug, not a missing size.
- `:926` computes `current_project_generation()` but only the non-parser branch (`:935`) prints it.
  The `has_code_parser` branch — the one that fired — omits it, so the caller cannot see the drift
  that caused the refusal.

**Proof the file WAS indexed**: `src/live_index/health_view.rs:297-306` — the `self.files.get(&path)`
branch returns exactly `tier: AdmissionTier::Normal, reason: None`. `Normal` → "Tier 1"
(`handlers.rs:896-900`); `None` → `"policy"`. The reported string is the byte-for-byte rendering of a
**successfully indexed** file that hit a publication race.

**Divergent oracles**: `capture_admission_tier_lookup_view`
(`src/live_index/health_view.rs:275-309`, read and verified) checks `manifest_entries` **first**
(`:280-296`, via `compatibility_admission_decision`) and only falls back to `files` (`:297`).
`get_file_context`'s parsed Tier 1 comes from the `files` record. So a path present in both renders
the manifest's exclusion from impact while context renders the parsed truth. One index, two admission
oracles, manifest wins in impact.

---

## 5. `SF-DOG-008` — the recovery is architecturally incapable of recovering

Untracked files are **not** intentionally excluded. `src/discovery/mod.rs:2192-2197` gates on
`SYMFORGE_EXCLUDE_UNTRACKED`, and `src/domain/index.rs:1392-1396` documents `SkipReason::Untracked`
as *"opt-in … (default OFF). Only minted when that env gate is explicitly enabled; the default
admission path never produces this reason"* — verified verbatim in source.

What `new_file=true` actually does:

- **Promise**, `src/sidecar/handlers.rs:800` — *"new_file=true (HOOK-06): Reads file from disk,
  parses it, indexes it."*
- **Reality**, `:855-859` → `handle_new_file_impact` → `:951`
  `crate::watcher::admit_and_index_single_path(...)`, and `src/watcher/mod.rs:277-284` shows that
  function is *literally* `read_and_index(relative_path, abs_path, shared, None, expected_gen)`.
  **There is no admission-override parameter anywhere in the signature.**
- The code says so itself, `handlers.rs:892-894` and `:921-922`: *"The admission gate applies to
  analyze_file_impact the same as bulk load and the watcher (no force-admit)."*

So the flag routes straight back into the gate that already refused the file.

Compounding it:

- `untracked_file_diagnostic` (`src/protocol/tools.rs:2191-2198`) mints
  `"To index the first match, call analyze_file_impact(\"…\", new_file=true)"` **unconditionally** —
  no admission consultation.
- `src/protocol/tools.rs:2184` tests only `guard.get_file(path).is_none()`. A deliberately Tier-2
  metadata-only file lives in the manifest, not in `files`, so it passes this test and is listed as
  recoverable when it is permanently excluded.
- Search already HAS the match and discards it. `matching_untracked_paths_for_search_text`
  (`:2410-2421`) reads each candidate via `repo.file_from_workdir(path)` and runs the real query
  through `untracked_text_matches` (`:2336-2365`, full regex/term matching) — then returns only
  `Vec<String>` of paths. `:3316-3323` computes it only when `result.files.is_empty()`, and `:3351`
  glues it on as prose. It never enters `result.files`. **Hence the unqualified zero.**
- The "unsupported language" mislabel on an ordinary `.ts` file is the §1 collapse at
  `store.rs:3360-3366`, not a separate defect. `sensitive_path_rule` (`src/knowledge/mod.rs:250-279`)
  is narrow and would not match that filename, so the path-rule route is excluded; content and
  encoding routes remain.

---

## 6. Index identity — reproduced live with two curl calls

During the investigation the PostToolUse hook injected **three mutually exclusive readings** while
work was confined to symforge:

```text
{"file_count":0,   "symbol_count":0,    "index_state":"Empty","uptime_secs":287}
{"file_count":2265,"symbol_count":76582,"index_state":"Ready","uptime_secs":31}   <- uptime RESET
{"file_count":891, "symbol_count":25050,"index_state":"Ready","uptime_secs":50}
```

Each mapped to a live listener: port 63032 → `0/Empty`; port 64700 → `2265/76582`; daemon 56651
sessions → `891/25050`.

**The decisive pair**, against port 64700, both carrying the `caller_root` the hook sends:

```text
GET /outline?path=Cargo.toml&caller_root=E%3A%2Fproject%2Fsymforge
  -> 409 "Sidecar index is rooted at E:\project\aap-rooms-019 but the caller is in
          E:/project/symforge — the shared session was likely retargeted…"

GET /health?caller_root=E%3A%2Fproject%2Fsymforge
  -> 200 {"file_count":2265,"symbol_count":76582,"index_state":"Ready","uptime_secs":592}
```

Same sidecar, same `caller_root`, same instant: the guard rejects `/outline` and waves `/health`
through. `2265/76582` is a **third** project (`aap-rooms-019`) — `T081`'s symptom exactly, with a
different foreign repository than the testpilot instance recorded in the task.

The code chain, all verified in source:

| Site | Defect |
|---|---|
| `src/cli/hook.rs:255` | `std::env::current_dir().unwrap_or_default()` — identity is bare CWD, silently empty on error |
| `src/cli/hook.rs:865-868`, `:885` | a Grep whose pattern is not `is_plausible_symbol_name`, and every unknown `tool_name`, route to `/health` — making it the hook's busiest endpoint |
| `src/cli/hook.rs:404`, `:893-901` | the hook DOES send a canonicalized `caller_root`. **The information is present.** |
| `src/sidecar/handlers.rs:344-346` | `if path != "/health" && path != "/stats" && …` — the guard is explicitly disabled for exactly those two paths |
| `src/sidecar/handlers.rs:88-93` | `HealthResponse { file_count, symbol_count, index_state, uptime_secs }` — no `project_root`, no `project_id`, no generation. Confirmed against the raw JSON above: nothing for a caller to check. |
| `src/sidecar/port_file.rs:337-348` / `handlers.rs:375-388` / `hook.rs:1083-1091` | three semantics for one question: no canonicalization / `dunce` strips `\\?\` / `fs::canonicalize` adds it. The daemon emits `//?/E:/project/symforge`; on-disk descriptors store `E:\\project\\symforge`. |
| `src/sidecar/port_file.rs:283-289` (+ `:161`, `:206`) | identity checked only `if let (Some(declared), Some(expected))` — a descriptor with `project_root: None` is accepted by every project |
| `src/sidecar/port_file.rs:495-505` | `read_sidecar_endpoint` falls through to `read_port_at(&dir)?` (`:501`) with **no** identity check, and accepts `selected.status.port` (`:496`) with no liveness probe |
| `src/cli/hook.rs:1041-1058` | daemon fallback picks `max_by_key(last_seen_at_unix_secs)` validated only against the registered `canonical_root` — an `index_folder` retarget is invisible here |
| `src/sidecar/port_file.rs:23`, `:62-64`, `:234-241` | the reader resolves `<control_state>/sidecar/sessions`, which does not exist; 22 descriptors sit orphaned in the legacy `<control_state>/sessions`, all 7 distinct ports dead. `cleanup_stale_descriptors_at` is only on the update/repair path, never the read path. |

**The daemon-routed path is clean** — all four symforge sessions report `891/25050`. The divergence
comes entirely from unregistered local sidecars reached via the descriptor/legacy-port path. That is
why §Assumptions in [spec.md](spec.md) binds every measurement in this feature to the MCP `status`
route.

---

## 7. Open questions, with owners

Findings whose cause is **undetermined** get an investigation task with a recorded output **before**
any fix task. Three qualify:

| # | Question | Owner task | Why it must be settled first |
|---|---|---|---|
| Q1 | Which `MetadataOnlyReason` actually demoted testpilot's `generated-auth-global-setup.ts`? Not indexed (out of scope). `sensitive_path_rule` (`src/knowledge/mod.rs:250-279`) is narrow and would not match that filename, so the candidates are `SensitiveContent` (a generated auth-setup file plausibly embeds a bearer token → `secret.authorization-header`, `src/knowledge/mod.rs:76-78`) or `UnsupportedTextEncoding`. | **T143** | The reason-collapse fix holds regardless, but `ACH-04`'s acceptance check requires the receipt and the search response to name the **same** reason — which needs the real reason known. |
| Q2 | Did the `exact-origin-proxy.ts` case take the `manifest` branch or the `files` branch of `capture_admission_tier_lookup_view`? Best reading: the **files** branch (`Tier 1 — reason: policy` comes out of the `None → "policy"` default; the manifest branch could produce it only for a `MetadataOnly`/`HardSkip` entry, which would render Tier 2/3 instead), meaning the file was indexed and the `Skipped` came from a publication race. Also unpinned: which of the ~8 `Skipped` sites fired. | **T132** | Making `files` authoritative (FR-011) and adding a stale-generation variant (FR-009) are different fixes. Guessing wrong ships one and leaves the other. The `trace!` at `src/watcher/mod.rs:572` and the `warn!` at `:576-578` would name it; neither is visible in a normal run. |
| Q3 | Are the two unregistered sidecars (port 63032 `Empty`, port 64700 rooted at `aap-rooms-019`) orphaned from crashed sessions, or current sessions that failed to register with the daemon? | **T155** | This decides whether descriptor hygiene alone is sufficient (FR-025) or registration itself is unreliable — a materially larger fix. |

Further open items, resolved by decision rather than investigation:

| # | Question | Owner task |
|---|---|---|
| D1 | **Full-file demotion vs. per-range suppression.** Today one finding kills a whole file's symbols. Even with a tightened rule, a file containing one genuine secret still loses all its code intelligence. FR-023/`T070` freezes "security dispositions must never be lexically read" — does that also forbid indexing the file's *other* symbols? | **T104** (owner ruling, recorded) |
| D2 | **`around_symbol` on a Tier-2 file**: explicit refusal, or degrade to a text search for the symbol name (i.e. treat it as `around_match`)? The ledger demands "a structured refusal instead of unrelated content" but does not forbid the substitute. Refusal is the fail-closed default; the substitute silently changes mode semantics and therefore needs an owner. | **T105** (owner ruling, recorded) |
| D3 | **`file_not_found` vs. a first-class `new_file` plan** for a path-shaped target that resolves to nothing. The ledger accepts either. Cheapest correct answer: `file_not_found` plus the `analyze_file_impact(new_file=true)` pointer the ledger itself names. | **T115** (decide in flight; record in the task receipt) |
| D4 | **Does `render_file_content_bytes` have callers beyond `tools.rs:8607` and `format.rs:3099`?** (e.g. `file_content_view` at `format.rs:3110`, the resources render path). Two hot callers were traced; the full set was not enumerated. Adding branches **without** changing the signature is the safer, lazier option. | **T122** (enumerate before touching the signature) |
| D5 | **Blast radius beyond Rust is unmeasured.** Only `src/**/*.rs` was enumerated (19/167 demoted). The full index is 891 files including Markdown, TOML, JSON, YAML, and `tests/`. The true count of files this rule silently withholds is higher and unquantified. | **T102** (measure; becomes the SC-015 baseline) |
| D6 | **One ledger file disagrees**: `backend/src/modules/testing/services/generator.service.ts` scans **CLEAN** under the verbatim detector (22912 bytes today vs "~28 KB" in the 2026-07-27 ledger). Either the file changed since the reproduction, or that single `SF-DOG-004` entry has a different cause. | **T103** (one `get_file_context` against a testpilot-bound index before assuming the fix covers it) |
| D7 | **`SF-DOG-001` may partly be this same root cause.** `stable_read_with_retries` (`src/live_index/store.rs:445`) is unfindable via `search_symbols`/`search_text` purely because its file is demoted. Once the 19 files return to Tier 1, the `SF-DOG-001` reproductions must be re-run — some "search silently misses" reports may collapse into the demotion rather than being separate defects. | **T168** |

## Constraint recorded for the implementer

`src/protocol/tools.rs` is **both** a demoted file **and** inside PR #479's footprint, so its canary
strings at lines 21385/21420 cannot be touched on this branch until #479 lands. The rule fix in
`src/knowledge/mod.rs` restores it **without editing it** — which is an additional argument for
fixing the rule rather than the fixtures.
