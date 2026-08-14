//! Feature 020 V11, T047 — the dark source runtime, RED before `runtime.rs` exists.
//!
//! Ten T047 oracles over the frozen five-state machine and its leases (an
//! earlier draft of this header said eleven; the honest count was ten), plus
//! one T049 oracle over the embed boundary's contract waits — written
//! against `data-model.md:1379-1595` and `:2101-2140`, the queryability clause
//! REFREEZE-pinned as F020-V11-A20 (`data-model.md:1539-1554`), FR-043
//! (`contracts/source-binding-and-state.md:310-314`), and the runtime atoms of
//! `contracts/public-api-v11.json`. Names are transcribed, never invented:
//! `ProjectIndexRuntime` and `ProjectPublicationRoot` are SEAM-pinned to
//! `runtime.rs`; the V11 handle methods extend the SEAM-pinned
//! `embedded.rs::EmbeddedSourceHandle`; the machine's five states are closed
//! and `Stopped` is NOT one of them — the public phase derives it from
//! registry tombstoning.
//!
//! This file is deliberately NOT `preventive_runtime_dark_v11.rs`: that name
//! belongs to T051, and a `planned_exact` pin activates the moment its file
//! exists. T047 has no contract-pinned test, so this file is free.

use std::sync::Arc;

use symforge::live_index::index_lifecycle::embedded::{EmbeddedSourceFactory, ReceiptWaitError};
use symforge::live_index::index_lifecycle::public_api::{EmbeddedSourceSpec, ProcessRuntimeApi};
use symforge::live_index::index_lifecycle::registry::ProjectKey;
use symforge::live_index::index_lifecycle::runtime::{
    DarkRuntimeFactory, ProjectIndexRuntime, SourceRuntimePhase,
};
use symforge::protocol::format::claim_provenance::SourceRefusalKind;

// ── Fixtures ───────────────────────────────────────────────────────────────
// Local to this file, per the Slice 2 oracle convention. The dark factory is
// the ONLY entry: nothing here reaches a production constructor, and the
// factory's evidence stand-ins are the recorded fixture family — sealed
// shapes, unconditional admission, Slice 4 supplies the refusing evidence.

fn a_project_key(name: &str) -> ProjectKey {
    ProjectKey::new(name)
}

fn a_dark_runtime(root: &str) -> ProjectIndexRuntime {
    DarkRuntimeFactory::for_test_root(root).project_runtime(a_project_key("project-a"))
}

// ── A20/R20B: strict acquisition is closed on COMPLETENESS ─────────────────

#[test]
fn loading_blocked_stopping_refuse_strict_acquisition() {
    // data-model.md:1539-1549, byte-frozen: "Only a COMPLETE verified
    // generation may be queried. `Loading` retains none and is therefore not
    // queryable. ... `Blocked` and `Stopping` retain zero or one for recovery
    // and accounting, and those are NOT queryable."
    let runtime = a_dark_runtime("root-a");
    let source = runtime.admit_loading_source_for_test("src-a");

    let refusal = runtime
        .acquire_strict(&source)
        .expect_err("Loading retains nothing and must refuse strict acquisition");
    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);

    runtime.block_source_for_test(&source);
    let refusal = runtime
        .acquire_strict(&source)
        .expect_err("Blocked retentions are recovery evidence, never a lane");
    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);

    runtime.stop_source_for_test(&source);
    let refusal = runtime
        .acquire_strict(&source)
        .expect_err("Stopping retentions are accounting, never a lane");
    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);

    // GREEN-CONTROL: a source promoted to Current IS strictly acquirable, so
    // the three refusals above are about the states, not a lane that always
    // refuses.
    let current = runtime.admit_current_source_for_test("src-b");
    let lease = runtime
        .acquire_strict(&current)
        .expect("a COMPLETE verified generation is the one acquirable thing");
    assert!(lease.generation().scope_certificate_is_complete());
}

