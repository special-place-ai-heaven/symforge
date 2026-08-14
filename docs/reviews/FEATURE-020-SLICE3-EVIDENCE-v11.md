# Feature 020 Slice 3 evidence (T041–T052)

## Round-5 review and its repairs (PR 3)

Round 5 attacked the round-4 repairs: **1 confirmed BLOCKER, 2 confirmed
majors (one defect seen by both verifiers), 0 refuted, 4 minor/note**.

- **The blocker was mine and it was sharp:** round 4's switch from
  total-count to distinct-count allowlist asserts DELETED round 3's
  multiplicity bind — an exact duplicate of an allowlisted line (a second
  `#[path]` mount of the dark directory under an innocuous alias being the
  worst case) would be silently absorbed with every test green. Fixed by
  binding BOTH: `(total, distinct) == (N, N)`, so a duplicate and a
  masked deletion each fail. The compiler independently rejects the
  double-mount case (duplicate type identities), but duplicable STRING
  lines were live — mutation **M35** (a duplicated allowlisted delta
  line, which compiles) observed caught at `(9, 8) != (8, 8)`. This also
  discharges the same-text-new-site minor.
- **The alias arm took three attempts to be honest.** The round-4
  use-prefix test missed `pub(crate)`, tabs, and leading attributes
  (falsifying "any use-declaration"); a raw word-boundary replacement
  FLOODED on English prose in assert strings; a naive collapsed
  path-segment test glued `include as` into `includeas`. The landed form:
  `include` in path-segment position on the collapsed line (after `::`,
  `{`, or `,`), boundary-clear or followed by the glued `as` keyword —
  which every resolvable aliasing form must write at its first hop from
  the std/core root. Zero flags on the tree; mutation **M36**
  (attribute-prefixed, `pub(crate)`, tab- and space-riddled alias)
  observed caught.
- Minors folded: the dead bidi exclusions inside the collapse are gone
  (the outright flag owns them); the "LEXER'S whitespace set" comment
  reworded to what the code does; the register's scope note now covers
  `embedded.rs`'s Slice-2 publics; the round-4 alias bullet in this
  document carries its amendment.

## Gate results for the round-5 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 3 passed, 0 failed; zero flags on the tree |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| mutations | M35, M36 each observed caught; restored (M35's double-mount variant additionally rejected by the compiler itself) |

## Round-4 review and its repairs (PR 3)

Round 4 attacked the round-3 repairs: **3 confirmed majors, 1 refuted, 3
minor/note**. The refutation is the important one — the adversarial attack
on the structural prose rule itself (a doctest carrying a call edge on a
`///` line) was REFUTED, so the load-bearing full-line-comment guarantee
held its first direct assault. The three confirmed were all completeness
gaps in the splice TRIPWIRE and the register:

- **Lexer-whitespace gap:** `char::is_whitespace` is Unicode White_Space,
  but Rust lexes Pattern_White_Space, which additionally holds the
  U+200E/U+200F bidi marks — `include\u{200E}!(...)` was legal and survived
  the collapse. Fixed twice over: the collapse now removes the lexer's set,
  AND any line containing a bidi mark is flagged outright (they have no
  legitimate use in this source). Mutation **M33** observed caught.
- **Alias route:** `use std::include as inc;` then `inc!(...)` was a
  single-line ASCII splice with no matching spelling. [Amended after round
  5: this bullet's "any use-declaration naming `include`" claim was
  falsified — the use-PREFIX test missed `pub(crate)`, tabs, and leading
  attributes; the arm now flags `include` in path-segment position on the
  collapsed line, which every resolvable aliasing form must write.]
  Mutation **M34** observed caught.
- **Register:** `EmbedOperationReceipt`'s `Clone` added, and the entry
  gains an explicit scope note for the boundary's scaffolding items.
- The residual statement is REFRAMED to what it is: the splice sweep is a
  fail-closed tripwire over known spellings, never a completeness proof —
  the load-bearing darkness guarantee is the full-line-comment rule over
  everything living in `src/`. The allowlist coverage asserts now count
  DISTINCT (file, line) pairs, so a duplicate sighting cannot satisfy a
  coverage claim, and the prose-rule header no longer overstates what
  string literals do on comment lines.

## Gate results for the round-4 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 3 passed, 0 failed; zero new flags on the tree |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| mutations | M33, M34 each observed caught; restored |

## Round-3 review and its repairs (PR 3)

Round 3 verified the five round-2 repairs adversarially: **4 confirmed
majors, 0 refuted** — three proving the mid-line-comment lexer an arms race
(raw-string quote parity laundered an edge in the exact residual the header
called safe; the escaped-quote char literal `'\''` leaked its closing quote
and revived the polarity flip; `include ! (` spacing evaded the matcher),
and one showing the trim register still omitted the common derives. Fixes:

- **The lexer is GONE (C8 ruling, second arm).** Prose is now only a
  FULL-LINE comment — first non-whitespace bytes `//`, after which Rust
  permits no code on the line — so a real call edge structurally cannot be
  tolerated and there is no scanner left to be wrong. The whole tree passes
  with zero new flags, proving every legitimate prose mention was already a
  full-line comment. Mutation **M31** (the round-3 raw-string laundering
  line) observed FLAGGED.
- **The splice matcher judges whitespace-collapsed lines**, so spacing
  cannot dodge the named spellings; the residual statement now claims
  exactly the line-based scope (multi-line splits and concat-constructed
  arguments stated). Mutation **M32** (`include ! (`) observed caught.
- **The register is completed a second time:** the full
  `Debug`/`Clone`/`Copy`/`PartialEq`/`Eq`/`PartialOrd`/`Ord`/`Hash` sets on
  the three re-exported enums and `ServerExit`'s five derives — every one
  absent from the contract's closed impl list — now named.
- The surviving "generated by" phrase in this document's round-1 header
  corrected; the T051 body amendment rewritten for the lexer-free rule.

## Gate results for the round-3 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` (lexer-free sweeps) | 3 passed, 0 failed; zero new flags on the tree |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| mutations | M31, M32 each observed caught; restored |

## Round-2 review and its repairs (PR 3)

Round 2 re-verified every round-1 repair against its ruling with three
verifiers and adversarial adjudication: **5 confirmed majors, 0 refuted, 8
minor/note, docs-truth clean** — convergence from round 1's 18, every
confirmed item a conformance gap in the repair work itself. Fixes:

- **Probe-predicate correction (operator ruling, D-ledger below):** the
  round-1 evidence framed `any(test, feature = "server")` as discharging
  C3. Corrected everywhere: under the Slice 0 predicate rule that cfg is
  PRODUCTION — the probes ship in the published server binary and only the
  embed build sheds them; T051 proves no in-tree call edge, not absence
  from the published graph.
- **C3 register completed:** `OperationKind::ALL`/`kind_name()`,
  `RetryAdvice::ALL`, and `PartialOrd`/`Ord`/`Hash` on the three
  re-exported enums added to the receipt's trim list; the "full superset"
  sentence made true by making the register complete.
