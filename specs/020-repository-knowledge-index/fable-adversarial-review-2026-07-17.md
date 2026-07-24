# Fable Adversarial Review

Feature 020 — Repository Knowledge Index. Fresh independent read-only pass per
`review-request-fable.md`. Execution: 8 adversarial dimension lenses (each read the
complete 11-artifact canonical SpecKit plus its assigned source seams), one
independent refuter per finding (53 findings adjudicated, 18 refuted on
cross-document evidence), a completeness critic over the 12 questions and the
invariant list, one gap-closing pass for the critic's uncovered invariants, and
lead source spot-checks (`src/paths.rs:96-111`, `src/live_index/store.rs:1183-1190`,
`src/live_index/persist.rs`). No repository file was modified; no mutating command
was run. 63 agents, ~7.6M tokens.

## Verdict

**PASS WITH CHANGES** — with blocking semantics: the two HIGH findings are
contract-unsafe as written and must land before the SpecKit freezes; per the
project's own standing gate rule, accepted HIGH findings then require a fresh
three-lens re-review before production code (see Re-review trigger).

Why not FAIL: the architecture withstood attack everywhere it was hit — the
metadata-first lifecycle, closed disposition/target types, one-capture publication
rule, strong snapshot identity, secret lane, and root/state separation all survived
independent refutation, and all three first-pass HIGH corrections (H-01..H-03) are
genuinely landed. Both new HIGHs have sentence-scale, local corrections that do not
disturb any concept in the complexity budget.

Why not PASS: both HIGHs permit a fully conforming implementer to violate a P0
safety/trust invariant, and two of the four handover blocker corrections landed
only partially (Findings 12 and 18).

## Findings

1. **[HIGH] Curation pending-intent recovery and idempotent replay are fenced only by ledger byte-images — state recorded for repository A can mutate repository B at the same path, or replay "applied" success that is not live**
   - Evidence: `contracts/knowledge-authority-hygiene.md:229-232` (durable intent = request/mutations + pre/post images only), `:243-244` ("same key/hash replays the stored result") and `:248-256` (recovery decides purely by pre/post byte-image comparison); `data-model.md:1480-1484`; `spec.md:570-582` (FR-037 recovery clause, byte-only); `spec.md:481-484` (FR-012 declares "a different repository at the same path is foreign state" — applied only to snapshot load/overwrite); `contracts/source-binding-and-state.md:104-107` ("`ProjectId` chooses placement only; it is not proof…" — caveat never extended to replay/intent records); `src/idempotency.rs:78-87,194-210` (existing `ReplayRecord` the plan reuses carries no source identity); `spec.md:248` (US5-6 states the principle for verifiers only).
   - Failure scenario: repo A at path P has no `.symforge-knowledge.toml` (canonical pre-image: absent/empty). `curate_knowledge` apply validates, durably records `pending_write` with post-image bytes, crashes before the temp write. User deletes A, clones unrelated repo B at P. `ProjectId` is path-derived, so the same `ProjectStateDir` is resolved; B also has no ledger → recovery's pre-image match "safely repeats the stored post-image write" and writes A's policy decisions into B's working tree. Independently: an idempotency replay of the A-era apply receipt under B returns stored success — `applied` reported for a project that is not live (exactly Q11's attack).
   - Violated invariant: "snapshot, reset, quarantine, checkpoint, and idempotency paths never reconstruct state from the source root"; US5-6 "a verifier captured for source/generation A cannot mutate source/generation B"; FR-012 foreign-state rule; Q11's replay postcondition (FR-049 pins it for `index_folder` but no analog exists for `curate_knowledge`).
   - Smallest correction: require the durable pending intent and the curation replay record to bind the strong source identity (repository fingerprint + `SourceId` + the already-guarded manifest/policy digests); recovery and same-key replay MUST verify that identity (and, for replay, current-session live binding to the same source) before finalize/retry/stored-success; any mismatch is a typed foreign-source conflict that quarantines the intent and writes nothing. Add red tests K-R13 (same-path replacement between `pending_write` and recovery → typed conflict, zero ledger write) and K-R14 (replay under replaced repo → typed conflict, never `applied=true`).

