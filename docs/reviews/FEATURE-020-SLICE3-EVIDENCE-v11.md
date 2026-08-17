# Feature 020 Slice 3 evidence (T041–T052)

## T052 — slice-closure gates and post-slice review (PR 4; in progress)

T052 began as run-and-review. Post-Round-3 verification required two scoped
production-honesty repairs in the already-shipped compact facade. First, its hidden
A-019 relay is restricted to a source-mutation-safe measurement allowlist, and the
real MCP seam preserves raw legacy text without fabricating a semantic result status
or inferring `isError` from valid source bytes. Second, an explicit foreign-project
selector is now either refused before any adapter-HOME early return or carried into
every primitive dispatched by a healthy daemon. Adapter-local path containment,
economics, co-change/fusion enrichment, root rendering, and project evidence may not
be substituted for the selected project. A missing or mismatched daemon receipt is
reported as unavailable and cleared from typed metadata, never relabeled as HOME.

The same selector audit exposed three deterministic adjacent failures that could not
be left behind an otherwise exact matrix. Relative `index_folder` paths were resolved
once in the adapter and again in the persistent daemon's potentially different CWD;
overlapping default opens could reverse the daemon ACTIVE and adapter mirror order;
and selector-less ACTIVE tools could only reject a wrong receipt after already
executing against a concurrently retargeted project. The repaired path forwards one
canonical root, serializes daemon activation through its mirror and reconnect lane,
and carries an adapter-authored private canonical pin for `inspect_match`,
`checkpoint_now`, `detect_impact`, and `conventions` without widening their public
schemas. `ask` now performs its local no-echo guard and then proxies the whole query so
classification happens in the selected runtime. Local `analyze_file_impact` now uses
the exact winning reindex publication for its text, co-change footer, and typed
evidence rather than pairing the sidecar result with an entry snapshot.

The tool-correctness harness is part of the same repair boundary: the relay remains
raw-result-status-free, while mandatory successful-envelope anchors are enforced for
every oracle case rather than being downgraded to human REVIEW. This is an explicit
PR 4 scope exception because the exact T050 audit exposed an existing
source-write-capable route behind a read-only facade, and the containment audit then
exposed selector-dependent HOME leakage in that same facade. Preserving either
behavior would knowingly preserve the defect. No frozen inventory member, authority
assignment, or normative clause is amended; the designed preactivation source-census
pins are regenerated below for the legitimately changed release code. V10 authority
is unchanged, and no Slice 4 activation work is included.

**Evidence-kind rule.** Every binding/current execution below names the tree or
candidate it observed. Older observations whose exact tree did not survive are
explicitly labeled unbound historical evidence and never satisfy a current gate.
"Observed" applies only to commands and mutations directly witnessed on the
named candidate. Source inspection, semantic
classification, and adjudication are inferred from cited pinned bytes unless
explicitly paired with an execution receipt. Reconstructed or source-unknown
outcomes are historical only and never satisfy a gate.

- **PRE-PATCH — unbound historical observation only** — T050 green, overlay as first authored. Through the immutable
  Round-3 SHA, every later PR 4 change remained in `tests/` and `docs/`. The
  initial post-Round-3 repair first changed `src/protocol/tools.rs` for the
  scoped facade-relay safety repair; later selector, daemon, and impact repairs
  widened the production-source diff recorded above.
- **VOID** — observed, but not binding as a gate because its compiled source is
  unknown; the row says why.
- **POST-PATCH (round 1) — unbound historical observation only** — after the 29-assignment-row `Refused` fix.
- **SUPERSEDED POST-ROUND-2 — unbound historical observation only** — the initial depth-aware `target` repair,
  observed directly but superseded by the follow-up hardening.
- **SUPERSEDED ROUND-2 REVIEW CANDIDATE** — immutable review SHA
  `606bbeb50ac11c781f9337a7109be290f8a93b08`, after the case,
  traversal-error, and skip-proof hardening. Round 3 returned trustworthy
  FINDINGS; later Rust-test repairs supersede its gate rows.
- **POST-ROUND-3 REPAIR CANDIDATE — superseded reviewed candidate** — the
  post-Round-3 repair tree whose gates are recorded in the checklist below. It
  was committed as the immutable candidate
  `e8d5ae5fac9d36ec814aa302697fd6f18770161d` and received three independent
  external reviews whose consolidated adjudication returned FINDINGS; its gate
  rows are historical evidence for exactly that reviewed tree.
- **EXTERNAL-REVIEW REPAIR CANDIDATE** — the current uncommitted repair tree:
  the reviewed candidate plus the C1/C2/C3 repairs recorded in the
  external-review section below. Its production source is frozen and its final
  source seal is observed there; the new non-closure commit identity and fresh
  review are still PENDING. No observation made while the relevant bytes were
  moving can substitute for the LF-normalized immutable-candidate gates or a
  fresh trustworthy CLEAN review; T052 remains in progress.

Live branch/PR/SHA state is deliberately NOT quoted here — run
`pwsh scripts/campaign-state.ps1` for that; the SHAs that do appear are
historical and already fixed.

### Round 1 of the post-slice review, and what it moved

The T052 gates were run first, then the overlay review ran against them. It was
not prose-only, so the assignment this slice carries is NOT the one the gates
below first saw. Recording that here because the section that reports has to be
the section that knows.

**MAJOR — `Refused` was keyed off input-struct docs, not handler bodies.** The
generation-backed family carried the basis "its input documents no refusing
project selector, so Refused is not sprayed on". The binding rule was the
opposite: body-level typed refusal counts, and the seven-struct list was never
closed. This is the `run_init_with_paths` / `index_folder` partial read
INVERTED — the docs were read and the handler was not.

**29 assignment rows gained `Refused`** (fixed in the round-1 patch commit):

- `foreign_project_refusal` — `get_symbol`, `get_repo_map`, `get_file_context`,
  `get_symbol_context`, `search_files`, `get_file_content`, `find_dependents`,
  `validate_file_syntax`, the seven edit tools, and their seven
  `edit_tools.rs` writers rows.
- `local_cross_project_refusal` (same selector class, `tools.rs:7636`) —
  `search_symbols`, `search_text`, `find_references`, `search_knowledge`,
  `review_knowledge`, and `curate_knowledge` on BOTH its tools row and its
  `tools.rs::SymForgeServer::curate_knowledge` writers row. The refusal lives on
  the tool handler, so `write_policy` / `apply` / `durable_replace*` keep their
  sets.

**At the Round-1 candidate, bases were rewritten, not concatenated.** A row
asserting both "Refused is not sprayed on" and "Refused is a BODY fact"
contradicted itself. At that tree, `conventions`, `context_inventory`,
`inspect_match`, and `symforge_retrieve` remained singleton, while the six
generation-backed resource wrappers and the hooks dropped `Refused` using the
then-current selector/fail-open reasoning. These are historical Round-1
classifications, not the post-Round-3 truth; the semantic supersession is mapped
below.

The other Round-1 citation repairs remain historical facts: the two `symforge`
co-change citations were unswapped; `FindChanges` was grounded in
`route_tool_name`, `src/stel/planner.rs`, and the actual worktree lane; and the
session `/health` and `/stats` twins were grounded in their daemon handlers.
Round 3 identified the stale fusion-anchor call citation; the post-Round-3
repair corrected it from `tools.rs:11029` to the then-current `tools.rs:11028`.
Those are historical line locations only. Later facade edits moved the call
again; the final activation bases cite the frozen production bytes rather than
reusing either historical line number.

None of this was visible to T050's green (see M63c below).

### 1. Provenance round trips — PRE-PATCH

| Command | Result |
|---|---|
| `cargo test --test claim_provenance_v11 -- --test-threads=1` | 21 passed, 0 failed |
| `cargo test --test read_gate_authority_v11 -- --test-threads=1` | 15 passed, 0 failed |
| `cargo test --test claim_provenance_v11 operation_contract_cartesian_matrix -- --exact` | 1 passed, 20 filtered |

### 2. Cfg matrix and 3. public-API harness — PRE-PATCH

| Command | Result |
|---|---|
| `python execution/aap_migration_receipt_v11.py --stage full --check` | exit 0; real lane: 71 cases — 35 resolution-failure, 33 compiles, 3 expected-failure; adapter lane: 35 expected-failure rows (106 result rows total) |
| `cargo test --test public_api_delta_v11 -- --test-threads=1` | 2 passed, 0 failed |
| `python execution/refreeze_v11.py verify-internal --target-ref HEAD` | passed |
| `node scripts/validate-lifecycle-oracle-traceability.cjs` | OK — 78 requirements, 24 acceptance oracles, 13 retirement categories |

`--check` regenerated `AAP-MIGRATION-RECEIPT-v11.json` with a one-line diff:
`repo_commit` only, because operation identities are recomputed fresh. **That
diff was discarded** — T052 does not mint a receipt — and
`claims_v11_exports_live: false` is unchanged.

### 4. Unchanged-V10 behaviour

**PRE-PATCH** — `src/` is untouched by the overlay patch, which is tests-only.

| Command | Result |
|---|---|
| `cargo test --test preventive_runtime_dark_v11 -- --test-threads=1` | 4 passed, 0 failed |
| `cargo test --test runtime_dark_v11 -- --test-threads=1` | 11 passed, 0 failed |
| `cargo test --no-default-features --features embed --lib -- --test-threads=1` (separate run) | **exit 0**, 1332 passed / 0 failed / 4 ignored. At this historical tests-only point, `--lib` did not compile `tests/activation_cut_v11.rs`, so that overlay patch did not create an embed rerun obligation. The later production facade repair does, and its fresh row appears in the current candidate gates. |

**VOID — not the slice-level gate.**

| Command | Status |
|---|---|
| `cargo test --all-targets -- --test-threads=1` (first run) | exit 0, 538s, 123 matched events, lib 3168/0/5 — kept as a historical observation only. It was launched before the overlay patch and compiled while `tests/activation_cut_v11.rs` was being edited, so what rustc had already built is unknown. Superseded by the re-run below, not reconstructed from incremental state. |

**POST-PATCH (round 1)** — overlay after the 29-row fix.

