# Review findings — Feature 020 V11 Slice 2

**Reviewer:** Cursor Grok 4.6  
**Date:** 2026-08-13  
**Branch:** `feature-020-slice-2-registry-capacity`  
**Lens:** the five items no prior review examined (A20 wording, regenerated digests, change size, Windows reparse, concurrency). The seven closed blockers are not re-opened.

---

### BLOCKER — A20’s eight openings are consistent with each other and false against the FRs Slice 4 implements

- **Where**: `specs/020-repository-knowledge-index/spec.md:14-17` vs `:690-696` (FR-017), `:1015-1018` (NFR-003), `:326-328`; `GOAL.md:63`, `:70-76`, `:95-96`, `:160-166`; `contracts/knowledge-authority-hygiene.md:31-41`; `contracts/search-knowledge.md:65-76`; `contracts/repository-mental-model.md:15-16` vs `:22-24`; `contracts/lifecycle-acceptance-oracles-v11.md:238-239`; `plan.md:65`
- **Claim**: The A20 sentence (“`Refreshing` without a permit is queryable”) was pasted into eight openings, while FR-017, NFR-003, GOAL’s operative bullets, and the rest of the same A19-hashed contract ranges still require a sealed `Current` lease, refuse every non-current source, and treat a retained generation as internal recovery material that cannot supply hits, findings, or a no-match.
- **Why it matters**: Slice 4 implements the query lease against this corpus. The same hashed range in `search-knowledge.md:57-77` both grants a Refreshing lease and forbids that lease from producing hits or an empty result. `ORACLE-QUERY-ATOMIC-LEASE` asserts both “refuses anything not proved Current and complete” and “a refreshing source still leases the complete generation it retains.” Two reviews found the missing sites; neither asked whether the replacement was the rule. It is not. It is a veneer.
- **Recommended fix**: Pick one rule and edit every load-bearing sentence in the A19 `spec.md:130-1179` range (at least FR-017, NFR-003, user-story no-match, SC-011) plus GOAL’s bullets and plan.md’s failure-isolation row, not only the openings. Then rewrite ORACLE assertion 3 so it names completeness-with-no-outstanding-permit rather than `Current`. Line-count-neutral if the freeze requires it.

### BLOCKER — A20 + R20A restore serving after a mutation permit retires, which is last-valid under a new name

- **Where**: `src/index_lifecycle/authority.rs:808-815`; `tests/project_index_authority_v11.rs:932-943`; `specs/020-repository-knowledge-index/quickstart.md:354-356`; `spec.md:881-884` (FR-043)
- **Claim**: `queryable_generation` returns the retained generation as soon as `active_permits` is drained; R20A asserts that retiring the mutation permit restores reads; quickstart step 1 tells Slice 4 to prove the same. After `replace_beneath`, disk is no longer the retained generation, and FR-043 — which A20 lists as a requirement it amends — still says no terminal path may restore the prior publication.
- **Why it matters**: The first review closed “serve while the permit is rewriting.” This is the sibling one step later, and the amendment encodes it as the positive. Reload serving (disk untouched, successor building) is the availability A20 exists for and is what `test_same_project_reads_prior_generation_during_reload` pins. Mutation-after-retire serving is the V10 last-valid path: a complete historical generation whose correspondence with disk nobody observed. `queryable_generation` reports safety from an empty permit set.
- **Recommended fix**: A freeze that issued a permit stays unqueryable until `install` of a successor `Current`. Distinguish reload-entered Refreshing (`freeze` with no `record_permit`) from mutation-entered Refreshing (a permit was recorded against this freeze, even if later retired). Retarget R20A’s “permit retires” arm as the paired negative, not the positive. Amend FR-043 in the same edit or stop listing it as an A20 requirement.

### MAJOR — A20’s digest record is a dodge, not honest attribution

- **Where**: `execution/refreeze_v11.py:222-235`; `specs/020-repository-knowledge-index/REFREEZE-MANIFEST-v11.md:828-872` vs `:779-818`
- **Claim**: A20’s `replacements` hash-pin only `spec.md:14-17` and `data-model.md:1539-1549`. The four contracts it “declares” in `contract_clause_ids` are A19 TARGET-02..05; the verifier checks that those headings exist and that `EXPECTED_AMENDMENT_MAPPINGS` matches, not that A20 owns the bytes it changed. A20’s `requirement_ids` are FR-037, FR-043, FR-051 (curation / ignore-hygiene / team artifact), not FR-017. Independently hashed: all six clause digests and the five inventory files I checked match; `verify-internal --target-ref HEAD` passed. The arithmetic is correct. The attribution is not.
- **Why it matters**: A19’s frozen hashes now attest to A20’s openings. A20 can list contracts it did not pin and FRs it did not edit, and the gate stays green. That is how FR-017 survived inside A19 TARGET-01 (`spec.md:130-1179`) while the four contract openings were line-count-neutrally patched in the same kind of range. `verify-internal` cannot catch a wrong amendment; it can only catch a miscounted one.
- **Recommended fix**: Either give A20 replacement ranges for every sentence it actually changes (and drop the overlap rule, or split A19’s ranges), or stop declaring those contracts on A20 and amend A19 in place under A19’s id with a new signed record. Put FR-017 (and NFR-003) in `requirement_ids` if queryability is the claim. Do not treat a green `verify-internal` as a review of the text.