2. **[HIGH] The `PublishedSourceSet` registry swap has no pinned commit boundary or rebase scope — concurrent lanes can silently lose each other's publication, and a conforming reading lets healthy P1 ref churn starve or abort P0 current-worktree publication**
   - Evidence: `data-model.md:1216-1237` (`registry_generation` appears exactly once, semantics never defined; "A source update copies the bounded local source map, replaces one immutable bundle, and swaps once" — copy point unspecified); `plan.md:126-135` (Publication rule: rebase/retry "if source/project generation changed" — scope of "changed" undefined for registry-level swaps); `spec.md:461-466` (FR-008); `spec.md:281-283` + `tasks.md L-R07/L-V04` (P1 cannot block P0 — oracles cover lane *failure* and *budgets* only, not success churn); no priority/fairness/preemption text exists anywhere in the corpus.
   - Failure scenario: (a) Lost update — Gate L live: the local-ref lane finishes a long off-lock ingestion, copies source map `{A:pub5, R:pub3}`, builds `{A:pub5, R:pub4}`, swaps; concurrently the watcher commits `{A:pub6, R:pub3}` under source A's writer lock; whichever swap lands second silently discards the other lane's publication. (b) Starvation — an implementer who reads plan.md:134's "project generation changed" as covering any registry-level publication must rebase/retry or abort the P0 cold-start scout commit on every P1 bundle landing; with enough refs or ref-movement reconciliation, P0 readiness is delayed unboundedly by a *healthy* P1 lane.
   - Violated invariant: "Long off-lock builds cannot overwrite a concurrent watcher/reconciliation update" (M-01 as landed); SC-005 (zero mixed-generation bundles); "Local-ref P1 failure or memory pressure cannot block current-worktree P0" (violated in spirit by its only uncovered mode).
   - Smallest correction: two sentences in plan.md's Publication rule, echoed at `data-model.md:1216-1237`: (1) the `ArcSwap<PublishedSourceSet>` swap is the single serialized commit boundary per `ProjectInstance`; every lane commits under that one per-instance writer lock, and the local source map is copied under that lock, never from an off-lock snapshot; (2) a long off-lock build's rebase/retry condition is scoped to its own source — a registry swap that adds/replaces a *different* source's bundle never invalidates a prepared commit, and a P1 bundle add/update/remove advances `registry_generation` only, never the current worktree's publication/content/project generation. Add red tests: L-R12 (P1 bundle churn racing a P0 long build — P0 commits without unbounded retry/abort) and a registry-generation semantics oracle.

3. **[MEDIUM] Snapshot background verification's commit fence omits publication/content generation — a completed verification can overwrite a concurrent watcher update**
   - Evidence: `plan.md:380` (fence = strong source identity + project generation; project generation does not advance on watcher publications); `src/live_index/persist.rs:1208-1274` + `src/live_index/store.rs:680-681` (current unfenced structure the gates preserve in name); `tasks.md:185-186` (E-R03 covers retarget only).
   - Failure scenario: restart loads a snapshot candidate; verification runs while the watcher publishes content generation N+1 for an edited README; verification, whose stable reads predate the edit, completes and commits rebuilt state over the newer publication — same project, same source, fence satisfied.
   - Violated invariant: "Long off-lock builds cannot overwrite a concurrent watcher/reconciliation update"; FR-008 rebase/retry-or-abort.
   - Smallest correction: amend `plan.md:380`/FR-012 to fence verifier commits by strong source identity **plus captured base publication/content generation**, rebasing/retrying or aborting when the base advanced; add red test E-R10 `background_verify_racing_watcher_update_rebases_or_aborts`.

4. **[MEDIUM] Stale async temporal completion is rejected with no re-trigger/convergence rule — `code_signals` can stay pending forever (or respawn unboundedly) on an actively edited repository**
   - Evidence: `tasks.md:341-344` (H-R09 rejects stale completions); `data-model.md:1193-1199,1246-1250`; `src/live_index/git_temporal.rs:32-87` (~30s-scale computation on large repos); `quickstart.md:345-348`.
   - Failure scenario: agent saves every few seconds; content generation always advances before the ~30s temporal computation completes; every completion is stale and rejected; temporal evidence remains Pending for the whole session — "temporal evidence comes from one captured generation" degenerates into "from no generation, ever".
   - Violated invariant: FR-031 coverage honesty (liveness hole); role-card/temporal one-generation invariant.
   - Smallest correction: one rule in data-model's async-temporal paragraph + H-R09: a rejected stale completion MUST schedule recomputation against the latest content generation with bounded coalescing backoff; optionally accept a completion newer than the currently carried `computed_for_content_generation` as a derived-only publication with explicit lag coverage.

5. **[MEDIUM] The edit-safety trust store is a durable state consumer missing from the closed state-owner matrix**
   - Evidence: `src/edit_safety/trust.rs:334-336` (persists `dirs::data_local_dir()/symforge/trust.json` — a third, self-resolved global location); FR-050 (`spec.md:670-685`) and the ownership tables (`data-model.md:277-283`, `contracts/source-binding-and-state.md:113-117`) enumerate consumers exhaustively — trust store absent; `tasks.md:56-62` (B-R29 inventory omits it).
   - Failure scenario: B-R29's "every state consumer uses its typed owner" spy passes while trust.json keeps its own resolution path; a protected-root session still reads/writes an untyped global file, and the "closed and exhaustive" ownership claim is false on day one.
   - Violated invariant: "Every state reader and writer … must use the same resolved state-placement oracle."
   - Smallest correction: add the edit-safety trust store to the `ControlStateDir` row of FR-050/both contract tables and to the B-R29 consumer inventory.

