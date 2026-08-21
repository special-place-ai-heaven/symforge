# Grove team report — for the Feature 020 / 029 agent

From: GROVE (campaign runner) with LINUS, HOLMES, JEZ, LARRY, JUNIO, FRED
To: the agent holding the 029 spec/plan and the Slice 5 brief
Date: 2026-08-21 (Europe/Ljubljana)
Repo: `special-place-ai-heaven/symforge`

This is one file. It is compiled from specialists' Output blocks, not a second review. Grove did not write the Rust, the review, or the git.

## SHAs

| Ref | SHA | What |
|---|---|---|
| `main` (v11 shipped) | `2f27cb681687b1b2948dc67838a449ae7ea69247` | Track A read this |
| `feature-029-mechanical-removal` | `cd6b18adf26f` | Nine 029 amendments; §5 read this |
| Prior brief (not restaffed) | `8e3edc23ed2d` | Slice 5 review-request; Grok+Composer already covered 029 |

## What this team will not do

- We will not spend compute making leftover Slice 0 oracles green so a draft PR looks busy.
- We will not run a third 029 spec/plan pass under the same framing as Grok and Composer.
- We will not patch, un-ignore, merge, or reopen **PR #603**.
- We will not treat default-suite green as Slice 0 success.

## PR #603 (leftover, retired)

Draft `fix(lifecycle): discharge Feature 020 Slice 0 daemon/watcher controls` opened hours after v11.0.0 (`#601` + `#602`). It was 2 commits ahead of `main` (`f568838`, `a12ce7c`) to un-ignore Slice 0 controls. CI went red (rust, then `live_index_integration::test_persist_round_trip`). Rob stopped leftover-test work. JUNIO Lane A: closed #603 without merge; deleted `cursor/feature-020-slice0-green-fb1a` (last SHA `a12ce7c56f96`, recovery: recreate ref). Main untouched.

## Track A — eleven still-RED Slice 0 controls

Independent read-only passes on `main` @ `2f27cb68`. LINUS file: `/workspace/REVIEW-FINDINGS-linus-feature-020-slice-0-reconcile-2026-08-21.md`. HOLMES tables in chat. Reconciled. No `cargo test` battery. No patches.

Keep `#[ignore]` + fail-closed Slice 0 oracle roster. Do not treat green default `cargo test` as Slice 0 success.

### Stale tests (move the body; do not SWITCH production to win the encoding)

| Control | Agreed | Why |
|---|---|---|
| `capacity_refused_open_creates_no_slot_and_no_watcher` | **control-stale** | V11 is `Ok` + typed `SourceRefusal` / non-ready slot, not `Err` + 0 slots. FR-004 strict acquisition is the lease. Residual: `activate` still starts a watcher (`daemon.rs:3398–3403`) — unmeasured by this body. |
| `whole_project_publication_preserves_latest_siblings` | **control-stale** | Frozen oracle is pause A / publish B / rebase / tokens / one store. Body is V10 `LiveIndex::reload` vs 1500 files / 150ms. Switching reload to win the race is not the oracle. |
| `configured_capacity_bounds_the_process_not_each_load` | **control-stale** | FR-004 is per-candidate catalog. SC-025 is `ProcessCapacityPool`. `SYMFORGE_MAX_INDEX_FILES` is per discovery pass. Making it process-wide fights FR-004 and still misses SC-025. |

### Code-wrong (keep the control; do not un-ignore as fake green)

| Control | Agreed | Live miss (file:line from the reviewers) |
|---|---|---|
| `empty_placeholder_publication_refuses_watcher_mutation` | **code-wrong** | `spec.md:14–20` Loading holds none. `add_file` `store.rs:2820–2831` has no EmptyBootstrap gate. Default-suite `store.rs:6402–6412` is a paper-over. |
| `failed_reload_retains_the_recovery_observer` | **code-wrong** | abort watcher then `?`; no replacement on Err |
| `observer_replacement_gap_is_latched_as_non_current` | **code-wrong** | `recompute_freshness_locked` drops historical gap → Current |
| `old_observer_delivery_after_promotion_is_not_current` | **code-wrong** | same rederive; no token fence |
| `snapshot_seed_is_not_queryable_before_verification` | **code-wrong** | persist hydrates files immediately; `get_file` has no Pending gate; `is_ready()` is status-only |
| `same_path_root_replacement_is_not_silently_adopted` | **code-wrong** | path-keyed map; publish Current |
| `concurrent_first_open_performs_exactly_one_cold_load` | **code-wrong** | load outside lock then `or_insert`; `admit_project` does not skip bootstrap |
| `watcher_mutation_during_candidate_build_is_not_discarded` | **code-wrong** | `store.rs:2403–2436` still `swap_and_publish`; `IsolatedCandidate` not on this path. Ignore-text “window unreachable” is false on this SHA. Do not un-ignore until a deterministic pause exists. Official TEST-CANDIDATE does not retire this seam. |