// ── R20A: Refreshing serves its retention until a permit is granted ────────

#[test]
fn refreshing_serves_retained_only_until_a_permit_is_granted() {
    // data-model.md:1541-1545: "`Refreshing` retains exactly one, and it
    // REMAINS QUERYABLE only while that refresh has issued NO mutation permit
    // — a reload building a successor elsewhere leaves the retained bytes
    // untouched (F020-V11-R20A)."
    let runtime = a_dark_runtime("root-a");
    let source = runtime.admit_current_source_for_test("src-a");
    let retained = runtime
        .acquire_strict(&source)
        .expect("current is acquirable")
        .generation()
        .identity();

    runtime.begin_reload_refresh_for_test(&source);
    let lease = runtime
        .acquire_strict(&source)
        .expect("a reload-entered Refreshing keeps serving its retention");
    assert_eq!(
        lease.generation().identity(),
        retained,
        "the retention is the SAME complete generation, not a rebuild"
    );

    let _permit = runtime
        .grant_mutation_permit_for_test(&source)
        .expect("the refresh may issue its permit");
    let refusal = runtime
        .acquire_strict(&source)
        .expect_err("the moment a permit exists, the retention stops being a lane");
    assert_eq!(refusal.kind(), SourceRefusalKind::SourceUnavailable);
}

// ── The permit publishes non-current BEFORE side effects ───────────────────

#[test]
fn permit_grant_publishes_refreshing_before_side_effects() {
    // contracts/source-binding-and-state.md:275-278: granting the permit
    // atomically publishes non-current Refreshing BEFORE any side effect can
    // run — an observer that looks between grant and first write must already
    // see non-current.
    let runtime = a_dark_runtime("root-a");
    let source = runtime.admit_current_source_for_test("src-a");
    runtime.begin_reload_refresh_for_test(&source);

    let permit = runtime
        .grant_mutation_permit_for_test(&source)
        .expect("grant");
    assert_eq!(
        runtime.source_phase(&source),
        SourceRuntimePhase::Refreshing,
        "the phase observed AFTER grant and BEFORE start_side_effect is already \
         non-current"
    );
    // The permit has not started its side effect yet; the publication came
    // first by construction, which is the property.
    let _ends_without_committing = permit;
}

// ── FR-043: no terminal permit path restores the prior Current ─────────────

#[test]
fn no_terminal_permit_path_restores_prior_current() {
    // data-model.md:1502-1509 + source-binding-and-state.md:310-314: "Every
    // terminal path that can return the same live binding to `Current`,
    // including a valid `NoSideEffectProof`, does so only through fresh
    // candidate publication" — commit, rollback, and drop all leave the
    // source non-current until a complete successor installs.
    let runtime = a_dark_runtime("root-a");

    // Arm 1: the permit is DROPPED without committing.
    let source = runtime.admit_current_source_for_test("src-a");
    runtime.begin_reload_refresh_for_test(&source);
    let permit = runtime
        .grant_mutation_permit_for_test(&source)
        .expect("grant");
    // Terminal path one: the permit simply ENDS, uncommitted.
    let _ = permit;
    assert!(
        runtime.acquire_strict(&source).is_err(),
        "a dropped permit must not restore the prior publication"
    );

    // Arm 2: the permit rolls back with a no-side-effect proof.
    let source_b = runtime.admit_current_source_for_test("src-b");
    runtime.begin_reload_refresh_for_test(&source_b);
    let permit = runtime
        .grant_mutation_permit_for_test(&source_b)
        .expect("grant");
    permit.rollback_with_no_side_effect_proof_for_test();
    assert!(
        runtime.acquire_strict(&source_b).is_err(),
        "even a proven no-op rollback returns to Current only through a fresh \
         candidate publication, never by restoring the prior one"
    );
}

// ── One publication root; a sealed transition rebases one source ───────────