- **C8 scanner:** round 2 proved the char-literal quote could FLIP polarity
  and fabricate a comment start inside a real string — laundering, not just
  hiding. Char literals are now consumed; the doc states the surviving
  raw-string residual and its flag-direction bias. Mutation **M30** (the
  exact round-2 laundering line) observed FLAGGED.
- **C9 matcher:** `include! {` and `#[cfg_attr(..., path = ...)]` evaded
  the token gate. The sweep now takes a predicate — any `include!`
  regardless of delimiter, `#[path`, and attribute lines carrying
  `path =`/`path=` — with the header's claims narrowed to what the text
  scan observes. Mutations **M28/M29** (both evasion forms) observed
  caught by name.
- **Stale present-tense claims:** four round-1-falsified sentences in this
  document's T048/T051 body sections now carry bracketed amendments at the
  spot instead of contradicting the dispositions section above them.
- Minors folded: the C1 pin rejects tuple-struct evasion (the brace must
  open immediately) and scans for `pub ` as a token; the receipt no longer
  claims to be "generated by" the script (it describes the run; the script
  generates the JSON) and states the rerun's clean-tree self-poison rule;
  `run()`'s doc no longer names a public `ActivationPending` variant.

**D-ledger — activation precondition (operator ruling, verbatim intent):**
`any(test, feature = "server")` is production: `any(...)` is test-only only
when every disjunct is, so with `feature = "server"` in the default crate
every `*_for_test` method and `OperationReceipt::for_test` ships in the
published server binary; embed sheds them, and T051 only proves there is no
in-tree call edge, not that the methods are absent from the published
graph. Before the keyword flip, probes become
`all(test, feature = "server")` by moving the oracles that call them
in-crate, or they sit behind a dedicated non-server test feature.
`cfg(test)` on a `tests/` consumer will not compile; that is the whole
reason this leak exists.

## Gate results for the round-2 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean after one catch — collapsible-if in the new sweep prose branch, rewritten as a let-chain |
| `preventive_runtime_dark_v11` (hardened sweeps) | 3 passed, 0 failed |
| lib suite | 3168 passed, 0 failed (the hardened C1 pin included) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| mutations | M28, M29, M30 each observed caught; restored |
| full `--all-targets` suite | 0 failures on the round-1 tree (exit 0, 9m00s); round-2 changes touch tests, comments, and docs only — the three affected suites re-run green |

## Round-1 adversarial review and its repairs (PR 3)

Five refute-stance reviewers over the five PR-3 commits, every
blocker/major independently re-verified: **18 confirmed (1 blocker, 17
major), 0 refuted, 20 minor/note, no dimension clean** — all in territory
the machine gates structurally could not see. Full verbatim record:
`docs/reviews/REVIEW-FINDINGS-claude-fable-slice3-pr3-2026-08-14.md`. The
repairs landed as one commit under the operator's per-finding rulings; the
dispositions in brief:

- **C1 (blocker), fixed:** `ServerBootstrapError` was a public enum; the
  frozen contract pins an OPAQUE STRUCT. The third T043-class invention in
  `server_api.rs` alone — and the shape of the miss matters: the trait-level
  oracles all passed, because item kind and constructability are invisible
  to them. Corrected to a private-field struct; the item kind is now pinned
  by a source assertion whose needles are built at runtime so the pin
  cannot match its own string literals.
- **C2, fixed; D4 AMENDED by ruling:** `server_api` gains
  `#[cfg(feature = "server")]` — the frozen contract pins its availability
  `feature=server` and the embed-v11 projection excludes it, so "activation
  is one keyword" is now TRUE because the gate is already present. D4's
  "std-only so the embed build compiles it unused" sentence is amended;
  every "one keyword, ungated" claim rewritten (module doc, lib.rs, delta
  renderer + regenerated JSON, sweep pin comment). Embed lib gate drops to
  1332 accordingly — the module and its tests correctly shed.
- **C3, recorded + gated (register completed in round 2):** the
  public-member and derive superset is in the receipt's divergence register
  as the activation trim list — round 2 found the first version omitted the
  ungated enum members (`OperationKind::ALL`/`kind_name()`,
  `RetryAdvice::ALL`, and the `PartialOrd`/`Ord`/`Hash` derives on all
  three re-exported enums); the register now names them. Every `*_for_test`
  probe carries `#[cfg(any(test, feature = "server"))]` — and per the
  operator's correction this predicate is PRODUCTION under the Slice 0
  rule, not a discharge of C3: see the D-ledger's activation precondition
  below. Consequence found by the embed check and fixed:
  `GenerationIdentity`'s import in `runtime.rs` became probe-only and is
  gated with them (the CLAUDE.md embed-gate unused-import class, caught
  before commit this time).
- **C4, fixed + oracle:** a closed handle's `runtime_view` reports
  `Stopped` from the flag the handle owns, never `Loading`; asserted both
  ways in the boundary oracle.
- **C5, fixed per ruling (rename, do not hash):**
  `OperationReceipt::for_dark_refusal(kind)` replaces `for_test` on every
  production-shaped refusal lane; the canonical hash covers the kind alone
  because hashing arguments the lane never examined would claim a binding
  that did not happen — recorded in the receipt's register. The runtime's
  refusal helper also threads the ACTUAL operation kind per call site
  (grant refusals say RefreshSource, not AcquireRuntime).
- **C7, fixed with the ruled third word + one real wrap:**
  `verbatim-reexport` covers exactly the three contract-verbatim enums, and
  the delta oracle verifies the actual `pub use` in the module source —
  never the table's self-report. `SourceRuntimePhase` was NOT that word: a
  public field typed `runtime::SourceRuntimePhase` was a D12 path-identity
  leak, so the boundary now owns its own six-variant enum and the view uses
  it.
- **C8/C9/C10, sweeps hardened:** the comment rule is string-aware (a `//`
  inside a string literal no longer launders a call edge; the conservative
  char-literal misparse can only FLAG, never hide); every `include!` and
  `#[path]` in `src/` is on an exact fail-closed allowlist with the
  concat-splice residual STATED in the file header instead of the old
  "real call edges cannot pass" overclaim; the `server_api` sweep now
  covers the dark directory with its seven wrap-table string lines
  allowlisted individually, so a real dark→stub call edge cannot hide
  behind a directory exemption.
- **C11, rewritten:** the vacuous post-grant `Refreshing` assert became
  `permit_grant_is_itself_a_publication` — the grant must move the
  publication root to a fresh identity, which a side-band-state-only grant
  fails; the before-side-effects half is stated as unobservable until
  Slice 4's real side-effect lane exists.
- **C13, accepting pair added:** a second `begin_close` joins the terminal
  source and its report says `already_terminal == true`.
- **C14, fixed twice honestly:** the renderer now PERFORMS the exact-match
  subtraction it claims (`introduced_minus_live`, all 64 today) — a first
  repair keyed it on the top-level module and wrongly subtracted all 60
  embed item atoms because V10's `pub mod embed` exists; caught by reading
  the regen diff, corrected, and pinned by an independent recomputation in
  the oracle. The write-mode tautology is dead: a regeneration run asserts
  against the PRE-write content, so it fails while repairing and the
  opt-in-free rerun verifies.
