# Codex follow-up recommendations — Feature 021 — 2026-07-28 — consultation `d4e82b`

**Purpose:** Owner-level recommendations for the five questions in `CODEX-FOLLOWUP-2026-07-28.md`, building on the settled T105 and WS5 rulings.

**Status:** Complete.

**Decision standard:** Preserve Feature 020's frozen no-raw-read invariant for `SensitivePath` and `SensitiveContent`, prefer one owner per behavioral seam, and accept a gate only when its assertion fails on the pre-fix behavior.

---

## Q1 — What detector rule satisfies both directions?

### Answer

Use a **two-stage detector**. A single regex is the wrong tool for this rule: it can extract a full assignment without look-behind, but it cannot safely combine identifier-component semantics, path-aware placeholders, balanced call-expression recognition, and the fail-closed default.

The regex should be a candidate finder only. A small Rust filter should make the classification decision. No new dependency is needed.

### Stage 1 — Extract the complete key and RHS

Use a `regex::bytes::Regex` 1.11-compatible candidate pattern:

```rust
r#"(?i)(?:^|[^A-Za-z0-9_$-])["']?(?P<key>[A-Za-z0-9_$-]*(?:api[_-]?key|secret|token|password|passwd|pwd)[A-Za-z0-9_$-]*)["']?[ \t]*[:=][ \t]*(?P<rhs>"(?:\\[^\r\n]|[^"\\\r\n])*"|'(?:\\[^\r\n]|[^'\\\r\n])*'|`(?:\\[^\r\n]|[^`\\\r\n])*`|[^\s"'`#]+)"#
```

Properties that matter:

- The consumed left delimiter applies to the **complete key**, so there is no forbidden look-behind.
- The `key` capture includes the entire identifier. The filter never mistakes an internal `token` substring for a key boundary.
- Snake case, kebab case, camel case, screaming snake case, quoted config keys, and object properties all become candidates.
- The `rhs` capture preserves whether a value was single-, double-, or backtick-quoted and captures an unquoted atom through structural trailing punctuation.
- Candidate extraction is intentionally permissive. A candidate is not yet a finding.

The existing `SecretRule { secret_capture }` loop models one captured value followed by one generic placeholder check. Context assignment now needs the path plus two semantic captures. The smallest honest implementation is a dedicated `scan_context_assignments(path, bytes)` beside the generic rule loop, compiled through the same fail-closed policy initialization. Do not add a general validator framework for this one special case.

### Stage 2 — Decide in Rust

Apply these rules in order:

1. **Tokenize the full key.** Split ASCII components on `_`, `-`, lower-to-upper camel transitions, and acronym-to-word transitions. Lowercase the components.
2. **Require credential components.** Accept an exact component `token`, `secret`, `password`, `passwd`, or `pwd`; adjacent `api` + `key` or `client` + `secret`; or the compact aliases `apikey` and `clientsecret`.
3. **Normalize the RHS.** Remove structural trailing `,`/`;`, then remove one matching quote pair while retaining a `was_quoted` flag. Preserve the existing minimum payload length of eight bytes.
4. **Apply placeholders.** Keep the current exact placeholder list plus `${…}` and `{{…}}`. Treat exact case-insensitive `{canary}` as a placeholder only when `LanguageId::from_path(path)` identifies a code language. Do not whitelist arbitrary `{…}`. In config, unknown, query, or output paths, `{canary}` remains sensitive.
5. **Quoted non-placeholder values are sensitive.** A call-looking string is still a literal value, not executable code.
6. **Exempt one proven code form.** Only for a recognized code-language path, an unquoted RHS may be clean when a small byte parser consumes the entire expression as:

   ```text
   path-or-member ("(" balanced-arguments ")") (member ("(" balanced-arguments ")"))*
   ```

   Require at least one call, balanced delimiters, and full consumption. This covers the four measured forms: method call, associated-function call, single call, and chained calls.
7. **Default ambiguity to sensitive.** Bare identifiers, member-only chains, malformed or unbalanced calls, trailing bytes after a call, unknown-path candidates, and every remaining config-file candidate remain fail-closed.

Do not add entropy scoring. It is unstable across credential providers and creates false negatives. Base64ish, base64url, hex, JWT-like, and other opaque atoms naturally reach the default-sensitive branch. Also do not use “contains parentheses” as the exemption; only the bounded full-expression parser may return clean.

Reuse `LanguageId::from_path` and `LanguageId::is_code_language` from `src/domain/index.rs:68-124`. Parsing the entire source with tree-sitter is not appropriate here because secret classification is the pre-parse trust boundary.

### Bidirectional oracle

Build every opaque fixture at runtime from non-secret fragments. Assert the exact `SecretScan` variant; for positives, also assert exactly one `secret.context-assignment` rule ID and `finding_count == 1`.

#### Expected `Clean`

| Path | Constructed assignment | Purpose | Pre-fix state |
|---|---|---|---|
| `.rs` | `let token = token.to_lowercase();` | method call | **RED** — currently sensitive |
| `.rs` | `let original_stop_token = Arc::clone(&watcher.stop_token);` | full-key boundary plus associated call | **RED** |
| `.ts` | `token: Symbol(sessionId),` | call-valued property | **RED** |
| `.ts` | `password = page.locator(passwordSel).first();` | chained calls | **RED** |
| `.rs` | `token={canary}` | exact source canary placeholder | **RED** |
| `.rs` | `password={canary}` | exact source canary placeholder | **RED** |
| `.rs` | `tokenizer = "<runtime opaque fixture>"` | reject keyword substring | GREEN guard |
| `.rs` | `secretary = "<runtime opaque fixture>"` | reject keyword substring | GREEN guard |
| `.env` | `token=${TOKEN}` | existing placeholder | GREEN guard |
| `.yaml` | `token: {{TOKEN}}` | existing placeholder | GREEN guard |

The first six rows are the mandatory RED-before-fix set named by the consultation.

#### Expected one sensitive context-assignment finding

| Path | Constructed assignment | Shape/convention | Pre-fix state |
|---|---|---|---|
| `.env` | `access_token=<runtime base64ish fixture>` | snake case, unquoted | GREEN guard |
| `.yaml` | `refresh_token: <runtime JWT-like fixture>` | snake case, unquoted | GREEN guard |
| `.rs` | `db_password = "<runtime punctuation-bearing opaque fixture>"` | snake case, quoted | GREEN guard |
| `.toml` | `clientSecret = "<runtime 32-digit hex fixture>"` | camel case, quoted | GREEN guard |
| `.yaml` | `api-key: <runtime base64url fixture>` | kebab case, unquoted | GREEN guard |
| `.env` | `AWS_SECRET_ACCESS_KEY=<runtime base64ish fixture>` | screaming snake case, full-key capture | **RED** — currently missed |
| `.rs` | `token=<runtime canary fixture>` | output-guard canary must remain positive | GREEN guard |
| `.env` | `token={canary}` | source-only placeholder exemption must not reach config | GREEN guard |
| `.rs` | `token = "page.locator(passwordSel).first()"` | quoted call text is a value | GREEN guard |

Add parser counter-oracles that must also remain sensitive:

- an unquoted bare identifier;
- a member-only chain with no call;
- an unbalanced call;
- a valid call followed by trailing non-structural bytes;
- the same unquoted call expression under `.env` or an unknown path.

These counter-oracles prevent the measured-code exemption from becoming a general “looks vaguely like code” bypass.

### Acceptance rule for T066

T066 is GREEN only when all four statements are true in the same focused test run:

1. every mandatory negative is exactly `Clean`;
2. every positive is exactly one context-assignment finding;
3. the existing output/query runtime-canary tests remain positive;
4. a final scan of the five measured repository paths shows those false-positive findings are gone without changing legitimate exclusion policy.

A logic-equivalent 16-case prototype passed all 16 cases. It validated the decision order, full-key handling, and call-expression exemption, but it was not execution of Rust's `regex::bytes`; the focused Rust oracle above remains the release proof.

---

## Q2 — Should T104 remain an open decision?

### Answer

No. Replace T104 with a **non-blocking carried-forward constraint**. Feature 021 should preserve Feature 020 v1's whole-file metadata-only disposition for detector-positive and indeterminate content. Per-range suppression is a separate security feature and must not gate ACH-02, WS5B, or Feature 021 close-out.

This is a contract decision, not a statistical one. More repository-hit counts cannot prove range suppression safe because today's detector matches are classification sentinels, not guaranteed complete redaction spans.

### Why this is the safe and smaller decision

Feature 020 already establishes a single pre-parse containment boundary:

- A positive or indeterminate scan maps the entire file to `MetadataOnly`.
- Transient bytes and their content hash are discarded before parsing or publication.
- `SensitiveContent` loses every content-bearing target and retains only safe identifiers and counts.
- The watcher, cold-load, and local-reference ingestion paths all stop before content enters the live index.

The runtime model also supports only rule identifiers and counts; it does not expose trustworthy sensitive ranges. Most decisively, `secret.private-key-envelope` detects an envelope marker, not the payload between the markers (`src/knowledge/mod.rs:69-73`). Suppressing only that match would leave the payload available for parsing and could remove the very sentinel used by a final re-scan (`src/knowledge/mod.rs:356-388`).

That creates an asymmetric trade-off:

| Choice | Availability | Security proof surface |
|---|---|---|
| Preserve whole-file demotion | Some code intelligence is lost for a detected file | One existing, bounded, pre-parse boundary |
| Add matched-range suppression | Some unaffected content may be recovered | Every parser, offset, reference, snapshot, excerpt, diagnostic, search, analytics, and raw-read lane must prove non-disclosure |

The first choice is appropriate for Feature 021. It is already implemented, is consistent with the frozen Feature 020 contract, and avoids writing a new redaction subsystem without a complete-range data model.

### Exact replacement wording

Replace T104 with:

> **T104 [CONSTRAINT — non-blocking] Preserve Feature 020 v1 secret disposition.** `SensitiveContent` and indeterminate scans remain whole-file metadata-only, lose every content target, and discard transient bytes/hash before parsing, publication, or raw/lexical fallback. Feature 021 does not decide or implement per-range suppression. This task does not gate ACH-02 or WS5B.

Replace D1 with:

> ### D1 — Secret-disposition granularity is carried forward, not reopened
>
> Feature 021 preserves Feature 020 v1 whole-file demotion. T105's structured refusal leaves ACH-02 with no raw/lexical security fallback, while WS5B T070 remains limited to typed non-security dispositions or explicit coverage. Per-range suppression requires a separate Feature 020 contract amendment: current findings are classifiers, not guaranteed complete redaction spans, and sentinel or indeterminate findings must remain fail-closed.

If owners want to retain a future-work note, use:

> **Deferred security design: range-aware secret containment.** Do not schedule until product demand justifies it. Any proposal must distinguish complete sensitive spans from sentinel findings, keep sentinel and indeterminate cases whole-file, redact before parsing, and prove that neither detected bytes nor derivatives reach cold-load, watcher, local-reference, snapshot, search, diagnostic, analytics, CCR, or output lanes. Until that contract is approved and adversarially tested, Feature 020 v1 full-file demotion remains authoritative.

### Required plan cleanup

- Remove T104 from dependency and stop-condition language in `tasks.md`.
- State explicitly that T070 may use a fallback only for typed **non-security** dispositions.
- Treat `SensitivePath`, `SensitiveContent`, and indeterminate scans as unreadable in every lexical/raw selector mode.
- Implement T105 as a caller-side admission check before any filesystem fallback, not merely an `around_symbol`/`around_match` formatter rule. The current fallback in `src/protocol/tools.rs:8588-8628` can otherwise read a metadata-only path directly.

### Acceptance check

One test matrix should exercise default, chunk, `around_symbol`, and `around_match` requests against each security disposition and assert a structured refusal occurs **before** any raw filesystem read. Non-security Tier-2 cases may follow the separately specified bounded fallback policy.

---

## Q3 — How do the 17 prior findings change after the two owner rulings?

### Answer

The rulings materially simplify the plan, but they do not make the review findings disappear:

- **3 are moot by decision**, although their stale instructions still need removal.
- **4 change shape** because ownership or the intended implementation moved.
- **8 remain live** defects.
- **2 become newly conflicting** with the now-owned WS5 work.

“Moot” below means the design question has been answered. It does not mean the current plan, tasks, or source already encode the answer.

| # | Prior finding | Status after rulings | Required disposition |
|---:|---|---|---|
| 1 | SC-015 requires legitimate exclusions to become Tier 1 | **Still live — blocker** | Partition detector false positives from legitimate exclusions. T120/SC-015 must assert opposite outcomes rather than forcing every T102 fixture into Tier 1. |
| 2 | ACH-02 cannot secure raw reads from `format.rs` | **Changed shape** | Implement one caller-side, pre-read admission veto in `tools.rs`. Remove the bounded security-fallback story; all selector modes must refuse security dispositions. |
| 3 | The detector fix has no positive oracle and can pass by suppressing everything | **Still live — blocker** | Replace the prose regex prescription with Q1's bidirectional oracle. T066 is now owned by Feature 021 and can start early. |
| 4 | T153 is already green before ACH-04 | **Newly conflicting** | Once owned WS5B qualifies negative search results, T153 loses its claimed RED state. Split it into a baseline full-index guard plus deliberate RED cases for post-index admission and non-security exclusion. |
| 5 | WS5 seams have duplicate owners | **Moot by ruling** | Cross-owner drift is gone, but consolidate duplicate executable paths: T111/T115 with WS5C and T148/T152 with WS5B. One feature should still issue one instruction per seam. |
| 6 | Five VERIFY tasks do not run the assertions they claim to prove | **Still live** | Put the exact behavioral command after every added assertion. T131 must verify refusal and requested-versus-honored annotation, not “first-line” success. |
| 7 | D5 makes retained stale parsed bytes authoritative | **Still live — blocker** | The current terminal disposition is authoritative; retained parsed bytes are only last-valid content. Represent disagreement as a typed stale/inconsistent state, never clean Tier 1. |
| 8 | SC-006 has no observable context-side GREEN | **Still live** | Add one shared public admission/identity envelope to context and impact, or narrow SC-006. Design it together with finding 7. |
| 9 | Twenty-one WS5 tasks have no owner or schedule | **Moot by ruling** | Replace the external three-task gate with owned implementation of T062-T082, the missing WS5B-E gates, and inherited-test reruns. |
| 10 | ACH-01 can choose a longer suffix filename after a path-shaped miss | **Still live** | Return on path-shaped zero-match before bare filename-suffix or symbol fallback. Add the `generator.service.spec.ts` versus `service.spec.ts` oracle. |
| 11 | ACH-01 “real reason” depends on reason codes that do not yet exist | **Changed shape** | Split the independent fail-closed safety veto from the honest metadata/recovery response. Fold the latter into owned WS5A/WS5C. |
| 12 | Phase 4 falsely gates ACH-02 and ACH-03 on all WS5 work | **Changed shape** | ACH-02 depends only on the settled refusal contract. ACH-03 depends on size plus the corrected D5/public-envelope contract. ACH-04 needs the reason/search chain. |
| 13 | Refusal annotation still claims the requested mode succeeded | **Moot by ruling** | Encode `requested` separately from `honored` in `tools.rs` and assert the refusal annotation. Remove text that claims the refused selector was honored. |
| 14 | Nonexistent-file recovery invokes an operation that returns 404 | **Newly conflicting** | Owned WS5C T074 now conflicts with T115. Create through the supported creation path and then invoke impact/index, or return an explicit refusal; never claim impact creates a file. |
| 15 | ACH-05 pre-implements every speculative investigation branch | **Still live** | Ship the measured `/health` guard and identity stamp. Let T155 decide descriptor, registration, or legacy follow-ups from evidence. Reuse the existing liveness result. |
| 16 | `Foo::bar` cannot exercise the path-shaped veto | **Still live** | Keep it as a general guard, but add a real known-extension-tail symbol such as `Config.go` or `Node.rs`. |
| 17 | T119's `cargo check` is vacuous proof of disposition mapping | **Changed shape** | Owned WS5A needs explicit disposition-to-reason and lawful round-trip assertions. Keep `cargo check` only as a compile gate. |

### First high-leverage correction

Define one **admission-oracle matrix** before rewriting individual tasks. Each row must carry:

1. input class: ordinary code false positive, placeholder, genuine synthetic credential, or legitimate exclusion;
2. expected detector result and rule identifier;
3. terminal disposition and reason code;
4. expected tier;
5. lexical-read permission;
6. public context, impact, and search outcome.

Use that matrix as the common source for T062/T063, T066, T102, T120/T121, and SC-015. It closes findings 1, 3, and 17 together and prevents T153 from becoming vacuous.

### Correct causal order

1. Apply T105 and WS5 ownership rulings; define the admission matrix and corrected D5/public-envelope contract.
2. Start independent roots in parallel: detector T066; reason codes T062-T065; shared size work; ACH-01 safety core; ACH-02 caller refusal; WS5D/E; and the measured ACH-05 core plus T155.
3. After reason codes, implement WS5B and the combined WS5C/ACH-01 honesty and recovery seam. Detector recovery does not need to gate reason-code work.
4. After size plus the D5/public-envelope contract, implement ACH-03. It does not depend on all WS5 tasks.
5. After WS5B, honest reasons/detector recovery, ACH-03's oracle/race result, and T143's recorded cause, implement ACH-04 by extending those established seams.
6. Re-run every inherited named test, then the repository-wide gates and receipt ledger.

`tools.rs` should have one serialized owner because ACH-02, WS5B, and ACH-04 share that file. Their only true causal edge is WS5B → ACH-04; ACH-02 merely shares the editing surface. Likewise, ACH-01's fail-closed safety veto can land before WS5C, while its honest reason/recovery response cannot.

---

## Q4 — Amend the current plan or produce a controlled re-draft?

### Answer

Produce a **controlled re-draft from the corrected constraints**, while preserving verified evidence, valid RED/GREEN test interiors, stable requirement identifiers, and useful commands. Do not attempt another layer of local amendments over the current executable plan.

This is not a recommendation to redesign Feature 021. It is a recommendation to remove mutually exclusive instructions before implementation begins.

### Why a targeted amendment is now riskier

The current artifacts can direct an implementer to do both sides of four incompatible choices:

- treat WS5 as external/inherited **and** implement it inside Feature 021;
- forbid edits to `tools.rs` **and** implement the settled admission-aware refusal there;
- honor Tier-2 `around_match` **and** refuse that security fallback;
- make every T102 fixture Tier 1 **and** keep legitimate exclusions intentionally withheld.

There are also duplicate executable paths at the same seams, plus test tasks whose claimed pre-fix state is already green. A patch that changes only phase headers or dependencies would leave stale instructions discoverable farther down the file. The failure mode is not aesthetic inconsistency; it is engineers implementing whichever contradictory sentence they encounter first.

### What “controlled” means

Retain:

- stable ACH, FR, SC, D, and task identifiers where they still describe the same obligation;
- already verified source locations and causal evidence;
- test cases that genuinely fail before their corresponding fix;
- exact commands that prove the behavior they name;
- the settled T105 and WS5 owner rulings verbatim.

Rebuild:

- the dependency graph and phase ordering;
- task ownership and serialized file ownership;
- T102/T120/SC-015 around the shared admission matrix;
- D5 and SC-006 around one honest public envelope;
- ACH-02 around pre-read refusal;
- T153, T119, and T112 around non-vacuous test interiors;
- close-out gates around the now-owned WS5 implementation.

Delete:

- obsolete external-owner gates;
- duplicate task paths for the same behavior;
- claims that refused modes were honored;
- speculative ACH-05 branches not supported by T155 evidence;
- verification labels that run only compilation or source inspection.

### Guard against re-draft risk

A re-draft can introduce task-number churn or accidentally drop good evidence. Use a small traceability ledger with one row per prior task:

| Old task | New task/section | Disposition | Requirement | Runnable proof |
|---|---|---|---|---|
| identifier | identifier | retained / merged / replaced / deleted | exact ID | exact command or “not a gate” |

The re-draft is acceptable only when all old executable tasks have a disposition and every normative requirement maps to at least one non-vacuous proof. This is less work—and less implementation risk—than maintaining a growing overlay of corrections.

---

## Q5 — Adjudicate the Cursor severity and Kimi watcher-race findings

### Q5a — Duplicate WS5 seam severity

Cursor's original **BLOCKER** rating was the better pre-ruling rating; my earlier **MEDIUM** understated the implementation risk. With two independently controlled features issuing overlapping instructions for the same protocol seams, a compliant implementation could still thrash, overwrite, or diverge. That is a plan-validity problem, not merely maintenance debt.

The owner ruling removes the cross-feature conflict, so the finding is now **moot by decision but not yet mechanically resolved**. The re-draft must still merge:

- T111/T115 (`tasks.md:161-188`) with WS5C T072-T074 (`specs/020-repository-knowledge-index/sift/tasks.md:241-245`); and
- T148/T152 with the owned WS5B path.

“One feature owns both” is necessary but insufficient if two executable task paths still tell that owner to change the same behavior differently. Once those tasks are consolidated and one serialized owner is named for the `edit_plan.rs` plus handler seam, no open blocker remains from this finding.

### Q5b — Omitted watcher exits

Kimi is substantively correct: the omission is **blocker-class**. The exits at `src/watcher/mod.rs:415`, `:453`, and `:511` are not legitimate policy-success skips. They are failed-publication fallthroughs currently mislabeled as `ReindexResult::Skipped`.

The branch pairs make the distinction explicit:

| Admission branch | Successful policy `Skipped` | Failed publication mislabeled `Skipped` |
|---|---:|---:|
| Named metadata terminal | `:342-350` | `:358-359` |
| Generated output | `:400-407` | `:415` |
| Stable-read hard skip | `:435-442` | `:453` |
| Content policy | `:496-503` | `:511` |
| Named hash skip | success branch before `:539` | `:539` |
| Indexed file | success branch before `:573` | `:573` |
| Attempt exhaustion | not applicable | `:579` |

The `Skipped` contract at `src/watcher/mod.rs:112-115` means the prior Tier-1 entry was removed and a terminal skip disposition was successfully recorded. A false result from `publish_terminal_disposition_at_generation` does not establish either fact.

### Important correction to Kimi's proposed classification

Do not blindly convert every failed publication to `StaleGeneration`. The publication helper's Boolean currently conflates at least four outcomes:

- project generation changed (`src/live_index/store.rs:2664-2675`);
- publication generation changed (`:2677-2685`);
- an internal mutation panicked (`:2694-2711`);
- scout-plan refresh failed (`:2714-2719`).

Therefore every listed exit is wrongly labeled `Skipped`, but not every one is proven to be a generation race.

### Required amendment

1. Expand FR-009, T137, and the research section to enumerate failed-publication exits `:359`, `:415`, `:453`, `:511`, `:539`, and `:573`, plus exhausted attempts at `:579`.
2. Preserve `Skipped` only for successfully published scope/policy outcomes at `:313`, `:350`, `:407`, `:442`, and `:503`.
3. Replace the conflating Boolean at the narrowest shared seam with a typed publication outcome—or, if changing that return type would be unnecessarily broad, compare both captured counters immediately after failure and return an existing typed error:
   - project generation changed → typed project-stale result;
   - same project generation and publication generation changed → retry, then typed collision on exhaustion;
   - neither counter changed → internal publication failure, never `Skipped` or `StaleGeneration`.
4. Make the result name which generation its expected/observed fields describe; project and publication counters are distinct.
5. Update freshening and reconciliation so a stale or publication-failure result is not counted as a successful repair.

The smallest correct implementation is one shared publication-outcome classification, reused by all six branches. Six local label changes would repeat logic and preserve the Boolean ambiguity.

### Deterministic proof required by T135/T137

Add tests that force failed publication for:

- generated-output policy;
- stable-read hard-skip policy;
- content-policy metadata-only disposition;
- repeated publication-generation collision through attempt exhaustion;
- same-generation internal publication failure.

For each policy branch, retain a success control that proves a successfully published policy outcome still returns `Skipped`. The failure cases must return retry/stale/collision/internal-failure as appropriate and must not be rendered by the impact handlers as a policy skip. Freshening and reconciliation must likewise avoid reporting those cases as successful repairs.

---

## Consolidated recommendation

Do **not** implement from the current Feature 021 revision. First produce the controlled re-draft described in Q4, then begin implementation in Q3's causal order.

The owner decisions should be:

1. Adopt the two-stage context-assignment detector and its bidirectional oracle.
2. Close T104 as a carried-forward whole-file security constraint.
3. Absorb WS5 implementation while consolidating every duplicate seam into one executable path.
4. Keep T105's refusal at the caller-side pre-read boundary for every selector mode.
5. Treat every watcher failed-publication fallthrough as non-`Skipped`, while distinguishing generation staleness from internal publication failure.

Implementation is ready to start only when the re-draft makes these five statements unambiguous and its traceability ledger accounts for all prior tasks. The first safe parallel work is then the independent root set in Q3; it is unnecessary to serialize the entire feature behind T066 or all of WS5.

## Evidence boundary and limitations

This consultation reviewed the follow-up brief; the three locked reviewer reports; Feature 020 and Feature 021 specifications, plans, and task ledgers; and the cited detector, admission, planner, raw-read, watcher, publication, impact, freshening, and reconciliation source paths.

Exact raw reads were used where the live index classified planning/source artifacts as metadata-only, as the brief explicitly permitted. No plan artifact, locked report, source file, or test file was changed.

What was not established:

- The proposed Q1 design has not yet been compiled or run as Rust. A logic-equivalent 16-case prototype passed, but the focused Rust oracle is authoritative.
- No implementation tests or repository-wide gates were run because this task was a read-only design adjudication.
- Source inspection proves the watcher exits are failed publications and that the Boolean conflates causes. It does not establish how frequently each cause occurs in production.
- The recommendations do not certify a future re-draft; its task-disposition ledger and runnable gates still require review.
