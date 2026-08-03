# Security finding — unguarded raw-read lanes in `tools.rs` — 2026-07-28

**Found by:** main loop, while verifying Codex's Q2 cleanup item 4
(`CODEX-FOLLOWUP-ANSWERS-2026-07-28-d4e82b.md:166`).

**Status of verification:** source-verified by exhaustive read/grep. **Not executed** —
`target/` is cold and no live probe was run. Every claim below is a code-reading claim
and is labelled as such. Lanes 1 and 2 are asserted to be live defects in shipped code
(npm 8.16.6); that assertion rests on the absence of any guard, established by
enumerating *all* raw-read call sites in the file and *all* references to the
disposition types.

---

## The invariant at stake

Frozen Feature 020 contract: **a lexical/raw read must never touch a file excluded for
`SensitivePath` or `SensitiveContent`.**

## Why the absence proof is sound here

A guard would have to name the disposition it gates on. Two exhaustive greps:

- `SensitivePath|SensitiveContent|is_sensitive` in `src/protocol/tools.rs` → **zero**
  matches in non-test code.
- `disposition|admission|MetadataOnly|metadata_only|manifest_entry|catalog_entry` →
  matches only *diagnostic* helpers (`admission_tier_degradation_for_path` and
  friends, `:2433-2608`) that **explain** degradation after the fact. None of them
  gates a read; `admission_degradation_view_from_disk:2454-2496` in fact reads a disk
  sample itself.
- `std::fs::read|fs::read_to_string` → three non-test lanes: `:2783`, `:8605`, `:8707`.

So: three raw-read lanes, zero disposition guards.

---

## Lane 1 — `get_file_content` not-in-index fallback — **LIVE BREACH**

**Where:** `src/protocol/tools.rs:8588-8628` (the `None =>` arm), reached from
`get_file_content` at `:8442`.

**Mechanism.** The indexed lookup is
`generation.live.capture_shared_file_for_scope(&options.path_scope)` (`:8539-8541`).
A `SensitiveContent`-demoted file is **absent** from that map — confirmed at
`src/live_index/store.rs:2696`, where `publish_terminal_disposition_at_generation`
calls `live.remove_file(path)` for every disposition that does not retain last-valid
content. A `SensitivePath` file was never admitted at all. Both therefore return
`None` and fall into the fallback arm, which performs:

1. `capture_repo_root()`
2. `edit::safe_repo_path(&root, &input.path)` — **containment only.** Verified at
   `src/protocol/edit.rs:22-44`: parent-traversal rejection plus
   `canon_path.starts_with(&canon_root)`. No sensitivity check.
3. `canon_path.is_file()`
4. `std::fs::read(&canon_path)` → `format::render_file_content_bytes` → returned to
   the caller.

**Consequence.** Full raw content of a security-excluded file is returned through the
code-intelligence surface. The comment at `:8589-8590` states the intent — "try raw
disk read for non-source files (Cargo.toml, package.json, workflow YAMLs, etc.)" — and
that intent is exactly the SF-DOG-001 confusion: *absence from the index is overloaded*
across ~11 unrelated reasons, and here one of those reasons is "we deliberately refused
to hold this file's bytes."

The same fallback shape is repeated in the estimate branch at `:8479-8495`, which
reports a size for the same files.

**Owner question, separate from the defect.** Whether the invariant *should* bind when a
human passes an exact path (`get_file_content(".env")`) is arguable. The defect is not
arguable: the implementation has no guard, so it cannot distinguish that case from an
agent sweeping a demoted source file. It fails open in both.

## Lane 2 — `validate_file_syntax` — **LIVE BREACH**

**Where:** `src/protocol/tools.rs:8696-8719`.

Identical guard set — `capture_repo_root`, `safe_repo_path`, `is_file` — then
`std::fs::read(&canon_path)` at `:8707`, then
`parsing::process_file_with_classification` and
`format::validate_file_syntax_result`. It parses a security-excluded file and reports
its syntax/symbol structure.