| Command | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy --all-targets -- -D warnings` | exit 0 |
| `cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact` | 1 passed, 4 filtered (the four T058 stand-ins) |
| `cargo test --all-targets -- --test-threads=1` (re-run, post round-1) | exit 0, 440s, 123 matched events, lib 3168/0/5. Covers the patched OVERLAY — `activation_cut_v11` reported 1 passed / 4 ignored inside this run — but predates the round-2 walk fix, so it is not the final gate. |

**SUPERSEDED POST-ROUND-2 OBSERVATION.** Terminal Commander first returned
`exit_code: 0` with `outcome_trust: reconstructed`, `restarted: true`, and
approximately 18 seconds of daemon uptime. That zero was discarded and the gate
was rerun. `reconstructed` was honest tool reporting; banking a green that no
live process witnessed would have violated the reporting invariant. The rerun
was observed directly:

| Command | Historical result |
|---|---|
| `cargo test --all-targets -- --test-threads=1` (pre-hardening round-2 candidate) | exit 0; 431,689 ms; 123 matched events; lib 3168/0/5; `activation_cut_v11` 1 passed / 4 ignored; `outcome_trust: observed`; `restarted: false` |

A later second Terminal Commander daemon restart evicted that job record. Both
daemon restarts are operational history, not Slice 3 residuals. The walk repair
below supersedes this result, so it is not used as the candidate gate.

**SUPERSEDED ROUND-2 REVIEW CANDIDATE — review SHA
`606bbeb50ac11c781f9337a7109be290f8a93b08`.**

The table below records the last gates observed before PR 4 post-slice Round 3.
It remains valid only as historical evidence for exactly that reviewed tree.
Round 3 returned trustworthy FINDINGS, and subsequent Rust-test repairs changed
the bytes under test; therefore none of these rows validates the post-Round-3
repair candidate.

| Command | Result |
|---|---|
| `cargo fmt --check` | exit 0 |
| `cargo clippy --all-targets -- -D warnings` | exit 0; 11,973 ms; `outcome_trust: observed`; `restarted: false` |
| `cargo test --test preventive_runtime_dark_v11 -- --test-threads=1` | 4 passed / 0 failed |
| `cargo test --test activation_cut_v11 -- --test-threads=1` | 1 passed / 0 failed / 4 ignored |
| `cargo test --test runtime_dark_v11 -- --test-threads=1` | 11 passed / 0 failed |
| `cargo test --test public_api_delta_v11 -- --test-threads=1` | 2 passed / 0 failed |
| `cargo test --all-targets -- --test-threads=1` (repaired round-2 candidate) | exit 0; 669,434 ms; 123 matched events; lib 3168/0/5; `activation_cut_v11` 1 passed / 4 ignored; `outcome_trust: observed`; `restarted: false` |

The first full-suite attempt on the hardened candidate was an observed RED, not
discarded noise: exit 101 after 211,024 ms with 6 matched events; lib reported
3167 passed / 1 failed / 5 ignored. The failing invariant was
`process_util::tests::test_no_raw_command_spawns_outside_hidden_command`, which
identified both new raw `Command` spawns in the preventive test. Both were
repaired to use `symforge::process_util::hidden_command`; the exact invariant
then passed 1/0, the preventive suite passed 4/0, and the clippy and full-suite
rows above were rerun on those repaired bytes.

Both feature sets were run sequentially, never interleaved in one `target/`, and
across the gate runs tabulated above no `E0786` / ICE / missing-crate signal
appeared. No `cargo clean`.
The four ignored T058 names are NOT execution evidence.

The embed `--lib` row remains historical evidence through the Round-3 SHA. The
post-Round-3 facade, routing, and evidence repairs change release source, so the
candidate owed a fresh embed `--lib` observation before its non-closure commit;
that current-source observation is recorded below.

The earlier decision to defer `cargo build --release` to PR CI is superseded.
This candidate changes production dispatch and the executable
tool-correctness harness, so both debug and release binaries must run both
fixture sets locally. PR CI remains corroboration, not a substitute for those
observations.

### Post-Round-3 immutable-candidate checklist — receipts for the reviewed candidate `e8d5ae5f` (superseded)

These rows bound the post-Round-3 repair candidate that was committed as
immutable `e8d5ae5fac9d36ec814aa302697fd6f18770161d` and then externally
reviewed. The consolidated review returned FINDINGS, and the C1/C2/C3 repairs
changed `src/` bytes, so per this checklist's own invalidation rule none of
these rows binds the current tree. The binding checklist for the repaired
candidate is in the external-review section below. A row changed
from PENDING only when directly observed on the frozen bytes; earlier focused
observations remain useful historical guard evidence but cannot fill a current
row. Results, hashes, counts, SHAs, and review disposition are recorded only
from direct observations on the named bytes.

| Required evidence | Binding command or observation | Current status |
|---|---|---|
| Final T051 source pin | Record the whole-`src/` SHA-256 tuple, file count, and LF-normalized byte count after every production source edit is complete. | **OBSERVED ON FROZEN SOURCE** — `8cf143e41d269ab4e0fcf1c48e09c4323d7ebc74020f3eb24e8b4d45cdc9c2cb` / 187 files / 8,968,263 LF-normalized bytes. The final refresh includes the completed selector/path/daemon/idempotency/sidecar repair and its in-module controls; no `src/` byte changed after this receipt. Any later `src/` edit invalidates this row. |
| Selector containment controls | Run the focused `daemon_facade_` controls, including foreign primitive routing, HOME-path/economics/fusion/co-change isolation, and the matching HOME controls. | **OBSERVED ON FROZEN SOURCE** — 12/0, `outcome_trust: observed`, job `job_01a0084917b375c097a9b78dd08b3fef`; the earlier removal/inversion mutation is recorded in the next row. |
| Missing/mismatched project evidence | Run `daemon_facade_never_reuses_home_evidence_when_foreign_receipt_is_missing` and `daemon_facade_rejects_a_mismatched_foreign_project_receipt`; verify both rendered text and typed `_meta` refuse HOME substitution. | **OBSERVED ON FROZEN SOURCE** — both named tests are in the final 12/0 selector-family receipt above and pin body plus typed `_meta`. |
| Selector containment mutation | Disable the one adapter-local-state containment decision, observe the selector controls fail semantically, restore exact bytes, and rerun the controls. | **OBSERVED** — forcing `may_use_local_project_state=true` made all 10 then-existing foreign-project controls RED (`job_01a006ed280070e0b3f9dbeb8670f222`); exact restoration restored 10/0 (`job_01a006edc5527e01b2d8b33246db2dc5`). The guard implementation did not change afterward, and the widened final family is 12/0 on frozen bytes (`job_01a0084917b375c097a9b78dd08b3fef`). |
| Layered T051 controls | Run the direct lexical sweeps, the 13-file excluded-runtime diagnostic seal, and the load-bearing whole-`src/` seal together. | **OBSERVED ON FROZEN SOURCE** — `preventive_runtime_dark_v11` 8/0 with `outcome_trust: observed`, job `job_01a00825786a786382c99105d9a42ebb`; the adversarial macro/alias receipt is in the next row. |
| Whole-source macro/alias mutation | Plant the reviewed outside alias/macro bridge: the lexical and narrow diagnostics may remain green, but the whole-source seal must fail; then restore exact bytes and rerun the whole seal. | **OBSERVED** — the mutant preserved the two allowlisted mount lines while exporting a spelling-free alias: all seven non-whole-source diagnostics stayed green (`job_01a006eb300772a3b3b254e724347561`), and only the exact whole-source seal RED (`job_01a006ebd9b372d1a1144990096519b8`). Exact restoration to `src/live_index/mod.rs` SHA-256 `500824888d3cac199e941869904c6fd9af300263ef540a6a2b49a946244dc3ad` restored 8/0 (`job_01a006ec1e9b7881ba17dcd2605fa848`). |
| A-019 relay and real MCP seam | Rerun the allowlist, mutating-relay denial, result-status-free malformed/missing controls, and real `Error:`-prefixed source-wire control. | **OBSERVED ON FROZEN SOURCE** — policy 1/0 (`job_01a0084d369f7ad2bb5d352cc82d1ccb`), serial facade family 12/0 (`job_01a0084917b375c097a9b78dd08b3fef`), and real MCP seam 1/0 (`job_01a0084996247df380e52216233f0812`). The earlier envelope-removal mutation made all four synthetic oracle cases RED at the intended terminal guard (`job_01a006ef244b7bd1a76d6376132e40ab`); the final comparator self-test is PASS and final script SHA-256 is `c99ff591d88a1b9875e0927ab5287cb60d0a0661dd072c7e292d429404744c68`. |
| Overlay exactness | `cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact` on the final overlay and citation bytes. | **OBSERVED** — 1/0, `job_01a00848e53770119dca889c0356ddd0`. |
| Focused dark/public suites | `preventive_runtime_dark_v11`, `runtime_dark_v11`, and `public_api_delta_v11`, each serially on the final candidate. | **OBSERVED** — preventive 8/0 (`job_01a00825786a786382c99105d9a42ebb`), runtime-dark 11/0 (`job_01a00848f5a57350a094e859222cdf90`), and public API 2/0 (`job_01a0084906447240b36abd60831a6d76`). |
| Formatting and lint | `cargo fmt --check`, `git diff --check`, lifecycle traceability, and `cargo clippy --all-targets -- -D warnings`. | **OBSERVED** — final-source fmt clean (`job_01a008488c8e7a11b7aad780d0b5368c`), clippy clean with denied warnings (`job_01a0084807b376839da5bc616eb1cfda`), worktree and cached diff checks clean, all 34 candidate paths report `w/lf`, verify-tools syntax/self-test are clean, and lifecycle traceability is OK at 78 requirements / 24 acceptance oracles / 13 retirement categories. |
| Embed feature | `cargo test --no-default-features --features embed --lib -- --test-threads=1`. | **OBSERVED** — 1,333 passed / 0 failed / 4 ignored, `outcome_trust: observed`, job `job_01a00849d4af7b419d77f4f40aa3e95f`. |
| Debug binary + both harness fixtures | Build the debug `symforge` binary, then run `scripts/verify-tools.cjs` once with `verify-tools` and once with `verify-tools-real`. | **OBSERVED** — debug build exit 0 (`job_01a00826729d7f0297ac9ae2a2b3b8cd`); `target/debug/symforge.exe` synthetic fixture 7 PASS / 1 REVIEW / 0 FAIL (`job_01a008269377743084416d59f49e640b`) and real fixture 10/1/0 (`job_01a00826d2957731a00f9dfa2d55e3c1`), all with `outcome_trust: observed`. The two REVIEW dispositions are adjudicated below. |
| Release binary + both harness fixtures | `cargo build --release --bin symforge`, then run the same two harness fixtures against that exact release binary. | **OBSERVED** — release build exit 0 in 281,920 ms (`job_01a0082702827491b9b38a8095d94f7f`); `target/release/symforge.exe` synthetic fixture 7 PASS / 1 REVIEW / 0 FAIL (`job_01a0082b8a47797396797080c413d5c1`) and real fixture 10/1/0 (`job_01a0082bb5db71a2b613e4766a9524ae`), all with `outcome_trust: observed`. This was observed locally, not deferred to CI. |
| Full suite | `cargo test --all-targets -- --test-threads=1` with a live `outcome_trust: observed` receipt on the final candidate. | **OBSERVED** — exit 0 with `outcome_trust: observed` in 420,660 ms (`job_01a008411c3b7c92be67858bc9d3820e`); the main library target reported 3,215 passed / 0 failed / 5 ignored and every integration target completed cleanly. |
| Non-closure commit | Commit the complete repaired candidate with a non-closure subject after all rows above are green. | **DONE for this candidate** — committed as immutable `e8d5ae5fac9d36ec814aa302697fd6f18770161d`, subject `fix(slice3): prepare activation-cut review candidate [non-closure]`, after one attestation manifest-pin rebind amended in place. |
| Fresh PR 4 review | Review one immutable committed full PR 4 range with no concurrent edits; adjudicate every substantive note. | **DONE for this candidate — FINDINGS** — three independent external reviews of the full immutable range were received and source-adjudicated; the consolidated verdict was FINDINGS with one confirmed MAJOR, two confirmed MINORs, and one false positive. See the external-review section below. |
| Evidence/ledger closure commit | Only after a trustworthy CLEAN review, record its archive and final receipts in a separate evidence/ledger commit. | **NOT REACHED for this candidate** — the FINDINGS verdict restarted the repair/gate/commit/review loop. |

The final gate sequence retained its RED diagnostics rather than hiding them.
The first final library run exposed five sidecar fixtures that still wrote
identity-free legacy state after root-scoped descriptor admission became
mandatory; binding those fixtures to the expected root restored the focused
sidecar family to 139/0 and the library to 3,215/0/5. The next all-target run
exposed nine hook-subprocess mocks advertising an arbitrary project id instead
of the id derived from their canonical fixture root; the corrected mocks passed
18/0. The following run exposed one latency fixture that omitted its descriptor
root and therefore measured the intentional 500 ms daemon fallback deadline;
an identity-bound descriptor restored the focused latency control to 1/0 in
40 ms. The complete rerun above is the binding green receipt. The last two
repairs are integration-test-only and do not alter either built binary; the
in-module sidecar/tool fixture edits are included in the final whole-source seal.

Both harness REVIEW results are expected, human-adjudicated unit differences,
not failures. `refs-verify_token` asks a reference index to match raw grep;
grep's three lines include the definition and prose while the tool returns the
actual `require_bearer` call site, so 7/1/0 is the honest synthetic result.
`text-StelRequest` compares ten raw grep lines with `search_text`'s default
five-matches-per-file rendering cap; the required term and source file are
present, so 10/1/0 is the honest real-fixture result. Debug and release agree
exactly on both dispositions.

Principal frozen-byte SHA-256 values at final observation are
`tools.rs=ae4c374b29eda7d170901e8608ab1733e3133621351602e939dfb5e5562c045d`,
`protocol/mod.rs=22ba17513dae25348f7c5f0c18f209e186f7cf1c6108c860ee6662456502e4d4`,
`daemon.rs=eb819103ea48d98a70913d5c5544a1c28fdeebf8acd8937fa69937f35e06111a`,
`activation_cut_v11.rs=9495ea9a6306aada36f41378268b02c870e18f0055405b283719cc26ba2833d0`,
`preventive_runtime_dark_v11.rs=fd3e983dfb10b8525ba675e4a2689834a2ab93e2207c83a9f1d8071b4769e734`,
and `verify-tools.cjs=c99ff591d88a1b9875e0927ab5287cb60d0a0661dd072c7e292d429404744c68`.
Terminal Commander directly observed the final selector family at 12/0, overlay
at 1/0, runtime-dark at 11/0, public API at 2/0, preventive suite at 8/0, and
the real MCP seam at 1/0 in the binding rows above.

These observations establish the named controls only. The final mutation and
candidate-gate receipts are recorded in the binding checklist; none substitutes
for the immutable non-closure commit and fresh review.

#### Deliberate preactivation source-census regeneration

The first frozen-candidate traceability run failed exactly four closure categories:
`cache`, `callbacks`, `publication_roots`, and `writers`. This was not an unplanned
inventory change. PR 4 legitimately changes release code in `daemon.rs`,
`protocol/mod.rs`, `protocol/edit.rs`, and `protocol/tools.rs`; those paths are owned
by exactly those four category closures. The opt-in emitter reproduced that exact
four-category set while `ccr` stayed byte-identical. The designed regeneration
procedure then updated:

| Pin | Before | After |
|---|---|---|
| `cache` closure | `6fb4cace44005ceb8019730721950c55a904fb1178da83e9782ffe21614fa095` | `760c5da2d416d7e654ef1adc32a126143f1f404608d05e4a31ef84a6c7a0ebb0` |
| `callbacks` closure | `026c548ba79c43b0e48ee1f1c4f87da9fd614d608615a41c059966f7a9fe577b` | `f0dd5624ff538db95b023a23bd02e163c3e10880fe9cd16519208a103f65eaa5` |
| `publication_roots` closure | `b90b8d8862519ac8451ca459288157759298135b5fbb8b9ea326e48755190b54` | `555e5219eee5668808ca81adadecf42e25e8fbe4f3f245e69db2a857e53826a5` |
| `writers` closure | `565e42273f56e3ac467fa09b13bceec9d3e6103695e49cc2f64abf47cfbf3e31` | `f2a177769a0828f3e09706d32fea4aee3dc58a84dbb8eb69e3f41556c04115a0` |
| second-order `retirement_records` | `d86bd17b3e6ce2cddf86e2755433fe9dc0b5ed91467ad805fc634be1bbe5ce29` | `e86162d5671e18c5bc6a8673980e5908c63383dac27715ed953a831bf6b40eea` |
| raw contract hash in the refreeze manifest | `b351235786073dc02a1684f7209c9456a84b20a9a8198cfcbf2001ab847bfef2` | `70db78519fa9fbebc35612ab9e4609397dd49c32308ea5a1983f765fe3ec3a4a` |

The final all-target run then exposed the Windows health-root spelling mismatch.
Its 94-byte `tools.rs` repair touched only the already-owned `writers` census.
The same opt-in regeneration procedure produced this final chain; every other
closure remained byte-identical:

| Pin | Pre-health-fix value | Final value |
|---|---|---|
| `writers` closure | `f2a177769a0828f3e09706d32fea4aee3dc58a84dbb8eb69e3f41556c04115a0` | `22fa98fd6ef5dbb72f3088039f4a07111e1fcb8beb5ffefb787d6b71b61b7b36` |
| second-order `retirement_records` | `e86162d5671e18c5bc6a8673980e5908c63383dac27715ed953a831bf6b40eea` | `d5b0003cd8e7150417495f89ef46a901bdd392a8287f211fae15b6fcc1758464` |
| raw contract hash in the refreeze manifest | `70db78519fa9fbebc35612ab9e4609397dd49c32308ea5a1983f765fe3ec3a4a` | `f02ff61105f1f5ad1dce29e2a9c36e50dd4d5465ff5b48dd9440059059272d83` |

At that health-fix stage the `ccr` closure remained
`8ad77748b8fd9e6eb31853cc9615730fc632a890898321deb915546e384ad246`.
The emitter observation was job `job_01a006c9ed6e7972aae089b26e152440`.

The later selector/path/daemon/idempotency/sidecar repair widened the legitimate
release-source census again. After production source quiesced, one final emitter
run and manifest refresh produced this cumulative chain from the previously
documented health-fix values:

| Pin | Previous documented value | Final candidate value |
|---|---|---|
| `cache` closure | `760c5da2d416d7e654ef1adc32a126143f1f404608d05e4a31ef84a6c7a0ebb0` | `90ac7e74c17485d9970a9fb8391e0a939997768bab8c08e0597e04458634d456` |
| `callbacks` closure | `f0dd5624ff538db95b023a23bd02e163c3e10880fe9cd16519208a103f65eaa5` | `5c0f31ac8c807e6cd81520e1e9af70056a6948bf18f36a9784c84471209d29a5` |
| `publication_roots` closure | `555e5219eee5668808ca81adadecf42e25e8fbe4f3f245e69db2a857e53826a5` | `9f8bcc30509c88f150828c26e868f740a1eba4690081ebfaf00091e9f36fbb7b` |
| `writers` closure | `22fa98fd6ef5dbb72f3088039f4a07111e1fcb8beb5ffefb787d6b71b61b7b36` | `8121e3478e4dc533208975575637db42ace2fa8297a22592b3ba19d0e4491273` |
| `ccr` closure | `8ad77748b8fd9e6eb31853cc9615730fc632a890898321deb915546e384ad246` | unchanged at `8ad77748b8fd9e6eb31853cc9615730fc632a890898321deb915546e384ad246` |
| second-order `retirement_records` | `d5b0003cd8e7150417495f89ef46a901bdd392a8287f211fae15b6fcc1758464` | `aaf7f6a276478b3f297fa6c1eee6880ccc0e8ceeb3b805cb7f8efeb025d8ce59` |
| raw contract hash in the refreeze manifest | `f02ff61105f1f5ad1dce29e2a9c36e50dd4d5465ff5b48dd9440059059272d83` | `4f6272565ca16c700cebee25222a4b73eba951b79bad92e1e926e6e1fdc07ae5` |
| validator raw SHA-256, committed baseline to final | `8585caa152455dc7a22f93a5ded63095bd550c11eeae2c802a5795684f52ab76` | `3c9836dd19f3cb82fbfcc4bad4af391d95e93b51bc89adf8f88e59ddb6fcf23b` |
| refreeze manifest raw SHA-256, committed baseline to final | `e1d083d338d4bae9dd3ff9a110acd1ed5fd83030480eaff822af04f0ae1bc9a9` | `8333b03e5829daadbcb60b0547e1ac81bed5e5d400bf8bc0b59576ab8dc2e6fe` |

Correction, recorded by the Round-2 fresh review: the five closure cells in the
final-value column above originally quoted the validator's never-moved
RETIREMENT_MEMBER_DIGESTS instead of the contract's content-closure digests at
the reviewed candidate — a documentation error that pre-existed at `e8d5ae5f`
and was missed by all three Round-1 reviewers. The cells now carry the actual
`e8d5ae5f` contract values, recomputed from `git show` of that commit; the
`ccr` closure never moved in PR 4. The second-order rows below them were
correct as originally recorded and are unchanged.

The final LF-normalized checker is clean at 78 requirements / 24 acceptance
oracles / 13 retirement categories. No member/path set, authority assignment,
or normative clause changed; only the designed source-census closures and their
second-order pins were regenerated for the expanded PR 4 repair. The detached
attestation's manifest pin was correspondingly rebound from
`e1d083d338d4bae9dd3ff9a110acd1ed5fd83030480eaff822af04f0ae1bc9a9`
to `8333b03e5829daadbcb60b0547e1ac81bed5e5d400bf8bc0b59576ab8dc2e6fe`;
the attestation remains explicitly not an approval or signature.

### External review of immutable candidate `e8d5ae5f`, and the C1–C3 repair round — binding current status

Three independent external reviews of the full immutable PR 4 range
(`6d1c58df..e8d5ae5f`) were received: Composer 2.5 (CLEAN), Grok 4.6
(FINDINGS — 1 MAJOR, 1 MINOR), and Kimi K3 (CLEAN with two reported MINORs and
a `tools.rs` sampling-coverage caveat). Adjudication was by candidate source
and execution path, not reviewer vote count; the full intake and
per-finding adjudication ledger lives outside the repository at
`C:\AI_STUFF\PROGRAMMING\LEDGER-symforge-feature-020-slice3-pr4-external-reviews.md`
so the immutable candidate stayed clean during review. The consolidated
verdict was **FINDINGS — do not land the candidate unchanged**:

| Canonical key | Disposition | Root cause |
|---|---|---|
| C1-ASK-NESTED-TARGET | CONFIRMED MAJOR — landing blocker | After an allowed whole-call HOME fallback, every nested tool dispatch inside `ask` set `project: None`; a recovered or concurrently retargeted daemon could serve ACTIVE instead of the project against which `ask` classified. Neither CLEAN review exercised this path, and Kimi's own coverage caveat names the under-sampled diff region it lives in. |
| C2-HOOK-DIAGNOSTIC | CONFIRMED MINOR | The all-fail-open branch of `format_hook_adoption` hardcoded the "no sidecar found" diagnosis even when every failure was a counted sidecar error, suppressing the later actionable sidecar-error message — a reporting-invariant violation. |
| C3-ENV-AUTHORITY-COMMENT | CONFIRMED MINOR | The comment at the `bind_workspace_from_client_roots` env-over-roots gate named the narrower legacy predicate rather than `workspace_root_env_is_authoritative()`, which production actually gates on. Behavior correct; explanation stale. |
| C4-FACADE-PARENTDIR-DOC | FALSE POSITIVE | The claimed doc/behavior mismatch in `facade_path_is_repo_relative` is contradicted by the caller's physical-containment check, which the reviewer themselves confirmed rejects escape. |

R1's informational sidecar observation was classified an expected operational
environment behavior, not a candidate defect, and R3's sampling caveat is
retained as a review-scope limitation. The three approved residual families
(D16, cancelled/timed-out `index_folder`, T051's lexical/reviewed-baseline
seal ceiling) are unchanged.

**The repairs, test-first.** RED witnesses were authored and observed failing
before each fix. C1: `ask` now snapshots the daemon client's resolved project
id once at handler entry (`nested_project`) and passes it in every one of the
13 nested tool dispatches, so a recovered daemon cannot reinterpret omission
as a sibling; two regression tests drive a failing-`ask`-then-echo daemon
fixture through the explicit-HOME and the omitted-HOME/concurrent-ACTIVE-
retarget races (`daemon_proxy_ask_fallback_keeps_explicit_home_for_nested_route`,
`daemon_proxy_ask_fallback_pins_omitted_home_across_active_retarget`). C2: the
"no sidecar found" branch gains the `total_sidecar_error == 0` conjunct, and
two formatter controls pin both the honest no-sidecar and the honest
all-sidecar-error renderings. C3: the comment now names the authoritative
predicate. The four new library tests raise the library target from 3,215 to
3,219; no production behavior outside the two fixes changed.

**Repair-mutation sensitivity, re-observed on the frozen bytes.** Reverting
the covered nested pin (the `FindSymbol` arm's `search_symbols` dispatch) to
`project: None` turned BOTH ask-fallback regressions RED (exit 101, 1/2/0,
`job_01a00fb5a27572309163ad9a89dc15aa`); byte-exact restoration was verified
by SHA-256 and the family returned 3/0
(`job_01a00fb70b4770c287ff2069bc3ecf35`). Removing the formatter's
`total_sidecar_error == 0` conjunct turned
`test_format_hook_adoption_names_all_sidecar_errors_honestly` RED (exit 101,
4/1/0, `job_01a00fb67215788088ba6072fb4b664d`); byte-exact restoration was
verified by SHA-256 and the family returned 5/0
(`job_01a00fb75a227481bcf42b2de07acbb6`). One honest coverage note: reverting
a nested pin on a route the regressions do not drive (the `FindReferences`
arm) left both regressions green (3/0,
`job_01a00fb496387542ae815c78df20a7f9`) — the regression pair witnesses the
snapshot-and-propagate mechanism through the routes it drives, not each of
the 13 dispatch sites individually; per-site correctness rests on the uniform
`nested_project.clone()` pattern, confirmed complete by inspection (no
`project: None` remains anywhere in the `ask` dispatch region).

**Census regeneration for the repair.** The C1 fix changes production bytes in
`src/protocol/tools.rs`, owned by exactly the `writers` closure; the C2 fix in
`src/protocol/format.rs` is owned by no closure category; the C3 comment and
all test additions are invisible to the normalized production census. The
opt-in emitter confirmed only `writers` moved while `cache`, `callbacks`,
`publication_roots`, and `ccr` stayed byte-identical, and the designed
regeneration procedure updated the second-order chain:

| Pin | Reviewed-candidate value | Repaired-candidate value |
|---|---|---|
| `writers` content closure (contract) | `8121e3478e4dc533208975575637db42ace2fa8297a22592b3ba19d0e4491273` | `780e468ecb7298c74e5f94952e821dac551f2459a983cab85d7b9d9b1b70e04a` |
| second-order `retirement_records` (validator) | `aaf7f6a276478b3f297fa6c1eee6880ccc0e8ceeb3b805cb7f8efeb025d8ce59` | `3b6870a3923476cbbdad962efdf1b1fb893c5ec3a3e29d5bf936d8fd4c22513d` |
| raw contract hash in the refreeze manifest | `4f6272565ca16c700cebee25222a4b73eba951b79bad92e1e926e6e1fdc07ae5` | `91642250d0400456c4cbe844c7b54d575d80ad56d9c897e5ce6c6611c8e63f74` |
| manifest pin in the detached attestation | `8333b03e5829daadbcb60b0547e1ac81bed5e5d400bf8bc0b59576ab8dc2e6fe` | `581a91ff18651677794f1008f73e9b8f1b137ea1543a6d5a9816c7ff8a8c5f37` |
| whole-`src/` T051 seal tuple | `8cf143e41d269ab4e0fcf1c48e09c4323d7ebc74020f3eb24e8b4d45cdc9c2cb` / 187 / 8,968,263 | `7ba5c4b3c2c82a2963df28a6d1559857b41f3db34e83019d57380e19369d9d04` / 187 / 8,979,117 |

The retirement member digests did not move: no member entered or left any
category. The 187-file count is stable — the repair edited existing files
only. The repaired seal tuple was additionally re-derived by an independent
re-implementation of the fingerprint (domain, LE-u64 count, per-record
length-prefixed path and LF-normalized content over a sorted `src/` walk) and
matched byte-exactly.

**Session provenance.** The repair session that authored the fixes terminated
mid-gates when its credit budget was exhausted: its debug build and both
debug-fixture harness runs were directly observed green, but its release build
was still in flight and its full-suite, clippy, and embed receipts for the
repaired bytes were either pending or lost to context compaction. A successor
session re-observed EVERY gate below live on the identical frozen bytes —
identity established by the per-file SHA-256 values here and the whole-source
seal — so no row below relies on the terminated session's unwitnessed state.

**Round-2 fresh review of immutable candidate `9de4f696`, and the three MINOR
repairs it forced.** The repaired candidate was committed as immutable
`9de4f69639beffc66d0a5828cdbc731ee41b7e2c`, the branch was pushed, and a fresh
four-lens adversarial review (repair-diff end-to-end, C1-adversarial,
evidence audit, full-range integrity) ran against the complete immutable range
with no concurrent edits; every finding was then independently challenged by
two adversarial refuters. The C1-adversarial and full-range-integrity lenses
returned CLEAN — every constructed attack on the nested-pin repair failed with
grounded reasons (the snapshot is the immutable per-connection HOME project
id; daemon recovery serves HOME via explicit-id resolution; an explicit
foreign outer selector is routed daemon-side or refused before the snapshot;
embedded `None` cannot reach daemon routing; no idempotency interaction), and
the census/seal chain was independently recomputed, including a second
independent re-derivation of the whole-source seal. Three findings survived
refutation, ALL MINOR, none behavioral: (1) the C2 fix's mixed
nothing-routed quadrant had no pinned oracle, so a crafted conjunct mutation
would have survived every existing test; (2) the superseded census table above
quoted never-moved member-list digests as `e8d5ae5f` closure values (corrected
in place above, with the correction noted); (3) the LF-audit row said "nine
changed paths" where the committed range changed eleven (reworded above). The
repairs: a third formatter control now pins the mixed quadrant
(`test_format_hook_adoption_mixed_no_sidecar_and_errors_stays_honest`), and
its oracle power was proven by observing the reviewer's exact crafted
mutation (`total_sidecar_error <= total_no_sidecar`) survive the five prior
oracles but die on the new one (5/1/0, exit 101,
`job_01a00fd78ba97840bfa3bf7c754c8998`), with `format.rs` then restored
byte-exactly (`beee0cf2…`) and the family green at 6/0. The test addition
moved the whole-source seal; the pin was regenerated RED-first (observed RED
printing the new tuple, `job_01a00fd8219c7db2b537098614504822`, then green
8/0). The checklist below carries the receipts re-observed after these
repairs.

#### External-review repair-candidate checklist — binding current status

| Required evidence | Binding command or observation | Current status |
|---|---|---|
| Final T051 source pin | Record the whole-`src/` SHA-256 tuple, file count, and LF-normalized byte count after every production source edit is complete. | **OBSERVED ON FROZEN SOURCE** — `78f32c8921a1c1878fc13c29aed8775914d6beb1dfeb878048e5fb9166f67bcb` / 187 files / 8,980,758 LF-normalized bytes, held by `FULL_SOURCE_PIN_V1` after the two post-closure CI repairs and the recompute-script correction below. Any later `src/` edit invalidates this row. |
| Repair regression families | `daemon_proxy_ask_` and `format_hook_adoption` library filters, serial. | **OBSERVED** — ask family 3/0 (`job_01a00fb70b4770c287ff2069bc3ecf35`, post-mutation-restore); formatter family 6/0 with the Round-2 mixed-quadrant control (`job_01a00fd6d9a27ab2b76e87ea708e01b3`, re-confirmed post-restore in the receipt below). |
| Repair mutation sensitivity | Revert each repair's guard, observe the intended witness RED, restore byte-exactly, re-observe green. | **OBSERVED** — nested-pin revert RED 1/2/0 (`job_01a00fb5a27572309163ad9a89dc15aa`); formatter-conjunct removal RED 4/1/0 (`job_01a00fb67215788088ba6072fb4b664d`); both restorations SHA-256-verified (`tools.rs=3976dcce…`, `format.rs=beee0cf2…`) and re-green. |
| Selector containment controls | `daemon_facade_` library filter, serial. | **OBSERVED** — 12/0, `outcome_trust: observed`, `job_01a00fafe3ed7bc3a72b1deebe9cb9bf`. |
| Layered T051 controls | `cargo test --test preventive_runtime_dark_v11 -- --test-threads=1` on the final bytes. | **OBSERVED** — 8/0 with the regenerated pin, `outcome_trust: observed`, `job_01a00fd898217901ac877a4628262555`; the pin regeneration itself was observed RED-first (`job_01a00fd8219c7db2b537098614504822`); the reviewed candidate's whole-source macro/alias mutation receipts remain valid guard evidence for the unchanged guard implementation. |
| Focused dark/public suites | `runtime_dark_v11` and `public_api_delta_v11`, serially. | **OBSERVED** — 11/0 (`job_01a00fb0299274608af87ab345b6189a`) and 2/0 (`job_01a00fb0470872e0857c70a89e27d977`). |
| Overlay exactness | `cargo test --test activation_cut_v11 all_ingress_uses_exact_typed_authority_branch -- --exact`. | **OBSERVED** — 1/0 with 4 filtered, `job_01a00fb066977f0081f13a99c1837e41`. |
| Formatting and lint | `cargo fmt --check`, `git diff --check`, lifecycle traceability, `cargo clippy --all-targets -- -D warnings`, LF audit. | **OBSERVED** — re-observed after the Round-2 repairs: fmt and worktree diff checks clean; traceability OK at 78 requirements / 24 acceptance oracles / 13 retirement categories; clippy exit 0 (`job_01a00fdae4da7ca28bb41573bb56c966`); all changed paths, including the two review documents finalized after the nine-path source audit, report `w/lf` with `eol=lf`. |
| Embed feature | `cargo test --no-default-features --features embed --lib -- --test-threads=1`. | **OBSERVED** — re-observed after the Round-2 repairs: 1,333 passed / 0 failed / 4 ignored, exit 0, `outcome_trust: observed`, `job_01a00fe493ea71a1b0223b40b50335b7` (the new formatter control is server-gated and correctly absent under embed). |
| Debug binary + both harness fixtures | Debug build current with the frozen bytes, then both `verify-tools.cjs` fixture sets. | **OBSERVED** — re-observed after the Round-2 repairs: build exit 0 (`job_01a00fe526477ac1a230f9e77d72f8ac`); synthetic fixture 7 PASS / 1 REVIEW / 0 FAIL and real fixture 10/1/0, the same two expected human-adjudicated REVIEW dispositions as the reviewed candidate. |
| Release binary + both harness fixtures | Release build current with the frozen bytes, then both fixture sets against that exact binary. | **OBSERVED** — re-observed after the Round-2 repairs: build exit 0 in 346,950 ms with `outcome_trust: observed` (`job_01a00fe5456472f1aa079d16c022d156`); synthetic fixture 7/1/0 and real fixture 10/1/0 against that exact binary, agreeing exactly with debug; observed locally, not deferred to PR CI. |
| Full suite | `cargo test --all-targets -- --test-threads=1` with a live `outcome_trust: observed` receipt on the final candidate. | **OBSERVED** — re-observed after the Round-2 repairs: exit 0, `outcome_trust: observed`, 587,665 ms, `job_01a00fdb584f7533ba7fb026d1e1961e`; the main library target reported 3,220 passed / 0 failed / 5 ignored and all 127 test targets completed cleanly. The count is the target census — 125 `tests/*.rs` integration targets plus the lib and the default-feature bin — not the deduplicated signal-stream count, which suppresses byte-identical empty-target result lines; Round 3 caught exactly that lossy-channel miscount ("122") in the original wording of this row. |
| AAP receipt check | `python execution/aap_migration_receipt_v11.py --stage full --check`. | **OBSERVED** — exit 0; real lane 71 cases (35 resolution-failure, 33 compiles, 3 expected-failure), adapter lane 35 expected-failure rows; the regenerated receipt honestly flagged the dirty pre-commit worktree and its diff was discarded, since T052 does not mint a receipt. |
| Non-closure commit | Commit the complete repaired candidate with a non-closure subject after all rows above are green. | **DONE** — two non-closure commits: immutable `9de4f69639beffc66d0a5828cdbc731ee41b7e2c` (`fix(slice3): repair C1-C3 external-review findings [non-closure]`) and immutable `9133bb36a499fafc42ad479902d67f589f167795` (`test(slice3): repair round-2 review MINORs [non-closure]`). |
| Fresh PR 4 review | Review one immutable committed full PR 4 range with no concurrent edits; adjudicate every substantive note. | **DONE — adjudicated across Rounds 2 and 3.** Round 2 (four lenses on the full immutable range at `9de4f696`): CLEAN on both code lenses, three MINORs, all repaired in `9133bb36`. Round 3 (two lenses on the repair delta at `9133bb36`): delta-audit CLEAN with every hash, seal, and census value independently re-derived; claims-audit found exactly one MINOR — the "122 test targets" lossy-channel miscount in this checklist — whose prescribed correction is applied in this closure commit. Zero code findings across both rounds; every finding was independently challenged by two adversarial refuters. This is recorded as the honest disposition rather than an unqualified CLEAN: the final surviving finding was an evidence-count cell, corrected here as the review itself prescribed. |
| Evidence/ledger closure commit | Only after a trustworthy CLEAN review, record its archive and final receipts in a separate evidence/ledger commit. | **THIS COMMIT** — records the Round-2/Round-3 dispositions, the census-derived target count, and the closure of the repair loop. The consolidated external-review adjudication ledger remains archived outside the repository. |

**Post-closure CI repair — Linux-only test-compile defect.** The PR CI `rust`
job failed at the clippy step on Linux with `E0277`: the `#[cfg(unix)]` test
`daemon_session_open_refuses_non_utf8_roots_before_transport` (added with the
PR 4 candidate) called `.expect_err(...)` on a
`Result<DaemonSessionClient, _>`, and `DaemonSessionClient` implements no
`Debug`. The cfg-gated body never compiles on Windows, so every local gate and
all review passes were structurally blind to it — the same defect class as the
documented embed-cfg trap, on the platform axis. The repair restructures the
assertion into a `match` (test-only; no production bytes moved; no `Debug`
impl added). A sweep of every `expect_err`/`unwrap_err`/`{:?}` site inside
`#[cfg(unix)]` bodies across the PR 4 paths found no second instance, and the
Linux compiler pass corroborates: it reported exactly one error for the whole
lib-test crate. The whole-source seal was regenerated for the edit
(independent re-derivation; the preventive suite and PR CI re-prove it) and
`daemon.rs` is now
`9488bb0c11759060ec6d62f3bac7a20f591e6c313c0fd664219dec794aa0454c`.

The re-run then surfaced a second member of the same blind-spot class — this
time a semantic one. The `#[cfg(unix)]` test
`local_project_selector_does_not_alias_non_utf8_root_to_lossy_utf8` executed
for the FIRST time anywhere on that CI run and failed: it expected a server
bound to a non-UTF-8 root to publish the native-bytes `project_id` with only
the lossy string suppressed, but production's fail-closed root discovery
(`resolve_root_candidate` returning Unbound) refuses such a root at binding —
the same stance the daemon session-open and sidecar-descriptor lanes enforce,
both of whose refusal tests passed on the same run. The test encoded a
superseded design intention; the shipped refusal-everywhere design makes its
premise unreachable. The repair aligns the test with the fail-closed stance
and strengthens it: it now asserts the binding itself is refused
(`capture_repo_root()` returns None), that `project_id` is `unbound`, and
that no lossy string form is published. The whole Linux lib target otherwise
passed (3,226/1/5), so no third member of the class remains in the library;
the two `#[cfg(unix)]` sites in the PR 4 integration targets were swept and
are a platform helper and a both-branches-tolerant test. Seal regenerated
again; `tools.rs` is now
`14b9fee3bd0786620675d6bfa8fc7a85b5b3bba4f2e0a867bd33b80dd46e37f5`.

**Recompute-script correction — the seal values in the two repairs above were
initially wrong, and the Rust oracle caught it.** The out-of-band Python
recompute used for both post-closure repins kept the `src/` prefix on record
paths, while `normalized_source_records` strips the src root before joining
components — a hash-only divergence, which is why both wrong values carried
the CORRECT file counts and LF byte totals. The Rust seal test itself caught
the drift on its first execution against a recomputed pin (printing the true
actual), the script was corrected and validated byte-exactly against that
Rust-printed tuple on a known tree, and the binding pin now carries the
Rust-oracle-consistent value quoted in the source-pin row above. The lesson
is the reporting invariant again: only the Rust test is the thing that knows,
and every out-of-band recomputation must be validated against it on a known
input before its output is pinned.

Principal frozen-byte SHA-256 values of the repaired paths at final observation
are `tools.rs=3976dcce27263acae75dff541bc59d80b0127e0a1983424ee15e28f461ee78ce`,
`protocol/mod.rs=6f001983e346821318078dcca0bfae24ca8ee8aa97e4055f8ad46c069012367f`,
`format.rs=beee0cf2781b8476892c2aad1d5e0aa14dbe6661d083088796f66d2337e1e1b5`,
`format/tests.rs=69c6092aa1d51fc8156cfbc16c4f9282fbcce7784d842885a97819fd78f0cb3a`
(after the Round-2 mixed-quadrant control),
`preventive_runtime_dark_v11.rs=01a44f13796d3a999a659ab16d81788cf47aacc2bad5a1c344527c0e2d9b81ef`
(after the Round-2 pin regeneration),
and
`validate-lifecycle-oracle-traceability.cjs=90d1de9617f0c17c3a78b97a012cde4fbf206247d5740aeaef5ed9b0d1e9d83a`.

### Round 2 of the post-slice review, and the hardening it triggered

Round 2's persisted output contains four confirmed entries: the same `target`
defect reported in both code and docs, plus two unique minors. Its executable
was a stale PR 3 / round-15 prompt, so those findings are evidence, but silence
elsewhere is not coverage. A corrected Round 3 subsequently reviewed the
immutable
`6d1c58df731910b6e8ee6c5a61a5d01f9e3be8ae..606bbeb50ac11c781f9337a7109be290f8a93b08`
range with no concurrent edits and with every candidate and substantive note
independently adjudicated. Its result is recorded below.

**MAJOR — the `target` skip was depth-blind while its justification assumed
otherwise.** The round-15 comment said the three skipped directories shared one
reason: "every one is gitignored, so a config placed there cannot be COMMITTED".
The conclusion holds for `.git` because it is repository metadata and for
`node_modules` because `.gitignore:22` is unanchored, but it is **false for
`target`**: `.gitignore:1` is `/target`, ROOT-ANCHORED. Measured with Git rather
than inferred — `git check-ignore` reports
`execution/target/.cargo/config.toml` and `npm/target/.cargo/config.toml` as not
ignored, while `target/.cargo/config.toml` and
`npm/node_modules/.cargo/config.toml` are ignored. The repository's own
`.gitignore:23 spacetime/*/target/` exists precisely because line 1 does not
reach nested targets. Since the walk skipped `name == "target"` at every depth,
moving the round-14 exploit file into `execution/target/.cargo/` made it
invisible while the suite stayed green.

**Repair: the walk and its claimed bound.** "Committable" for this pin means
normally add-visible: `git check-ignore` does not ignore the path, so an ordinary
`git add` would see it. Force-add is outside the bound, like a config outside
the repository. `.git` and `node_modules` remain skipped at any depth; `target`
is skipped only as the repository root's own child. A nested `target/` is walked
because it can be committable. Mutation **M64** placed
`execution/target/.cargo/config.toml`, observed it caught by name, restored it,
and confirmed that the root `target` skip still holds.

The pre-round-3 audit then hardened three more parts of the same bound:

- `.cargo` matching is ASCII-case-insensitive on Windows and when Git records a
  case-insensitive checkout, closing the Windows `.CARGO` false green without
  pruning an ordinary upper-case directory on Linux. Skip-name matching for
  `.git`, `node_modules`, and root `target` follows Git's `core.ignorecase` for
  the same normal-add boundary.
- Directory, entry, and metadata observation failures now fail closed instead
  of returning, flattening, or becoming a false `is_file == false`.
- The exact `/target` and `node_modules/` lines and the whole `.gitignore` file
  are pinned as change detectors. `git ls-files` must also report no tracked
  path below a skipped directory, so an ignored-after-tracking config cannot
  hide there.

The two unique round-2 minors were reporting errors and are repaired here too:
the AAP row now separates its 71 real-lane cases from the 35 adapter-lane rows,
and the universal "every in-tree config/directory" comments are narrowed to the
actual normally add-visible, in-repository, non-`.cargo`-CWD bound.

The hardening was exercised with three fresh RED/control mutations, all
restored before the candidate gates:

- **M65 — Windows case alias.** Planting the normally add-visible
  `execution/.CARGO/config.toml` made `no_gate_builds_doctests` fail and name the
  path; removing it restored the focused test to 1/0.
- **M66 — skip-rule drift.** Changing `.gitignore`'s exact `/target` line to
  `/target/` preserved the relevant ignore behavior but changed the whole-file
  fingerprint; the focused test failed with the observed and expected
  fingerprints, then passed after the exact file was restored.
- **M67 — tracked-then-ignored path.** An isolated temporary Git index made
  `target/slice3-tracked-skip-probe/config.toml` tracked without touching the
  real index. The tracked-under-skips guard failed and named the path; the
  temporary index and probe were removed, and the focused suite passed 4/0.

The traversal now panics on directory, entry, and metadata observation errors.
That fail-closed behavior is covered by source inspection and the compiled
gates above; no Windows permission mutation is claimed.

### PR 4 post-slice Round 3 — trustworthy FINDINGS on
`606bbeb50ac11c781f9337a7109be290f8a93b08`

Round 3 reviewed the complete immutable range
`6d1c58df731910b6e8ee6c5a61a5d01f9e3be8ae..606bbeb50ac11c781f9337a7109be290f8a93b08`,
not only the final repair commit. Finders read pinned Git objects and did not
edit, stage, commit, mutate fixtures, or run Cargo. Post-review integrity
re-established the same clean worktree, branch, base, review SHA, merge base,
and four-path changed set. The archive is
`C:\AI_STUFF\PROGRAMMING\symforge-review-artifacts\feature-020\slice3-pr4\round-3\606bbeb50ac11c781f9337a7109be290f8a93b08`;
its `SHA256SUMS` file hashes to
`d58e286a2aa2e32e552cd3fc775738ae65bee03a850d7a82ca44d6c929a9c25e`
and pins all nine substantive artifacts. The archived result records
`run_integrity_valid: true`, `verdict_trustworthy: true`, and
`review_status: FINDINGS`.

| Disposition | Count | IDs |
|---|---:|---|
| confirmed blocker | 1 | `R3-DR-001` |
| confirmed majors | 10 | `R3-WP-001`, `R3-WF-001`, `R3-WF-002`, `R3-WD-001`, `R3-WD-002`, `R3-OS-001`, `R3-OS-002`, `R3-OS-003`, `R3-OS-004`, `R3-ROOT-001` |
| confirmed minors | 5 | `R3-WP-002`, `R3-WP-003`, `R3-WD-003`, `R3-EP-001`, deduplicated `R3-WD-004 / R3-OS-005` |
| actionable substantive notes | 2 | `R3-WF-N01`, `R3-WD-N01` |
| accepted nonblocking notes | 1 | `R3-WD-N02` |

`refuted_candidates: 0`; `inconclusive_candidates: 0`;
`dead_dimensions: []`. Every finder and adjudicator completed. Overlay
mechanical closure and contract/ledger consistency were clean dimensions. One
`diff_symbols` worktree-reachability subclaim was refuted while the narrower
Git-authority candidate remained confirmed, so it does not increment the
refuted-candidate count.

The blocker demonstrated that the lexical darkness sweeps are not a compiler
call graph: a crate-local trait/type outside the excluded module can receive an
impl inside `index_lifecycle`, and an outside caller can dispatch through the
trait without spelling the guarded module name. The same exclusion class
applies to `server_api.rs`. The repair is deliberately layered. The direct
outside caller/splice sweeps remain diagnostic; a SHA-256 seal over the 13
excluded Rust sources diagnoses semantic drift inside the dark implementation
set; and a second, load-bearing SHA-256 seal covers every regular source
candidate beneath `src/`. The whole-source seal is what catches an outside
alias, trait, registration, re-export, or macro bridge that changes no excluded
byte. The exact explicit production lib/bin topology in root `Cargo.toml` is
also pinned and confined beneath that same source root, so an arbitrary or
extensionless target cannot escape the source set. Review of the narrow
baseline found 82 explicit impls, every self type defined inside that sealed
set, and no outward alias, registration, or exported-ABI bridge. This is
reviewed semantic-baseline preservation, not a general Rust name-resolution
proof. The final whole-source tuple is now frozen and observed in the binding
checklist; the required macro/alias mutation is observed in M82 and the
binding checklist.

The walk findings require one coherent boundary rather than isolated string
fixes: both source and Cargo-config walks fail closed and sort observations;
descendant links and Windows reparse points are refused and canonical directory
identities stay below a visited root; `.cargo` aliases are decided by actual
filesystem identity, not `core.ignorecase`; root-config identity must be found
exactly once; root `.gitignore` must be a regular non-link file; and Git decides
whether a concrete root `target` or `node_modules` directory is ignored before
it may be skipped. `git check-ignore --no-index --stdin -z` receives one exact
NUL-framed pathname, prefixed with lexical `./` so pathspec-looking bytes remain
literal. Immediately before a skip, a literal, case-insensitive
`git ls-files -z` query must return no raw bytes; that output is never decoded.
The source walk now observes every regular file beneath `src/`, while an exact
root `Cargo.toml` topology pin requires every explicit production lib/bin target
to remain beneath that source root. Fingerprint records serialize normalized
path components with `/` rather than replacing separators in an already-joined
path; a nested `a/b.rs` and a literal `a\b.rs` filename therefore cannot
collapse to one record. Non-normal or non-UTF-8 components fail closed, and
sorted records must remain unique. Together the repairs close the
symlink/escape, fail-open, nested-unignore, pathspec-magic, case-renamed tracked
path, arbitrary Cargo target, opaque-filename, path-record-collision,
nondeterministic-order, root-alias, and anti-vacuity findings within the stated
in-tree bound.

#### Semantic overlay supersession and exhaustive second sweep — cardinality unchanged

Round 3 **identified** twelve assignment defects and one stale citation; the
post-Round-3 candidate maps those repairs and the exhaustive second body-level
audit across all 102 rows. That wider audit found a materially broader class:
dry-run edit modes, disk-degradation results, worktree diagnostics, sidecar
aliases, hook refusals, source-free successful modes, and subordinate Git/state
effects had not been classified consistently.

The shipped schema-hidden, production-reachable facade relay also allowed
write-capable legacy tools behind the read-only `symforge` annotation; the scoped
production repair closes that path instead of widening the facade to all eight
states. Its exact allowlist is nine read measurements plus `batch_rename` only
when `dry_run=true`. Outer facade `project`/`projects` selectors are not a back
door into this compatibility relay. "Source-mutation-safe" is the deliberate
bound: normal read-path cache, frecency, coupling, reconciliation, and
index-refresh effects may still occur. The relay preserves raw legacy renderer
text and emits no `symforge/result_status`, because arbitrary returned source
text cannot prove a semantic outcome class. The shared MCP boundary may still
attach selected `symforge/project_evidence`, but an admitted relay is exempt
from its rendered-text `isError` heuristic; a real `/mcp` regression test pins
exact `Error:`-prefixed file content as successful data. The result is
result-status-free, not metadata-free.

Containment of the normal compact planner is separate from containment of the
relay. A local or degraded topology refuses a foreign selector before preview,
cache-hit, PFF-bypass, economics-bypass, or any other early result. A healthy
daemon injects the selector into every planned primitive. While such a foreign
request is routed, adapter-HOME path checks, source-byte economics, temporal
co-change footers, fusion anchors, and root evidence are disabled; only a typed
daemon receipt that names the selected project may be rendered or attached. A
missing or mismatched receipt produces an explicit unavailable-root line and no
typed project evidence. The complete selector suite and its locality-inversion
mutation are observed in the immutable-candidate checklist and M83.

The existing `verify-tools.cjs` harness is hardened along with that boundary.
Legacy read cases continue through the exact raw A-019 relay and
`batch_rename` remains dry-run-only. Every declared `must_contain` substring is
now a hard FAIL for every judge. Source-bearing results must expose either the
compact leading `Trust:` envelope or the expanded `Source authority:` envelope;
an intentionally empty result is accepted only when the case declares its exact
empty-result prefix. That last rule exposed and corrected the real fixture that
mistook two `as_str` definitions for references. Both fixture sets are directly
observed green against the debug and release binaries in the checklist above;
no harness green is inferred from source inspection.

This repair does not claim global facade-status closure. Normal STEL execution
still uses the pre-existing `classify_compact_tool_output` rendered-text
classifier; that classifier is byte-unchanged by this repair and has known
truncation/caller-text ambiguity. A complete follow-up must carry typed outcome
data before rendering. No Feature 020 authority assignment or T052 gate below
uses those inferred outcome classes as evidence.

A mechanical parser compared the post-Round-3 overlay candidate with pinned
`606bbeb50ac11c781f9337a7109be290f8a93b08`: 102 rows remain 102, with no member
addition/removal; 60 rows change allowed sets and 42 remain unchanged; 91
authority cells are added and 10 removed, net +81. Total authority cells move
from 185 to 266. The partition remains 102 `SURFACE_OVERLAY` rows + 3
`NON_INGRESS_EXCEPTIONS` + 11 `AUTHORITY_FREE_INGRESS` members = 116. The
eight-state vocabulary and approved eleven-slot T066 residual are unchanged.

| Surface | Changed rows | Added cells | Removed cells |
|---|---:|---:|---:|
| compatibility aliases | 2 | 2 | 0 |
| tools | 25 | 39 | 3 |
| writers | 12 | 23 | 0 |
| sidecar | 8 | 8 | 4 |
| hooks | 6 | 8 | 3 |
| resources | 7 | 11 | 0 |
| **total** | **60** | **91** | **10** |

| Authority | Baseline cells | Candidate cells | Added | Removed |
|---|---:|---:|---:|---:|
| `DiskObserved` | 8 | 34 | 27 | 1 |
| `GenerationLeased` | 50 | 76 | 28 | 2 |
| `GitObserved` | 11 | 5 | 1 | 7 |
| `MutationPermitted` | 33 | 33 | 0 | 0 |
| `Refused` | 60 | 83 | 23 | 0 |
| `RuntimeHealthObserved` | 10 | 11 | 1 | 0 |
| `StateWriteAuthorized` | 8 | 9 | 1 | 0 |
| `WorktreeScopeObserved` | 5 | 15 | 10 | 0 |
| **total** | **185** | **266** | **91** | **10** |

Reproduction recipe: read the baseline with
`git show 606bbeb50ac11c781f9337a7109be290f8a93b08:tests/activation_cut_v11.rs`
and the candidate with `Get-Content -Raw`; isolate `const SURFACE_OVERLAY`
through its closing `];`; parse rows with the single-line expression
`\(\s*"(?<cat>[^"]+)"\s*,\s*"(?<member>[^"]+)"\s*,\s*&\[(?<allowed>.*?)\]\s*,`;
extract quoted authority names from `allowed`; key on category plus member; and
set-diff both maps. Both independent runs asserted 102 keys, no key-set delta,
and 185 → 266 authority cells before producing the tables above.

The ten removals are classified by the authority table above: seven
`GitObserved`, two `GenerationLeased`, and one `DiskObserved`. The principal
additions are exact-mode dry-run branches on the seven edit tools and their
writer duplicates; disk modes on read/context/sidecar surfaces; generation
modes on mixed impact derivations; worktree modes on applicable edit previews
and observation surfaces; typed refusal on loading-guard, resource, and hook
paths; the timestamp-generation mode on `what_changed`; and the
calibration-state mode on `status`. Published Git ranking/enrichment, edit
co-change footers, Tee/idempotency writes, and cache warming remain subordinate
effects rather than separate selected authorities. The final source line
citations are bound to the frozen Rust candidate and remain subject to the
pending immutable review.

The full 40-tool audit and all 62 non-tool rows were independently re-read. The
matrix above is the measured current delta, not an extrapolation from the original
twelve findings.

### Post-Round-3 repair and mutation evidence — historical, not T052 closure

Every mutant below was planted alone. Source mutants were reversed with an
exact patch, never `git restore`; the immediately affected files were then
SHA-256 checked against their preimages. Fixture controls were removed and the
real Git index remained unchanged. These observations prove the repair guards,
not a clean-review verdict. They are preserved verbatim as historical
observations on evolving post-Round-3 repair trees; later selector, source-seal,
overlay, and harness edits supersede them as current-candidate gates.

| ID | Guard removed or adversarial fixture | Focused observation | Restore/control |
|---|---|---|---|
| M68a | Read-directory error returned an empty successful walk. | `walk_observation_seams_fail_closed_and_controls_pass` RED: "read-directory error must not become an empty successful walk." | Exact reverse; accepting sorted-input control remained green. |
| M68b | Per-entry error was flattened away. | Same focused test RED: "directory-entry error must not be flattened away." | Exact reverse. |
| M68c | Optional metadata errors other than `NotFound` became absence. | Same focused test RED on injected `PermissionDenied`; the `NotFound` control remained accepted. | Exact reverse. |
| M68d | Link/reparse refusal became a no-op. | Same focused test RED: "link or reparse point must never be followed." | Exact reverse. |
| M68e | Sorted-child observation was replaced by an order-preserving no-op. | Same focused test RED with observed `z,a` vs required `a,z`. | Exact reverse. |
| M68f | Required metadata failure fell back to unrelated metadata. | Same focused test RED on the guaranteed-missing path. | Exact reverse; the real source-file metadata control remained accepted. |
| M68g | The regular-file pin accepted a directory. | Same focused test RED on `tests/`. | Exact reverse. |
| M68h | Canonical visited-set rejection was removed. | Same focused test RED on the second identity. | Exact reverse. |
| M68i | Canonical root containment was removed. | Same focused test RED when `src/` was admitted beneath the `tests/` root. | Exact reverse. |
| M69a | `git check-ignore` exit 1 was treated as skippable. | `cargo_walk_policy_controls` RED. | Exact reverse; exit 0/1 controls restore ignored/walk decisions. |
| M69b | Non-empty raw `git ls-files -z` output was accepted. | Same focused test RED on `opaque-FF` bytes without UTF-8 decoding. | Exact reverse; empty raw output control remained accepted. |
| M69c | The exactly-one root-config assertion was made tautological. | Same focused test RED on empty discovery. | Exact reverse; one-root control remained accepted. |
| M70 | Existing `execution/node_modules/.cargo/config.toml` was first ignored, then made normally add-visible by a nearer `execution/.gitignore` with parent/descendant negations. | Ignored control GREEN; after `git check-ignore --quiet` changed from 0 to 1 and `git status` showed both files, `no_gate_builds_doctests` RED and named the config. | Removing the nearer `.gitignore` restored GREEN; config and empty directories then removed. |
| M71 | The same ignored config was force-added only to an isolated temporary `GIT_INDEX_FILE`. | Focused test RED: one tracked path below `execution/node_modules`; the ordinary real-index control stayed GREEN. | Temporary index deleted, environment restored, real cached diff empty, fixture removed. |
| M72 | Normally add-visible `execution/.CARGO/config.toml` on Windows. | `git check-ignore` exit 1; focused test RED and reported logical `execution/.cargo/config.toml`, proving filesystem alias recognition rather than lexical casing. | File and empty directory removed; focused test GREEN. |
| M73 | Empty Windows junction beneath `src/`, targeting a separate empty directory. | Ordinary empty-directory control GREEN; junction control RED before recursion and named the exact reparse path. | Junction removed without recursion, external target verified intact then removed. |
| M74a | Dormant inherent method on `LiveIndex` defined in `index_lifecycle/runtime.rs`, called outside without the guarded module token. | Both old lexical darkness tests GREEN; the excluded-source seal RED at 13 files / 205,144 normalized bytes. | Runtime and caller restored byte-identically; reviewed seal GREEN. |
| M74b | Same dispatch shape with the inherent impl in `server_api.rs`. | The old `server_api` lexical test GREEN; the excluded-source seal RED at 13 files / 205,151 normalized bytes. | Server and caller restored byte-identically; reviewed seal GREEN. |
| M75a | Restore the case-sensitive `.rs` extension filter and present an upper-case `.RS` source candidate. | `walk_observation_seams_fail_closed_and_controls_pass` RED because the candidate vanished from the bounded source observation. | Exact reverse; the final walker observes every regular file beneath `src/`, independent of suffix. |
| M75b | Add an extensionless `execution/slice3-extensionless-probe` as an explicit `[[bin]]` target and call the public dark runtime without a guarded splice token. | `cargo check --bin slice3-extensionless-probe` GREEN, proving Cargo accepts the arbitrary target path; `no_gate_builds_doctests` RED at the exact production-target topology guard. | `Cargo.toml` restored to its exact preimage, the source removed, and no probe target remained. |
| M76 | Track `execution/node_modules/.cargo/config.toml` in an isolated index, then case-rename the on-disk parent to `NODE_MODULES` on NTFS. | The repaired focused test RED with one tracked descendant. Temporarily restoring the exact-case literal `ls-files` query made the same fixture GREEN, proving the former false green. | Query code restored byte-identically; directory renamed back; fixture, isolated index, and empty directories removed; real cached diff empty. |
| M77 | Ask the ignore seam about the literal pathname `:(top)target`, whose bytes resemble Git pathspec magic. | Before lexical `./` disambiguation, the check incorrectly selected the ignored root `target`; the repaired `--stdin -z` control returns visible while the real `target` control remains ignored. | Permanent paired controls remain in `cargo_walk_policy_controls`; no fixture or index mutation survives. |
| M78 | Short-circuit the A-019 measurement allowlist before dispatch. | `symforge_facade_rejects_mutating_probe_relay` RED: the denied `index_folder {}` reached the decoder and returned raw text without the typed denial receipt. The fixture is safe under the mutant because missing `path` prevents dispatch. | Exact reverse to SHA-256 `1e38c092c2f2c526b0297071af24dc476f6baaa3cf106a19d7e2524f7a770121`; focused denial GREEN. |
| M79 | Replace the relay's result-status-free raw result with fabricated `OutcomeClass::Found`. | `symforge_facade_preserves_malformed_probe_without_fabricated_status` RED and displayed `invalid tool parameters:` falsely paired with `outcome_class: found`. | Exact reverse to the same SHA-256 preimage; malformed and missing-batch raw-result controls GREEN. |
| M80 | Remove `symforge_edit` from the executable replay-residual table. | `all_ingress_uses_exact_typed_authority_branch` RED at exactly 7 vs required 8 edit-tool modes. | Exact reverse to its then-current SHA-256 preimage; focused activation guard GREEN. |
| M81 | Invert the real MCP seam's admitted-relay exemption so it runs the rendered-text error heuristic on raw relay data. | `admitted_facade_measurement_preserves_error_prefixed_source_at_wire` RED: exact file content remained intact but the wire result gained `isError:true`. | Exact reverse to SHA-256 `55cc432bc181d0e7b25d0b67e90ea2b2f25bba93e64667f6e4dc06437a2d0289`; focused real-HTTP control GREEN 1/0. |
| M82 | Wrap the two byte-exact allowlisted lifecycle-mount lines in a macro that also exports the captured module under a spelling-free alias outside the 13-file diagnostic seal. | All seven lexical/narrow diagnostics GREEN (`job_01a006eb300772a3b3b254e724347561`); the load-bearing whole-`src/` seal alone RED (`job_01a006ebd9b372d1a1144990096519b8`). | Exact reverse to `src/live_index/mod.rs` SHA-256 `500824888d3cac199e941869904c6fd9af300263ef540a6a2b49a946244dc3ad`; full preventive suite GREEN 8/0 (`job_01a006ec1e9b7881ba17dcd2605fa848`). |
| M83 | Force the facade's single local-state authority decision to `true` for every routed project. | Every one of the 10 `daemon_facade_` controls RED, covering receipt, path/economics, fusion, and temporal HOME contamination (`job_01a006ed280070e0b3f9dbeb8670f222`). | Exact reverse to `tools.rs` SHA-256 `ff1eb8d70990fe0910e5396ee201a8088bf5bfde8aee3dec6591815b8d560c22`; selector family GREEN 10/0 (`job_01a006edc5527e01b2d8b33246db2dc5`). |
| M84 | Strip only `Trust:` / `Source authority:` lines from oracle responses after the MCP call, leaving every required content anchor intact. | All four synthetic oracle cases RED at `successful search terminal absent`; all four snapshot/write cases stayed GREEN (`job_01a006ef244b7bd1a76d6376132e40ab`). | Exact reverse to `scripts/verify-tools.cjs` SHA-256 `74a304feaf5f92a219f71b375f84b60e085fdd199b3a9fe3f49d852ec48417af`; synthetic harness GREEN 8/0/0 (`job_01a006ef624572d2a54ea0427d0ff6b5`). |

**Historical observation on an earlier repair tree:** the restored preventive
suite was 7 passed / 0 failed. Its narrow diagnostic seal observed the reviewed
baseline as 13 files / 205,026 LF-normalized source bytes / SHA-256
`09b51bdbe46837b860a7144387a643b5de4fbd2428fce4bc9ff651036aa6ebca`.
That receipt is preserved, but it is not the load-bearing whole-`src/` seal and
does not substitute for the separately observed final source-pin row above.
Four production tests also corroborate the corrected overlay bases:
`test_what_changed_returns_result`,
`test_diff_symbols_reports_trust_envelope`,
`daemon_proxy_reset_calibration_clears_proxy_store_and_reports_receipt`, and
`cache_hit_and_ccr_counters_surface_in_context_inventory` each passed 1/0.

The scoped facade controls below each passed 1/0 on that earlier repair tree;
they are historical, and they were rerun with the selector and harness controls
on the final frozen source as recorded in the binding checklist:

| Command | Result |
|---|---|
| `cargo test --lib facade_probe_policy_allows_only_source_mutation_safe_measurements -- --test-threads=1` | 1 passed / 0 failed |
| `cargo test --lib symforge_facade_rejects_mutating_probe_relay -- --test-threads=1` | 1 passed / 0 failed |
| `cargo test --lib symforge_facade_preserves_malformed_probe_without_fabricated_status -- --test-threads=1` | 1 passed / 0 failed |
| `cargo test --lib symforge_facade_preserves_missing_batch_rename_without_fabricated_status -- --test-threads=1` | 1 passed / 0 failed |
| `cargo test --test rmcp3_protocol admitted_facade_measurement_preserves_error_prefixed_source_at_wire -- --test-threads=1` | 1 passed / 0 failed; real `/mcp` boundary; `outcome_trust: observed` |

This remains a non-closure result. T052 is open, but the final selector,
whole-source, and harness-envelope mutations are now directly observed and
restored, and every candidate gate in the binding checklist is observed. The
remaining obligations are a non-closure commit and a new review over that
immutable SHA. A final evidence/ledger commit is permitted only after a
trustworthy CLEAN review.

### Residuals carried out of Slice 3

**T051's Cargo/workflow pin remains deliberately bounded.** It does not cover a
doctest-running effect hidden behind a script, make target, or composite action;
a Cargo config outside the repository; or `.cargo/.cargo/config.toml` when the
outer `.cargo` itself becomes the working directory. No pinned CI gate uses a
`.cargo` directory as its working directory. Workflow discovery accepts exact
lower-case `.yml` and `.yaml`; mixed-case suffixes on Windows remain a stated
residual, not a second pin, because GitHub's case-sensitive runner discovery
does not treat them as workflow files. The walk deliberately enters ignored
trees other than its three named skips, so configs below `/target-*/`, `/.*/`,
`**/.symforge/`, `/mcps/`, or `spacetime/*/target/` can over-flag. The source
splice sweep likewise remains a fail-closed tripwire over known spellings, not
a completeness proof for every possible `include!` or `#[path]` construction.
The fail-closed filesystem policy also conservatively rejects a checkout whose
root itself is a symlink or Windows junction, and rejects any descendant
link/reparse entry before an ignore skip could make it irrelevant. Those are
safe false reds accepted for this preventive oracle; supporting such mounts
later requires an explicit root/link policy rather than relaxing observation.

**T051's semantic-darkness seal is reviewed-baseline preservation, not a
compiler call graph.** The 13-file excluded-runtime seal is a narrow diagnostic,
and the outside caller/splice sweeps explain direct textual edges. The
whole-`src/` seal is load-bearing: it also changes for an outside alias, macro,
trait, registration, or re-export bridge that leaves all 13 excluded sources
untouched. The source walk and path-record encoding fail closed, and the exact
manifest topology prevents an explicit production target from escaping the
sealed root. This still does not make an incorrectly approved pin refresh
impossible, expand macros or proc macros, inspect generated `OUT_DIR` source,
or prove compiler/dependency/external-consumer behavior. The final tuple and its
macro/alias mutation are both observed above. The seal therefore proves exact
reviewed-baseline preservation for this candidate while retaining the
compiler-semantic limitations stated here.

**D16 remains a cross-process body/publication atomicity residual.** The typed
daemon evidence header is ancillary metadata, not a transaction that pins every
handler body, watcher refresh, and receipt to one immutable content publication
under arbitrary concurrent publication. PR 4 removes its deterministic local
impact mismatch and rejects missing, malformed, wrong-project, and inconsistent
multi-step receipts; it does not claim product-wide cross-process atomicity.
Slice 4 owns that structured activation boundary.

**Daemon ACTIVE cancellation is an explicit unknown-outcome residual.** A completed
`index_folder` call now holds one connection-scoped lane through canonical daemon
dispatch, adapter mirror publication, and any reconnect replacement. If the caller
cancels or times out after the daemon begins its non-abortable activation but before
the adapter observes the response, however, the distributed outcome can remain
unknown. Subsequent project-bound calls pin the last observed canonical adapter
project, so this residual does not silently retarget a completed read or write. An
activation epoch or authoritative ACTIVE re-sync is the recovery follow-up; PR 4 does
not misreport an unobserved activation as a completed one.

**What T050's green does not prove.** The three-way surface split closes over
116 slots: 102 `SURFACE_OVERLAY` rows — 2 compatibility aliases, 6 hooks,
8 resources, 24 sidecar members, 40 tools, and 22 writers — plus
3 `NON_INGRESS_EXCEPTIONS` and 11 `AUTHORITY_FREE_INGRESS` members. Separately,
the authority join is bijective over the 244 frozen operation slots; every
surface member appears in exactly one of those three surface sets with a
non-empty basis; no allowed set names a branch outside `MODEL-SURFACE`; and the
union of the allowed sets contains all eight branches. It does **not** prove any
individual member's set is exactly right: dropping `Refused` from
`symforge_edit` leaves the suite green because the union still closes on other
rows (mutation M63c). Per-member correctness rests on the cited bases and on
review. Do not add a ninth `MODEL-SURFACE` name to make M63c fail.

**Approved T066 residual — eleven authority-free ingress members vs
`INV-SURFACE`.** `symforge://glossary`,
`symforge://tools/catalog`, `hook:PreTool` and the eight prompts are ingress
that run, succeed, pin no publication and observe no source. That falsifies
"every ingress resolves exactly one typed authority branch" as written.
Recorded rather than silently crossed: T066 must either exclude these from the
invariant or add a branch. Frozen prompts assertions 1 and 3 belong to the same
approved residual — they govern how generation-backed prompt context is selected
WHEN a prompt fetches it, and no V10 prompt fetches; V10 emits instruction text
plus `resource_link` URIs the client may read.

**Approved replay residual — eight edit-ingress modes are not represented by
the frozen eight branches.** The seven granular edit tools and `symforge_edit`
can return an identical stored-success replay before edit dispatch. The seven
`edit_tools.rs` writer rows are duplicate census appearances of those same
ingresses, not seven additional members. `ReplayRecord` v1 stores request/key
hashes, status/timestamps, and response text, but no verified repository/source
identity, authority branch, post-image receipt, or continuity proof. The handlers
do prepare the current source before probing replay, but that fresh Disk/Generation
source observation is not bound to the stored response. Worktree routing occurs
after the replay point. The terminal replay
therefore performs no source write, acquires no fresh `SourceMutationPermit`, and
cannot honestly be called `MutationPermitted` or inherit the preparatory authority;
it is not a ProjectStateDir write or refusal either. PR 4 does not invent a branch
or inherit an authority the record does not carry.

This is a mode-level residual, so it does not change the 102 + 3 + 11 member
partition. Its executable owners are T058 (causal RED), T064 (source-bound replay
and mutation integration), T066 (register/formally classify the replay lane), and
T072 (full activation campaign). Slice 4 must persist and verify a typed,
source-bound operation receipt and then either formalize replay-result authority
or amend the model explicitly. Reacquiring a mutation permit solely to return
stored text would falsely publish non-Current for a zero-write response and is not
an acceptable PR 4 shortcut.

**Approved source-free semantic-result residual — sixteen successful modes on
otherwise branch-bearing members.** These are mode exceptions, not sixteen
whole-member additions to `AUTHORITY_FREE_INGRESS`: ancillary untyped
`ProjectEvidence` wire `_meta` remains the recorded D16 activation gap and does
not manufacture a typed Current branch.

- one hook mode: non-source `hook:Read` pass-through;
- seven direct argument-only `estimate=true` modes: `analyze_file_impact`,
  `search_text`, `inspect_match`, `search_files`, `what_changed`, `explore`, and
  `diff_symbols`;
- one static guidance mode: `ask` ToolHelp; and
- seven compact-facade modes: relayed `search_text` estimate, relayed
  `search_files` estimate, preview PFF plan floor, preview plan floor,
  non-preview PFF plan floor, economics-bypass plan floor, and served `ask`
  ToolHelp.

The executable table pins exactly 16 triples, split 1 hook / 15 tool, and binds
owners: T064/T066/T067/T072 for the hook, and T066/T067/T072 for the tools. It
also inventories every advertised `estimate` declaration directly from the
three protocol modules: `[8, 4, 4]`, 16 total. Their dispatch dispositions are
exactly **7 pre-authority / 5 source-derived / 3 ignored-but-source-derived / 1
alias-drops-estimate-then-source-derived**. The five source-derived modes are
`get_symbol`, `get_file_content`, `get_repo_map`, `get_file_context`, and
`get_symbol_context`; the three ignored flags are `search_symbols`,
`find_references`, and `find_dependents`; the alias case is `trace_symbol`
dispatching `get_symbol_context`. Sharing the parameter spelling does not make
these semantics interchangeable. The final exact activation test is observed
1/0 in the binding checklist, so these cardinalities now have a current green
without turning that green into a substitute for semantic review.

**Other target-vs-current activation debts remain explicit.** The overlay is a
named frozen target owner, not proof that V10 already implements every target
branch. `detect_changes`/`detect_impact` target pure Git/worktree observation,
while the current delegated implementation still consumes generation symbols
and caller graph; T064 owns that refactor. Repeat-cache results and CCR handles
still lack the publication/source identity fence required before they may claim
the target generation result; T064/T066/T067 own those fences. The static
glossary and tools-catalog resources currently receive unfenced ancillary
project evidence even though their successful bodies are source-free; T066/T067
must remove or replace it rather than falsely add `GenerationLeased`. Standalone
and session health target a caller-root-mismatch refusal that current V10 does
not yet enforce; T066/T067 own that activation repair. The edit replay residual
above is separate and remains owned by T058/T064/T066/T072. Recording each gap
is the no-silent-gap rule; none authorizes Slice 4 work in PR 4.

**D14 is still unfalsifiable, and is not coverage.**
`read_gate_authority_v11.rs::a_failed_observation_refuses_without_disturbing_the_current_generation`
takes `let before = generation.identity();` and then asserts
`generation.identity() == before`. The refusal path (`into_failed_read`) never
touches `generation`, so no behaviour could make that assertion fail. It is
owned by live-observer invalidation under T056/T063, not the T047 stand-in. It
was not counted toward any gate above and was not "completed" with a fake
observer.


## Round-15 review and its repairs (PR 3)

Round 15 attacked the round-14 repairs: **0 blockers, 4 confirmed
majors, 2 confirmed minors, 2 refuted, 2 notes**. For the first time
since round 8, **not one finding is a hole in the mechanism.** Every
confirmed item is a false or self-contradictory claim in comments and
prose, and all of them are mine.

- **MAJOR — the doc-comment theft was not repaired; it changed hands.**
  Round 14's fix gave `CARGO_CONFIG` its own doc block but inserted
  `WORKFLOW_FINGERPRINTS` into the same slot, so the allowlist's doc
  comment — "WITH THE NUMBER OF TIMES it must occur … Grouped by why,
  so the judgement is auditable" — now documented a two-entry
  fingerprint list, and `CARGO_LINES` had no doc at all. The
  adjudicator proved the mechanism I had missed both times: **a blank
  line does not end a `///` run; only an intervening item does**, so
  the separation that reads like a fix is inert. Repair: the block was
  MOVED to sit immediately above `CARGO_LINES`, with a note telling the
  next editor to put new constants below it. Verified by compiler, not
  by reading — a `deny(missing_docs)` probe passes on the repaired
  shape and fails on the defective one.
- **MAJOR — the two residual lists contradicted each other.** The
  header listed two residuals and explicitly retired a third; the test
  body said "Three, matching the header." Worse, the retired one was
  provably caught: `if: false` above a gate turns the test RED via the
  fingerprint. The body also stated residual 1 more narrowly than the
  header — conditioning it on the line naming neither `cargo` nor
  `rustdoc`, when the real boundary is where the BEHAVIOUR lives (the
  allowlisted `python execution/release_ops.py publish-cargo` names
  cargo and is pinned, yet what that script runs is not). Both lists
  are now the same two residuals, stated by effect-location.
- **MINOR ×2 — two more claims wider than the code.** "Every `.cargo`
  config IN the tree is pinned" ignored the walk's own skip list. Round
  15 narrowed the claim to *committable* configs but still incorrectly
  treated `.git`, `node_modules`, and every `target` as one uniformly
  ignored class; the round-2 candidate repair above records the corrected
  normal-add bound. And the header still listed
  the bidi-mark flag as an arm of the splice tripwire when round 12
  moved that decision into `sweep` — the file said so correctly in
  three other places.
- **REFUTED ×2:** that the Round-14 section misattributes three of its
  prose findings, and that a residual-list amendment is stale.
- **Both retired residuals were re-verified by mutation** rather than
  left as claims: a descendant config (M57a) and `if: false` (M58) each
  observed RED.

## Gate results for the round-15 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed, 0 failed; dual counts unchanged at (4,4)/(9,9)/(6,6) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| doc attachment | `deny(missing_docs)` probe passes on the repaired shape, fails on the defective one |
| mutations | M57a (descendant cargo config) and M58 (`if: false` on a gate) each observed caught; all restored |

## Round-14 review and its repairs (PR 3)

Round 14 attacked the round-13 repairs: **0 blockers, 7 confirmed
majors, 3 confirmed minors, 1 refuted, 3 notes**. Three majors are new
holes in the gate pin; four are prose I wrote in the round-13 commit
itself. The first is the most serious finding of the whole review, and
the only one so far to demonstrate the darkness guarantee failing end
to end.

- **MAJOR — a DESCENDANT `.cargo/config.toml` re-points a gate, and the
  full laundering chain was executed.** Cargo merges configs from the
  working directory and every ancestor, so my round-13 root pin left
  every subdirectory open. The adjudicator committed
  `execution/.cargo/config.toml` with an aliased `fmt`, added
  `working-directory: execution` to the fmt step, and drove it through:
  the gate ran `cargo test --doc`, an inert `///` line calling
  `index_lifecycle` was tolerated as prose by the sweep, the doctest
  executed, and the dark-directory marker file was written — exit 0,
  all four tests GREEN, both workflows byte-unchanged, `git check-ignore`
  confirming both new files are committable. That is the STATED BOUND
  failing with the tripwire reporting all-clear. Repair: every `.cargo`
  directory in the tree is found (skipping `target`, `.git`,
  `node_modules`); the root config must match its verbatim pin and no
  other config may exist. Mutations **M57a** and **M57a2** (descendant
  `config.toml` and legacy `config`) observed caught.
- **MAJOR ×2 — the pin's unit is a LINE; what executes is a YAML
  scalar.** A continuation line that is itself pinned extends a pinned
  command (`cargo test --all-targets -- --test-threads=1` +
  `python execution/release_ops.py publish-cargo`, which libtest
  swallows as extra filters, exit 0) — the case both horns of round
  13's refutation missed. And the occurrence counts are file-blind: a
  gate RELOCATED from ci.yml to release.yml keeps every count and the
  whole triple while PR CI loses it. Patching the line walk a fourth
  time would only move the seam, so **`no_gate_builds_doctests` fingerprints
  each accepted lower-case `.yml`/`.yaml` file under `.github/workflows/`
  whole**. The line checks stay for the auditable
  judgement they record; the fingerprint is what makes any change
  impossible to miss. It is a change detector, not a security boundary,
  and says so. Mutations **M57b**, **M57c** and **M57d**
  (`working-directory` added to a gate) observed caught.
- **MAJOR ×4 + MINOR ×3 — prose, all of it mine, all from the round-13
  commit.** The retired "no word model left to be wrong about" survived
  verbatim in the in-test summary because I fixed only the header copy;
  my new `CARGO_CONFIG` constant was inserted directly above
  `CARGO_LINES` and stole its doc comment, so it was documented as the
  thing it is not; my Round-11 amendment asserted a `rustdoc` line is a
  residual one round after making `rustdoc` a selector; "`--doc` and
  `rustdoc` are the two spellings that open the lane" is falsified by
  `cargo t` and by an aliased `fmt`; and the deleted bidi branch was
  still described in the present tense. Each is corrected against what
  the code now does, and the residual lists in both the header and the
  test body were rewritten rather than spot-patched — spot-patching is
  what produced this cluster three rounds running.
- **REFUTED:** that the Round-11 section's major count is unaccounted
  for.

## Gate results for the round-14 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed, 0 failed; dual counts unchanged at (4,4)/(9,9)/(6,6) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| gate pin | per-line multiset, (30, 26, 2), verbatim root cargo config, no other `.cargo` config in the tree, and both workflow fingerprints all bound |
| mutations | M57a/M57a2 (descendant cargo configs), M57b (relocation shape), M57c (continuation extending a pinned command), M57d (`working-directory` on a gate) each observed caught; root `[ alias ]`, the uniform-path alias and bidi-on-prose re-observed as controls; all restored |

## Round-13 review and its repairs (PR 3)

Round 13 attacked the round-12 repairs: **0 blockers, 4 confirmed
majors, 3 confirmed minors, 1 major refuted, 5 notes**. Two majors are
the code half and docs half of one defect — the `[alias]` check added
one round earlier — and it is the same mistake in a third file format:
matching a syntax with a literal string instead of pinning the file.

- **MAJOR — the `[alias]` check was a literal-prefix match on one
  filename.** `starts_with("[alias]")` misses three valid TOML spellings
  of the same table — `[ alias ]`, `["alias"]`, and a root-level
  `alias.fmt = [...]` dotted key — and never opens the legacy
  extensionless `.cargo/config`, which cargo still honours. Weaponized
  and verified against real cargo: `[ alias ]` plus
  `fmt = ["test", "--doc", "--", "--skip"]` turns the allowlisted
  `run: cargo fmt --check` into a full doctest run that exits 0, with
  both workflow files byte-unchanged, every per-line count matching, and
  the test green. (Aliases cannot shadow BUILT-IN subcommands, so
  `cargo test` is not re-pointable — `fmt` and `clippy` are external
  subcommands and are, which also falsified my own rationale comment.)
  Two further holes, same root: `if let Ok(...)` made the whole check
  no-op silently when the file was absent, and the legacy path was never
  read. **Repair: the config is pinned VERBATIM**, the read must
  succeed, and `.cargo/config` must not exist. Mutations **M56a–M56e**
  (three spellings, legacy path, absent file) each observed caught.
- **MAJOR — the line filter selected on `cargo` alone.** The file named
  `rustdoc` as an equally sufficient spelling of the doctest lane in its
  own orthogonal check, then applied that knowledge only to the
  allowlist and never to the workflow text. A first-class
  `run: rustdoc --test src/lib.rs` step — no cargo anywhere — walked
  past the filter and left all counts untouched. Verified live: it runs
  the doctest. Repair: the filter selects `cargo` OR `rustdoc`.
  Mutation **M56f** observed caught.
- **MAJOR — "there is no word model left to be wrong about" was false.**
  Normalization IS a word model: `split_whitespace` uses Unicode
  White_Space while bash's IFS is space/tab/newline, so a line with
  U+00A0 between `--` and `--test-threads=1` normalized onto the pinned
  gate while bash would have run a different command. The reviewer was
  scrupulous about the limit — gluing can only merge tokens of
  allowlisted entries, and no merge yields a doctest-running command —
  so the safety property survived and only the sentence was false.
  Repair: split on ASCII space and tab, which is what bash splits on;
  the sentence now says the pin recognizes lines, not commands.
  Mutation **M56g** observed caught, with the NBSP verified at byte
  level rather than assumed.
- **MINOR ×3 —** the in-test residual list still called the repo's
  `.cargo/config.toml` unreachable fifty lines above the code that reads
  it; the `[alias]` rationale used an impossible exemplar (`cargo test`
  cannot be aliased); and the Round-11 section repeated the same retired
  residual. All corrected against what cargo actually does.
- **Notes acted on:** the bidi branch inside `splice_matcher` became
  unreachable when round 12 moved the decision into `sweep`, so it is
  deleted rather than left describing itself in the present tense; and
  `if: false` on a step disables a gate with no line change at all,
  which is now a STATED residual rather than an unexamined gap.
- **REFUTED:** that a plain multi-line YAML continuation defeats the
  per-line pin. [Overturned by round 14, which found the case both
  round-13 horns missed: a continuation line that is ITSELF pinned.
  Appending `python execution/release_ops.py publish-cargo` to the
  `cargo test --all-targets` scalar left every line allowlisted and
  every count matching, and libtest swallowed the trailing words as
  filters, exit 0.]

## Gate results for the round-13 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed, 0 failed; dual counts unchanged at (4,4)/(9,9)/(6,6) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| gate pin | per-line multiset, (30, 26, 2), and the verbatim `.cargo/config.toml` all bound |
| mutations | M56a–M56c (three TOML alias spellings), M56d (legacy `.cargo/config`), M56e (config absent — the old check no-opped silently), M56f (bare `rustdoc --test` step), M56g (U+00A0 normalization collision, NBSP verified at byte level) each observed caught; M55a/M55c and the uniform-path alias re-observed as controls; all restored |

## Round-12 review and its repairs (PR 3)

Round 12 attacked the round-11 repair: **0 blockers, 1 confirmed major,
4 confirmed minors, 2 refuted, 2 notes (not enumerated below; both were
observations rather than defects)**. The pinned-allowlist design
held — no evasion was found that adds a doctest-building gate — and the
one major is a counting flaw in how the pin detects DELETION, not a hole
in what it admits.

- **MAJOR — compensated deletion held both counts.** Four allowlisted
  lines legitimately occur twice (two test gates, two builds, each in
  both workflows), so at `(30, 26, 2)` the pair is not a bijection:
  delete one copy of a gate and add a duplicate of any other allowlisted
  line, and the total stays 30 while the distinct set stays 26, because
  the deleted string survives via its twin. Verified live — replacing
  ci.yml:143 (`cargo test --all-targets`) with a second `cargo fmt
  --check` left the test GREEN, deleting the entire Rust test gate from
  PR CI. The adjudicator added the boundary: uncompensated deletion is
  caught (29), rewording a 1× line is caught (25 distinct), and only the
  four 2× lines are blind, only under a compensating edit. The comment
  claiming "as everywhere else in this file" was the tell — everywhere
  else `total == distinct`, which forces the bijection this pair does
  not. Repair: `CARGO_LINES` now carries a per-line occurrence count and
  the observed multiset must equal the declared one exactly, so a
  deletion, a rewording, and a duplicate each fail individually and the
  message names both halves of the drift. Mutations **M55a** and
  **M55b** (compensated deletion of each duplicated gate) observed
  caught.
- **MINOR — the `[alias]` residual was reachable all along.** The header
  listed a `.cargo/config.toml` `[alias]` re-pointing an allowlisted
  line as outside any line-based scan — while sitting in a file this
  test can simply open. It now reads it and fails on an `[alias]` table.
  A user-level `~/.cargo/config.toml` alias stays a real residual: it is
  outside the repo, and CI runners have none. Mutation **M55d**
  observed caught. [Amended after round 13: that read was a literal
  `starts_with("[alias]")`, which three valid TOML spellings and the
  legacy `.cargo/config` walked straight past, and which no-opped
  silently when the file was absent. The file is now pinned verbatim.
  Also corrected there: the rationale's exemplar was impossible — cargo
  refuses to let an alias shadow a BUILT-IN subcommand, so `cargo test`
  is not re-pointable; `fmt` and `clippy` are external subcommands and
  are.]
- **MINOR — "bidi marks are flagged OUTRIGHT" was false.** The matcher
  named them, and then the prose exemption forgave them: a U+200E on a
  `//` line counted as tolerated prose. The bidi check now runs before
  every exemption, allowlist included, which makes the claim true rather
  than restated — `src/` holds zero such marks, so the stronger rule
  costs nothing. Mutation **M55c** observed caught.
- **MINOR ×2 — prose, both mine.** "Every major landed on the gate walk,
  for the fourth round running" is falsified by this document's own
  Round-9 section, where the uniform-path major was an alias-arm
  finding; and round 11 left the Round-10 section describing the deleted
  walk in the present tense. Both amended at the spot.
- **REFUTED ×2:** that the orthogonal `--doc`/`rustdoc` check fails to
  cover the likeliest careless allowlist addition, and that dropping
  round 10's escape-glue residual left a gap (the pinned design makes
  escape-glued lines fail as unrecognized).

## Gate results for the round-12 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed, 0 failed; dual counts unchanged at (4,4)/(9,9)/(6,6) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| gate pin | per-line multiset observed equal to the declared counts; (30, 26, 2) still bound |
| mutations | M55a/M55b (compensated deletions), M55c (bidi on an inert comment), M55d (`[alias]` table) each observed caught; `cargo rustdoc -- --test`, `cargo +nightly test --doc` and the uniform-path alias re-observed caught as controls; all restored |

## Round-11 review and its repairs (PR 3)

Round 11 attacked the round-10 repairs: **0 blockers, 4 confirmed
majors, 5 confirmed minors, 1 refuted, 2 notes (neither enumerated
below — they were folded into the minors)**. All four majors landed
on the gate walk [amended after round 12: the original sentence said
"every major … for the fourth round running", which this document's own
Round-9 section falsifies — round 9's uniform-path `use include as X;`
major was an ALIAS ARM finding. Rounds 10 and 11 were gate-walk-only;
round 9 was not], and the pattern is the finding: a scan that must MODEL
the shell's word rules to locate the command keeps losing to the shell.
So the walk was deleted.

- **MAJOR — `cargo rustdoc -- --test` cleared the walk.** It puts
  `rustdoc` before the bare `--` and `--test` after it, so the head held
  a plain word (no offense), contained neither `test` nor `t` (not an
  invocation), and `--doc` never appeared. Liveness was proven with a
  marker-writing doctest: it compiles the doctest lane, RUNS it, and
  fails the step on failure — a real gate. Worse, appended in place to
  the existing gate line it measured byte-identical to clean HEAD, so
  the round-10 count pin gave no backstop either.
- **MAJOR — the tokenizer's word model was not the shell's.** Quote
  erasure SPLITS words the shell JOINS: `cargo te"st" --doc` tokenized
  as `te`/`st`, and `car"go" test --doc` split the cargo token itself so
  nothing matched at all. In the other direction, shell grouping GLUED
  tokens the walk needed whole — `X=$(cargo test --doc)` and
  `(cargo test --doc)` were invisible. All four are valid YAML, all four
  ran the doctest lane under bash, all four measured green.
- **MAJOR — my own round-10 line-skip hid an executing gate.** I added
  a `name:`/`if:`/`#` skip to reduce friction a *note* complained about.
  The test is key-shaped but ran on every physical line, including shell
  content inside a `run: |` block, where `if:` is a legal bash function
  name: `if:() { cargo test --doc; }` measured 7/0 under the new walk
  and 8/1 under the old one. I traded away over-flagging for tidiness
  and it cost exactly what this file keeps saying it costs.
- **Repair — the walk is gone; the lines are pinned.** Every line of
  every workflow that mentions cargo, case-insensitively, must appear
  VERBATIM in a `CARGO_LINES` allowlist (30 lines, 26 distinct, across 2
  files — all three counts bound), grouped by why each cannot build
  doctests: prose and configuration, commands with no test harness, and
  the seven test gates with their selectors. There is no word model left
  to be wrong about — an unrecognized cargo line fails whatever it says,
  in any quoting, grouping, or subcommand. [Amended after rounds 13–14:
  that sentence is false twice over. Normalization is itself a word
  model (round 13, U+00A0), and the unit compared is a LINE while the
  unit executed is a YAML scalar (round 14) — a continuation extended a
  pinned command and a relocation moved a gate between files, both with
  every line pinned. Whole-file fingerprints are the backstop now.] A
  second, orthogonal check
  rejects any allowlist entry naming `--doc` or `rustdoc`, so a careless
  addition still trips. Residuals are now the two no line-based scan can
  reach: a gate with no `cargo` on the line at all, and a
  `.cargo/config.toml` `[alias]` re-pointing an allowlisted line.
  [Amended after rounds 12–13: the repo's `.cargo/config.toml` was never
  a residual — it is a file this test can open, and it is now pinned
  verbatim after round 12's `[alias]`-search replacement was defeated by
  three TOML spellings and the legacy `.cargo/config`. The residual list
  is also longer than "two". [Corrected after round 14: the two examples
  this amendment reached for were both wrong by the time it was written
  — round 13 had already made `rustdoc` a SELECTOR, so a rustdoc line is
  caught rather than residual, and round 14's whole-file fingerprints
  see an `if: false` even though it changes no cargo line.] See the
  Round-13 section.]
- **MINOR ×5, all prose, all mine.** The "quoting cannot hide a command"
  absolute survived unamended in the doc; the retired universal alias
  claim survived in a third place my round-10 sweep missed because the
  phrase wraps across two lines; the in-test rule summary still
  described the subcommand finder deleted twelve lines below it; "the
  shape every line-spanning wrap produces" was a universal a wrap
  keeping `cargo test` intact falsifies; and the residual-3 exemplar had
  lost its backslash in both places, leaving "a ` ` form would survive"
  — an example that named nothing. All are moot or amended.
- **REFUTED:** "`--test-threads` and `--tests` satisfy neither test" —
  the reviewer read a contradiction with the selector allow-list;
  adjudicated not-real.

## Gate results for the round-11 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed, 0 failed; dual counts unchanged at (4,4)/(9,9)/(6,6) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| gate pin | (30, 26, 2) observed — total lines, distinct lines, workflow files |
| mutations | M54a (`cargo rustdoc -- --test`), M54b (quote-split word), M54c (`$(...)` grouping), M54d (`if:()` shell function), M54e (selector dropped from a real gate), M54f (gate line deleted), M54g (careless allowlist addition, caught by the orthogonal check) each observed caught; M53a/M53i and M52a re-observed caught under the pin; all restored |

## Round-10 review and its repairs (PR 3)

Round 10 attacked the round-9 repairs: **0 blockers, 2 confirmed majors,
4 confirmed minors, 1 major REFUTED, 4 notes**. Both majors are one
defect — the gate walk's subcommand finder — and four of the six
findings are errors in prose I had written one commit earlier.

- **MAJOR ×2 — `cargo +nightly test --doc` walked straight past the
  pin.** The walk named "the first token after `cargo` that starts with
  neither `-` nor `$`" as THE subcommand. Cargo accepts arguments before
  its subcommand: a rustup override (`+nightly`) starts with `+`, and a
  global option's value (`--color always`, `--config k=v`, `-Z x`) is a
  plain word. The finder therefore resolved to `+nightly`/`always`/`k=v`,
  decided it was not a test invocation, and skipped in silence —
  measured at `invocations=7 offenders=0`, byte-identical to clean HEAD,
  for seven distinct shapes. Liveness was proven rather than assumed: in
  a probe crate whose doctest writes a marker file, `cargo +1.96.0 test
  --doc`, `cargo --color always test --doc`, and bare `cargo +stable
  test` all ran the Doc-tests lane with the marker present; this CI
  provisions with `rustup toolchain install`, so `+toolchain` is live on
  the runner. The docs half of the pair is the same defect in prose: the
  header's "STATED RESIDUALS of the pin, and now the only two".
  **Repair — the subcommand finder is deleted.** Identifying *which*
  token is the subcommand was one more thing to be wrong about, so the
  walk stopped asking and now asks only: does a `test`/`t` token appear
  before the bare `--`? (`--tests` is a distinct token, so sibling
  masking still fails.) Closed alongside it: every `cargo` in a segment
  is judged, not just the first; `&` joins the split set; cargo spelled
  as a path or as `cargo.exe` counts as cargo; and `\t`/`\n`/`\r`
  escapes are un-glued before tokenizing. The residual list is now kept
  as HISTORY — three known classes, never "the only" — because round 9
  wrote "the only two" and round 10 produced a third the same day.
  [This whole bullet describes a walk that no longer exists: round 11
  deleted it after `cargo rustdoc -- --test` and shell word-splitting
  defeated it. Read it as history, not as the current mechanism.]
- **The anti-vacuity floor became a pin.** `invocations >= 5` was how a
  silently ADDED gate hid twice: round 9's flow mapping and round 10's
  `+toolchain` shape both left the count at 7. It now asserts exactly 7,
  so a gate added, removed, or reworded fails loudly and updates this
  test deliberately. Mutation **M53j** (a real gate line rewritten to
  `cargo build`) observed the pin fire at 6. [Superseded twice: round 11
  replaced the invocation count with the (30, 26, 2) line triple, and
  round 12 replaced that with per-line occurrence counts after a
  compensated deletion held both numbers.]
- **MINOR ×4, all mine, all the same habit.** "No quoting spelling can
  hide the command" was an absolute falsified by a YAML escape
  (`run: "cargo\ttest"` left one glued token); "two such lines exist in
  `src/`" was really **76**, asserted without measuring in the very
  paragraph written to replace an overclaim with a measurement; the
  retired universal alias claim survived in three more places; and a
  round-7 bracket still described the `run:`-scalar parser that round 9
  deleted. Each is repaired against a measurement taken on this tree —
  76 counted with the arm's own predicate, 184 `.rs` files (the earlier
  "172" was a scan count mislabelled as a file count), one Note bullet
  in the Round-9 section (which said two).
- **REFUTED:** "macro-token indirection defeats both the `include!` and
  `#[path]` arms on single physical lines, with no `concat!`."
  Adjudicated not-real.
- **Notes accepted as friction, not defects** (two of the four; the
  other two were the count-vs-list mismatch and a mislabelled scan
  count, both folded into the minors above): the scan can flag
  non-command lines (step names, `if:` expressions, YAML comments) — now
  narrowed by skipping those non-executing keys — and the `useinclude`
  opener can flag an English word ending in "-use" adjacent to
  "include" inside a string. Over-flagging forces a human decision,
  which is the friction this file is built on. [Amended after round 11:
  that narrowing was a MISTAKE and is gone. The key-shaped skip ran on
  shell content inside `run: |` blocks too, where `if:()` is a legal
  bash function name, and it hid a live doctest gate. Trading
  over-flagging for tidiness is the one trade this file must not make.]

## Gate results for the round-10 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed, 0 failed; dual counts unchanged at (4,4)/(9,9)/(6,6) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| gate-walk count | pinned at exactly 7, observed |
| mutations | M53a–M53e (`+toolchain`, `+version`, `--color always`, `--config k=v`, `-Z x`), M53f/g (path-spelled cargo), M53h (escape glue), M53i (`cargo.exe`), M53j (the count pin) each observed caught; M52a/M52c/M52e and M49a/M49e re-observed caught under the new walk; all restored |

## Round-9 review and its repairs (PR 3)

Round 9 attacked the round-8 repairs: **0 blockers, 3 confirmed majors,
2 confirmed minors, 1 refuted, 1 note**. The inert-comment rule survived
a ninth round untouched, and the round-8 stripper repairs were confirmed
sound by an independent battery (544 comment-interleave candidates over 8
live splice templates; every enumerated template had ZERO live escapes).
But the review found a hole in the centre of the alias arm, and killed
the gate pin's parsing approach outright.

- **MAJOR — `use include as X;` is a live splice alias that evaded every
  view.** Rust 2018 UNIFORM PATHS let a use declaration name a prelude
  macro with no leading path, so the first segment needs no `::` at all.
  The adjudicator verified it on this crate's own edition (2024):
  `use include as mount;` + `mount!("payload.rs");` compiles and executes
  the splice, while the matcher returns `None` on both lines. **The
  decisive probe:** a copy of the REAL `src/` (184 `.rs` files; the "172"
  written here in round 9 was the first sweep's SCANNED count, which
  excludes the dark directory — a scan count mislabelled as a file
  count) with those two
  lines appended to `daemon.rs` (an asserted ingress lane) and a shim
  outside `src/` calling `index_lifecycle::dark_entry()` produced sweep
  output IDENTICAL to clean HEAD — violations=0, (4,4)/(9,9)/(6,6). The
  suite could not distinguish clean HEAD from HEAD carrying a live dark
  splice. This was not a stated residual: one physical line, a literal
  path, an edition property of the crate we compile today. Repair: a
  FOURTH opener, `useinclude`, and — equally the point — the universal
  claim ("the form every alias site must write, whatever its visibility,
  spacing, grouping, comment interleaving, or `r#` spelling") is retired
  for the ENUMERATION it always was. An opener set widened four times is
  a tripwire, not a proof, and now says so. Mutations **M50** (the bare
  uniform-path alias) and **M51** (its comment-interleaved variant)
  observed caught, restored.
- **MAJOR ×2 — the gate pin's `run:`-scalar parser missed six ordinary
  spellings**, confirmed by two independent adjudicators against two
  independent YAML parsers: `run: "cargo test"` (the token was `"cargo`),
  `run: 'cargo test …'`, a plain multi-line scalar, `-   run:` with an
  extra space after the dash, `- {run: cargo test}` as a flow mapping,
  and `cargo t` (a real cargo builtin alias that runs doctests). Worse
  than a miscount: an ADDED doctest gate in flow-mapping form left
  `invocations` at 7, so the anti-vacuity floor gave zero tell. The
  second major is the same defect in the prose — the STATED BOUND's
  "parses every `run:` scalar" and the evidence doc's "the pin's REAL
  residuals" were both false.
  **Repair — the pin stopped parsing YAML.** This is round 3's lesson
  arriving a second time: a scan that must MODEL a syntax to find the
  command loses to that syntax, exactly as the mid-line-comment lexers
  lost to Rust. The walk is now a fail-closed PHYSICAL-LINE scan that
  erases YAML quoting and flow punctuation before tokenizing (so quoting
  cannot hide a command), splits compound commands into segments, and
  treats a `cargo` segment with no resolvable subcommand — the shape
  every line-spanning wrap produces — as an OFFENSE rather than a skip.
  It refuses to guess and says so. [Amended after round 11: both
  parentheticals are FALSE. Quoting INSIDE a word (`cargo te"st" --doc`)
  made the erasure split a word the shell joins, hiding the command; and
  "the shape every line-spanning wrap produces" was a universal that a
  wrap keeping `cargo test` intact falsifies. The whole scan was
  replaced in round 11 — see that section.] Observed: still exactly 7 invocations
  on the real workflows; mutations **M52a–M52f** (double-quoted,
  single-quoted, flow mapping, dash-space, `cargo t`, plain multi-line
  scalar) each observed caught, and all five round-8 controls
  (**M49a–M49e**) re-observed caught under the new design. A wrap that
  keeps `cargo test --all-targets` intact on its line still passes — the
  friction fires only where the command genuinely cannot be resolved.
- **MINOR ×2 — two stale mechanics in prose.** The round-6 `r#include`
  bullet still said the collapse strips `r#` "in both views" (round 8
  made it four views and moved the strip out of the collapse), and the
  round-8 claim that the views "only ever judge lines whose delimiters
  are real" was false in two ways: quote-bearing lines WITHOUT a splice
  token still reach the views (76 such lines exist in `src/`, counted
  with the arm's own predicate at 0f41db7f — round 9 wrote "two" without
  measuring, inside the paragraph added to replace an overclaim with a
  measurement), and a
  quote-free line can be the interior of a multi-line string. Both
  errors run in the over-flag direction only — an under-flag would need
  a live splice whose `include`/`path` token is hidden, and the
  ambiguity arm tests raw text before any stripping. Repaired to state
  the DIRECTION rather than an exactness, in both the header and here.
- **REFUTED:** "the `>= 5` floor against 7 observed lets two invocations
  vanish silently." Adjudicated not-real.
- **Note:** the round-8 summary said the gate pin had "two silent-pass
  classes" while its own bullet listed three; corrected below.

## Gate results for the round-9 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed, 0 failed; dual counts unchanged at (4,4)/(9,9)/(6,6) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| gate-walk anti-vacuity | observed 7 invocations under the rewritten walk (floor probe) |
| mutations | M50/M51 (uniform-path alias, bare and comment-interleaved) and M52a–M52f (six gate spellings) each observed caught; M49a–M49e re-observed caught under the new walk; all restored |

## Round-8 review and its repairs (PR 3)

Round 8 attacked the round-7 repairs: **0 blockers, 4 confirmed majors,
0 refuted, 1 note**. The inert-comment rule again survived untouched —
all four majors landed on the two round-7 artifacts themselves: the
depth-aware stripper (two distinct evasion paths) and the gate pin
(three silent-pass classes plus its falsified "fails loudly" sentence —
this line said "two" while the bullet below listed three; corrected
after round 9).

- **MAJOR — whitespace collapse fabricated `/*` openers.** The round-7
  pipeline deleted all whitespace FIRST, then stripped comments — gluing
  non-adjacent `/ *` into an opener the Rust lexer never saw. Three live
  rustc-verified constructs evaded both views: a spaced comment interior
  (`use std::/* / * */include as inc;`), a divide-by-deref (`let a = b /
  *c;` preceding a commented alias), and the same glue defeating the
  `#[path` arm. Repair: `strip_block_comments` now runs on the RAW line,
  before any collapse — on a quote-free line, raw-text `/*` adjacency is
  a real comment opener to the lexer too. Mutation **M44** (the spaced-
  interior alias, compiling) observed caught, restored.
- **MAJOR — the depth-0 `*/` clear deleted flagged prefixes.** A `*/`
  later on the line as string or trailing-line-comment content wiped an
  already-collected `::include` prefix from the stripped view while the
  balanced comment hid it from the plain view (`use std::/*c*/include as
  x; let s = "*/";` — live, silent). Repair: the stripper never discards
  collected output; a depth-0 `*/` is skipped and everything kept (over-
  flag only). Mutation **M45** (the trailing-`*/` form) observed caught.
  With both stripper repairs in, one class remained that the round-8
  probes implied but did not cite: string CONTENT (`"/*"`) can poison
  any line-local comment tracking and hide a splice from every view. The
  repair closes it as a class, not an instance — the new AMBIGUITY ARM
  flags outright any line carrying a `"` alongside a `/*` or `*/` plus a
  splice token, so the views only ever judge lines whose comment
  delimiters are real [amended after round 9: that last clause was false
  in two ways — quote-bearing lines WITHOUT a splice token still reach
  the views, and a quote-free line can be the interior of a multi-line
  string. What the arm buys is a DIRECTION, not an exactness: a fake
  delimiter can only remove text and over-flag, while an under-flag
  would need a live splice whose `include`/`path` token is hidden, and
  the arm tests raw text before any stripping]. Zero existing `src/`
  lines trip the arm (the
  allowlists and dual-count binds are unchanged). Mutations **M46** (the
  string-poisoned alias, observed caught by the ambiguity arm
  specifically), **M47** (comment-interleaved `#[path]`, the F4 form),
  and **M48** (the round-7 nested control, still caught) all observed,
  restored. The `r#` strip also became a pair of EXTRA views instead of
  an in-place edit — removal can fabricate or destroy adjacency, and an
  extra view only ever adds a flag.
- **MAJOR — the gate-pin tokenizer had three silent-pass gaps** (sibling
  `--tests` on the same line masking a bare `cargo test`; `.yaml`
  workflows invisible to the `.yml`-only filter; `cargo  test` with a
  doubled space not counted), and — the fourth major — **the STATED
  BOUND's "fails loudly" sentence was falsified** by a `.yaml` gate and
  by a wrapped `run:` block, neither carved out as a residual. One
  repair for both: `no_gate_builds_doctests` now parses every `run:`
  scalar in `*.yml`/`*.yaml` (inline, literal `|`, folded `>`), joins
  shell continuations, splits compound commands into segments, and
  judges each `cargo … test` segment on its own tokens — the excluding
  selector must sit before any bare `--` (a trailing `--test` is a
  libtest filter, not a selector), `--doc` anywhere is an offense. The
  walk was observed finding exactly 7 invocations on the real workflows
  (floor probe), and the pin's REAL residuals are now stated in the
  header: indirection (script/make/composite action) and `run:` values
  assembled from YAML anchors or `${{ }}` expressions [amended after
  round 9: "REAL residuals" was an overclaim — six more spellings
  (quoted scalars, plain multi-line scalars, flow mappings, dash-space,
  `cargo t`) were invisible to this walk and named nowhere. The parser
  was replaced by a fail-closed physical-line scan; see the Round-9
  section]. Mutations
  **M49a** (sibling masking) / **M49b** (`.yaml` + `--doc`) / **M49c**
  (double space) / **M49d** (backslash-wrapped invocation) / **M49e**
  (folded-block split) each observed caught against the mutated
  workflows, all restored.
- **Note:** the T051 section still said "two executing tests"
  present-tense; bracket-amended at the spot (the file has held four
  since round 7).

## Gate results for the round-8 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed, 0 failed; allowlist dual-counts unchanged at (4,4)/(9,9)/(6,6) |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| gate-walk anti-vacuity | observed 7 invocations across ci.yml/release.yml via the floor probe |
| mutations | M44–M48 (matcher) and M49a–M49e (gate walk) each observed caught, restored; `git status` clean but for the test file and this document |

## Round-7 review and its repairs (PR 3)

Round 7 attacked the round-6 repairs: **0 blockers, 2 confirmed majors,
1 confirmed docs minor, 0 refuted**. The headline is what did NOT break:
the inert-comment rule survived its direct assault — the code verifier
probed every `//`-leading spanning-lexeme tail construction and confirmed
each one that returns to code carries its `"` or `*/` closer on the line.
Both majors were repairable without touching the rule.

- **MAJOR — nested block comments defeated the stripper.** Rust block
  comments NEST; the round-6 stripper did minimal non-nested pairing, so
  `use std::/*x/*y*/z*/include as inc;` (one legal nested comment, a live
  alias, rustc-verified) evaded both views — it mis-paired `/*x/*y*/`,
  then the dangling-`*/` branch ate the `::` opener. That sat inside the
  claimed "comment interleaving" coverage, not a stated residual. Repair:
  `strip_block_comments` is now a single linear scan tracking NESTING
  DEPTH — an unclosed `/*` still comments out the rest, a dangling `*/`
  (depth zero) discards the prefix [round 8: that discard was itself an
  evasion — a later string or trailing line comment carrying `*/` wiped
  an already-flagged prefix; the stripper now runs on the raw line
  before any collapse and never discards collected output]. Mutation
  **M42** (the exact nested spelling) observed caught; **M38 replanted**
  as the non-nested control to prove the rewrite regressed nothing —
  also caught.
- **MAJOR — my round-6 refutation ground was false as written.** The
  Round-6 section's parenthetical said a `///` line "genuinely cannot
  execute code"; the verifier ran one — rustdoc extracts fenced
  doc-comment text into doctest crates that a bare `cargo test` (or
  `--doc`) builds and RUNS, and the dark directory is publicly nameable,
  so a fenced doctest line would be a tolerated, compiling, executing
  edge. The true ground is narrower: no gate in this repo builds doctests
  (all seven `cargo test` invocations across ci.yml/release.yml carry
  `--all-targets`/`--lib`/`--test`), and a doctest resolves only the
  recorded public surface. Repaired in BOTH directions: the sentence now
  states the narrow ground, and the bound stopped being a hand-checked
  snapshot — new test `no_gate_builds_doctests` pins every `cargo test`
  line in the CI workflows to a doctest-excluding target selector and
  forbids `--doc`, with the test-file header carrying the STATED BOUND
  paragraph [round 8: this first walk had three silent-pass gaps
  (`.yaml` invisible, sibling-token masking, spacing/wrapping); it now
  parses `run:` scalars into command segments, and the pin's own
  residuals — indirection and expression-built commands — are stated in
  the header. Superseded twice since: round 9 deleted the `run:`-scalar
  parser for a fail-closed physical-line scan, and round 10 deleted that
  scan's subcommand finder. The residual list is now kept as history,
  never as "the only"]. Mutations **M43a** (selector dropped from a gate line)
  and **M43b** (`--doc` added) each observed caught, restored.
- **Docs minor:** the round-1 amendment bracket still said "eight
  allowlist entries count-pinned" present-tense; since round 6 the
  sibling sweep pins nine. Bracket-amended at the spot.

## Gate results for the round-7 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 4 passed (the suite grew `no_gate_builds_doctests`), 0 failed |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| mutations | M42 (nested-comment alias) and the M38 replant (non-nested control) each observed caught; M43a (gate selector dropped) and M43b (`--doc` added) each observed caught against the mutated ci.yml; all restored |

## Round-6 review and its repairs (PR 3)

Round 6 verified the round-5 repairs adversarially: **2 confirmed
BLOCKERS, 1 confirmed major, 1 refuted, 2 confirmed docs minors, 3
notes**. Both blockers landed on the same recurrence class — insignificant
separators the line-based rules never modeled — and one of them refuted
the load-bearing prose rule itself, the first breach of that rule to
survive adjudication.

- **BLOCKER — the prose rule was refutable (string-tail masquerade).** A
  line-spanning string literal's continuation line may begin with `//` as
  string CONTENT and still execute code after the literal's closing quote:
  `let _s = "` / `//"; <live call edge>;` compiled, ran, and was counted
  as tolerated prose by all three sweeps. This is NOT round 4's refuted
  doctest attack [amended after round 7: this parenthetical originally
  said "a `///` line genuinely cannot execute code", which is FALSE —
  rustdoc extracts fenced doc-comment text into doctest crates that a
  bare `cargo test` or `--doc` builds and RUNS. The true ground is
  narrower: no gate in this repo builds doctests (every `cargo test` in
  ci.yml/release.yml carries `--all-targets`/`--lib`/`--test`), and a
  doctest edge resolves only the recorded still-public paths. That bound
  is now OBSERVED, not hand-checked: `no_gate_builds_doctests` pins every
  gate invocation to a doctest-excluding selector — round 8 hardened the
  pin's walk (run-scalar parsing, both extensions, segments) and stated
  its residuals (indirection, expression-built commands)]; it is a
  lexically different construction, and the header's "Rust permits no code
  after a line-start `//`" was false as written. The repair closes the
  whole class, not the instance: Rust has exactly two lexeme kinds that
  span physical lines — string literals (plain, raw, byte, C — every form
  closes with a `"`) and block comments (every level closes with `*/`) —
  and handing control back to code mid-line requires that closing
  delimiter on the line. The exemption is now the INERT full-line comment:
  first non-whitespace bytes `//`, no `"`, no `*/`. Four legitimate
  quoting comments across the three sweeps lost the exemption and are
  allowlisted individually (dual-count binds updated to (4,4)/(9,9)/(6,6)).
  Mutations **M37** (the exact string-tail smuggle, compiling, planted in
  `src/lib.rs`) and **M41** (the block-comment-tail variant `/* m41` /
  `// tail */ <live type edge>;`, which the reviewer's `"`-only framing
  would have missed) each observed caught, restored.
- **BLOCKER — block comments are token separators the collapse never
  removed.** `use std::/*c*/include as inc;`, `include/*c*/!("dark.rs")`,
  and `#/*c*/[path = ...]` all compiled and evaded every arm — the third
  insignificant-separator class after round-3 whitespace and round-4 bidi
  marks, including the composite `include/*c*/!(concat!(...))` that also
  defeats the token sweep and therefore sat strictly outside the stated
  concat residual. Repair: every arm now judges TWO views of the line —
  the whitespace-and-`r#`-collapsed form, and that form with `/*…*/`
  spans removed (unclosed `/*` comments out the rest of the line, a
  dangling `*/` the start) — and flags on EITHER, so over-removal on
  pathological string content can only over-flag [round 8 falsified the
  bracketed mechanics AND that last clause: collapsing before stripping
  fabricated openers, the dangling-`*/` discard deleted flagged
  prefixes, and string content CAN under-flag by poisoning the tracking
  while comment bytes blind the plain view. The stripper now runs raw
  and never discards, the `r#` strip became extra views, and
  quote-plus-delimiter lines with splice tokens are flagged outright by
  the ambiguity arm instead of judged]. A block comment
  SPANNING lines is the already-stated split residual and the header now
  names it as such, along with the split+concat compound. Mutations
  **M38** (comment-interleaved alias creation) and **M40** (the exact
  composite, cfg-gated so it compiles) observed caught, restored.
- **MAJOR — `r#include` is a resolvable alias-creation spelling.**
  `use std::r#include as inc;` compiles and wrote no matchable opener
  (`r#` broke the `::include` adjacency). The collapse now strips `r#`
  sequences in both views [amended after round 9: superseded twice over
  — the collapse filters whitespace only, and `r#`-removal became a pair
  of EXTRA views (four total) in round 8 so that removal can never
  destroy an adjacency. Do not restore the in-place strip]; mutation
  **M39** observed caught, restored.
- **REFUTED:** "the compiler-backstop claim is false — an innocuous-alias
  double `#[path]` mount compiles cleanly." The adjudicator reproduced
  the opposite: this tree's `authority.rs` references
  `PhysicalRootIdentity` by ABSOLUTE path while `registry.rs`/`mutation.rs`
  type the same values by relative path, so an alias double-mount makes
  the two paths resolve to distinct types and rustc rejects it ("similar
  names, but are actually distinct types") while the single mount
  compiles. The round-5 sentence stands, with its honest residual stated:
  the backstop is contingent on `authority.rs` keeping its absolute
  paths — the `(total, distinct)` bind, not the compiler, is the stated
  catch.
- Docs minors repaired: the receipt's round-5 scope note wrongly called
  `embedded.rs::SourceCloseReport` Slice-2-owned (it is T047's — and its
  contract twin `symforge::embed::SourceCloseReport` IS a real atom, so
  provenance matters there most) and omitted `EmbeddedSourceHandle` from
  the Slice-2-publics enumeration — both corrected in place; the round-4
  "fixed twice over" bullet now carries its bracketed amendment (round 5
  deleted the dead bidi exclusions the sentence still claimed).
- Notes folded: the path-segment tail-check comment now states its three
  deliberate widenings (end-of-line, non-ASCII, `as`-prefixed identifiers
  — all over-flag only); "every resolvable alias-creation site must
  write" is qualified to SINGLE-LINE sites, with the split declaration
  named as the header's stated residual. [Amended after round 11 — the
  third and last instance of this universal, missed by round 10's sweep
  because the phrase wraps across two lines: qualifying it to
  single-line sites did not save it. `use include as mount;` is one
  physical line and writes no path segment at all. The arm enumerates
  four openers and claims the enumeration, not the universe.]

## Gate results for the round-6 repair chunk

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `preventive_runtime_dark_v11` | 3 passed, 0 failed; the four new allowlist entries were the complete flag set on first run |
| `runtime_dark_v11` + `public_api_delta_v11` | 11 + 2 passed, 0 failed |
| `server_api` lib tests | 2 passed, 0 failed |
| mutations | M37, M38, M39, M40, M41 each observed caught (each plant COMPILES — the lib built clean before every sweep run); restored |

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
  the std/core root [amended after round 9: that universal is FALSE.
  Rust 2018 uniform paths let `use include as mount;` bind the macro
  with no first hop at all — live on this crate's edition, and the
  sweeps could not see it. A fourth opener, `useinclude`, was added; the
  arm is an enumeration of four spellings, not a proof about every
  form]. Zero flags on the tree; mutation **M36**
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
  [Amended after round 6: "twice over" lasted one round — round 5's minors
  deliberately deleted the dead bidi exclusions from the collapse (the
  outright flag owns U+200E/U+200F entirely), so the collapse filters only
  `char::is_whitespace` again. The guarantee is unchanged; do not re-add
  the exclusions.]
- **Alias route:** `use std::include as inc;` then `inc!(...)` was a
  single-line ASCII splice with no matching spelling. [Amended after round
  5: this bullet's "any use-declaration naming `include`" claim was
  falsified — the use-PREFIX test missed `pub(crate)`, tabs, and leading
  attributes; the arm now flags `include` in path-segment position on the
  collapsed line, which every resolvable aliasing form must write.]
  [Amended again after round 9: that replacement universal was false too
  — uniform paths (`use include as mount;`) write no path segment. The
  arm now enumerates four openers (`::include`, `{include`, `,include`,
  `useinclude`) and claims the enumeration, not the universe.]
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
  [Corrected by PR 4 Round 3: the full-line rule is lexical too and cannot
  prove Rust trait or inherent-method dispatch. The post-Round-3 mechanism is
  the reviewed excluded-source seal composed with the caller tripwires; it is
  still not a compiler-semantic call graph.]

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
  FULL-LINE comment — first non-whitespace bytes `//` [amended after round
  6: that alone was refutable via a line-spanning string or block-comment
  tail; the exemption now also requires no `"` and no `*/` on the line] —
  so a real call edge structurally cannot be
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

## T051 — original PR 3 call-edge proof (historical; superseded by the layered Round-3 repair)

`tests/preventive_runtime_dark_v11.rs` exists now — its creation is T051's
own act, held back from every earlier chunk on purpose — and it turns the
darkness paragraph of `index_lifecycle/mod.rs` into two executing tests
[amended after round 8: "two" was the count at this section's writing and
went stale — the file has held four tests since round 7: the two darkness
sweeps this section describes, plus `source_splicing_is_allowlisted`
(the C9 tripwire, round 1) and `no_gate_builds_doctests` (the doctest
bound pin, round 7)].

This section preserves the PR 3 chronology and receipts. It is not the current
closure claim: Round 3 proved these lexical sweeps can miss semantic dispatch,
so the load-bearing whole-`src/` seal, narrow diagnostic seal, and final
mutation/gate receipts are recorded in the T052 section above.

**The sweep rule is fail-closed.** A line naming the dark surface outside
its directory passes only as prose or as one of the exactly-two
mount-declaration lines in `src/live_index/mod.rs`. [Amended after rounds
1–3: the original "a real call edge cannot pass" was an overclaim, and two
successive mid-line-comment lexers each laundered an edge through some
literal form — string literals, char-literal polarity, raw-string quote
parity. Round 3 took the C8 ruling's second arm and DROPPED the mid-line
comment exception: prose is now only a FULL-LINE comment.] [Amended after
round 6: "after which Rust permits no code on the line" was itself false —
the tail line of a line-spanning string literal or block comment may begin
with `//` as CONTENT and execute code after its closing delimiter on the
same line. The exemption is now the INERT form: first non-whitespace bytes
`//` AND neither `"` nor `*/` anywhere on the line — every string form
closes with a `"` and every block-comment level with `*/`, so a line free
of both cannot hand control back to code. The two legitimate quoting
comments this surfaced are allowlisted, not silently tolerated.] A
string-literal or trailing-comment mention FAILS and forces a
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
count-pinned.] [Amended after round 7: the pin is NINE since round 6 — the
quote-narrowed prose exemption surfaced one quote-bearing doc comment in
`public_api.rs`, allowlisted with the dual-count bind at (9, 9).]

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
`find_references` (11 → 1), `append_impact_footer`, `edit_plan`, and the original
PR-2 `analyze_file_impact` entry capture. Post-Round-3 review found that the impact
sidecar can publish a newer winning bundle, so the current candidate deliberately
supersedes that one site: the sidecar returns its exact receipt publication, and the
text, co-change footer, and local typed evidence all consume that same bundle rather
than a free before/after sample.
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

This is a living slice record; T052 remains open until a trustworthy CLEAN
terminal review. Command and mutation outcomes explicitly labeled observed were
executed on the tree named by that entry. Source-inspection conclusions,
authority classifications, and adjudications are reasoned from cited pinned
bytes unless explicitly paired with an execution receipt;
reconstructed/source-unknown outcomes are historical only.

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
  preserving-Current half compares an immutable local identity to itself.
  T047's stand-in never touches that generation, so the assertion remains
  unfalsifiable after T047. Closure belongs to live-observer invalidation in
  T056/T063. T052's review must not count it as coverage until then.
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
