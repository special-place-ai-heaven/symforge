# Review findings — Feature 020 V11 Slice 2 (lens: ordering and lifetime)

Reviewer: claude-opus-5. Scope: `git diff main...HEAD` on
`feature-020-slice-2-registry-capacity`.

## Addendum — uncommitted working-tree edits, not authored by me

While this review was being written, `src/index_lifecycle/registry.rs`,
`authority.rs` and `mutation.rs` acquired uncommitted changes in this worktree
(`git status` shows three ` M` entries). I did not make them; this review is
read-only, and what follows is a review of `git diff main...HEAD`, the committed
state. Two findings are already addressed by those edits:

- **The eviction BLOCKER is closed.** `install`, `cancel` and `stop` now reinsert
  a non-matching occupancy before refusing, at all three sites.
- **The binding-clone BLOCKER is closed at the grant gate.**
  `BindingAuthority` now carries an `Arc<AtomicBool>` whose liveness every clone
  shares, `LiveProjectSlot::revoke` retires it, and `SourceMutationPermit::grant`
  refuses with `AuthorityRefusal::BindingRevoked`. **Residual**: revocation does
  not reach a permit that was *already* granted — `start_side_effect`
  (`mutation.rs:224-249`) and `replace_beneath` (`:183-193`) check
  `self.lease.is_live()` but never `self.authority.binding().is_live()`, so a
  permit obtained before the stop keeps writing after it. Add the binding check to
  `start_side_effect`, with the oracle: grant, stop the slot, `start_side_effect`
  must refuse.

No other finding below is affected by those edits.

## BLOCKER

### BLOCKER — `install`, `cancel` and `stop` delete the occupancy before matching it, evicting a live slot with no revocation and no tombstone

- **Where**: `src/index_lifecycle/registry.rs:308`, `:333`, `:348`
- **Claim**: `let Some(Occupancy::Pending(pending)) = state.keys.remove(key) else { return Err(NotAdmitted) }`
  removes the entry unconditionally; when the key held `Occupancy::Live`, the
  `Arc<LiveProjectSlot>` is dropped from the map on the refusal path, so after
  `install()` or `cancel()` on a live key the registry has no entry for that key,
  `is_tombstoned(slot)` is `false`, `revoked` is still `false`, and the
  outstanding handle keeps returning `Ok(&binding)` from `binding()`. The comment
  at `:309-310` says "Put back whatever was there"; nothing is put back.
- **Why it matters**: this reconstructs the exact defect the module says it
  exists to fix (`registry.rs:3-13`) — a handle that outlives map membership with
  nothing revoking it — and adds a new one: a subsequent `admit()` on that key
  mints a second identity, so two live handles serve one key with no relationship
  between them and no tombstone recording the first. `stop()` on a *pending* key
  is the same shape: the admission is destroyed, `install()` afterwards returns
  `NotAdmitted` forever, and the identity every joiner received is neither live
  nor tombstoned.
- **Recommended fix**: match on `state.keys.get(key)` (or `remove` and reinsert
  the non-matching occupancy) before removing. Add oracles for
  `install`-over-live, `cancel`-over-live and `stop`-over-pending; the current
  suite exercises none of the three.
- **Verified**: reproduced standalone with rustc 1.96.0 on a transcription of the
  exact `let`-else/`remove` shape — `contains_key` is `true` before,
  `Err("NotAdmitted")` is returned, `contains_key` is `false` after.

### BLOCKER — `stop()` revokes one handle's flag; a `BindingAuthority` taken before the stop keeps the whole authority chain

- **Where**: `src/index_lifecycle/registry.rs:182-188`, `:346-358`;
  `src/index_lifecycle/authority.rs:86`
- **Claim**: `binding()` checks liveness at *acquisition* and returns
  `&BindingAuthority`, which is `Clone`; `let b = slot.binding()?.clone();
  registry.stop(&key)?;` leaves `b` fully usable, and `stop()` revokes neither the
  `PhysicalRootLease` nor any `SourceRuntime` (the registry imports neither).
  `SourceMutationPermit::grant` (`src/index_lifecycle/mutation.rs:154-176`) accepts
  that clone: it checks only `authority.binding().physical_root() == lease.identity()`
  and `lease.is_live()`, both still true after the stop.