- **C6/C12/C15/C16/C17/C18, harness honesty:** `--check` exits nonzero on
  any unmet gated expectation; diagnostics are `package_id`-attributed so a
  dependency error cannot masquerade as a case result; nine closed
  NEGATIVE cfg sentinels complement the positive six (M27 observed
  caught); worktree cleanliness is recorded and check-gated; recorded
  paths are sanitized; the machine artifact is committed as
  `docs/reviews/AAP-MIGRATION-RECEIPT-v11.json` with its executable
  `rerun_command` inside it.

**Minors triage (after the majors, not instead):** fixed in this chunk —
the census-parser whitespace divergence (both legs now split-whitespace
tolerant), contract-normative parameter names (`request`, `deadline` — the
underscore prefixes were rustdoc-visible name changes), the
write-then-compare tautology, the runtime refusal kinds, and the E0425
citation in the receipt (now cited from the artifact, not asserted).
Recorded, deliberately not changed: the in-band zero sentinels
(`source_version`/`observer_epoch` 0 — honest dark values, contract-shaped
fields; a typed absence is an activation-shape question), the `held_by`
evidence discard (D18: surfacing it would mint), `wait_for_test` returning
the internal report (T047's oracle shape, probe-gated), and the
runtime_dark test file's server-path imports (integration tests never build
under the embed gate). Each stays visible in the review findings document.

## Gate results for the repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| lib suite (server default) | 3168 passed, 0 failed (+1: the item-kind pin) |
| embed lib gate | 1332 passed, 0 failed — server_api correctly shed by the C2 gate |
| plain embed build (`cargo check --no-default-features --features embed`) | clean, after the probe-import gate fix |
| three oracle suites | 11 + 2 + 3 passed, 0 failed |
| export delta | regenerated; write-mode run FAILED on the pre-write content as designed, verify run clean; 64/64 atoms survive the exact-match subtraction |
| harness `--stage full --check` | one failure: worktree dirty (the docs being written); case results identical — 35/35 adapter expected-failures, positive compiles; the clean-tree rerun lands with the committed artifact |
| traceability checker | OK (78 requirements, 24 oracles, 13 categories) |
| mutations | M27 evaluator (nine negative sentinels + exit 1); M20–M26 remain as recorded |

## T051 — the call-edge proof (PR 3)

`tests/preventive_runtime_dark_v11.rs` exists now — its creation is T051's
own act, held back from every earlier chunk on purpose — and it turns the
darkness paragraph of `index_lifecycle/mod.rs` into two executing tests.

**The sweep rule is fail-closed.** A line naming the dark surface outside
its directory passes only as prose or as one of the exactly-two
mount-declaration lines in `src/live_index/mod.rs`. [Amended after rounds
1–3: the original "a real call edge cannot pass" was an overclaim, and two
successive mid-line-comment lexers each laundered an edge through some
literal form — string literals, char-literal polarity, raw-string quote
parity. Round 3 took the C8 ruling's second arm and DROPPED the mid-line
comment exception: prose is now only a FULL-LINE comment, after which Rust
permits no code on the line, so the tolerance rule has no lexer to be
wrong.] A string-literal or trailing-comment mention FAILS and forces a
human decision rather than being silently tolerated. The seven task-named ingress lanes (daemon, stdio,
serve, embed, snapshot, observer, mutation) are all `src/` production code,
so one sweep covers them; their roots are asserted to EXIST so a moved lane
cannot make the claim vacuously true, and the anti-vacuity asserts require
both mount lines seen, prose mentions actually tolerated, and >100 files
walked.

**The sibling assertion.** `server_api::run` staying uncalled is its own
test, not a substitute: the same sweep over `server_api`, with lib.rs's
`pub(crate) mod server_api;` pinned in its pub(crate) FORM so a premature
keyword flip drops it from the allowlist and fails the test — activation
updates the pin and the keyword in one deliberate change. [Amended after
round 1: the original version allowlisted only the lib.rs line and EXCLUDED
the dark directory by a transitivity argument; C10 ruled that exemption
away, so the sweep now covers the dark directory with its seven wrap-table
string lines allowlisted individually, eight allowlist entries
count-pinned.]

**Mutation ledger.** M24 (planted
`use crate::live_index::index_lifecycle::registry::ProjectKey;` in
`src/embed.rs` → caught, named `src/embed.rs:34` verbatim), M25 (planted
`use crate::server_api::ServerExit;` in the same lane → caught by the
sibling test), M26 (flipped lib.rs to `pub mod server_api;` → caught: the
flipped line is flagged AND the declaration pin reports the census would
widen). All restored; `git diff src/` empty before the gates below.

## Gate results for the T051 chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean (after one rustfmt reflow of the new file) |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 2 passed, 0 failed |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| embed lib gate | not owed: no `src/` content change in this chunk (mutants restored byte-identical; T049's 1333/0 stands for this tree) |
| closure digests | no censused file content changed; the five frozen pins stand |

## T049 — the AAP migration receipt and the wrap-list discharge (PR 3)

The full receipt is `docs/reviews/AAP-MIGRATION-RECEIPT-v11.md`, describing
the runs of `execution/aap_migration_receipt_v11.py` (the script generates
the machine artifact; the prose is authored); this section records the
campaign side of the chunk.

**RED was observed and it named the plan exactly.** The dark adapter
(harness-only, maps contract atom names onto the boundary) was compiled
against the T048 tree first: FOUR errors — one E0432 listing precisely the
eight missing `public_api` items, and three E0603s naming
`OperationKind`/`RetryAdvice`/`SourceRefusalKind` as private (the
`lifecycle_identity` nameability gap). Nothing extra, nothing missing; the
compiler-named set IS what T049 then built, the E7 proof-driven pattern
repeated at the boundary.

**Two transcription defects found and fixed, both the T043 class.**
`ServerExit` carried an invented `Clean` variant where the frozen contract
pins `RefusedToStart`/`Success`; `ReceiptWaitError` omitted the contract's
`DeadlineElapsed`. Both caught by the contract-projected consumer fixture,
both corrected against `public-api-v11.json`, both documented at the site.

**The harness itself was caught lying once.** The first materialized
dependent-positive crate omitted the fixture's `embed` feature, so its whole
consumer module was cfg'd out and "compiles" was a claim about an empty lib —
exposed by mutation M22 (a removed fixture-pinned method still "passed"),
fixed in the generator with the receipt comment naming the incident, then
re-observed: the mutant fails E0599, the restored tree compiles. The same
class as every reporting-invariant defect this feature exists against: the
thing that reported was not the thing that knew.