### MAJOR — `concurrent_opens_join_one_admission` never overlaps two calls

- **Where**: `tests/project_registry_lifecycle_v11.rs:260-282`; `src/index_lifecycle/registry.rs:256`; `src/index_lifecycle/capacity.rs:209`; `tests/process_capacity_pool_v11.rs`; `tests/embed_lifecycle_v11.rs`
- **Claim**: The one test named concurrent admits sequentially. Across the five Slice-2 oracle files there is no `std::thread::spawn`. The 33 Arc/Mutex/atomic oracles therefore do not observe an interleaving; a build that deleted the `Mutex` and used a bare `HashMap` would still pass them.
- **Why it matters**: The Mutex makes overlapping `admit`/`reserve` unrepresentable *while it remains*. The oracles do not pin that it remains. The 23 `SourceRuntime` tests are `&mut self` and already recorded as not surviving T060; this is the same reporting shape on the 33 that are supposed to carry forward.
- **Recommended fix**: One test that overlaps two `admit`s of one key on two threads, and one that overlaps `reserve`/`drop` against a tight limit. That is the proof the Mutex is load-bearing. Loom can wait.

### MAJOR — this PR is above the reviewable ceiling; the A20 miss is the measurement

- **Where**: `git diff origin/main...HEAD` — 46 files, +7128/−496 (src+tests +4751/−430), 58 commits, one PR
- **Claim**: Two adversarial reviews closed seven runtime blockers and did not examine whether the A20 replacement was the rule, whether the digests attributed the right FRs, whether Windows reparse was the claimed predicate, or whether any oracle overlapped two threads. That is what an unreviewable bundle produces, not a style complaint.
- **Why it matters**: Slice 4 will implement the query lease against the corpus half. Landing “the reviews were green” over that half ships the veneer in BLOCKER 1.
- **Recommended fix**: Do not four-way split the runtime now — the seven blockers were closed as one unit and re-slicing them is theater. Do isolate any A20 text+digest correction as its own stacked change, and do not start T060/T063 until that change exists. A four-way stack would have been the right call *before* the first review, not after.

### MINOR — Windows link refusal is still a comment about `is_symlink`, and cap-std will follow in-sandbox links if that check misses

- **Where**: `src/index_lifecycle/physical_root.rs:11-15`, `:198-200`, `:226-228`; `tests/physical_root_lease_v11.rs:239-246`
- **Claim**: The never-follow policy is the userspace `metadata.is_symlink()` walk, not cap-std: cap-std follows in-sandbox links and only refuses those that escape the capability. Rust 1.96 treats name-surrogate reparse tags (symlink *and* `IO_REPARSE_TAG_MOUNT_POINT` junctions) as `is_symlink`, so the common unprivileged junction *would* hit this branch *if* the tag is present on cap-std’s `symlink_metadata`. OneDrive/cloud/WOF tags would not. No test creates any Windows reparse point. The comment still says “reports a reparse point as a symlink.”
- **Why it matters**: The users are on Windows. Unix `symlink(2)` does not prove `CreateFileAtW`. If cap-std’s Windows metadata omits the tag, `is_symlink` is false and an in-root junction is followed; a cross-root junction then depends on cap-std’s sandbox, which this suite also does not run on Windows.
- **Recommended fix**: Soften the comment to name-surrogate tags, not “a reparse point.” Add `#[cfg(windows)]` coverage with a directory junction (`mklink /J` or `std::os::windows::fs::symlink_dir`). Optionally refuse `FILE_ATTRIBUTE_REPARSE_POINT` regardless of tag so the check does not depend on tag plumbing.

### NIT — A20-TARGET-01 never contains the token `F020-V11-A20`

- **Where**: `spec.md:14` (`(A20)`); association check in `execution/refreeze_v11.py:2974-2978`
- **Claim**: The semantic-association gate is satisfied entirely by `data-model.md:1539-1549`. The spec preamble A20 replaced would pass without naming the amendment.
- **Why it matters**: A later edit can strip the id from the data-model paragraph and keep a preamble that only says “(A20).”
- **Recommended fix**: Spell `F020-V11-A20` in TARGET-01, same as TARGET-02.

---

## Author confidence verdicts