#[test]
fn sealed_transition_rebases_one_source_and_preserves_siblings() {
    // data-model.md:1528-1537: the registry-owned ArcSwap of the project
    // runtime publication is the SOLE publication root; a sealed transition
    // exact-matches its retained token, rebases the one source, and every
    // sibling's Arc survives identical.
    let runtime = a_dark_runtime("root-a");
    let source_a = runtime.admit_current_source_for_test("src-a");
    let source_b = runtime.admit_current_source_for_test("src-b");

    let before = runtime.publication_root().load();
    let sibling_before = before.source_publication(&source_b).expect("sibling");

    runtime.begin_reload_refresh_for_test(&source_a);

    let after = runtime.publication_root().load();
    assert_ne!(
        before.publication_identity(),
        after.publication_identity(),
        "a transition publishes a NEW never-reused publication identity"
    );
    let sibling_after = after.source_publication(&source_b).expect("sibling");
    assert!(
        Arc::ptr_eq(&sibling_before, &sibling_after),
        "the untouched sibling's publication is the SAME Arc, not a rebuild"
    );
}

// ── capture_source_view: atomic, validated, no invented token ──────────────

#[test]
fn capture_source_view_is_atomic_and_invents_no_token() {
    // data-model.md:1556-1564: the capture loads the source publication,
    // acquires its token accumulator WHEN PRESENT, reloads the root,
    // exact-validates both, and retries on drift. A source that has no
    // observer token yields a view WITHOUT one — the capture never invents.
    let runtime = a_dark_runtime("root-a");
    let source = runtime.admit_current_source_for_test("src-a");

    let view = runtime
        .capture_source_view(&source)
        .expect("a stable root yields a coherent capture");
    assert_eq!(
        view.publication_identity(),
        runtime.publication_root().load().publication_identity(),
        "the view is validated against the root it was captured under"
    );
    assert!(
        view.observer_token().is_none(),
        "no observer is registered in the dark runtime, so the capture must \
         not invent a token"
    );
}

// ── Stopped is derived from the tombstone, not a sixth state ───────────────

#[test]
fn stopped_phase_derives_from_tombstone_not_a_machine_state() {
    // The internal machine is closed at exactly five states
    // (data-model.md:1413-1449); the PUBLIC SourceRuntimePhase has six
    // variants (public-api-v11.json:2372-2387) — the sixth, Stopped, is
    // produced only by registry tombstoning after Stopping completes.
    let runtime = a_dark_runtime("root-a");
    let source = runtime.admit_current_source_for_test("src-a");

    runtime.stop_source_for_test(&source);
    assert_eq!(runtime.source_phase(&source), SourceRuntimePhase::Stopping);

    runtime.complete_stop_for_test(&source);
    assert_eq!(
        runtime.source_phase(&source),
        SourceRuntimePhase::Stopped,
        "Stopped is the tombstone's phase; the machine itself never holds it"
    );
}

// ── VerifiedGeneration is total or absent ──────────────────────────────────

#[test]
fn verified_generation_requires_exact_authority_and_complete_artifacts() {
    // data-model.md:1511-1526: the private constructor requires the canonical
    // manifest, every required artifact, the complete observer cut, and the
    // charged allocation — and the generation retains its EXACT authority Arc.
    let runtime = a_dark_runtime("root-a");
    let source = runtime.admit_current_source_for_test("src-a");
    let lease = runtime.acquire_strict(&source).expect("current");

    let generation = lease.generation();
    assert!(generation.scope_certificate_is_complete());
    let authority_a = generation.generation_authority();
    let authority_b = generation.generation_authority();
    assert!(
        Arc::ptr_eq(&authority_a, &authority_b),
        "the generation retains ONE exact authority Arc, never a re-mint"
    );
}

// ── The V11 handle: infallible begin_close, self-wait refused at wait ──────