**Final harness results** (T049 tree over 34952e1c, `CC=clang-cl` — see the
receipt's environment note for the cold-MSVC repo hazard): all-cfg inventory
26 cells, 6/6 sentinels, SHA `826b9c4f…`; dependent-positive COMPILES against
the adapter; compile-fail 71 cases — adapter lane 35/35 expected E0277, real
lane 35 resolution failures (D15's prediction), 33 still-public V10 paths
recorded, 3 expected today (with the `server_api::health` passes-for-the-dark-
reason nuance recorded).

**Wrap-list discharge.** All nine `wrap-planned-t049` obligations flipped to
`wrapped-here`; the export delta regenerated through its write-then-verify
opt-in (nine obligation lines, nothing else). New in the boundary: the claim
family (`EmbedAtomicAuthority`/`EmbedClaim<T>`/`EmbedClaimProvenance`/
`EmbedEvaluationProvenance`), `EmbeddedSourceSpec::current_worktree`,
`ShutdownReport`/`SourceCloseReport` contract records,
`EmbedShutdownReceipt::wait` (observed zeros), `SourceCloseReceipt::wait`
(self-wait guard at the wait), `EmbeddedSourceHandle::search_*` now returning
`EmbedClaim<..>`, `ProcessRuntimeApi` with contract-pinned `Clone`+`Drop` and
`open_embedded_source`/`begin_shutdown`, `pub use` nameability for the three
enums, `Display`+`Error` on `ReceiptWaitError`, and the
`PhantomData<Box<dyn Any + Send + Sync>>` unwind-safety opt-out on the five
handle types (contract auto-trait matrix; proven by the ten adapter-lane
unwind cases).

**D-ledger.** D18 (new): `open_embedded_source` maps `SourceAlreadyOpen` →
`SelectionUnavailable` + `OnEvent` + sentinel evidence — a dark-side judgment
call, recorded in the receipt's divergence register for ratification,
reversible before activation. Also recorded there: the derive-surface
superset vs the contract's closed 17-impl list (activation graph proof owns
the trim), and the by-design external unnameability of `server_api` (its
shapes pinned by the new in-crate unit test instead).

**Oracles.** `tests/runtime_dark_v11.rs` gains its eleventh test —
`contract_waits_guard_self_wait_and_open_refuses_a_held_source` — the
refusing case AND accepting pair for each new guard; the file header's stale
"Eleven oracles" claim (a RED-draft leftover that survived the count
correction to ten) was fixed to say ten-plus-one honestly.

**Mutation ledger.** M20 (self-wait guard removed → caught by the named
oracle alone), M21 (refusal kind flipped → caught by the kind assertion),
M22 (fixture method removed → first exposed the vacuous harness leg, then
caught as E0599 once honest), M23 (`EmbedRefreshTicket` unwind marker
removed → the adapter-lane RefUnwindSafe case COMPILES under the mutant,
refuses E0277 restored). All restored.

## Gate results for the T049 chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean (after one wrap of a long assert) |
| `cargo clippy --all-targets -- -D warnings` | clean |
| lib suite (server default) | 3167 passed, 0 failed (+1: the `server_api` shape test) |
| embed lib gate (`--no-default-features --features embed --lib`) | 1333 passed, 0 failed, 4 ignored (+1: same test compiles under embed) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| export delta regen | opt-in write, then verified WITHOUT the opt-in; diff = the nine obligation flips |
| traceability checker | OK (78 requirements, 24 oracles, 13 categories), re-run on the final tree |
| closure digests | no censused file touched; the five frozen pins passed the checker unchanged |
| full `cargo test --all-targets` | — (runs before the PR, not per chunk) |

## T048 — the wrap table, the flip-ready module, and the delta (PR 3)

RED first: one unresolved import, the absent `public_api` module; then, at
the test-binary compile, the four missing E1 handle methods named by the
compiler.

**Every escalation ruling is code now.** `EmbedSourceRefusal` and
`EmbedOperationReceipt` render KIND-PREFIXED identity strings stored at wrap
time (E3), with `EVIDENCE_ABSENT` as the closed sentinel a renderer that
always emits `<kind>-<digits>` cannot produce (E2), and `Display` + `Error`
implemented as the contract's trait_impls demand. `ProcessRuntimeApi::
acquire()` takes no arguments and delegates to `incarnate` with
`PROVISIONAL_ACQUIRE_PROCESS_BYTES` — a named constant, 256 MiB, recorded
here as PROVISIONAL and not policy; deliberately not the live V10 env budget
(E4). The four V11 handle methods live on the SEAM-pinned `embedded.rs`
handle (E1) under the transcribed contract shapes and REFUSE honestly in the
dark — an empty search result would be a claim about content that does not
exist. `server_api` is a REAL `pub(crate)` module in `lib.rs` (D4): std-only,
no `pub use`, no `index_lifecycle` edge, `run` refusing rather than
pretending a server ran. [Amended after round 1: as written at T048 the
module was ungated and its error an enum with a public `ActivationPending`
variant, and "activation is one keyword" was unqualified — C1 made the
error the contract's opaque struct, and C2 added the contract's
`feature = "server"` cfg gate with D4's std-only-under-embed sentence
amended, so the one-keyword claim is true only BECAUSE the gate is already
present.] Its scoped `allow(dead_code)` carries its receipt in the file.

**The delta is closed JSON, recomputed never trusted.**
`docs/reviews/FEATURE-020-EXPORT-DELTA-v11.json` carries the contract SHA,
all 64 atoms, the live pub-mod census, per-atom D12/D13 obligations from the
wrap table — the module's own shape judgment, never path identity — and the
two forbidden citizens: the `claim_provenance` mount and `LimitBreach`
through `TruncationBreaches`. [Amended after round 1: as written at T048 the
"minus the live census" was a description the renderer never performed, and
the regeneration opt-in compared AFTER writing — the C14 ruling ordered
both fixed; the renderer now performs the exact-match subtraction it
claims, independently pinned in the oracle, and a write-mode run asserts
against the PRE-write content.] The wrap
table asserts coverage of exactly the 30 top-level atoms and that NO embed
atom claims `direct-reexport`; the `wrap-planned-t049` entries are the
inherited work list, recorded so they cannot be forgotten.

**Mutations M18/M19** — the sentinel replaced by a minted `auth-0`, and an
embed atom claiming direct re-export — each caught by its named oracle
alone, restored. Nineteen mutations across the slice: eighteen caught by
name, one historical survivor that forced its oracle, one guard structural.
One clippy catch: the constant assertion on the provisional budget was
rightly refused as always-true and replaced with the honest pin — acquire
delegated, the runtime exists, the value lives in this ledger.

Gates on the T048 tree: both T048 oracles green plus the ten T047 oracles;
embed gate 1332 passed 0 failed — the obligation for this commit; lib 3166
passed 0 failed; clippy denied warnings clean; fmt clean; checker OK; all
five closure digests byte-identical.

## T047 — the dark runtime, RED to GREEN (PR 3)

Observed RED first, twice honestly: the initial file failed for three reasons,
two of which were MY invented constructors where real ones existed —
`ProjectKey::for_test` for `ProjectKey::new`, `EmbeddedSourceFactory::
for_test_root`/`open_for_test` for `new`/`open` — repaired in the tests before
any src work, the transcription discipline applied to my own file. Final RED:
the unresolved `runtime` module plus the two genuinely-new handle members.

**The E7 relocation was proof-driven, exactly as ruled.** `runtime.rs` was
written importing its refusal vocabulary from `lifecycle_identity`, which did
not yet hold it; the first compile NAMED the set — `OperationKind`,
`OperationReceipt`, `RetryAdvice`, `SourceRefusal`, `SourceRefusalKind` — and
exactly those five (plus `CanonicalArgumentHash`, embedded in the receipt)
moved to the shared ungated home, with `claim_provenance` re-exporting so
every oracle path kept resolving. `GenerationAuthority` moved the same way per
the explicit E7 ruling, its provenance-only promotion staying behind as a
same-crate inherent impl. `SourceRefusal::for_runtime` is the crate-visible
mint the dark runtime uses; nothing outside the crate can construct one.

**The binding embed obligation is retired on this commit**: 1332 passed,
0 failed — green BECAUSE the relocation kept `runtime.rs` off `protocol`,
which the naming errors proved rather than speculation.

What landed: `DarkRuntimeFactory` as the single door;
`ProjectIndexRuntime` + `ProjectPublicationRoot` (SEAM-pinned names); the
private five-state machine with the frozen state names and the public
six-variant `SourceRuntimePhase`, `Stopped` derived from the registry
tombstone; `VerifiedGeneration` retaining ONE exact authority Arc;
`acquire_strict` closed on completeness per F020-V11-A20/R20A/R20B; FR-043
across ended and proven-no-op permits; per-source sealed republish preserving
sibling Arcs under a never-reused publication identity; the validated
`capture_source_view` that never invents a token; and the V11
`begin_close`/`SourceCloseReceipt`/`ReceiptWaitError` family on the
SEAM-pinned `embedded.rs` handle with the self-wait guard relocated to the
wait. Payload simplifications versus the frozen machine (observer phases,
mutation epochs, revocation packages, `NonCurrentWork`) are Slice 4
obligations, recorded here as D17. The remaining four V11 handle methods —
`request_refresh`, `runtime_view`, `search_symbols`, `search_text` — land
with T048's wrapper chunk where their contract shapes get pinned, deliberately
not as untested surface now.

**Mutation ledger, continued** — suite 46 oracle tests green after restores:

| # | Mutation | Caught by |
|---|---|---|
| M14 | permit-holding refresh still leases its retention | CAUGHT TWICE — `refreshing_serves_retained_only…` AND `no_terminal_permit_path…`, which is correct: that lease IS a restore path |
| M15 | tombstone never derives `Stopped` | `stopped_phase_derives_from_tombstone…`, alone |
| M16 | republish re-mints every sibling record | `sealed_transition_rebases_one_source…`, alone |
| M17 | self-wait guard at the wait disabled | `begin_close_is_infallible…`, alone |

Seventeen mutations across the slice: sixteen caught by name, one historical
survivor that forced its oracle, one guard proven structural.

Gates on the T047 tree: oracle files 46 passed 0 failed; full lib suite 3166
passed 0 failed; embed gate 1332 passed 0 failed — THE binding run for this
commit; clippy all targets denied warnings clean after two lint-only test
fixes; fmt clean; traceability checker OK; all five closure digests
byte-identical — every touched file is uncensused.

## T045 — the lanes and the measured envelope (PR 2)

**Batch one** routed the three disk lanes through `observe_disk_beneath` and
closed D8 by routing `detect_impact`'s base seed through `admit_git_text`,
deleting the tripwire's sentinel allowlist outright. The `writers` drift was
OBSERVED, not assumed: the checker was run after the first `tools.rs` edit and
reported `RETIREMENT_CLOSURE_MISMATCH` for `writers` and for `writers` alone.

**Batch two — the forgeable envelope axis.** `format_search_envelope` collapsed
to the compact `Trust:` banner on `source_authority == "current index"` — a
string equality any caller could satisfy by assertion. Two lanes did exactly
that: the context bundle passed the literal whenever it had not disk-refreshed,
and `what_changed`'s Timestamp arm passed it unconditionally — both collapsing
the envelope while the index could be Verifying or Degraded. The second lane
was found by the COMPILER during the migration, not by the census.

The collapse now rides on `SourceAuthority`, a type honest by construction:
`from_freshness` is the only constructor that can produce a collapsible value
and it takes the index's measured `FreshnessStatus`; `never_collapse` covers
disk-refreshed, composite, and git authorities whose labels are display only.
A lying literal is UNREPRESENTABLE — no constructor accepts a caller-chosen
string and marks it collapsible. Behavior is byte-identical for measured
Current and for every already-loud lane; the sanctioned change is that the two
asserting lanes now go loud with the honest label when freshness is not
Current. Composite labels keep their existing text, including the recorded
wart that they say "current" unconditionally — a text change was not in scope.

Mutation M13 flipped the Degraded arm to collapsible and was caught by
`a_measured_degraded_authority_never_collapses_however_clean_the_rest_is`
alone, then restored. Twelve mutations across the slice: eleven caught by
name, one survivor that forced its oracle, one guard proven structural.

**D16 — `ProjectEvidence` and the structured `_meta` surface stay untyped in
this PR, deliberately.** The MCP `_meta` object already carries an untyped
provenance record with `generation` / `load_source` / `index_state`. Replacing
it with `Claim`/`ClaimProvenance` is a client-visible schema change, not a
read-gate migration, and no frozen atom requires it preactivation. Recorded
here next to D12/D13 as T048/structured-activation work: the competitor is
untyped strings versus the provenance types, and the swap belongs to the
activation surface, not to T045's task-text word "structured".

Gates on the batch-two tree: lib suite 3166 passed 0 failed including the new
envelope oracle; clippy all targets denied warnings clean; embed 1332 passed
0 failed; fmt clean. At the time batch two landed, the checker reported the
expected `writers`-only mismatch; the regeneration has since HAPPENED — the
T046 section's before/after table is the truth, and the pins are clean.

## T046 — per-caller single capture, and the one regeneration (PR 2)

Every approved site now takes ONE `published_generation()` capture at entry and
reads every axis — live rows, freshness, health counts, temporal, outline —
off that capture, which is possible because every accessor already resolves
through the bundle; the defect was per-call re-loading, not field scatter.

Migrated: `health_for_runtime` and `health_compact_for_runtime` (four loads
each → one), daemon `project_health` (freshness now describes the same
publication as the counts beside it), the daemon call-evidence block and
`local_project_evidence` (generation number, load_source, counts, and state
all off `current_generation()`; the atomic counter is no longer a side
channel — including `runtime_status_for`, whose reported project-generation
is now a caller-supplied parameter: the health pair passes its captured
bundle's value, and the two capture-less callers pass the atomic EXPLICITLY,
named at the site), `search_symbols`, `search_text` (handler + renderer share the
caller's capture through a new parameter), `search_files` (13 loads → 1),
`find_references` (11 → 1), `append_impact_footer`, `edit_plan`, and
`analyze_file_impact`, whose capture is taken BEFORE the sidecar await so the
co-change footer describes a publication the impact result actually saw.
`terminal_dispositions` was re-rooted from the raw `live` field onto the
bundle, closing the store-order window where new content could pair with the
old publication. The write-only `published_repo_outline` ArcSwap field was
deleted after re-verifying zero loads on the current HEAD; the accessor and
both its tests read the bundle and keep working.

Left alone, by prior agreement: the read-MUTATE-read publish paths, watcher
reconcile, Tier-3 mutex-held store functions, `what_changed` — same class as
the search tools, recorded as OUT of this PR rather than silently expanded —
and the `scout_plan` / `source_exclusions` / `project_state_dir` ArcSwaps,
which the bundle has no fields for.

Behavior neutrality: the full library suite passed 3166 to 0 with ZERO test
adjustments — the RISK-B worry that tests pinned torn interleavings did not
materialize, and the Slice-0 root-split oracle got strictly stronger and
stayed green.

**The one regeneration — prediction versus measurement.** The PR 2
first-commit decision predicted FIVE categories dirty. Measured at the end:
FOUR moved, `ccr` byte-identical, because CCR was trimmed out of T045 batch
two by review. The regen updates exactly the four that moved:

| category | before | first regen | after re-crank (HEAD) |
|---|---|---|---|
| writers | `5137cd7b…3af7dd` | `bafa517a…daeee1` | `565e4227…bf3e31` |
| callbacks | `48938137…97e8b22` | `026c548b…fe577b` | unchanged |
| publication_roots | `e37555ad…61e82d` | `b90b8d88…190b54` | unchanged |
| cache | `4eb220e8…5c18a38` | `6fb4cace…14fa095` | unchanged |
| ccr | `8ad77748…84ad246` | UNCHANGED | unchanged |

The checker's own second-order pin (`FROZEN_DIGESTS.retirement_records`) was
regenerated through its emit opt-in the same way: `4c118fab…76a6fb` →
`313dceda…9c21bf` at the first regen, → `d86bd17b…e5ce29` after the
re-crank. Checker reports OK after each.

**Correction, on review.** The re-crank commit's message claimed "the
evidence table now carries the final writers value" while touching only the
contract and the checker — the table had NOT been updated, which is the same
reporting class as a stale pin: the thing that reported was not the thing
that knew. This row-level history is the repair, added as a docs-only commit
after the full suite went green on the re-cranked tree, so the receipt and
the table describe the same HEAD.

## T044 — the authority choice is explicit (PR 2)

Observed RED first: both oracles failed `E0432` naming exactly the three new
seam items and nothing else. Then the seam, in `src/protocol/read_gate.rs`,
on the policy/bytes/git/disk split #571 carved:

- `resolve_generation_bytes` — serves `IndexedFile.content`, the bytes the
  generation PUBLISHED. **The defect it exists to prevent is structurally
  unrepresentable in it**: the function takes no workspace root, so an
  in-function disk backfill cannot even locate a file, and its return borrows
  from the index, so owned disk bytes cannot be returned without a deliberate
  leak. This is recorded INSTEAD of a mutation for the never-reads-disk
  guard, because the only writable mutant is one whose `fs::read` cannot find
  the fixture and therefore survives for reasons unrelated to the property —
  a theatrical mutant would be evidence-shaped noise. The oracle still pins
  the behavior: published bytes survive a disk rewrite, and an unindexed
  file resolves `NotInGeneration`, never disk content.
- `observe_disk_beneath` — the deliberate lane, lexically confined beneath
  the workspace root, refusing absolute paths, prefixes, and `..` components
  BEFORE any read; the refusal never carries escaped content. Symlink policy
  deliberately remains the crate's existing never-follow walk; the ceiling
  and upgrade path are marked in the code.
- Both re-exported through `claim_provenance` the same way as the identities,
  because `read_gate` is crate-private and the oracles are a separate crate.
  No `protocol/mod.rs` edit; no census atom.

Mutation M12 — confinement disabled — caught by
`a_disk_observation_is_confined_beneath_its_root` alone, restored. Eleven
mutations across the slice so far: ten caught by name, one survivor that
forced a new oracle, plus one guard proven structural rather than mutated.

Gates on the T044 tree: oracle files 36 passed 0 failed; clippy all targets
denied warnings clean; embed 1332 passed 0 failed; fmt clean; traceability
OK; all five closure digests byte-identical — T044 touched only uncensused
files, per the PR 2 first-commit decision.

Living document for the slice; T052 completes it. Every claim here was observed,
not inferred. Where a command is cited, it was run on the named tree.

## T041 + T042 — observed RED (durable record)

The RED observation lives in branch commit `cdb3ff20`, which a squash-merge will
collapse, so the evidence is recorded here as well.

Command, on `cdb3ff20`'s tree (before `claim_provenance.rs` existed):

```
cargo test --test read_gate_authority_v11 --test claim_provenance_v11 --no-run
```

Observed output:

```
error[E0432]: unresolved import `symforge::protocol::format::claim_provenance`
   |                                 ^^^^^^^^^^^^^^^^ could not find `claim_provenance` in `format`
error[E0433]: cannot find `claim_provenance` in `format`   (x4)
error: could not compile `symforge` (test "claim_provenance_v11") due to 5 previous errors
```

Every error names the missing module and nothing else, so the RED was about the
absent types, not a malformed test.

## T043 — GREEN transition and the mutation ledger

After `src/lifecycle_identity.rs`, `src/protocol/claim_provenance.rs`, and the
`#[path]` anchor in `format.rs` landed, the same two files compiled and passed:
initially 22, then 23 after M2 forced a new oracle (below).

**Mutation ledger.** Each guard was flipped in production, the suite run, the
named oracle observed failing ALONE, and the guard restored. A guard whose
mutation survives is not enforced; one did, and the response was a new test, not
a shrug.

| # | Mutation (production change) | Expected catcher | Observed |
|---|---|---|---|
| M0 | `AtomicAuthority::proves_repository_absence` → `true` | `no_local_negative_receipt_can_be_widened_to_repository_absence` | CAUGHT — that test alone failed, message named `DiskObservation` |
| M1 | empty-derivation refusal disabled (`if inputs.is_empty()` → `if false`) | `a_derivation_refuses_an_empty_input_set` | CAUGHT — alone, 11 held |
| M2 | bijection LENGTH check disabled (`false && captured.len() != …`) | — | **SURVIVED. 12 passed.** See below. |
| M2' | same mutant, after the new oracle | `a_selected_aggregate_refuses_an_extra_unselected_generation` | CAUGHT — alone, 12 held |
| M3 | `roots_are_compatible` → always `true` | `a_derivation_across_two_roots_is_refused_rather_than_composed` | CAUGHT — alone, 9 held in its file; other file 13 green |
| M4 | `render_bounded` mints a fresh `ProvenanceIdentity` | `truncated_coverage_never_enters_a_claim_identity` | CAUGHT — alone, 12 held |

Final state after all restores: **23 passed, 0 failed** across both files.

**The M2 survival was a real test gap, not a weak mutant.** The bijection
condition is `len_mismatch || !all_contained`; the mutant disabled only the
length half, and the containment half caught the only fixture the suite had
(missing generation). The length guard alone is what catches an EXTRA captured
generation nobody selected — "Missing, extra, forged, or uncaptured inputs
refuse" (`data-model.md:1893`) — and no test exercised that arm. The new oracle
`a_selected_aggregate_refuses_an_extra_unselected_generation` was written while
the mutant was live, observed catching it, and kept.

## T043 stand-ins that must not be "completed" casually

- **`ObservationLease::completed_render_authority` always returns `Ok`.**
  `OutputCoverage::Truncated` is gated on holding a `CompletedRenderAuthority`;
  in Slice 3 that token is obtainable from any `ObservationLease`, because the
  real strict-lease machinery is Slice 4 (T047/T060). The gate is the TYPE, not
  a runtime check. Do not "complete" this method by adding a fake check that
  pretends to verify lease completion it cannot observe — that is the reporting
  defect this feature exists to prevent. Slice 4 replaces the constructor's
  evidence, not its shape.
- The other lease constructors (`observe_missing_path`, `complete_scope_scan`,
  `admit_generation`) are the same shape: sealed constructors whose *evidence*
  arrives with the real runtime. Their `Result` returns exist so the signatures
  do not change when the evidence does.

## Deliberate decisions in force (recorded before code was written)

- **D3** — `DerivedLimitKind`/`LimitBreach` are the LIVE eight-variant types from
  `live_index::knowledge_bridge`, imported, never transcribed. The frozen six is
  stale; a later corpus amendment may add the two names. Confirmed by the
  compiler: the integration crate imports the live type directly.
- **D9** — where `data-model.md` and `contracts/public-api-v11.json` disagree,
  the ATOMS win (opaque `SourceRefusal` + `SourceRefusalKind` + `RetryAdvice`,
  `Claim::producing_runtime_identity`), because the activation rule is
  machine-enforced and the prose is not. Neither document was amended.
- **One identity counter** — `identity_newtype!` and `NEXT_IDENTITY` moved to
  `src/lifecycle_identity.rs` (`pub(crate)` in `lib.rs`, so the public-API
  census gains no atom); `index_lifecycle/authority.rs` re-exports its six
  identities from there. No `protocol → index_lifecycle` call edge exists, so
  T051's darkness proof is intact.
- **The `#[path]` anchor lives in `format.rs`**, not `protocol/mod.rs`
  (censused; also `read_gate` is `pub(crate)` so the oracles — a separate crate —
  could not see the module through it).

## The adversarial audit of the T043 draft, and what it changed

A 5-agent audit ran against the committed draft `225b18bf` — four independent
auditors over seam fidelity, atom coverage, task-text completeness, and embed
cfg, then a synthesizing verdict, each verifying against the frozen corpus
before promoting anything. Every finding below was RE-VERIFIED here before
being acted on.

**Fixed in the follow-up commit, each with its reason:**

- **`OutputCoverage::Truncated` was FORGEABLE while claimed sealed** — a pub
  struct variant, so `Truncated { breaches: vec![] }` compiled anywhere with no
  authority, while doc and commit message claimed the seal. The audit named it
  for what it was: reporting an enforcement the type system did not provide.
  Now `Truncated(TruncationBreaches)` with a private field and no public
  constructor; the ONLY producer is `CompletedRenderAuthority::truncate`.
- **`RetryAdvice` and `OperationKind` violated the module's own atoms-win
  rule.** The contract fixes `RetryAdvice = Automatic | Never | OnEvent |
  Operator` and `OperationKind` as the SEVEN-variant runtime vocabulary; the
  draft invented three retry variants and squatted the OperationKind name with
  four provenance shapes. Both now verbatim from the contract; provenance
  shapes are named by `ClaimProvenance::kind_name` alone.
- **`ObservationLease::refuse` fabricated evidence** — it filled
  `evidence_identity` with a fresh identity corresponding to nothing examined,
  and the oracle blessed it by asserting only `is_some`. The parameter now
  forces the caller to name what it examined, and the Cartesian asserts the
  EXACT identity round-trips.
- **`render_bounded` discarded its coverage argument**, making the retention
  oracle unfalsifiable. Coverage is now retained on the claim, readable via
  `rendered_coverage`, still off provenance identity.
- **`KnowledgeVoice` validated an invented model** — a `Consistency` variant
  that exists in no frozen document, while dropping `Current`, which the
  frozen default selection MUST include. Now the frozen six; "never selects
  consistency" is structural, since no such voice is expressible.
- **`SelectedAggregate` could not name its own evidence** — `authorities()`
  yielded nothing for it while `authority_count()` counted its generations, it
  dropped the frozen `additional_authorities` field, did no root check, and
  `BTreeMap::from_iter` silently collapsed forged duplicate keys.
  `authority_count()` is now literally `authorities().count()`.
- **`into_failed_read` minted a `for_test` receipt on a non-test path**; the
  caller now supplies the operation being served.
- **Identity newtypes had gained `Ord`**, making mint order observable — an
  inference channel added only so a test could sort. Reverted to the original
  derive set; the test uses a `HashSet`.
- **Both oracle files lacked the sibling-convention `#![cfg(feature =
  "server")]`** — invisible to the `--lib` embed gate but a break of the
  documented all-targets embed invocation. Added.
- **The darkness prose in `index_lifecycle/mod.rs` had become false** — it
  claimed grep-level absence, which `lifecycle_identity.rs`'s doc comments now
  violate in prose. Restated as the call-edge property T051 will formalize.

**Mutation ledger, continued.** The three new guards were each flipped,
observed caught BY NAME, and restored — final suite 29 green:

| # | Mutation | Caught by |
|---|---|---|
| M5 | comparison root gate disabled | `a_comparison_across_two_roots_is_refused_rather_than_composed`, alone |
| M6 | duplicate-key forgery guard disabled | `a_selected_aggregate_refuses_a_forged_duplicate_capture` — via the KIND assertion, proving forgery is distinguishable from a selection mismatch |
| M7 | aggregate root check disabled | `a_selected_aggregate_refuses_a_foreign_root_authority`, alone |

**Deferred with records — the D-ledger:**

- **D10 — receipt-field simplifications vs the frozen data model.** The Slice 3
  receipts drop `parent_identity`, `stable_read`/`ByteDigest`, `FileStamp`,
  `policy_versions`, `started_at`/`finished_at`, `manifest_digest` and
  `stable_entry_count` on scope coverage, `repository_id`/`resolved_from`/
  `object` on Git receipts, and use `String` where the model has
  `CatalogPath`/`PhysicalRootIdentity` typed paths. All prose-only — no atom,
  oracle, or seam pins them — and the machinery that makes them load-bearing
  is Slice 4. NOTE: the `String` paths cannot carry non-UTF8 opaque paths,
  which collides with T053's lossless opaque-path oracle; Slice 4 must widen.
- **D9 append** — every `ClaimProvenance` variant carries `identity` per the
  atom `ClaimProvenance::identity`, which the data-model prose lacks.
- **D11 — duplicate `PhysicalRootLease` name.** The provenance fixture
  coexists with the real `index_lifecycle/physical_root.rs` type the data
  model references. The recon census wrongly listed it as nonexistent, which
  caused the duplication. Reconciliation belongs to the Slice 4 wiring that
  connects provenance to the real lease; no enforced check breaks today.
- **D12 — activation-time surface unwind.** The module is mounted at
  `symforge::protocol::format::claim_provenance`, and `OutputCoverage`
  publicly exposes `live_index::LimitBreach` — both forbidden by negative
  assertions AT ACTIVATION, both legal today because `observed_graph.status`
  is `pre_activation_required`. T048's embed boundary must wrap or unwind.
- **D13 — atom accessor shapes are the EMBED boundary's problem.** The
  contract fixes `&str` identity returns, reference returns, `Display` +
  `Error` on `SourceRefusal`, and opaque structs where this module has enums.
  The atoms describe `symforge::embed::*`; T048's re-export layer wraps the
  internal types into contract shapes, and T049's dependent-positive fixture
  is the enforcement. Recorded so T048 does not assume a 1:1 re-export.
- **D14 — one T042 clause is currently unfalsifiable.** The
  preserving-Current half compares an immutable local identity to itself; it
  becomes falsifiable when T047's runtime exists. T052's review must not count
  it as coverage until then.
- **D15 — compile-fail harness sequencing.** `cases.json`'s T043-era subjects
  resolve only after T048's re-exports; T049 must not run before T048. The
  harness has zero `OutputCoverage` cases; the seal fix above is what makes
  them writable.
- **ClaimContext / `acquire_claim_context` are still absent** — named by
  T043's task text, needed by T042's rebind clauses. They are the NEXT chunk
  of T043, not a deferral.

**Dogfood catch — a symforge defect observed by an auditor.** `get_symbol` for
`LimitBreach` returned `Decision: cache_hit` with "Reuse the content already
loaded in this session" and `session_age_secs=5402` — in a subagent session
that had never loaded that content. A cache voucher pointing at content the
requesting context never observed is symforge's own reporting-invariant
failure class; `force_refresh=true` was the workaround. Reported separately;
not a campaign item.

**Audit-environment lesson.** Two auditors read this worktree WHILE the
mutation loop held a live mutant and promoted the mutant to a blocker. Any
audit fanned out into a mutation-owned worktree must read from a pinned
`git show SHA:` baseline, not the working tree.

## The ClaimContext chunk — the last piece of T043's named surface

`ClaimContext`, `ClaimContextInput`, `CurrentQueryLease`,
`OperationRelationshipContract`, and the free function `acquire_claim_context`
now exist, on the frozen shape from `data-model.md:1844-1872` under the
recorded Slice 3 adaptations: `String` keys per D10, the local lease per D11,
and a `Vec` whose emptiness is refused in the constructor per D10's
NonEmptyVec record.

The closed relationship table is derived from `OperationKind` and nothing
else: search operations permit the cross-source relation and require a
`Current` lease per input; runtime lifecycle operations act on one source and
require none. Both directions of every rule carry an accepting pair:

- empty acquisition → `InvalidSelection`; one-input acquisition admitted
- root drift between acquisitions under `CloseSource` → `SourceUnavailable`;
  the SAME two roots under `SearchText` admitted, because that is the closed
  contract's explicit cross-source relation, not a loophole
- `SearchText` input without a `Current` lease → `AdmissionUnavailable`; with
  the lease admitted; `RefreshSource` legitimately omits it
- a returned context retains exactly the roots, sources, and repository ids
  captured at acquisition — the falsifiable half of "a rebind after return
  does not trigger a trailing live-state check"

`current_query_lease` joins the fixture-evidence family: shape sealed, its
`Ok` unconditional until Slice 4's strict-lease machinery provides the
refusing evidence. Same rule as `completed_render_authority`: do not complete
it with a fake check.

**Mutation ledger, continued** — final suite 34 green:

| # | Mutation | Caught by |
|---|---|---|
| M8 | empty-acquisition guard disabled | `a_context_refuses_an_empty_acquisition`, alone |
| M9 | root-drift guard disabled | `a_rebind_between_input_acquisitions_is_refused`, alone |
| M10 | requires-current guard disabled | `a_generation_structured_operation_requires_a_current_lease_per_input`, alone |

Ten mutations total across T043: nine caught by a named oracle alone, one
survivor that forced a new oracle and was then caught by it.

## The traceability catalog caught an invented name (T041)

First run of `node scripts/validate-lifecycle-oracle-traceability.cjs` on the
T043 tree FAILED:

```
ERROR PLANNED_TEST_CASE_MISSING: trace.catalogs.tests.TEST-PROVENANCE:
tests/claim_provenance_v11.rs::operation_contract_cartesian_matrix
```

The frozen catalog pins `TEST-PROVENANCE` to that exact function name
(CMD-PROVENANCE, owner T041, `introduced_slice: 3`), and the pin activates the
moment the FILE exists. The Cartesian test had been written under an invented
name — the Slice 2 failure mode, caught by the machine this time. Renamed to the
pinned name and WIDENED to match it: the pinned name says OPERATION contract, so
the operation kind became an axis — 4 operations x 4 refusal kinds x 3 retry
advices, `seen == 48`. The pinned command was then run verbatim and observed:
`cargo test --test claim_provenance_v11 operation_contract_cartesian_matrix --
--exact` -> `1 passed; 0 failed; 12 filtered out`.

## Embed-gate result, and why it passes by design

Prediction before running: FAIL, because the nine new `lifecycle_identity` items
are consumed only by `claim_provenance`, which sits under the server-gated
`protocol` module. Observed: **PASS, 1332 passed, 0 failed** (up 3 — the new
module's own unit tests run under embed).

The prediction missed `src/lib.rs:4`:
`#![cfg_attr(not(feature = "server"), allow(dead_code))]`, whose comment states
the policy: under embed an embedder uses a subset of the engine API, so
unused-but-public helpers are expected, not dead. `protocol` IS absent under
embed (`lib.rs:67`), and the identities are idle there BY DESIGN. No cfg-gating
of the new items is needed, and none was added.

## Gate results for the T043 chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean, 29s warm |
| embed lib gate (`--no-default-features --features embed --lib`) | 1332 passed, 0 failed, 4 ignored |
| traceability checker | OK (78 requirements, 24 oracles, 13 categories) — after the pinned-name fix above |
| pinned CMD-PROVENANCE, verbatim | 1 passed, 0 failed, `--exact` |
| both oracle files | 23 passed, 0 failed |
| five closure digests re-emitted | byte-identical to the pinned values |
| full `cargo test --all-targets` | — (runs before the PR, not per chunk) |