- **Why it matters**: the slice's central claim is "a stopped slot refuses every
  authority-conferring read... a holder that never thinks to ask whether it is
  stale does not get silently served" (`registry.rs:136-143`). It refuses reads
  *through the slot*; it cannot retract authority already handed out, which is the
  `Arc` problem restated one level down. The tombstone therefore does not reach
  the chain that actually authorizes disk writes.
- **Recommended fix**: stop handing out `BindingAuthority` by clonable reference.
  Either return a non-`Clone` guard borrowing the slot that re-checks `is_live()`
  on each authority-conferring use, or make `stop()` revoke the binding itself (a
  revoked flag inside `BindingAuthority`, checked by `SourceMutationPermit::grant`)
  so the retraction reaches the permit gate. Add an oracle: bind, clone, stop,
  then `SourceMutationPermit::grant` must refuse.

### BLOCKER — A20 leaves a source queryable while its own mutation permit is rewriting its disk

- **Where**: `src/index_lifecycle/authority.rs:749-757`, contradicting `:190-192`
  and `:948-949`; oracle at `tests/project_index_authority_v11.rs:857-881`
- **Claim**: `request_mutation_grant` freezes to `Refreshing` for the stated
  purpose of making the source non-queryable *before* a mutation is authorized
  ("no holder of a grant can perform a source side effect while the source is
  still queryable", `:948-949`); A20 makes `Refreshing` queryable, so that
  publication no longer has the property both doc comments name, and a reader is
  served the retained generation while `SourceMutationPermit::replace_beneath`
  replaces the files it describes.
- **Why it matters**: this answers packet question 4 with "yes" — a `Refreshing`
  source with a non-empty `active_permits` holds a retention that is complete and
  must not be served, because a permit is actively invalidating it. A20's own
  rationale ("it was `Current` immediately before the refresh") is sound for a
  *reload* refresh and silently extends to a *mutation* refresh, a distinction the
  evidence document never draws. R20A reaches `Refreshing` through
  `request_mutation_grant` specifically, so the oracle establishing the
  availability half is asserting the mutation case is queryable.
- **Recommended fix**: split the condition rather than the phase —
  `queryable_generation()` returns `None` for `Refreshing` when
  `active_permits.is_drained()` is false. That keeps the reload availability A20
  exists for and restores the freeze-before-grant ordering. Re-target R20A at a
  reload-entered `Refreshing` (`SourceRuntime::refreshing`) and add the
  mutation-entered case as the paired negative.

### BLOCKER — `release_owner` ignores child owners, so releasing an owner that has children promises the same bytes twice

- **Where**: `src/index_lifecycle/capacity.rs:305-325`, reachable through
  `src/index_lifecycle/process_runtime.rs:161-172`
- **Claim**: the guard is `row.outstanding.is_empty()`, but `child()` (`:204-208`)
  records a child's promise in `charged`, never in `outstanding`, so an owner with
  live children always looks drained. Sequence: `root(1000)`,
  `a = child(root, 600)`, `b = child(a, 500)`, `release_owner(a)` returns `Ok(600)`
  and `available(root)` returns to 1000 while `b` still holds a 500-byte limit and
  can still `reserve` against it; `child(root, 1000)` then succeeds, so 1500 bytes
  are promised beneath a 1000-byte root. `release_owner(b)` afterwards returns
  `Ok(500)` with the parent row gone, reporting a refund it credited nowhere.
- **Why it matters**: conservation is the one property this module exists to keep
  ("the sum of everything charged beneath a root can never exceed that root",
  `:156-157`), and `ProcessRuntime::detach` inherits the hole under a doc comment
  claiming the opposite ("Refuses while the surface still holds charges",
  `process_runtime.rs:156-160`). Slice 4's obvious next step — per-project child
  owners beneath a surface owner — is exactly the three-level shape that breaks.
- **Recommended fix**: track children per row (a count or set) and refuse
  `release_owner` while any child exists, alongside the outstanding-charge check;
  and return a refusal rather than `Ok(limit)` when the recorded parent row is
  absent, instead of reporting a refund that went nowhere.
- **Verified**: reproduced standalone with rustc 1.96.0 on a verbatim
  transcription of `OwnerRow`/`child`/`reserve`/`release_owner`; output as quoted.

## MAJOR

### MAJOR — `StagedReplacement::commit` performs the destructive rename with no lease-liveness check

- **Where**: `src/index_lifecycle/physical_root.rs:441-467`; contrast
  `src/index_lifecycle/transition.rs:102-104`
- **Claim**: `commit` uses its own `Dir` clone (`:406`) and its captured
  `PhysicalRootIdentity` and never consults `PhysicalRootLease::is_live()`; a
  stage taken before `transition::apply` revokes the outgoing lease commits
  successfully after it, and the resulting `WriteReceipt` names that lease, so
  `SourceMutationPermit::commit` (`mutation.rs:262`) accepts it and reports
  `Committed`.
- **Why it matters**: Install revokes the outgoing lease precisely "so no
  surviving permit can resolve a path under the replaced root"
  (`transition.rs:102-103`). The two-phase split introduced by this slice opens
  the window that revocation was ordered to close, and the receipt then attests a
  write performed under an authority that had been withdrawn.
- **Recommended fix**: have `StagedReplacement` hold `Arc<PhysicalRootLease>`
  rather than a bare `Dir` plus identity, and re-check `is_live()` at the top of
  `commit`, returning `RootRefusal::LeaseRevoked` and letting `Drop` remove the
  temporary. Oracle: stage, revoke, then `commit` must refuse and the target must
  still hold its preimage.

### MAJOR — `admit` discards the caller's binding, protection and placement when an occupancy already exists, and returns `Ok`

- **Where**: `src/index_lifecycle/registry.rs:267-273`
- **Claim**: on both the `Pending` and `Live` branches the `binding`,
  `protection`, `authorized` and `placement` arguments are dropped and the
  existing occupancy's identity is returned as success; a caller that admits key K
  as `Protected`/`UserLocal` joins an occupancy admitted earlier as
  `Normal`/`ProjectLocal` and is told `Ok`, so state will be written beneath a
  root the second caller declared protected.
- **Why it matters**: this is the repository's named recurring shape — `admit`
  reports that the request it was given was honoured without observing that it
  was. It also silently accepts a second, different `BindingAuthority` (a
  different physical root) for one key, which is the disagreement the binding
  identity exists to detect.
- **Recommended fix**: on join, compare the presented binding, protection and
  placement against the occupancy's and return a new
  `RegistryRefusal::BindingMismatch`/`PlacementMismatch` when they differ, rather
  than `Ok`. At minimum, refuse when the placements disagree.

### MAJOR — the widened plural `active_permits` is never read by any gate

- **Where**: `src/index_lifecycle/authority.rs:684-689`, `:883-903`;
  `src/index_lifecycle/transition.rs:68`, `:80`, `:97`
- **Claim**: `active_permits()`, `record_permit()` and `retire_permit()` have no
  caller outside `tests/project_index_authority_v11.rs` (grep of `src/` returns
  nothing), and `transition::apply` still takes a single `&PermitDrainSignal` and
  gates on `has_ended()` alone; a source with three outstanding permits passes
  Drain if the one signal handed in has ended.
- **Why it matters**: the slice's stated reason for widening the model is that
  "Slice 1 tracked a single `PermitDrainSignal` and refused a transition on one
  outstanding permit, which cannot express a source draining several at once"
  (`authority.rs:357-361`). The model became plural; the enforcement did not, so
  `install()`'s comment "reaches this line only after Drain has confirmed nothing
  is outstanding" (`:934-936`) is true of one signal, not of the source.
- **Recommended fix**: make `transition::apply` additionally require
  `runtime.active_permits().is_drained()` before pushing `TransitionStep::Drain`,
  with the oracle: two recorded permits, one retired, transition must refuse.

### MAJOR — `redeem` accepts a grant issued by a different ledger; the issuing ledger leaks the charge and reports zero anomalies

- **Where**: `src/index_lifecycle/capacity.rs:252-260`, `:331-351`
- **Claim**: `redeem` copies `grant.owner`/`grant.bytes`/`grant.charge` into a
  `ChargedAllocation` stamped with `self`, with no check that the grant came from
  this ledger; `let g = a.reserve(owner_a, n)?; let alloc = b.redeem(g);` then
  refunds into `b` on drop, where the owner row is absent, so `b.unknown_refunds`
  increments while `a` keeps `charged += n` and its `outstanding` entry
  permanently, `a.unknown_refunds()` stays `0`, and `a.release_owner(owner_a)`
  refuses forever.
- **Why it matters**: this is the answer to packet question 2 — `unknown_refunds`
  is not fooled into silence, it fires on the *wrong* ledger. The ledger that
  actually lost capacity reports nothing wrong, which is worse than no detector,
  because an operator checking `a.unknown_refunds()` sees a clean account.
- **Recommended fix**: give `CapacityLedger` an identity, stamp it into
  `CapacityGrant`, and have `redeem` return `Result` refusing a foreign grant.
  Since `OwnerIdentity` is process-globally unique,
  `self.rows.contains_key(&grant.owner)` is a sufficient one-line check if
  changing the signature is unwanted.

### MAJOR — the `unknown_refunds` oracle never drives the counter above zero

- **Where**: `tests/process_capacity_pool_v11.rs:226-248`
- **Claim**: `a_refund_for_an_unknown_charge_invents_nothing` never names a charge
  the ledger did not issue — it allocates from a second ledger and drops it, which
  refunds correctly into that second ledger — and then asserts
  `ledger.unknown_refunds() == 0`. Every `unknown_refunds` assertion in the suite
  (`:57`, `:220`, `:234`, `:243`, `tests/project_registry_lifecycle_v11.rs:203`)
  asserts zero, so both `fetch_add` sites (`capacity.rs:334`, `:347`) are
  unexecuted by any test.
- **Why it matters**: the test is named for the detector and proves only that the
  detector stayed quiet where it should. The mutation sweep cannot distinguish
  this from a `refund` that silently returns 0 without counting. It is the
  repository's own reporting-invariant failure, in the oracle layer.
- **Recommended fix**: drive it directly — construct a `ChargedAllocation` whose
  charge the ledger has already forgotten (via the cross-ledger `redeem` above,
  once that is made refusable, or a test-only refund entry point) and assert
  `unknown_refunds() == 1` with `charged` unchanged.

### MAJOR — `FINALIZING` is scoped to the thread, not to the source, and `Drop` bypasses it entirely

- **Where**: `src/index_lifecycle/embedded.rs:88-92`, `:184-186`, `:203-213`,
  `:216-225`
- **Claim**: the thread-local is a bare `bool` set by `finalize()` on any handle,
  so `a.finalize(|| b.close())` returns `EmbedRefusal::WouldSelfWait` for an
  unrelated source `b`; and `Drop::drop` performs the identical `close_one` work
  with no `FINALIZING` check, so `a.finalize(|| drop(b))` is permitted while
  `a.finalize(|| b.close())` is refused.
- **Why it matters**: the refusal reports a self-wait without observing that the
  handle being closed is the one finalizing — the information needed is available
  and simply not recorded. Either the hazard is real, in which case `Drop` walks
  into it unchecked, or it is not, in which case a legitimate close is refused and
  the source left open under a diagnosis naming something that did not happen.
- **Recommended fix**: store `Cell<Option<EmbeddedIdentity>>` and refuse only when
  it equals `self.identity`. Then decide `Drop`'s behaviour explicitly and state
  it: either apply the same check (documenting that the source stays open until
  the finalizer returns) or record that `Drop` is safe here because `close_one`
  never waits.

### MAJOR — `execute_plan` discards the plan's capacity owner and re-takes the protection decision from its own arguments

- **Where**: `src/index_lifecycle/adapters.rs:135-151`
- **Claim**: `execute_plan` forwards only `plan.key()` and `plan.placement()`;
  `plan.owner()` is never used, and `protection`/`authorized` are supplied fresh
  by the caller rather than read from the plan (which does not record them), so
  the admission executed can differ from the admission planned and nothing
  detects it.
- **Why it matters**: T038's stated value is that the decision made now is the
  decision Slice 4 must make under real traffic (`adapters.rs:11-13`). Two of the
  plan's three outputs do not survive execution, so the separation proves the
  decision is *computable*, not that it is *applied* — and the capacity owner is
  the output the whole of T034 exists to determine.
- **Recommended fix**: record `protection` and `authorized` in `AdmissionPlan`,
  have `execute_plan` read them from the plan rather than from parameters, and
  carry `plan.owner()` through to the `install` that follows. Add an oracle that
  the installed slot is charged to the owner the plan named.

## MINOR

### MINOR — `install`'s tombstone check cannot fire

- **Where**: `src/index_lifecycle/registry.rs:313-315`
- **Claim**: a `SlotIdentity` becomes tombstoned only via `cancel` (which removes
  the pending entry) or `stop` (which requires `Occupancy::Live`), so no
  `Occupancy::Pending` in the map can ever carry a tombstoned identity and the
  branch is unreachable.
- **Why it matters**: the doc at `:300-301` attributes "a cancelled or stopped
  admission can never be revived" to this branch; what actually makes it true is
  that `cancel` removed the entry. A guard implying an observation it never makes
  is the shape this repository's reporting invariant names.
- **Recommended fix**: delete the branch and state the real reason in the doc, or
  keep it and add the reachability (e.g. `cancel` leaving a tombstoned pending in
  place) that would make it meaningful.

### MINOR — `retained_generation`'s doc contradicts A20 twelve lines above the code implementing A20

- **Where**: `src/index_lifecycle/authority.rs:719-723` vs `:735-757`
- **Claim**: "Strict queryability is closed: only `Current` holds a query-granting
  generation... A retained generation is never queryable" is false for `Refreshing`
  as `queryable_generation()` now implements it.
- **Why it matters**: this is the invariant Slice 4 will read when it wires reads,
  and it states the pre-A20 rule as current.
- **Recommended fix**: rewrite the paragraph to point at `queryable_generation`
  and A20.

### MINOR — `has_shut_down` latches, and `open()` after shutdown is not refused

- **Where**: `src/index_lifecycle/embedded.rs:99`, `:112-125`, `:132-135`,
  `:138-146`
- **Claim**: `shutdown` is set when the map empties and never cleared; `open()`
  never reads it, so after the final close a fresh `open()` succeeds and
  `has_shut_down()` returns `true` while `open_count()` is 1.
- **Why it matters**: "Whether the final owner has closed and the registration has
  shut down" is then a claim about a past moment presented as present state.
- **Recommended fix**: clear `shutdown` in `open()`, or refuse `open()` once shut
  down — whichever the lifecycle intends — and pin it with an oracle.

### MINOR — a failed `try_clone` leaves the staged temporary behind

- **Where**: `src/index_lifecycle/physical_root.rs:406-409`
- **Claim**: the `?` on `dir.try_clone()` returns after the temporary has been
  created and written, with no `remove_file`, so that error path litters the
  leased root — contradicting `:22-24` ("an abandoned stage removes its own
  temporary").
- **Why it matters**: small, but it is the one path where the stated cleanup
  property does not hold, and the `Drop` guard that enforces it elsewhere does not
  exist yet at that line.
- **Recommended fix**: clone the `Dir` before creating the temporary, or remove
  the temporary on that error branch.

## Verdicts on the seven points of least confidence

1. **`capacity.rs` never blocks.** Sound as a deadlock argument; I found no
   blocking path to contradict it. But "accounted, never enforced" is the right
   reading: nothing refuses work on the basis of a `CapacityRefusal` — `reserve`
   has no caller outside tests, and the leaf budget it defers to is a separate
   accounting system with no link to this one. The invariant "the leaf keeps its
   own per-load budget" is currently unproven rather than merely non-blocking: no
   oracle demonstrates that a leaf refusal and a ledger refusal agree.
2. **Conservation against physical `Drop`.** The `Drop`/`release` pair is sound —
   `release` by value plus the `refunded` flag makes a double refund
   unrepresentable, and I could not construct one. Conservation nonetheless breaks
   twice: the child-owner BLOCKER and the cross-ledger `redeem` MAJOR.
   `unknown_refunds` is real but fires on the wrong ledger and is never exercised.
3. **`#[path]` module placement.** Legitimate; I would not spend a finding on it.
   Two contracts genuinely disagree — the seam paths name `src/index_lifecycle/...`
   and `introduced_v11_atoms` names no public lifecycle module — and `#[path]`
   satisfies both without adding a public atom. It is not evasion: the set of
   public names reachable through `symforge::live_index::index_lifecycle` is the
   same either way, so nothing is hidden from a census that walks the module tree.
   The T060 deletion is one line with no file moves, which is what makes it cheap
   rather than load-bearing.
4. **Amendment A20.** Wrong as written — see the BLOCKER. There is a state whose
   retention is complete and must not be served: `Refreshing` entered through
   `request_mutation_grant` with a permit outstanding. The availability argument is
   correct for reload; the amendment never asked what else reaches `Refreshing`.
5. **The temp-before-replace oracle.** It observes disk.
   `the_target_still_holds_its_preimage_while_the_replacement_is_staged`
   (`tests/physical_root_lease_v11.rs:120-156`) reads the temporary's bytes and the
   target's bytes from the filesystem with the stage held; a build that renamed
   first would fail both assertions, not just the label. The label has not simply
   moved. `replacement_creates_its_temporary_before_replacing` (`:187`) still
   asserts on the receipt alone, but it is now redundant rather than load-bearing.
6. **Dark by design.** The oracles prove properties of the model, and the A20
   BLOCKER is the demonstration that this is not free: R20A pins a queryability
   rule decided without the mutation path in view, and it is green. Darkness makes
   these defects cheap to fix, not cheap to have. I did not propose a seam.
7. **The RED stub.** Honest. The body panics, the `#[ignore]` reason names the task
   that owns it, and `expect_execution` in
   `scripts/validate-lifecycle-oracle-traceability.cjs` refuses an ignored-only run
   as execution evidence. Removing the attribute without writing the body fails
   loudly. I found no path to a silent receipt.

## Verification performed

- Read the full diff surface: `registry.rs`, `capacity.rs`, `physical_root.rs`,
  `embedded.rs`, `process_runtime.rs`, `adapters.rs`, `authority.rs`, plus the
  unchanged-but-load-bearing `mutation.rs` and `transition.rs`, and all five
  oracle files.
- `git diff main...HEAD --stat` for scope.
- Greps establishing that `active_permits`/`record_permit`/`retire_permit` have no
  `src/` caller, that every `unknown_refunds` assertion in the suite asserts zero,
  and that `index_lifecycle` has no production consumer.
- Compiled and ran two standalone repros (rustc 1.96.0, edition 2024, outside the
  repo): one for the `let`-else-over-`remove` eviction, one transcribing
  `OwnerRow`/`child`/`reserve`/`release_owner` verbatim for the child-owner
  conservation break. Both reproduce as described.
- **Not run**: `cargo test`/`clippy` in this worktree. A cold build here is ~25
  minutes and exceeds the tool timeout; per `CLAUDE.md` a kill mid-write risks
  corrupting `target/`. Every finding is therefore grounded in source reading plus
  the two standalone repros, not in a suite run.

## Negatives

Checked and found sound.

- **`stop()`'s revoke-then-tombstone ordering.** No interleaving exposes the
  tombstone without the revocation. `revoke()` is a `Release` store issued before
  the `tombstones.insert`, and any reader observing the tombstone did so through a
  `lock()` that `Acquire`-synchronises with `stop`'s unlock, which the store
  happens-before. The ordering claim at `registry.rs:344-345` holds as stated. It
  is the *sufficiency* of that revocation that fails, not its order.
- **`admit` single-flight.** Check and insert are under one mutex hold; concurrent
  opens of one key receive the same identity, and `joiners` increments only on the
  pending branch. No path mints two identities for one key.
- **Identity non-reuse** (`SlotIdentity`, `OwnerIdentity`, `EmbeddedIdentity`,
  `PhysicalRootIdentity`, and the shared `NEXT_IDENTITY` newtypes). All draw from
  monotonic process-global `AtomicU64` counters with no free list; reopen after
  tombstone mints a fresh value and the old identity stays refused.
- **`CapacityGrant` single redemption.** Not `Clone`, and `redeem` takes it by
  value; a second redemption is unrepresentable, not merely discouraged.
- **`ChargedAllocation::release` followed by `Drop`.** `release` sets `refunded`
  before the value falls out of scope, so the implicit drop is a no-op; no double
  refund. `reserve` charges and records `outstanding` under one lock.
- **`reserve` on a released owner fails closed.** The row is gone, so the
  `ok_or(Exhausted)` branch is taken rather than a permissive default.
- **`ProcessRuntime::attach`/`detach` at the surface level.** A second `attach` of
  the same surface is refused rather than silently replacing (which would orphan
  the previous owner's charges); a second `detach` returns `SurfaceNotRegistered`
  and does not return the promise twice.
- **`EmbeddedRegistration::open` sole-handle invariant.** Contains-check and insert
  are under one mutex hold; `EmbeddedSourceHandle` is not `Clone`; no path yields
  two handles for one key.
- **`close`/`Drop` coalescing.** Both go through `closed.swap(true, AcqRel)`, so
  exactly one reaches `close_one`; a closed handle's drop cannot release a source
  that has since been reopened, because `close_one` compares the identity before
  removing.
- **`finalize`'s panic safety.** The `Guard` clears `FINALIZING` on unwind, so one
  bad finalizer cannot wedge every later close on the thread.
- **`resolve_beneath` escape refusal.** `ParentDir`/`RootDir`/`Prefix` are refused
  before the handle sees them, intermediate components are checked for links
  through the capability, and `cap-std` confines the open regardless — a link
  planted mid-call cannot redirect out of the leased directory, because the open is
  handle-relative too. The capability genuinely closes the TOCTOU for *resolution*;
  what it does not cover is authority revocation (the `commit` MAJOR).
- **The unpredictable temporary name.** `create_new` plus a pid+counter+attempt
  suffix and a bounded retry loop; a pre-created name is refused rather than
  opened, and the loop cannot spin.
- **`StagedReplacement::Drop`.** `commit` clears `temp_relative` before returning,
  so the drop cannot remove the file just renamed into place; an abandoned stage
  removes its temporary, except on the `try_clone` path (last MINOR).
- **`WriteReceipt` lease attribution.** The receipt names the lease that produced
  it and `SourceMutationPermit::commit` compares it against its own pinned lease,
  so a receipt for root B cannot be attested by a permit pinned to root A.
- **`request_mutation_grant` leaves no trace on refusal.** All three refusal
  branches return before `freeze()`, so neither the epoch, the phase nor
  `grants_issued` moves.
- **`freeze()`** carries `observer_phase`, `active_permits` and `work` across a
  re-freeze rather than resetting them, and returns `None` for phases that store no
  publication instead of minting an identity nothing published.
- **`ActivePermits::retire` double-retire.** Returns `false` on the second call
  rather than draining twice; `record` is idempotent per grant identity.
- **`transition::apply` checks Drain before Freeze**, so a refusal leaves the phase
  and epoch untouched — the retry-on-`Err` hazard is genuinely closed.
- **Every test owns its own root.** `tempfile::tempdir()` per test across all five
  oracle files; no shared-temp leasing survives.
- **Darkness.** A grep for `index_lifecycle` across `src/` returns only the
  `#[path]` declaration in `src/live_index/mod.rs`. No production caller exists.