Already RESOLVED, not classified: `internals::watcher::tests::generation_before_root_split_cannot_authorize_root_a_reindex_into_root_b`.

### Track A residuals

- No `cargo test` on the ignored roster in these passes.
- `src/server_api` not fetched; MCP `is_ready` vs `get_file` unaudited.
- LARRY was not given a punch list. Do not reopen 603 to discharge these.

## §5 — two questions, uncontaminated, on `cd6b18ad`

LINUS file: `/workspace/REVIEW-FINDINGS-linus-feature-029-two-questions-2026-08-21.md`. HOLMES independent (did not read findings). Did not re-review the rest of 029. No prior Grok/Composer endorsement used.

### 1. Bracket-as-P1

**Team: the control is discipline; making the bracket the slice is over-engineering.**

- LINUS: **split.** Keep C-1 / US1 Independent Test (must name the field that moved) as a **gate**. Demote “primary deliverable is not the removal but the neutrality bracket” (`plan.md:20–24`) and “standalone value / any future removal” (`spec.md:32–42`). Put roster disposition back as the slice. Do not ship a bracketing platform so the slice has a product when it deletes nothing.
- HOLMES: **over-engineering.** SWITCH the priority: bracket is a gate, not the slice. Frozen job is delete-only-proven-unreachable. C-2 already owns public-surface neutrality. Markdown baseline + void→armed diagram is not a product. “Any future removal” is YAGNI. **Keep C-1 and US1 scenario 3 as the canary — do not NIT the canary away.** Demote US1 to admission-for-removal; if anything is evidenced-unreachable, US2 is P1.

They agree on the change: demote US1 from P1 product to admission instrument. Keep the canary.

### 2. C-7 empty removal is a pass

**Team: discipline. Stay.**

- LINUS: C-7 requires C-1 armed ∧ C-6 every prediction discharged with command+output ∧ evidence says nothing removed (`neutrality-bracket-v1.md:149–159`). Honest disposition, not a line count (`spec.md:231–235`). Would not change C-7. `plan.md:45` is looser wording, not the defect.
- HOLMES: governing constraint forbids unevidenced deletion; failing the slice for an empty recon would force the invented removal C-7 exists to block. Residual: pass still needs C-6 verbatim transcripts. Empty ≠ skip disposition. **Do not combine with bracket-as-P1 to close Slice 5 on markdown alone.**

They agree: empty is a pass only as full roster disposition, not as “zero lines.”

## What we recommend you do next

1. Treat Track A as a **disposition table**, not an implementation queue. Retarget the three stale bodies. Keep the eight code-wrong controls ignored-fail-closed until a real seam owner exists. Do not invent a 603-class PR.
2. On 029 @ `cd6b18ad`: demote bracket-as-P1 per both reviewers; keep C-1 canary; leave C-7; do not close the slice on the diagram.
3. If you want code, name **one** seam and Grove will @ LARRY for that seam only. Default: no.

## Board at report time

| Job | Owner | Status |
|---|---|---|
| 603 leftover CI / discharge | JEZ / LARRY / FRED | cancelled; PR closed, branch deleted |
| Track A Slice 0 RED controls | LINUS + HOLMES | done-with-evidence (agreed table) |
| §5 bracket-as-P1 + C-7 | LINUS + HOLMES | done-with-evidence (agreed substance) |
| Implementation of Track A seams | LARRY | not dispatched |
| Push / merge | JUNIO | Lane B unless Rob says |

Blocked on human: none for this report.
