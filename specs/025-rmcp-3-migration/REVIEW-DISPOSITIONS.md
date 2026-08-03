# Review dispositions — 025 rmcp 3.x migration spec

Running ledger, one section per review round. Vocabulary: **FOLDED** (document
edited), **REJECTED** (with evidence), **DUPLICATE** (already covered or fixed
by an earlier round), **DEFERRED** (real, moved to implementation),
**NO-EDIT** (acknowledged, deliberately not changed).

## Round 1 — Cursor agent (Grok), `REVIEW-REPORT-2026-08-03.md`

Every finding verified against the tree/docs before folding; verification
commands run 2026-08-03 on `feat/knowledge-llm-sift` + docs.rs/3.1.0.

| id | severity | disposition | detail |
|---|---|---|---|
| H1 | Blocker | **FOLDED** | Real contradiction created by the appended Addendum. FR-305 rewritten as a three-layer contract (struct construction pins `Some(Complete)` / direct-serde batteries tolerate the key without asserting / transport wire is version-aware per FR-A6); SC-302 rewritten to claim only what those batteries actually pin (M3 folded into the same edit). |
| H2 | High | **FOLDED** | FR-309 now states its authority over adopted owner-test 15; research.md reconciliation narrows test 15 to the modern/metadata-required path. Legacy header-less requests keep HTTP-200 JSON-RPC error semantics by design. |
| H3 | High | **FOLDED (incomplete — completed in round 2/F3)** | research.md Phase 3 narrative rewritten under owner decision 2 — no lazy-bind, no `RequestMetaObject` fallback; fallback chain + FR-319 evidence disclosure. Marked SUPERSEDED with pointer to spec. |
| M1 | Medium | **FOLDED** | Verified: `mod.rs:1570-1576` doc comment claims "shared by both transports" while `serve_directly` skips handshake enforcement (hook never fires over `/mcp`). Added to FR-316's doc list with required scoping. |
| M2 | Medium | **FOLDED** | The gem of this round: FR-311's `Public` reasoning covered stale-call rejection (INV-2) but not schema DISCLOSURE across mixed-surface instances behind one origin. Added binding deployment assumption (c): one surface configuration per HTTP origin, no shared public cache for mixed surfaces. |
| M3 | Medium | **FOLDED** (into H1) | SC-302 no longer claims the batteries pin `resultType`. |
| M4 | Medium | **FOLDED** | Verified: 33 `#[tool(` in tools.rs + 7 in edit_tools.rs = 40, and `SYMFORGE_TOOL_NAMES` lists 40. Both "35" (research/spec) and "39" (spec US3) were wrong. Normalized to 40 with an explicit counting rule at first mention. **DEFERRED sub-item:** repo `CLAUDE.md` says "39-tool surface" — stale, but it is a TRACKED file in a working tree on another campaign's branch; fix it in PR-A, not from this checkout. |
| L1 | Low | **FOLDED** | Verified `with_server_info` at `:1474` (`get_info` at `:1462`). Anchors updated; the remaining `:1461`/`:1467` version-advertisement cites were spot-checked OK by the reviewer and left as-is. |
| L2 | Low | **NO-EDIT** | The verification report is the owner-agent's artifact, preserved verbatim by policy (023 precedent). The spec header already pins `/3.1.0/` URLs; the report's `/latest/` links are noted here rather than edited. |

**Round-1 net effect:** spec.md and research.md are now internally consistent
under Addendum A; the two documents no longer disagree on wire shape, metadata
strictness, Phase-3 posture, or tool counts.

## Round 2 — Codex, pasted 2026-08-03

Verdict was REJECT for plan sign-off on three Highs; all three verified
against the tree and folded. This round also caught a **round-1 ledger
error**: H3 was marked FOLDED while the same rejected lazy-bind prescription
survived in research.md's breaking-changes TABLE row (only the §Migration
order narrative had been patched). Corrected below.

| id | severity | disposition | detail |
|---|---|---|---|
| F1 | High | **FOLDED** | Verified in code: `with_project_evidence_scope` (mod.rs:1540-1544) only SCOPES; attachment lives solely in `ResultStatus::into_call_tool_result` (result_status.rs:121-142) — plain-`String` tools (`health`) and `resources/read` bypass it. FR-319 rewritten to mandate CENTRAL attachment at the `call_tool`/`read_resource` seam with an explicit unbound marker and a stated `_meta`-parity exception; SC-316 widened to statused tool + `String` tool + resource read + foreign + unbound cases. |
| F2 | High | **FOLDED** | US1's independent test DID initialize first, contradicting its own promise. Rewritten: authenticated version-headered `server/discover` as the FIRST request, no handshake ever; SC-311 now pins discover-first plus tools/list + tools/call on the same connection. |
| F3 | High | **FOLDED + ledger correction** | The lifecycle-discover table row still prescribed AtomicBool lazy-bind + `RequestMetaObject` fallback. Row actions rewritten under owner decision 2; design-decision 3 marked RESOLVED. Round-1 H3 disposition corrected from "FOLDED" to "FOLDED (incomplete — completed in round 2)". |
| F4 | Medium | **FOLDED** | Correct: `cargo tree -d` lists any duplicated crate's subtrees — exit status cannot distinguish one rmcp major from two. FR-A4 rewritten to an executable `cargo metadata` assertion (distinct majors of rmcp + rmcp-macros == {3}). |
| F5 | Medium | **FOLDED** | INV-4 rewritten: `ttl_ms = 0` + `Private` are HINTS; client stale-on-error can still serve expired entries; the server-side guarantee is the 023 admission gate refusing content regardless of client caches; exact-consistency clients disable stale-on-error. Scope stays frozen. |
| F6 | Medium | **FOLDED** | Adopted owner test 17 had no criterion. Added SC-316b: legacy stdio initialize → initialized → roots/list binding verified end-to-end. |
| F7 | Medium | **FOLDED** | (a) Assumptions bullet re-anchored to FR-305's three-layer contract instead of "reopens SC-302"; (b) research PR-B harmonized to Phases 2-4; (c) FR-A3 corrected — `ListToolsResult` is NOT non-exhaustive (which is exactly why the mod.rs:1557 literal compiles); constructor-only rule now scoped to genuinely non-exhaustive models. |
| Low | Low | **FOLDED** | FR-A2 softened to informative (the compiler enforces wildcard arms; the grep records the baseline); research "short TTL" → "zero TTL". Remaining anchors: none named beyond round-1 fixes. |