6. **[MEDIUM] Moving sidecar port/PID/session descriptors to one process-global `ControlStateDir` removes the per-project discovery the current sidecar depends on, with no multi-project namespacing rule**
   - Evidence: `contracts/source-binding-and-state.md:116`; `data-model.md:281`; `src/sidecar/port_file.rs` (current per-project `.symforge` port-file discovery).
   - Failure scenario: two projects/daemons on one host write descriptors into one control directory with no specified namespacing; status readers cannot tell which descriptor belongs to which project instance; discovery becomes ambiguous or last-writer-wins.
   - Violated invariant: closed state ownership with reader/writer agreement (FR-050) — the reassignment is specified but its discovery semantics are not.
   - Smallest correction: one sentence — sidecar/daemon descriptors under `ControlStateDir` are namespaced per `ProjectId` (or daemon instance ID) and status readers resolve through the same namespace; note the migration from per-project files.

7. **[MEDIUM] Transient per-file failures (`Unreadable`, `UnstableDuringRead`) have no coverage or reconciliation linkage — they never self-heal**
   - Evidence: `data-model.md:499-513,557-560` (dispositions don't affect `CoverageStatus`); `spec.md:499` (FR-011 retries degraded *walks* only); equal-digest no-op rule (`data-model.md:650-654`) preserves the failed entry.
   - Failure scenario: an editor briefly locks a file → `Unreadable`; coverage stays Complete; digests equal; reconciliation no-ops forever; the file silently never becomes searchable until an unrelated change touches it.
   - Violated invariant: "Degraded coverage self-heals and can never be reported as complete no-evidence" — the per-file analog is unpinned; US4 freshness.
   - Smallest correction: transient-class `Unreadable`/`UnstableDuringRead` dispositions either mark coverage Degraded or carry a bounded re-observation trigger; the equal-digest no-op MUST NOT apply while transient-failure dispositions exist. Add red test D-R09.

8. **[MEDIUM] Refusing a single read larger than the total in-flight budget has no terminal-disposition mapping and no oracle**
   - Evidence: `plan.md:329` (Phase 2 "Refuse an individual read request larger than the global in-flight budget" — no disposition named); `tasks.md:137-138` (C-G06, no matching C-R red); `data-model.md:462-466` (no obvious variant assigned).
   - Failure scenario: a 2GB admitted-class file vs a 1GB in-flight budget: one implementer drops it (path disappears — the US2 sin), another marks `Unreadable(ResourceExhausted)`, another `HardSkip(PerFileCeiling)`; accounting equality diverges across implementations.
   - Violated invariant: "Every in-scope regular file has exactly one representable terminal disposition."
   - Smallest correction: name the disposition (natural fit: `HardSkip(PerFileCeiling)`) in plan Phase 2 and data-model, and add red test C-R07 `read_larger_than_inflight_budget_is_terminal_hard_skip`.

9. **[MEDIUM] The "tested platform atomic-durability contract" gating curation apply is undefined on Windows**
   - Evidence: `spec.md:570-582` (FR-037 requires "durable parent-directory commit under a tested platform contract"); `contracts/knowledge-authority-hygiene.md:229-256`; no artifact defines what operation constitutes parent-directory durability on NTFS (no `fsync(dir)` equivalent) or what the "test" is.
   - Failure scenario: Gate K's crash-recovery reds pass against a vacuous contract (implementer defines the untestable step as a no-op on Windows); apply is advertised Available on a platform where the required guarantee was never established — on the project's own primary platform.
   - Violated invariant: FR-037 "no best-effort weakening is allowed."
   - Smallest correction: specify the per-platform contract table (Unix: temp `write_all`+`fsync`, rename, `fsync(parent)`; Windows: `write_all`+`FlushFileBuffers`, `MoveFileEx(REPLACE_EXISTING|WRITE_THROUGH)` or documented equivalent, plus the startup probe that must pass) and state that an unprobeable platform exposes apply as Unavailable.

10. **[MEDIUM] `VerificationBaseline`/`ChangedSinceVerification`/`verified_against` is deletable v1 machinery (Q7)**
    - Evidence: `data-model.md:889-901,917-920,1043-1046,1066-1067,1090`; no user story, FR, or accepted prior finding requires it; no v1 producer exists (nothing generates baselines; curation may store `verified_against` but no tool creates one).
    - Failure scenario: Gate H carries a whole evidence family (types, hash validity rules, display state, policy field) that cannot fire in v1 — pure spec surface area and test burden with no reachable behavior.
    - Violated invariant: complexity budget ("Anything else requires a demonstrated missing behavior").
    - Smallest correction: remove `VerificationBaseline`, `ChangedSinceVerification`, and `verified_against` from v1 (fold to `ReviewDue`/`RelevantCodeChangedSinceDocument` signals); or name the v1 producer and fix Finding 16. Deleting also dissolves Finding 16.

11. **[MEDIUM] FR-004's required "typed capacity reason" has no representable type**
    - Evidence: `spec.md:445-452` (FR-004: abort before a `RepositoryManifest` exists; cold start non-Ready "with a typed capacity reason"); `data-model.md:584-585` (`EntryBudgetExceeded`/`MetadataBudgetExceeded` exist only as `ScoutIssueKind`), `:628` (`RepositoryManifest.issues` is their only container), `:571-577` (`FreshnessReason` closed, no capacity variant); `quickstart.md:155-160` (requires the two budget kinds distinguishable in health).
    - Failure scenario: cold-start metadata-budget exhaustion: no manifest may exist, so no `ScoutIssue` can be published; `FreshnessReason` offers only `ObservationFailed`. The implementer must invent an untyped string or smuggle a partial manifest — the quickstart assertion is unimplementable against the specified types.
    - Violated invariant: FR-004's own typed-reason MUST; the owned-typed-reason design rule.
    - Smallest correction: add capacity variants to `FreshnessReason` (or define a small health-owned `ObservationRefusal { kind }` record) and state that budget `ScoutIssueKind`s never appear in a published manifest.

12. **[MEDIUM] Gate E depends on `HistoryCoverage`/`HistoryLimit`, which the artifacts assign to Gate H — the Gate-E forward-dependency correction only partially landed**
    - Evidence: `data-model.md:1193-1198` (`CodeSignalsSnapshot.coverage: HistoryCoverage` is a Gate-E core field), `:954-966` (both types defined in the Gate-H authority section), `:1223-1228` ("Neither earlier gate depends on a future type"); `tasks.md:350-358` (H-G01/H-G04 own temporal-coverage types), `:211-213`.
    - Failure scenario: Gate E cannot compile without a type whose defining task is seven gates later — the exact defect class the third handover blocker was resolved to eliminate.
    - Violated invariant: corrected invariant 3 (Gate E implements only the core bundle after its types exist).
    - Smallest correction: reassign `HistoryCoverage`/`HistoryLimit` to the core type task (B-G01 or E-G01 wording) and note in the staging paragraph that they are Gate-E core types reused by Gate-H.

13. **[MEDIUM] `FileDisposition::Indexed` embeds `ParseStatus` without specifying it — the existing `ParseStatus` carries unbounded free text into the hashed manifest**
    - Evidence: `data-model.md:499-513` (field), `src/live_index/store.rs:225-232` (current `ParseStatus` with free-text error strings), `data-model.md:634-643` (disposition is a digest input), `spec.md:459-461` (FR-007 bounded diagnostics).
    - Failure scenario: a tree-sitter upgrade rewords an error message; every affected file's disposition — and the manifest digest — changes with zero logical repository change; valid snapshots are rejected as source drift. Also unspecified: what `parse_status` a knowledge-only file carries.
    - Violated invariant: NFR-001 determinism; FR-007; owned-serializable-reason design rule.
    - Smallest correction: define a bounded owned disposition-level `ParseStatus` (`Parsed | PartialParse | Failed`, no embedded text; free-text diagnostics stay in operational health outside the digest); state its value for knowledge-only files; add the missing `StableRead -> Unreadable(FullRead)` and parse-stage arrows to the transition diagram (Finding 28).

14. **[MEDIUM] Finding/provenance ID derivation is unspecified — the "stable finding/provenance/link IDs" invariant is implementable only for link IDs**
    - Evidence: `spec.md:487-490` (FR-014); `data-model.md:826-831` (link-ID derivation specified) vs `:1135-1141` (`finding_index` — "safe opaque" only); `contracts/search-knowledge.md:196-198` (test 17 requires IDs to resolve through `review_knowledge`).
    - Failure scenario: an implementer derives finding IDs from record indices or a per-publication counter (both "safe opaque"); a derived-only republication (same content generation) reorders records; captured finding IDs now resolve to wrong dossiers.
    - Violated invariant: corrected invariant 4 (stable IDs).
    - Smallest correction: mirror the `KnowledgeCodeLinkId` paragraph — a finding/provenance ID is a stable digest of (unit anchor identity, rule ID, evidence kind), excluding record index and publication generation; add a red test asserting resolution across a derived-only republication.

15. **[MEDIUM] `history`-scope membership of proven-divergent (Suppressed) units is internally contradictory**
    - Evidence: `contracts/knowledge-authority-hygiene.md:94` (history = rejected/deprecated/superseded/archived/historical) vs `:97-99` ("Proven-divergent … remain retrievable only through `history`/`all`"); `spec.md:362-371` (US9 independent test relies on historical scope retrieving the diverged unit); `data-model.md:1287-1291`.
    - Failure scenario: a diverged unit has lifecycle=Active, voice=Suppressed; one implementer maps `history` by lifecycle (unit unretrievable except `all`), another by voice — the US9 fixture passes on one and fails on the other.
    - Violated invariant: category-(a) internal inconsistency in the voice→scope projection.
    - Smallest correction: make the hygiene scope list voice-based and explicit ("`history`: units whose derived voice is HistoryOnly **or Suppressed**"), or state that proven-divergent Active units are `all`-only, and align spec.md US9 wording.

16. **[MEDIUM] `VerificationBaseline` persisted in the repo-portable policy ledger binds machine-local `SourceIdentity` and process-local `content_generation` — every committed baseline is void on any other clone**
    - Evidence: `data-model.md:895-901` (baseline fields), `:1084-1093` (`verified_against` inside the committed ledger entry); `SourceId` is a digest over machine-local worktree identity (`data-model.md:50-56`); content generations are process counters.
    - Failure scenario: team member A commits a ledger entry with `verified_against`; on B's clone the `SourceIdentity`/generation never match; the baseline is silently stale-on-arrival, so `ChangedSinceVerification` is unconstructible cross-machine — the field cannot do its one job.
    - Violated invariant: ledger portability (repo-owned input, `contracts/knowledge-authority-hygiene.md:104-139`).
    - Smallest correction: if Finding 10's deletion is not taken, make the persisted baseline portable: commit + anchor fingerprints only; `SourceIdentity`/`content_generation` remain a machine-local cache, never ledger material.

17. **[MEDIUM] Async temporal fencing binds only content-generation equality — nothing binds the analyzed Git history to the captured `SourceVersion`**
    - Evidence: `data-model.md:1246-1250`; `src/live_index/git_temporal.rs:56-77` (computation reads live repo state at run time); a `git commit`/ref move changes history without changing tracked bytes (content generation unchanged).
    - Failure scenario: capture at content generation N on commit C1; user commits (bytes identical, history now C2); the async computation reads C2's history and is accepted — its generation label matches N, but its temporal evidence contradicts the bundle's captured `SourceVersion.commit`.
    - Violated invariant: "Branch/timestamp/state labels never substitute for exact identity"; temporal evidence from one captured generation.
    - Smallest correction: fence temporal completion to (content generation AND the captured `SourceVersion` commit/tip object ID); mismatch rejects and reschedules (composes with Finding 4's rule).

18. **[MEDIUM] The SourceVersion-propagation correction only partially landed: `KnowledgeReviewSourceResult` carries no `source_version`**
    - Evidence: `data-model.md:1431-1440` (struct lacks the field) vs `contracts/knowledge-authority-hygiene.md:151-156` (review returns per-source identity/version incl. closed working-tree state) and FR-017 (`spec.md:499-503`, every per-source response envelope).
    - Failure scenario: review responses cannot satisfy their own contract without an undeclared field; an implementer omits it and review envelopes silently lack the closed working-tree state everywhere else mandates.
    - Violated invariant: corrected invariant 2 (SourceVersion in every per-source envelope).
    - Smallest correction: add `source_version: SourceVersion` to `KnowledgeReviewSourceResult`.

19. **[MEDIUM] `CodeEvidenceDisplay`'s "fixed precedence" is never defined**
    - Evidence: `data-model.md:1043-1046,1057-1060` ("compact deterministic projection with fixed precedence" — no order given anywhere).
    - Failure scenario: an implementer ranks `ConsistentForCheckedClaims` above `DeterministicConflict`; a unit with both shows consistent-first display; every text's determinism claim is satisfied while the display buries the conflict.
    - Violated invariant: deterministic authority display honesty (FR-014/FR-029).
    - Smallest correction: enumerate the normative order once in data-model (DeterministicConflict > BrokenAnchor > ImplementationGap > SuspectedConflict > ChangedSinceVerification > RelevantCodeChangedSinceDocument > ReviewDue > Partial > ConsistentForCheckedClaims > Unresolved > NotApplicable, or the intended equivalent).

20. **[MEDIUM] Ranking contracts contradict each other on authority down-ranking**
    - Evidence: `contracts/knowledge-authority-hygiene.md:90-91,99-101` (needs-review/unknown "labeled and **down-ranked**" in current answers) vs `contracts/search-knowledge.md:64-67` (rank chain = phrase/heading/term/source-precedence/path-line; "Document authority is a separate filter/label, never conflated") and `tasks.md I-R05` ("source precedence and document authority remain independent").
    - Failure scenario: an implementer adding the mandated down-ranking fails I-R05/contract-test-8 determinism wording; one omitting it violates the hygiene contract — the two documents cannot both be satisfied as written.
    - Violated invariant: category-(a) cross-contract inconsistency.
    - Smallest correction: pick one — add a final authority-tier factor to the deterministic chain (after source precedence, before path tie-break), or delete "down-ranked" from the hygiene contract; align I-R05 text either way.

21. **[MEDIUM] The designated deep-read path (`get_file_content`) keeps a session-lifetime repeat-read cache with no generation identity**
    - Evidence: `contracts/search-knowledge.md:170` ("Existing `get_file_content` remains the deep-read path after a knowledge hit"); current repeat-read dedup in the read-tools path has no publication/content-generation key; the mental-model contract fixes cache identity for context tools only (`contracts/repository-mental-model.md:139-141`).
    - Failure scenario: agent gets a knowledge hit at generation N, deep-reads the file, watcher publishes N+1, agent re-reads — the session cache serves N-era content labeled as a fresh read; stale evidence relabeled current through the officially recommended follow-up path.
    - Violated invariant: "No reader can observe stale evidence labeled current" (Q4).
    - Smallest correction: extend the generation-aware repeat-cache identity rule (project/source/publication/content generation) to `get_file_content`, or exempt it explicitly and require re-serving current bytes after a publication.

22. **[MEDIUM] The CCR retrieval handle is unredeemable on the compact surface**
    - Evidence: `src/protocol/ccr.rs:214-216` (footer names `symforge_retrieve` as the sole retrieval vehicle); compact surface = `symforge`/`symforge_edit`/`status` (FR-015); `contracts/search-knowledge.md:160-165` (facade returns this contract's result shape, including truncation/CCR).
    - Failure scenario: a compact-surface client's knowledge query truncates to CCR; the footer names a tool the client cannot call; the handle is dead and the evidence unreachable — recreating the broad-read fallback M-06 was accepted to prevent.
    - Violated invariant: M-06's intent; provenance-preserving truncation (US3-5).
    - Smallest correction: state that on the compact surface CCR retrieval routes through the `symforge` facade (retrieval intent) and the stored footer names that route; add an I-R09 sub-oracle for a compact-surface CCR round-trip.

23. **[MEDIUM] The OID-dedup share key is unpinned — a parse result is not a pure function of the object ID, but L-R02's "parsed once" invites keying on it alone**
    - Evidence: `data-model.md:1260-1262` ("blob **content** may be shared … keyed by object ID"), `tasks.md:554` (L-R02 "parsed once with multiple source mappings"), `research.md:350` ("deduplicate parsing/search content by object ID") — three unreconciled formulations; extraction is path-routed (`research.md:139-153`), and the detector's determinism domain includes `path` (`data-model.md:1184-1185`).
    - Failure scenario: the same blob is `notes.md` on ref A (Markdown → section units, Knowledge) and `notes.txt` on ref B (generic text → line units); a parse cache keyed by OID alone serves B with A's segmentation, heading breadcrumbs, and target class — structural provenance misattribution (authority/policy/bridge/identity each re-derive per source and are individually pinned; the parse layer is the one unpinned seam).
    - Violated invariant: FR-019 label fidelity (structural half); L-R10 parity oracle's own exception list.
    - Smallest correction: one sentence at `data-model.md:1260-1262` (echoed in L-G03): dedup sharing is limited to raw bytes and to parse/extraction/secret-scan results keyed by (object ID, scout classification/extraction route, extractor version, secret-policy version); a source whose per-path classification differs re-derives; roles, voice, bridge links, authority, temporal evidence, and policy always re-derive per source. Add red test L-R14 (same OID under two extensions).

24. **[MEDIUM] Budget exhaustion × hash-valid suppression has no defined fail direction and no red test** *(lead re-grade of a refuted HIGH: the refuter showed FR-032/SC-014/ledger-controls-voice already force the outcome — a conforming implementation cannot voice the unit as current — but conceded the residual below; lead holds it at MEDIUM because the two clauses are uncomposable without the missing sentence)*
    - Evidence: `contracts/knowledge-authority-hygiene.md:281-284` (policy entries/authority records have independent limits; exhaustion sets coverage) vs `:286-289` (fail-open/closed defined only for malformed/unsupported/hash-stale policy); `data-model.md:799-806` (`AuthorityRecords` is a truncatable derived set); `spec.md:548-551` (FR-032 unconditional).
    - Failure scenario: a ledger larger than the policy-entry/authority-record budget cannot both truncate derivation (sanctioned) and keep every hash-valid suppression applied (mandated); the implementer discovers the contradiction mid-Gate-H with no rule to follow; the worst conforming outcome is a superseded unit re-entering default results as Unknown-labeled.
    - Violated invariant: composability of FR-032 with the derived-budget clause; the feature's core anti-goal (stale prose re-entering default retrieval).
    - Smallest correction: one sentence — suppression-bearing hash-valid policy entries and proven-divergence findings have reserved budget priority and are never dropped by derived truncation; if a limit would drop one, affected units fail closed to NeedsReview voice and the response reports which suppressions were skipped. Add red test H-R12 (superseded unit past the AuthorityRecords cutoff stays out of default voice).

25. **[LOW] Operator profile and onboarding state are per-project today but reassigned to process-global `ControlStateDir` with no migration/semantics note** — Evidence: `src/cli/operator_profile.rs:3-5` (persisted to `<project>/.symforge/operator-setup.json`) vs `data-model.md:281`. Scenario: per-project profiles silently collapse to one global profile. Invariant: FR-050 closed ownership (semantic fidelity). Correction: one sentence stating the intentional per-project→global change and first-run migration/read-fallback behavior.
26. **[LOW] Same-project placement recovery is ambiguous** — "Placement is stable for a project-instance lifetime" (`contracts/source-binding-and-state.md:137-140`) vs FR-049 durable-recovery wording. Scenario: implementers disagree whether re-`index_folder` of the same live project re-resolves placement. Correction: state that placement re-resolution occurs only on a new `ProjectInstance`.
27. **[LOW] Circuit-breaker scope and post-trip semantics are unspecified** — Evidence: `spec.md:153`, `data-model.md:1497`. Scenario: a code-parse failure storm aborts knowledge extraction for unrelated prose; observation-level coverage effect undefined. Correction: scope the breaker per lane/stage and define post-trip coverage as Degraded with reconciliation retry.
28. **[LOW] State-transition diagram omits `StableRead -> Unreadable(FullRead)`** — Evidence: `data-model.md:1488-1499` vs US2-2 and `AccessStage::FullRead`. Correction: add the missing arrows (fold into Finding 13's edit).
29. **[LOW] Degraded-reconciliation termination is ambiguous** — "retries with bounded backoff … or remains explicitly degraded" (`spec.md:217-220`, FR-011) permits abandoning retry permanently. Correction: state that a terminally degraded walk still re-triggers on the next uncertainty signal and never becomes a silent stop.
30. **[LOW] "Observe intended failure" is unsatisfiable for red tests that reference not-yet-existing types/tools** — Evidence: `tasks.md:4-5`; most B/E/I reds cannot compile before GREEN. Correction: define the red-oracle rule to accept compile-fail as the observed failure for type-level oracles, with mandatory conversion to a runtime red before the gate closes.
31. **[LOW] L-R05's negative assertion has no observation mechanism** — Evidence: `tasks.md:557`. Correction: name the mechanism (process-spawn spy, `git2` fetch-callback assertion, filesystem sentinel on LFS smudge paths).
32. **[LOW] SC-006/M-004's "materially fewer tokens" has no threshold — the release oracle cannot fail** — Evidence: `spec.md:739-741`, `tasks.md:603-604`. Correction: set a numeric floor (e.g., ≥50% vs the broad-discovery baseline) or reclassify as measurement-only.
33. **[LOW] `KnowledgeUnitKind::StructuredElement` is a dead variant in v1** — Evidence: `data-model.md:668-672`; no producer in any task/contract/fixture. Correction: delete, or name its Gate-F producer.
34. **[LOW] `HardSkipReason::PolicyDenied` has no defined producer** — Evidence: `data-model.md:462-466`. Correction: name the producing policy (or delete; note Finding 8 proposes a producer for `PerFileCeiling`).
35. **[LOW] Redundant duplicated identity/generation fields make forbidden states representable** — Evidence: `SourceIdentity`/generations duplicated across `PublishedGeneration`, hits, and envelopes (`data-model.md:1200-1237,1330-1353`); a degraded-wrapper with mismatched duplicate fields is constructible. Correction: state that envelope/hit fields are derived at format time from the captured bundle, never stored independently.
36. **[LOW] `SearchKnowledgeHit.line` is pinned 1-based; `line_range`'s basis and inclusivity are unstated** — Evidence: `data-model.md:1330-1342`, `spec.md:95` (knowledge-hit invariant names 1-based lines). Correction: one sentence pinning `line_range` basis and end-exclusivity.

## What withstands scrutiny

- **The metadata-first total-disposition lifecycle with independent budgets.** H-01/H-02 corrections are landed and survived dedicated deadlock/collision attack; permit-release-at-handoff is coherent; metadata-terminal zero-read/zero-charge held everywhere it was probed.
- **`IndexTargets`/`ScoutDecision`/`FileDisposition` as landed.** The empty-target state is genuinely unrepresentable; the decision→disposition mapping is total; refuters killed every attempted illegal-state construction except the named residuals (Findings 11, 13, 28).
- **The one-capture `PublishedSourceSet` rule and degraded-wrapper freshness semantics.** Lead-verified against real defects: `swap_and_publish` (`src/live_index/store.rs:1183-1190`) publishes three `ArcSwap`s non-atomically today — the single-swap boundary fixes an actual, observed hazard. Do not weaken; Finding 2 pins its commit semantics, it does not question the design.
- **Strong `SnapshotSourceIdentity` (FR-012).** Source-grounded as necessary, not speculative: today's snapshot header is `{version, files}` with **no** identity (`persist.rs:108-111,915-916`), verification is recognized-only with unrestricted reads and unfenced mutations (`persist.rs:1017,1242,1229` vs the fenced variants at `store.rs:874-892`), and a size/mtime-preserving same-path replacement would be inherited today. E-R08/E-R03 are real oracles; keep every clause.
- **The secret lane.** Both dedicated attack scenarios died at three independent canonical blocks each (template paths still scanned; per-entry dispositions; the whole-hit output guard re-scanning every visible field). Discard-before-publication for both targets, policy-versioned snapshots/CCR, and no-echo query guarding are consistently specified across all artifacts. M-07/M-08/M-09 landed.
- **Root binding / state placement / control-state separation.** Grounded in verified defects (`select_runtime_data_base`'s CWD-relative fallback at `paths.rs:96-111` is real). The two-decision model (RootResolution then StatePlacement), session-scoped protected authority, and the live-postcondition replay rule (FR-049) withstood every inheritance/replay attack for `index_folder` — Finding 1 asks only that curation replay meet the same bar.
- **Authority axes independence and the deterministic proof matrix.** Age-never-proof, intent-preservation, unit-level granularity, and the implementation-gap rule survived all fixtures thrown at them; the refuter that killed the authority HIGH did so by demonstrating the spec's own MUSTs force the right outcome.
- **No second search store; Section-span projection; complexity budget.** The minimalist lens traced every budgeted concept except Finding 10's baseline machinery to a user scenario, accepted finding, or invariant; nothing rejected in research.md crept back.
- **Compact-3 preservation with no-match-as-success gating (M-06).** Correctly pinned against the real `serve_chain` collapse; FR-015/I-R09 are the right gate. Finding 22 completes it for CCR handles.
- **The prior review's rejected proposals stay rejected.** Local-ref P1 in-feature (bounded, isolated) and heading/current-source ranking precedence both re-survived; no relitigation is warranted.

## Missing tests

- `curation_recovery_rejects_foreign_source_pending_intent` (K-R13) — same-path repository replacement between `pending_write` and recovery must yield a typed foreign-source conflict with zero ledger bytes written; fails while recovery is byte-image-only.
- `curation_replay_requires_live_same_source_binding` (K-R14) — replaying an apply receipt under a replaced repository must return a typed conflict, never `applied=true`; fails while replay is receipt-only.
- `p1_bundle_churn_cannot_starve_p0_commit` (L-R12) — N local-ref bundle commits land while a P0 long off-lock build prepares; P0 must commit without unbounded retry/abort; fails under the literal "project generation changed" rebase reading.
- `p1_bundle_add_advances_registry_generation_only` — a P1 add/update/remove must not advance P0 publication/content/project generations.
- `concurrent_lane_swaps_lose_neither_publication` — watcher and ref-lane commits race on one `PublishedSourceSet`; both bundles must be present in the final registry state.
- `background_verify_racing_watcher_update_rebases_or_aborts` (E-R10) — verification captured at publication P must not commit over the watcher's P+1 (Finding 3).
- `stale_temporal_rejection_reschedules_and_converges` — a rejected stale temporal completion must lead to a later accepted one under continuous edits (bounded backoff observable); fails if rejection is terminal (Finding 4).
- `temporal_completion_rejected_on_history_mismatch` — a completion whose analyzed tip differs from the captured `SourceVersion` commit must be rejected even with equal content generation (Finding 17).
- `budget_truncation_never_restores_suppressed_voice` (H-R12) — a hash-valid superseded entry past the AuthorityRecords/policy-entry cutoff must stay out of default voice with an explicit skipped-suppression report (Finding 24).
- `transient_unreadable_self_heals_via_reconciliation` (D-R09) — an `Unreadable` disposition from a transient lock must become Indexed after reconciliation without any other repository change; fails while equal-digest no-op preserves it (Finding 7).
- `read_larger_than_inflight_budget_is_terminal_hard_skip` (C-R07) — one deterministic named disposition, no deadlock, accounting equality preserved (Finding 8).
- `cold_start_budget_exhaustion_yields_distinct_typed_capacity_reasons` — entry-budget vs metadata-budget exhaustion produce distinguishable typed health reasons with zero manifest (Finding 11).
- `windows_parent_durability_contract_probe_gates_apply` — the platform durability probe must be executable and must be able to fail; apply exposed as Unavailable when it does (Finding 9).
- `parse_status_is_bounded_and_digest_stable` — reworded parser diagnostics must not change the manifest digest; knowledge-only files carry a defined parse status (Finding 13).
- `finding_ids_survive_derived_only_republication` — captured finding/provenance IDs must resolve identically through `review_knowledge` after an async temporal republication (Finding 14).
- `deep_read_after_publication_serves_current_bytes` — `get_file_content` after a watcher publication must not serve session-cached prior content (Finding 21).
- `compact_surface_ccr_handle_roundtrip` — a truncated knowledge result on the compact surface must be retrievable through the facade (Finding 22).
- `same_oid_different_extension_derives_per_source_units` (L-R14) — one blob as `.md` and `.txt` yields per-source unit segmentation and target class; secret scan re-evaluated per classification (Finding 23).
- `trust_store_uses_typed_state_owner` — extend the B-R29 spy inventory to the edit-safety trust store (Finding 5).
- `same_file_symbol_twins_stay_ambiguous` — two same-name/kind symbols at different spans in ONE file must remain `Ambiguous` (G-R02 covers cross-file twins; the same-file case has no oracle).

## Re-review trigger

Yes. Two HIGH findings are expected to be accepted, and the project's standing gate
rule is explicit: any accepted HIGH finding requires correction and a **fresh
three-lens run**; partial lens reuse does not satisfy the gate. After Findings 1
and 2 (and the MEDIUM batch) land in the canonical artifacts, run one fresh
independent Architect/Skeptic/Minimalist pass with emphasis on: (a) the
publication/commit semantics seam (plan Publication rule, `PublishedSourceSet`,
verifier fencing — Findings 2, 3, 4, 17), and (b) the curation/idempotency lane
(hygiene contract apply/recovery, FR-037/FR-049 parity — Finding 1). The MEDIUM
corrections are consistency edits and do not individually require re-review beyond
that pass; record rejected findings with evidence in the adversarial-review log as
before.