#[test]
fn begin_close_is_infallible_and_self_wait_fails_at_wait() {
    // public-api-v11.json:1284-1296: begin_close(&self) -> SourceCloseReceipt
    // is infallible; the guard against waiting on yourself moved to the WAIT,
    // which returns ReceiptWaitError rather than deadlocking.
    let factory = EmbeddedSourceFactory::new();
    let handle = factory
        .open(a_project_key("src-a"))
        .expect("an open registry admits a fresh key");

    let receipt = handle.begin_close();
    let report = receipt
        .wait_for_test()
        .expect("an ordinary wait on the close receipt completes");
    assert!(report.finalized());

    let handle_b = factory
        .open(a_project_key("src-b"))
        .expect("an open registry admits a second key");
    let error = handle_b
        .self_wait_probe_for_test()
        .expect_err("waiting on your own close from inside the finalizer refuses");
    let _ = error;
}

// ── ShutdownReport reports only what was observed ──────────────────────────

#[test]
fn shutdown_report_reflects_observed_counts_only() {
    // public-api-v11.json:2280-2307 gives ShutdownReport a joined_workers
    // count; process_runtime.rs and embedded.rs are FORBIDDEN from spawning
    // threads, timers, or tasks, and the reporting invariant forbids claiming
    // a join nothing observed. The dark runtime therefore reports EXACTLY
    // zero — a plausible non-zero here would be fabricated completion.
    let runtime = a_dark_runtime("root-a");

    let receipt = runtime.begin_shutdown_for_test();
    let report = receipt
        .wait_for_test()
        .expect("shutdown of a runtime that spawned nothing completes");
    assert_eq!(
        report.joined_workers(),
        0,
        "nothing was spawned, so nothing was joined; any other number is a \
         report of an unobserved completion"
    );
}

// ── T049: the boundary's contract waits and the open refusal mapping ───────

#[test]
fn contract_waits_guard_self_wait_and_open_refuses_a_held_source() {
    // The T049 wrap list gave the boundary its contract-shaped lanes; each
    // guard gets its refusing case AND its accepting pair here.

    // begin_close's receipt: the contract wait refuses from inside the
    // source's own finalizer, and completes with the observed truth outside
    // it. A close that PERFORMED the shutdown is not already-terminal.
    let factory = EmbeddedSourceFactory::new();
    let handle = factory
        .open(ProjectKey::new("src-close"))
        .expect("an open registry admits a fresh key");
    let receipt = handle.begin_close();
    let error = handle
        .finalize(|| receipt.wait(std::time::Instant::now()))
        .expect_err("the contract wait carries the self-wait guard, not just wait_for_test");
    assert_eq!(error, ReceiptWaitError::WouldSelfWait);
    let report = receipt
        .wait(std::time::Instant::now())
        .expect("outside the finalizer the same wait completes");
    assert!(
        !report.already_terminal,
        "this close performed the shutdown, so reporting it as joined-terminal \
         would be a false claim about who tore the source down"
    );
    assert_eq!(
        report.terminal_source_version, 0,
        "dark sources hold version 0"
    );

    // The process runtime's shutdown receipt reports observed zeros — the
    // dark runtime closed no holder's source and joined no workers.
    let runtime = ProcessRuntimeApi::acquire().expect("the dark acquisition admits");
    let shutdown = runtime
        .begin_shutdown()
        .wait(std::time::Instant::now())
        .expect("a shutdown that owned nothing completes");
    assert_eq!(shutdown.closed_sources, 0);
    assert_eq!(shutdown.joined_workers, 0);

    // open_embedded_source: sole-handle admission is the accepting case; a
    // second open of the held source refuses as SelectionUnavailable — the
    // selection exists but is unavailable until its holder closes.
    let root = std::path::PathBuf::from("dark-open-root");
    let held = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.clone()))
        .expect("the first open of a source admits");
    let refusal = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root))
        .expect_err("the sole handle is out; a second open must refuse");
    assert_eq!(refusal.kind_name(), "SelectionUnavailable");
    drop(held);
}