Codex's own lead judgment rejected two of its sub-reviewers' overstatements
(FR-A3-as-High; DiscoverResult lacking supported_versions) — both agreed
with, no action needed.

**Round-2 net effect:** the three documents agree with the code's actual
evidence mechanism, the discover lifecycle is tested as promised, the
mixed-major gate is executable, and cache hints no longer masquerade as
enforcement.

## Round 3 — Cloud Codex (own clone + rmcp 3.1.0 SOURCE access), pasted 2026-08-03

Verdict: AMEND then GO. The unique lens paid off: this reviewer read the rmcp
3.1.0 source itself and overturned one premise and resolved every deferred
API question. Its central claim was independently re-verified against
docs.rs source pages before folding.

| id | severity | disposition | detail |
|---|---|---|---|
| CC1 (§1) | High (premise) | **FOLDED, re-verified** | The default `supported_protocol_versions()` returns `KNOWN_VERSIONS` INCLUDING `V_2026_07_28` (verified verbatim: handler/server.rs:340, model.rs:181-187). Our "silently stays at 2025-11-25" premise conflated `LATEST` with the supported set. FR-307 rewritten: override retained as a deliberate ALLOW-LIST FREEZE (the `"3.1"` semver range could otherwise auto-advertise future untested revisions); intro narrative flipped; research.md amended. |
| CC2 (§2) | High | **DUPLICATE (round 2/F2) + refinement folded** | Discover-first already fixed in round 2. Adopted refinements: SC-311's flow now runs to prompts/resources surfaces with explicit no-initialize / no-initialized / on_initialized-not-run assertions (folded into SC-311 in round 2's rewrite; flow-widening kept). |
| CC3a (§3.1) | Medium | **FOLDED** | Q4 CLOSED from source: `ToolRouter::call` returns `Result<CallToolResponse, ErrorData>` — no conversion in `call_tool`; `read_resource` maps `ReadResourceResponse::Complete` at the boundary (INV-1 strengthened). Deferred-verification task removed from FR-304 and Assumptions. |
| CC3b (§3.2) | Medium | **FOLDED** | `StreamableHttpService::new` still REQUIRES the session-manager argument — `LocalSessionManager` retained; only the config-method rename lands. FR-306's "drop if permitted" resolved: not permitted. |
| CC4 (§4) | Medium | **FOLDED** | `with_stateless_protocol_metadata_required` defaults `false` (source) — FR-309 now pins it explicitly against future default flips; SC-314 gains four modern negative cases (missing `_meta`; header/`_meta` version disagreement; `Mcp-Method`/`Mcp-Name` body mismatch; well-formed modern behind bearer accepted). |
| CC5 (§5) | Medium | **DUPLICATE (round 1/H1)** | The reviewer saw the pre-round-1 spec; base FR-305/SC-302 already carry the three-layer contract it requests. Verified current text matches its recommended structure. |
| CC6 (§6) | Medium | **FOLDED (refines round 2/F7c)** | New source finding: `ReadResourceResult` IS non-exhaustive (constructor-only mandatory); paginated lists exhaustive with builder style as a POLICY (builders seed `Some(Complete)` free); PR-A plain constructors / PR-B cache builders split stated. |
| CC7 (§7) | Medium | **FOLDED** | FR-A5 RESOLVED: both transport features exist on 3.1.0, keep them; added Cargo.lock closure assertion (resolve exactly 3.1.0, record `cargo tree -p rmcp`). Mixed-major exit-code caution: DUPLICATE of round 2/F4 (already an executable `cargo metadata` assertion). |

Endorsements (no action): two-PR fault boundary, 114b793 base gate as first
executable task, compact real-dispatch tripwire, MRTR/Tasks containment,
roots posture, cache policy.

**Round-3 net effect:** zero deferred API questions remain — every signature
and default the plan will encode is source-verified; the protocol-exposure
story is now correct in DIRECTION (freeze against auto-advertisement, not
activation).

## Final state after three rounds

Reviewer verdicts: Cursor "almost ready" → folded; Codex "REJECT" → all
three Highs folded; cloud Codex "AMEND then GO" → all amendments folded.
Convergent findings (discover-first test, resultType layering, FR-A3
exhaustiveness) were each found by 2+ independent reviewers — high
confidence. The spec is GO for `/speckit-plan`.