Lower severity than lane 1 (structure, not content) but the invariant says *never
touch*, and this touches, reads, and parses.

## Lane 3 — `tier2_reference_disclosure` — **currently safe, and safe for the wrong reason**

**Where:** `src/protocol/tools.rs:2744-2783` (fn `tier2_reference_disclosure`).

It sweeps demoted files for a caller-supplied `name` and reports which ones contain it
textually — a content oracle over metadata-only files. It is currently **not** a breach,
because its candidate filter is:

```rust
.filter(|f| f.reason() == Some(SkipReason::SizeThreshold))
```

### The ordering hazard nobody flagged

That filter is protective **only by accident.** SF-DOG-004 is the defect that the true
reason is erased and hardcoded to `SkipReason::UnsupportedLanguage`
(`src/live_index/store.rs:3780-3795`, and independently collapsed at `:3360-3366`).
Security-demoted files therefore do not carry `SizeThreshold` today and are excluded
from the sweep as a side effect of the dishonest label.

**Fixing reason-code honesty can arm this lane.** T062–T065 (honest reason codes) is the
plan's *earliest independent root* in Codex's causal order
(`CODEX-FOLLOWUP-ANSWERS:223`). If honest reasons land while this filter stays a
positive match on one enum variant — or worse, if anyone generalizes it to "all
metadata-only files" on the reasonable-sounding grounds that the sweep is useful — then
a caller-supplied string becomes a probe against files the policy excluded. Repeated
probes are a search oracle over content that was never allowed to be read.

**Required:** the filter must become an explicit **allowlist of non-security reasons**,
asserted by a test that fails if a security disposition ever enters the candidate set.
A denylist, or a single-variant equality check, will not survive the reason-code fix.

---

## Disposition for Feature 021

1. **Lanes 1 and 2 should be their own small PR, ahead of the re-draft.** They are live
   in shipped code, the fix is one shared pre-read admission veto, and they do not
   depend on any open question in 021. This is the same fix Codex prescribed for T105 —
   "a caller-side admission check before any filesystem fallback, not merely an
   `around_symbol`/`around_match` formatter rule" — applied to **three** lanes rather
   than one, and needed now rather than as plan work.
2. **The veto must be one shared helper**, consulted by every lane, with the sensitive
   dispositions RED-tested per lane. Three local checks will drift; the file already
   demonstrates how (`is_error_output` existed twice and one copy went unfixed in
   PR #479).
3. **Lane 3's allowlist inversion is an ordering constraint on T062–T065**, not a
   separate task. It belongs in the same phase, gated by the same test.
4. **The re-drafted plan must state that `tools.rs` has one serialized owner.** Codex
   already required this for ACH-02/WS5B/ACH-04 (`:229`); lanes 1–3 add three more
   edit sites in the same file, which strengthens rather than changes that requirement.

## What is not established

- No live probe. Nothing here was executed; `target/` is cold. The RED tests in
  disposition 2 are what would convert these from source-verified to proven.
- Lane 1's reachability for `SensitivePath` specifically assumes the excluded file
  exists inside the canonicalized repo root (true for `.env` at the root; not checked
  for every `SensitivePath` pattern).
- **`src/protocol/edit.rs` has seven non-test raw-read lanes** — `:355`, `:412`,
  `:482`, `:553`, `:2128`, `:2733`, `:3155` — which were **not** analysed. They are a
  different class (read-modify-write against an edit target, not retrieval), and whether
  the frozen invariant binds the *edit* surface at all is an owner question rather than a
  defect: an edit tool must read what it edits. Flagged because a diff/preview lane can
  echo content back, and because Cursor's `edit_plan.rs` co-ownership finding already
  touches this file. **Not a claim of breach — a claim of unexamined surface.**
- Sweeps of `src/cli/**` (config/hook/init reads) were dismissed as a different class:
  fixed internal config paths, not caller-supplied repo paths crossing the MCP boundary.
  `src/sidecar/port_file.rs` and `src/live_index/persist.rs` likewise read own-state
  files only.