1. **`capacity.rs` never blocks.** Sound as an API: `reserve` fails closed with `Exhausted`; it does not wait, so it cannot deadlock the rayon pool. Process capacity is therefore enforced for callers of this ledger and unenforced in production because the slice is dark. That is the known limit, not a hole in the deadlock argument.
2. **Conservation vs `Drop`.** No remaining double-refund or wrong-ledger path found in `reserve` / `redeem` / `Drop` / `release_owner`. Lock is dropped before a grant is returned, so grant-`Drop` does not re-enter the mutex. `unknown_refunds` is still unreachable through the public API; I did not fool it, and I did not add a backdoor to try. Not loom-proved.
3. **`#[path]` placement.** Legitimate. File location and census granularity are different contracts; `live_index/mod.rs:11-25` states that without claiming the types are unreachable. Same census reading Slice 1 already survived. Not evasion.
4. **A20.** Wrong as written for mutation-after-retire, and incomplete against FR-017 / NFR-003 / GOAL. Reload-entered Refreshing without a permit is the one state where a complete retention must be served. Mutation-entered Refreshing, including after retire and before `install`, is a state where a complete retention must not. See both BLOCKERs.
5. **Temp-before-replace.** The new oracle observes disk: `the_target_still_holds_its_preimage_while_the_replacement_is_staged` (`tests/physical_root_lease_v11.rs:119-145`). The leftover `replacement_creates_its_temporary_before_replacing` still checks receipt labels; it is not the load-bearing one.
6. **Dark slice / oracles.** The 23 `SourceRuntime` oracles prove a `&mut self` toy and do not survive T060, as the evidence already says. The 33 Mutex/atomic oracles prove properties of the types that will ship *if* those types keep their locks; they do not prove interleavings (MAJOR above). T039 through registry/adapters is coverage of planning, not of production callers, which is the dark design.
7. **Planned RED stub.** Honest. The body panics; `#[ignore]` keeps it out of the default suite; `validate-lifecycle-oracle-traceability.cjs` `expect_execution` refuses `... ignored` (`:3385-3392`). Same shape as Slice 0. Removing `#[ignore]` without writing a body fails loudly. Replacing the panic with a pass would be a different defect; it is not one today.

---

## Negatives

- **Seven closed blockers** (evict-on-refuse, grant-gate clone, live-permit queryability, child `release_owner`, grant-`Drop` leak, freeze-attestation on `Stopping`, two-of-eight A20 sites): not re-opened. `install`/`cancel`/`stop` reinsert non-matching occupancy; `SourceMutationPermit::grant` checks `binding().is_live()`; `queryable_generation` is drained-permits-only for Refreshing; `CapacityGrant::Drop` refunds unless redeemed; `freeze` returns `None` rather than installing over `Stopping`.
- **Independent digest arithmetic:** A20-TARGET-01/02, A19-TARGET-02..05, GOAL.md, spec.md, quickstart.md, plan.md, CONTEXT.md, and the attestation’s manifest pin all matched a direct SHA-256 of the committed blobs. `python3 execution/refreeze_v11.py verify-internal --target-ref HEAD` passed in this checkout. The gate is doing the arithmetic it claims. It is not doing a review of the sentences.
- **A20 openings (the eight sites):** spec preamble, data-model 1539-1549, four contract openings, quickstart 32-35, GOAL appendix 271-283. Those sentences agree with each other, including the permit condition. That is necessary and not sufficient.
- **Reload-entered Refreshing:** `SourceRuntime::refreshing` / `freeze` without `record_permit` leaves `active_permits` drained, so `queryable_generation` serves the retention. That half matches production `test_same_project_reads_prior_generation_during_reload` and is the availability A20 was decided for.
- **R20B remnant half:** `Blocked`/`Stopping` return `None` from `queryable_generation` even when they retain. The paired Refreshing-with-same-retention case in that test is the right negative. Unchanged by the post-retire hole, which is Refreshing-shaped.
- **`stop()` ordering:** revoke, then tombstone, under the registry mutex (`registry.rs:444-450`). No window in which the map says retired while `binding()` still returns `Ok` for a holder that asks again.
- **Disk-order replacement:** staging observes temp bytes and preimage on disk before commit. Abandoned stage deletes its temp. `commit` after `revoke` refuses. Sound.
- **`#[path]` / census:** see verdict 3. Sound reading of the frozen checker.
- **Capacity non-blocking / Drop conservation (single-threaded):** see verdicts 1–2. Sound as read; not concurrently evidenced.
- **Planned stub / ignored-only guard:** see verdict 7. Sound.
- **Nothing at BLOCKER in the runtime modules themselves** beyond A20’s queryability predicate, which is a spec-shaped defect sitting in `queryable_generation`. The seven ordering/lifetime blockers look closed in the committed tree.
- **No NIT-level style findings** beyond the `(A20)` token.
