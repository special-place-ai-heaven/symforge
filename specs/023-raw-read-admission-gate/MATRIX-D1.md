# D1 pinning matrix — measured

Date: 2026-07-31 · Branch `fix/raw-read-admission-gate` @ `20b51c8` · nothing committed
Variants: **V0** = baseline `20b51c8` · **V1** = D1 (`,` added to `CONTINUATION`) · **V2** = V1 + D4 mate check (measured, then reverted — D4 is deferred).
Build spec: proposal v2 §6. Every placeholder capture ≥ 8 bytes (no vacuous rows). `finding_count` asserted, not just SENSITIVE/CLEAN.

## Row matrix (detector level, `scan_secret_bytes`)

| Row | Path | Shape (abstract) | V0 | V1 | V2 |
|---|---|---|---|---|---|
| C5a | .js | multi-declarator, line break after trailing comma | CLEAN | **SENSITIVE[1]** | SENSITIVE[1] |
| C5b | .js | same, one line (control) | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| C5d | .rs | multi-binding, line break after trailing comma | CLEAN | **SENSITIVE[1]** | SENSITIVE[1] |
| G10b | .rs | struct literal, sibling field value | CLEAN | SENSITIVE[1] — **accepted regression** | SENSITIVE[1] |
| D1-scan | .js | long consumed walk passing over a later independent match | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| P1 | .py | tuple: quoted placeholder, then literal | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| P4 | .py | tuple: interpolation placeholder, then literal | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| E2 | .py | unquoted placeholder, comma inside capture | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| K1 | .yaml | sibling named `apikey` — IS a keyword | SENSITIVE[2] | SENSITIVE[2] | SENSITIVE[2] |
| K2 | .yaml | sibling named `bearer` — walk-only coverage | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| K3 | .yaml | sibling named `credential` — walk-only | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| K4 | .yaml | sibling named `accesskey` — walk-only | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| F5 | .env | comma-joined list bound to one key | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| F3 | .env | one-comma evasion variant | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| F3ctl | .env | no-comma control | SENSITIVE[1] | SENSITIVE[1] | SENSITIVE[1] |
| Y1u | .yaml | flow map, unquoted `${…}` swallows the comma | SENSITIVE[1] | SENSITIVE[1] — accepted ceiling | SENSITIVE[1] |
| Y1q | .yaml | flow map, quoted `${…}` sibling | SENSITIVE[1] | SENSITIVE[1] — accepted ceiling | SENSITIVE[1] |
| A3 | .yaml | flow map, `note:` sibling (walk coverage for keyword gaps) | SENSITIVE[1] | SENSITIVE[1] — accepted ceiling | SENSITIVE[1] |
| B1 | .toml | bracketed array — 1-byte capture, never matches | CLEAN | CLEAN — stated blind spot | CLEAN |
| X3 † | .js | unquoted capture immediately before a fence | CLEAN | CLEAN | **SENSITIVE[1]** — D4 closes it |
| X1 † | .js | apostrophe inside a double-quoted placeholder literal | CLEAN | CLEAN | CLEAN — D4 does NOT close it |

† addendum rows beyond spec §6, included to size the D4 mate check for its deferred review.

Pre-existing oracle rows S20a–d / S21a–d (depth-1 arg comma, FO-2 concat): covered by the `oracle_` suite per variant — stayed green (demoted group) in all three.

## Per-variant suites

| Variant | `oracle_` suite | tripwire (`src/`+`tests/`, zero findings required) |
|---|---|---|
| V0 | 27 passed, 0 failed | green |
| V1 | 27 passed, 0 failed (G10b moved to the demoted group as the accepted D1 regression) | **1 finding, adjudicated** — see below; green on rerun |
| V2 | 27 passed, 0 failed | **1 finding, adjudicated as D4 cost** — see below; not shipping, no source change |

**V1 tripwire adjudication.** `src/server/serve.rs` — the struct field `pub api_key: Option<String>,` (capture `Option<String>,`) is not placeholder, so exemption B's walk runs; under D1 the trailing comma continues the walk off the field line into the next field's doc comment, whose backticked flag name (13-byte fenced run) reads as a payload. False positive of the accepted comma-walk class — the "payload" is a CLI flag in a comment. Fixed per Ruling 1 (source, not detector): removed the two backticks; the walk then dies at that line's end (trailing byte not a continuation). Green on rerun.

**V2 tripwire adjudication (D4 sizing).** `src/live_index/local_ref_scout.rs` — a Ruling-1 fixture byte-string contains `…=your_…_here`; the capture is placeholder by the `your_` prefix but was never quote-wrapped, so under D4 there is no mate and **no step**: the walk starts *at* the byte-string's closing quote instead of past it, and withdraws somewhere in the following test code. Under V0/V1 the step made the walk consume immediately. This is a **D4-introduced false positive on real source** — the mate check's cost class, measured: never-quoted captures ending before a quote.

## Acceptance (proposal §6) — all met at V1

- No row moves toward CLEAN: ✓ (only C5a/C5d/G10b move, all toward SENSITIVE)
- `finding_count` never drops: ✓ (K1 stable at 2; D1-scan stable at 1; all others stable)
- C5 rows flip to SENSITIVE: ✓
- Oracle green: ✓
- Tripwire green or individually adjudicated: ✓ (one adjudication, above)
- Full lib suite at V1: **3081 passed, 0 failed, 4 ignored** (726 s)

## Notes for the deferred D4 review (measured here)

- D4 **closes X3** (unquoted capture before a fence): CLEAN → SENSITIVE[1].
- D4 **does not close X1** (apostrophe parity inversion): the `'` arm's one-byte advance lands the walk in the same state the step used to produce.
- D4 **costs** a new false-positive class on real source (the local_ref_scout fixture above). The mate check is not free; its own review must weigh X3-class leaks against never-quoted-capture FPs.
- The D1-scan row pins the monotonicity proof's one undischarged assumption: a consumed walk does not advance the regex scanner's resume position past a later independent match (count stayed 1 in all variants).

## Permanent artifacts in the worktree (uncommitted, for the D1 commit)

- `src/knowledge/mod.rs` — D1: `,` added to `CONTINUATION` + comment rewrite (the exclusion rationale is reversed, `/` and `<`/`>` exclusions retained); `comma_continuation_d1_matrix_pins` test asserting every row above at V1, with exact `finding_count`; runtime-assembled keyword fragments per Ruling 1.
- `src/protocol/tools.rs` — oracle row G10b moved to the demoted group, re-labeled as the accepted D1 regression; comment rewritten.
- `src/server/serve.rs` — doc-comment backticks removed (tripwire adjudication, above).

Verified final state: worktree contains exactly these changes on `20b51c8`; scratch reporter and verdict file deleted.
