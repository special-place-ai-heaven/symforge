//! Feature 020 Slice 0 causal positive controls.
//!
//! These reproduce defects named in
//! `docs/superpowers/specs/2026-08-11-project-index-lifecycle-prevention-design.md`
//! against the current implementation. They are RED by design: each one must
//! fail for the reason its name states before the fix it names exists, so
//! every control carries `#[ignore]` naming its carried owner, the way this
//! repo already gates its other out-of-default-suite tests. Remove the
//! attribute in the fixing change; the fix's acceptance is the control
//! passing without it. The Slice 4 activation cut (spec 028) ran all of
//! these before merge and observed every one still red at the daemon/watcher
//! seams they drive, so the attributes name carried post-cut work rather
//! than the frozen slices their original prose predicted;
//! `scripts/slice0-oracle-artifact.cjs` pins each control's current
//! expected outcome.
//!
//! Run them with:
//! `cargo test --test project_index_lifecycle_slice0 -- --ignored --test-threads=1`
//!
//! Every control observes first, tears down second, and asserts last. A daemon
//! or watcher left running by a panic keeps its OS-level notify threads alive,
//! so the test binary never exits — which on Windows also holds its own `.exe`
//! open and makes the next `cargo test` fail to link (LNK1104). Assertions
//! before teardown are how that happens; assertions after it cannot.

// Server-only integration test: depends on `#[cfg(feature = "server")]`
// modules (daemon/watcher). Gating the whole file keeps
// `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use symforge::daemon::{DaemonState, OpenProjectRequest, ProjectHealth, spawn_daemon};
use symforge::domain::{FreshnessReason, FreshnessStatus};
use symforge::live_index::LiveIndex;
use symforge::live_index::index_lifecycle::candidate::{
    CandidateSource, IsolatedCandidate, ProjectArtifactRoot, PromotionRefusal, SourceContentToken,
    SourceId, SourceObservation,
};
use symforge::live_index::index_lifecycle::capacity::{CapacityRefusal, ProcessCapacityPool};
use symforge::live_index::index_lifecycle::process_runtime::{
    ProcessIndexRuntime, RuntimeRefusal, SurfaceKind,
};
use symforge::live_index::index_lifecycle::supervisor::SourceSupervisor;
use symforge::live_index::store::SnapshotVerifyState;
use symforge::protocol::format::claim_provenance::{
    OperationKind, OperationReceipt, PhysicalRootLease, SourceRefusalKind, acquire_claim_context,
};
use symforge::watcher::{WatcherInfo, run_watcher_with_stop};
use tempfile::TempDir;

/// Serialized process-env mutation. Capacity limits are read from the
/// environment, and the suite runs `--test-threads=1`, so a scoped guard is
/// enough to keep one control's limit out of another's.
struct EnvVarGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

#[allow(unsafe_code)] // test-only env guard; the suite is single-threaded.
impl EnvVarGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        // SAFETY: these tests run under the project-mandated `--test-threads=1`.
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

#[allow(unsafe_code)] // test-only env guard; the suite is single-threaded.
impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: these tests run under the project-mandated `--test-threads=1`.
        unsafe {
            match &self.previous {
                Some(previous) => std::env::set_var(self.key, previous),
                None => std::env::remove_var(self.key),
            }
        }
    }
}

fn run_daemon_test<F>(future: F)
where
    F: Future<Output = ()>,
{
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .expect("build test runtime")
        .block_on(future);
}

fn write_project_files(root: &Path, prefix: &str, count: usize) {
    let src = root.join("src");
    std::fs::create_dir_all(&src).expect("create src dir");
    for index in 0..count {
        std::fs::write(
            src.join(format!("{prefix}_{index}.rs")),
            format!("pub fn {prefix}_{index}() -> usize {{ {index} }}\n"),
        )
        .expect("write project file");
    }
}

fn is_typed_catalog_capacity_refusal(freshness: &FreshnessStatus) -> bool {
    matches!(
        freshness,
        FreshnessStatus::Degraded { reason_codes, .. }
            if reason_codes.contains(&FreshnessReason::CatalogEntryCapacityExceeded)
                || reason_codes.contains(&FreshnessReason::CatalogMetadataCapacityExceeded)
    )
}

/// A slot has left the EmptyBootstrap/Loading placeholder when it is Ready,
/// Degraded with typed evidence, or carries a per-load catalog-capacity refusal.
fn slot_is_settled(health: &ProjectHealth) -> bool {
    if is_typed_catalog_capacity_refusal(&health.freshness) {
        return true;
    }
    matches!(health.index_state.as_str(), "Ready" | "Degraded")
}

async fn wait_for_settled_project_health(
    daemon: &DaemonState,
    project_ids: &[String],
    timeout: std::time::Duration,
) -> Vec<ProjectHealth> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let healths: Vec<_> = project_ids
            .iter()
            .filter_map(|id| daemon.project_health(id))
            .collect();
        if healths.len() == project_ids.len() && healths.iter().all(slot_is_settled) {
            return healths;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "Part C residual: project slots never settled to Ready or typed refusal \
                 (still EmptyBootstrap/Loading placeholders?): {healths:?}"
            );
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

/// Design defect 2.1 / FR-004 — typed catalog refusal and non-queryable cold start.
///
/// V10 converted capacity refusal into `Ok(LiveIndex::empty())` with no typed
/// evidence. V11 registers a non-ready slot and surfaces
/// `CatalogEntryCapacityExceeded` on health; strict acquisition is the lease.
/// The residual under review is side effects (`activate` still starts a watcher)
/// against a slot that was never admitted to a complete generation.
#[test]
#[ignore = "Feature 020 Slice 0 RED control for design defect 2.1 / FR-004. CONTROL-STALE→retargeted-body: asserts V11 Ok+typed CatalogEntryCapacityExceeded+non-ready slot and cold-start non-queryability; still RED until activate stops side effects for refused admissions and/or EmptyBootstrap gates watcher mutation"]
fn capacity_refused_open_creates_no_slot_and_no_watcher() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        write_project_files(project.path(), "refused", 40);
        // One catalog entry admitted against forty on disk: the scout refuses
        // with CatalogEntryCapacityExceeded before any RepositoryManifest exists.
        let _cap = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", "1");
        let pfx = "slice0-refusal-";
        let auth_token = [pfx, "auth", "token"].concat();
        let _auth = EnvVarGuard::set("SYMFORGE_DAEMON_AUTH_TOKEN", &auth_token);

        let daemon = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let opened = daemon
            .state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "slice0-refusal".to_string(),
                pid: Some(std::process::id()),
            })
            .expect("V11 keeps the daemon responsive on typed catalog refusal");

        let health = daemon
            .state
            .project_health(&opened.project_id)
            .expect("non-ready slot remains registered for typed refusal evidence");
        let registered = daemon.state.list_projects().len();

        assert_eq!(
            registered, 1,
            "V11 registers a non-ready slot so typed refusal evidence is reachable"
        );
        assert!(
            matches!(
                health.freshness,
                FreshnessStatus::Degraded {
                    last_valid_content_generation: 0,
                    ref reason_codes,
                } if reason_codes == &[FreshnessReason::CatalogEntryCapacityExceeded]
            ),
            "catalog refusal must surface typed SourceRefusal evidence, got {:?}",
            health.freshness
        );
        assert_ne!(
            health.index_state, "Ready",
            "a catalog-capacity refusal must leave the slot non-ready"
        );
        assert_eq!(
            health.file_count, 0,
            "FR-004 cold start must not publish a partial manifest"
        );

        // Load-bearing residual (not the bare-lease unit mirror below):
        // daemon get_repo_map must refuse, and the watcher window must not
        // admit files without a complete generation.
        let map_body = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/v1/sessions/{}/tools/get_repo_map",
                daemon.port, opened.session_id
            ))
            .bearer_auth(&auth_token)
            .json(&serde_json::json!({ "detail": "compact" }))
            .send()
            .await
            .expect("call daemon get_repo_map")
            .text()
            .await
            .expect("get_repo_map body");

        // Unit mirror of claim_provenance_v11.rs — supplements, does not replace,
        // the daemon get_repo_map + watcher checks above.
        let lease = symforge::protocol::format::claim_provenance::ObservationLease::for_test_root(
            PhysicalRootLease::for_test_root(&opened.canonical_root),
        );
        let bare = lease.context_input(&opened.project_id, &opened.canonical_root, None);
        let transport_refusal = acquire_claim_context(
            OperationReceipt::for_test(OperationKind::SearchText),
            vec![bare],
        )
        .expect_err("strict acquisition without a Current lease must refuse");
        assert_eq!(
            transport_refusal.kind(),
            SourceRefusalKind::AdmissionUnavailable,
            "V11 query transport must refuse non-queryable cold start with AdmissionUnavailable"
        );

        assert!(
            map_body.contains("AdmissionUnavailable"),
            "daemon query must map catalog refusal to typed SourceRefusalKind \
             AdmissionUnavailable, not `{map_body}`"
        );

        // Residual: if activate started a watcher against the empty placeholder,
        // reconciliation may admit catalog entries without a complete generation.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut after_watcher_window = health.file_count;
        while std::time::Instant::now() < deadline {
            after_watcher_window = daemon
                .state
                .project_health(&opened.project_id)
                .map(|h| h.file_count)
                .unwrap_or(after_watcher_window);
            if after_watcher_window > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        let _ = daemon.shutdown_tx.send(());

        assert_eq!(
            after_watcher_window, 0,
            "a typed catalog refusal must stay non-queryable: observed \
             {after_watcher_window} admitted file(s) without a complete \
             generation — activate likely started a watcher against the \
             non-ready placeholder (daemon.rs:3505-3510 residual)"
        );
    });
}

/// Design defect 2.2 / 2.3 — a mutable empty placeholder is published as if it
/// were an index, and the watcher is a competing loader against it.
///
/// The local startup path publishes `LiveIndex::empty()`, detaches the real
/// load, and starts the watcher against the same handle
/// (`src/main.rs:384-619`); a fresh watcher runs full reconciliation before
/// consuming its event queue (`src/watcher/mod.rs:879-1143`). The placeholder
/// is therefore query-visible and mutable while no generation has ever been
/// published, so a reconcile can admit paths into a publication that does not
/// exist.
///
/// An empty placeholder must not accept mutations: it is the absence of a
/// publication, not an empty one.
#[test]
fn empty_placeholder_publication_refuses_watcher_mutation() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        write_project_files(project.path(), "placeholder", 8);
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "1");

        // Exactly the handle the cold-start path publishes before any load has
        // run: no generation, no root, no manifest.
        let placeholder = LiveIndex::empty();
        assert_eq!(
            placeholder.read().all_files().count(),
            0,
            "precondition: the placeholder starts with no files"
        );

        // The watcher started against that same handle, as startup does.
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = tokio::spawn(run_watcher_with_stop(
            project.path().to_path_buf(),
            placeholder.clone(),
            Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Arc::clone(&stop),
        ));

        // `admitted == 0` is also true when the watcher never started or never
        // reconciled, so without proof it ran this control cannot tell a correct
        // refusal from a dead observer. A second root the watcher DOES own gives
        // that proof: it must admit there while admitting nothing here.
        let live = TempDir::new().expect("live project dir");
        write_project_files(live.path(), "live", 4);
        let witness = LiveIndex::load(live.path()).expect("load witness project");
        let witness_stop = Arc::new(AtomicBool::new(false));
        let witness_watcher = tokio::spawn(run_watcher_with_stop(
            live.path().to_path_buf(),
            witness.clone(),
            Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Arc::clone(&witness_stop),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        std::fs::write(
            live.path().join("src").join("live_witness.rs"),
            b"pub fn live_witness() {}\n",
        )
        .expect("write witness file");

        // Its first action is a full reconciliation, before any event queue.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut admitted = 0;
        while std::time::Instant::now() < deadline {
            admitted = placeholder.read().all_files().count();
            if admitted > 0 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        let witness_saw_its_own_edit = {
            let mut seen = false;
            let witness_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while std::time::Instant::now() < witness_deadline {
                if witness.read().get_file("src/live_witness.rs").is_some() {
                    seen = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            seen
        };
        stop.store(true, Ordering::Release);
        witness_stop.store(true, Ordering::Release);
        let _ = watcher.await;
        let _ = witness_watcher.await;

        assert!(
            witness_saw_its_own_edit,
            "precondition: the watcher machinery must be observing at all; a \
             sibling watcher failed to admit its own edit, so zero admissions \
             into the placeholder would prove nothing about publication semantics"
        );
        assert_eq!(
            admitted, 0,
            "the watcher admitted {admitted} path(s) into a never-published empty \
             placeholder; an absent publication must refuse mutation rather than \
             accumulate a partial one that queries can already see"
        );
    });
}

/// Design defect 2.10 — a failed reload removes the recovery observer.
/// **Resolved by Track A seam 2.** This is now a regression guard, not a
/// positive control, and it carries no `#[ignore]`.
///
/// `ProjectSlot::reload_with` stops the old watcher before building the
/// replacement. When the build failed, the error path returned without
/// starting one, so last-known-good content stayed in memory with its only
/// source-change retry trigger gone: later edits were never observed and
/// nothing retried. That path now starts a replacement observer before
/// propagating the error, on the root the project is still bound to.
///
/// A failed reload must leave the project as observable as it was before.
///
/// Reaching `reload_with` requires an ALREADY-OPEN slot whose rebuild then
/// fails; a failing first open never gets that far, because the cold load fails
/// in `ensure_project_slot_for_binding` while the old watcher is still running.
/// `index_folder` on an open project is the path that reloads it in place, and
/// a catalog-capacity limit imposed between the two calls is what makes that
/// rebuild fail. Capacity refusal is only converted to `Ok(empty)` on the cold
/// bootstrap path; on reload it propagates, which is what drives the error path
/// this control exercises.
#[test]
fn failed_reload_retains_the_recovery_observer() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        write_project_files(project.path(), "retained", 8);
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "1");
        let pfx = "slice0-retention-";
        let auth_token = [pfx, "to", "ken"].concat();
        let _auth = EnvVarGuard::set("SYMFORGE_DAEMON_AUTH_TOKEN", &auth_token);

        let daemon = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let opened = daemon
            .state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "slice0-retention".to_string(),
                pid: Some(std::process::id()),
            })
            .expect("open project session");
        let before = daemon
            .state
            .project_health(&opened.project_id)
            .expect("project health")
            .file_count;
        assert!(before > 0, "precondition: the project indexed its files");

        // Reload the OPEN project under a limit its own tree cannot satisfy.
        // `reload_with` stops the watcher, the rebuild fails, `?` returns.
        let failed = {
            let _cap = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", "1");
            reqwest::Client::new()
                .post(format!(
                    "http://127.0.0.1:{}/v1/sessions/{}/tools/index_folder",
                    daemon.port, opened.session_id
                ))
                .bearer_auth(&auth_token)
                .json(&serde_json::json!({
                    "path": project.path().display().to_string()
                }))
                .send()
                .await
                .expect("call daemon index_folder")
                .text()
                .await
                .expect("index_folder body")
        };
        // Any non-success body would satisfy a bare `!starts_with("Indexed ")`,
        // so an auth rejection or a routing change would leave the old watcher
        // alive, let the post-failure edit be observed, and pass this control
        // with defect 2.10 fully present. Require the specific admission
        // refusal this control induces.
        let rebuild_failed = !failed.starts_with("Indexed ")
            && (failed.contains("capacity") || failed.contains("too large to index"));

        // A new edit is the only thing a surviving observer would react to.
        std::fs::write(
            project.path().join("src").join("retained_after_failure.rs"),
            b"pub fn retained_after_failure() {}\n",
        )
        .expect("write post-failure file");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut after = before;
        while std::time::Instant::now() < deadline {
            after = daemon
                .state
                .project_health(&opened.project_id)
                .map(|health| health.file_count)
                .unwrap_or(before);
            if after > before {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        let _ = daemon.shutdown_tx.send(());

        assert!(
            rebuild_failed,
            "precondition: the in-place rebuild must fail, got: {failed}"
        );
        assert!(
            after > before,
            "an edit made after a failed reload was never observed ({before} files \
             before, {after} after): the failed build stopped the old watcher and \
             returned before starting its replacement, leaving the retained content \
             with no source-change retry trigger"
        );
    });
}

/// Design defect 2.6 / observer replacement — a replacement gap is not latched
/// and any later clean publication erases it.
///
/// V10 does mark a gap window non-Current, so the naive form of this control
/// passes. The defect is narrower and worse: `recompute_freshness_locked`
/// (`src/live_index/store.rs:1840-1909`) explicitly DROPS the previous
/// `ObservationFailed`, `ReconciliationPending`, and
/// `SnapshotVerificationFailed` reasons and rederives them from present state —
/// currently-unreadable entries and current scout coverage. A gap is a
/// historical fact, but freshness is a pure function of the present, so the
/// first publication that happens to look clean reports `Current` again with
/// nothing having proved the missed window was ever recovered.
///
/// The design requires handoff to publish non-Current, drain the predecessor,
/// and retire the gapped token before a successor may serve Current — a latch
/// that only a proved complete scope clears.
///
/// The edit is deliberately made while NO observer exists, which is a fact
/// about the window rather than a race: the predecessor is stopped and awaited
/// before the write, and the successor starts after it.
#[test]
fn observer_replacement_gap_is_latched_as_non_current() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        write_project_files(project.path(), "gap", 8);
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "3600");

        let index = LiveIndex::load(project.path()).expect("load project");
        assert!(
            matches!(*index.freshness_status(), FreshnessStatus::Current),
            "precondition: a freshly loaded index is Current"
        );

        // Predecessor observer, fully stopped and drained before the edit.
        let stop = Arc::new(AtomicBool::new(false));
        let predecessor = tokio::spawn(run_watcher_with_stop(
            project.path().to_path_buf(),
            index.clone(),
            Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Arc::clone(&stop),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        stop.store(true, Ordering::Release);
        let _ = predecessor.await;

        // The replacement window: no observer exists for this change.
        std::fs::write(
            project.path().join("src").join("gap_during_handoff.rs"),
            b"pub fn gap_during_handoff() {}\n",
        )
        .expect("write during the handoff gap");

        // Successor observer registers after the missed change.
        let successor_stop = Arc::new(AtomicBool::new(false));
        let successor = tokio::spawn(run_watcher_with_stop(
            project.path().to_path_buf(),
            index.clone(),
            Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Arc::clone(&successor_stop),
        ));

        // Wait for the successor to absorb the missed change. Asserting before
        // it settles would pass on a transient Verifying/ReconciliationPending
        // state, which is scheduling noise, not a latched gap.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut absorbed = false;
        while std::time::Instant::now() < deadline {
            if index.read().get_file("src/gap_during_handoff.rs").is_some() {
                absorbed = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        // Let any pending status settle after the reconcile completes.
        tokio::time::sleep(std::time::Duration::from_millis(1_000)).await;

        let gapped = (*index.freshness_status()).clone();
        successor_stop.store(true, Ordering::Release);
        let _ = successor.await;

        // A perfectly ordinary clean publication of the same root. It proves
        // nothing about the missed window; it only rebuilds present state.
        index.reload(project.path()).expect("clean reload");
        let after_clean_publication = (*index.freshness_status()).clone();

        assert!(
            absorbed,
            "precondition: the successor must pick the missed change up at all; \
             without that this control cannot distinguish a latched gap from a \
             successor that never ran"
        );
        assert!(
            !matches!(gapped, FreshnessStatus::Current),
            "precondition: the gap window itself must be non-Current, but the \
             index reported {gapped:?}"
        );
        assert!(
            !matches!(after_clean_publication, FreshnessStatus::Current),
            "a clean reload erased the observer-replacement gap: freshness went \
             from {gapped:?} to {after_clean_publication:?} without anything \
             proving the missed window was recovered. A gap is a historical \
             fact and must latch until a complete scope is proved, not be \
             rederived away by the next publication that happens to look clean"
        );
    });
}

/// Design defect 2.8 / old-observer delivery — an event captured before a
/// promotion is applied after it without making the promoted generation
/// non-Current.
///
/// The design's oracle is explicit: "Queue an old-observer event before
/// promotion and deliver it afterward; prove the stable ObserverToken makes the
/// promoted generation non-Current." The promoted generation never observed
/// what the predecessor observation epoch saw, so it cannot claim to be current
/// about it until it proves a complete scope of its own.
///
/// V10 has no observer token and no epoch: the queued delivery re-syncs its
/// fence to whatever generation is live and applies, leaving no durable mark
/// that the promoted generation consumed a predecessor's observation.
///
/// Asserting only "non-Current right after the delivery" is vacuous — a test
/// tree routinely sits in `Degraded` for unrelated present-state reasons, and
/// this control passed that way before being reframed. What V10 cannot do is
/// RETAIN the fact: `recompute_freshness_locked` rederives freshness from
/// present state, so the next clean publication erases it. The assertion is
/// therefore that the consumed-delivery fact survives an ordinary clean reload.
#[test]
fn old_observer_delivery_after_promotion_is_not_current() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        write_project_files(project.path(), "queued", 6);
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "3600");

        let index = LiveIndex::load(project.path()).expect("load project");
        let info = Arc::new(parking_lot::Mutex::new(WatcherInfo::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = tokio::spawn(run_watcher_with_stop(
            project.path().to_path_buf(),
            index.clone(),
            Arc::clone(&info),
            Arc::clone(&stop),
        ));
        // Let the observer register before anything is queued against it.
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        let events_before = info.lock().events_processed;

        // Queue an event under the current observation epoch, then promote a
        // new generation before the debounce window can deliver it.
        std::fs::write(
            project
                .path()
                .join("src")
                .join("queued_before_promotion.rs"),
            b"pub fn queued_before_promotion() {}\n",
        )
        .expect("write queued file");
        index
            .reload(project.path())
            .expect("promote a new generation");
        let events_at_promotion = info.lock().events_processed;

        // Now let the predecessor's event land on the promoted generation.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut delivered_after_promotion = false;
        while std::time::Instant::now() < deadline {
            if info.lock().events_processed > events_at_promotion {
                delivered_after_promotion = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let after_delivery = (*index.freshness_status()).clone();
        stop.store(true, Ordering::Release);
        let _ = watcher.await;

        // An ordinary clean publication. It proves nothing about the consumed
        // cross-epoch delivery; it only rebuilds present state.
        index.reload(project.path()).expect("clean reload");
        let after_clean_publication = (*index.freshness_status()).clone();

        assert_eq!(
            events_at_promotion, events_before,
            "precondition: the event must still be queued when the promotion \
             happens, otherwise this control observes an ordinary in-epoch \
             delivery rather than an old-observer one"
        );
        assert!(
            delivered_after_promotion,
            "precondition: the queued event must actually be delivered after the \
             promotion; nothing was delivered within the deadline"
        );
        assert!(
            !matches!(after_clean_publication, FreshnessStatus::Current),
            "a generation that consumed a predecessor epoch's delivery reported \
             {after_delivery:?} and then {after_clean_publication:?} after an \
             ordinary clean reload. Nothing durable records that the promoted \
             generation answered for an observation it never made, so the next \
             publication that looks clean erases it. A stable observer token must \
             latch the promoted generation non-Current until it proves its own \
             complete scope"
        );
    });
}

// Design defect 2.7 / 2.9 — the watcher mutates the live index while a reload
// is building its replacement, and the swap discards those mutations.
//
// Product closed by PR #683 (carry-or-fail-closed). The former sleep-race
// daemon/watcher body could not establish the outside-lock precondition
// without racing, so it is retired. The durable causal guard is the lib
// successor `reload_outside_lock_admitted_mutation_survives_swap_or_publish_fails_closed`
// in `src/live_index/store.rs`, exercised via the ordered outside-lock seam
// hook and tracked in `scripts/slice0-oracle-artifact.cjs` RESOLVED_CASES.

/// FR-008 / FR-009 / SC-005, `INV-PUBLICATION` / `TEST-PUBLICATION`
/// (`lifecycle-acceptance-oracles-v11.md::ORACLE-PUBLICATION-WHOLE-ROOT`).
///
/// Frozen oracle: pause candidate A at the commit point, publish candidate B as
/// the new whole-project root, resume A against the latest root, and prove the
/// latest of every sibling survives in exactly one store. Candidate tokens are
/// opaque equality capabilities — numeric epochs never authorize publication.
///
/// Preflight runs on `candidate.rs::IsolatedCandidate` / `ProjectArtifactRoot`.
/// Product gap keeps RED until `runtime.rs::ProjectPublicationRoot` backs the
/// query-visible trunk (T017/T060 activation).
#[test]
#[ignore = "Feature 020 Slice 0 RED control for FR-008/FR-009/SC-005 / INV-PUBLICATION. CONTROL-STALE→retargeted-body: TEST-PUBLICATION pause-A/publish-B/rebase on IsolatedCandidate; still RED until ProjectPublicationRoot wires query-visible publication (T017/T060)"]
fn whole_project_publication_preserves_latest_siblings() {
    fn content_source(id: u64, rel: &str, token: u64) -> CandidateSource {
        use symforge::domain::index::CatalogPath;
        CandidateSource {
            id: SourceId(id),
            observation: SourceObservation::Content {
                path: CatalogPath {
                    public_id: format!("pid-{rel}"),
                    normalized_utf8: Some(rel.to_string()),
                },
                token: SourceContentToken(token),
                bytes: 64,
            },
        }
    }

    fn stamp_derive(source: &CandidateSource) -> u64 {
        match &source.observation {
            SourceObservation::Content { token, .. } => token.0.wrapping_mul(31),
            other => panic!("derive called for a non-content observation: {other:?}"),
        }
    }

    run_daemon_test(async {
        let pool = std::sync::Arc::new(ProcessCapacityPool::new());
        let owner = pool.root(1_000_000);
        let supervisor = SourceSupervisor::new();
        let root = ProjectArtifactRoot::empty();

        // ORACLE-PUBLICATION-WHOLE-ROOT precondition: candidate tokens are
        // opaque — numeric epochs never authorize publication.
        assert_eq!(
            root.publish_claiming_epoch_only(1),
            PromotionRefusal::EpochIsNotAuthority
        );

        // Baseline: sibling sources A and B under one whole-project root.
        let baseline_attempt = supervisor.begin_attempt();
        let baseline = IsolatedCandidate::prepare_full(
            &pool,
            owner,
            &baseline_attempt,
            vec![
                content_source(1, "source_a/src/a_base.rs", 1),
                content_source(2, "source_b/src/b_base.rs", 1),
            ],
            stamp_derive,
        )
        .expect("capacity headroom exists")
        .commit(&root)
        .expect("baseline publishes one whole-project root");

        // Step 1 — Pause candidate A at T017's final commit point (prepared, held).
        let delta_a_attempt = supervisor.begin_attempt();
        let delta_a = IsolatedCandidate::prepare_delta(
            &pool,
            owner,
            &delta_a_attempt,
            content_source(1, "source_a/src/a_delta.rs", 2),
            Some(SourceContentToken(1)),
            stamp_derive,
        )
        .expect("capacity headroom exists");

        // Step 2 — Publish candidate B as the new whole-project root while A pauses.
        let delta_b_attempt = supervisor.begin_attempt();
        let with_b_latest = IsolatedCandidate::prepare_delta(
            &pool,
            owner,
            &delta_b_attempt,
            content_source(2, "source_b/src/b_latest.rs", 2),
            Some(SourceContentToken(1)),
            stamp_derive,
        )
        .expect("capacity headroom exists")
        .commit(&root)
        .expect("source B publishes exactly once");
        let b_latest_before = std::sync::Arc::clone(&with_b_latest.sources[&SourceId(2)]);

        // Step 3 — Resume A, rebase against the latest root, commit once on success.
        let patched = delta_a
            .commit(&root)
            .expect("source A rebases on B and commits one whole-project root");

        assert_eq!(supervisor.committed_generations(), 3);
        assert_eq!(
            patched.sources.len(),
            2,
            "one whole-project root carries every sibling"
        );
        assert_eq!(
            patched.sources[&SourceId(2)].token,
            SourceContentToken(2),
            "source B's latest token must survive source A's publication"
        );
        assert!(
            std::sync::Arc::ptr_eq(&patched.sources[&SourceId(2)], &b_latest_before),
            "source B's latest generation must survive as the identical Arc sibling"
        );
        assert_eq!(
            patched.sources[&SourceId(1)].token,
            SourceContentToken(2),
            "source A's delta must land in the same store"
        );
        assert!(
            std::sync::Arc::ptr_eq(&root.load(), &patched),
            "exactly one whole-project root store is query-visible after both commits"
        );
        assert!(
            !std::sync::Arc::ptr_eq(&root.load(), &baseline),
            "precondition: B's publication advanced the root before A resumed"
        );

        // Product seam (INV-PUBLICATION): production_seams name
        // CandidateCommit / ProjectIndexRuntime / ProjectPublicationRoot
        // (`lifecycle-acceptance-oracles-v11.md`). The candidate preflight above
        // passes; the LiveIndex reload trunk is not yet wired to those seams.
        let store_src = include_str!("../src/live_index/store.rs");
        assert!(
            store_src.contains("ProjectPublicationRoot") || store_src.contains("CandidateCommit"),
            "query-visible publication must route through ProjectPublicationRoot \
             / CandidateCommit (T017/T060), not LiveIndex::reload wholesale swap"
        );
    });
}

/// Design defect 2.11 — snapshot restoration bypasses candidate isolation.
///
/// Startup deserializes a checkpoint straight into the shared live index and
/// verifies it asynchronously, so the snapshot's contents are query-visible
/// while `SnapshotVerifyState` is still `Pending`. Any edit made while the
/// process was down is served as current until verification happens to catch
/// up.
///
/// A snapshot is a seed, not a publication: nothing from it may answer a query
/// before its identity and completeness are re-proved.
#[test]
fn snapshot_seed_is_not_queryable_before_verification() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        write_project_files(project.path(), "snap", 6);
        let tracked = project.path().join("src").join("snap_0.rs");

        // `index_folder` is the path that persists the published generation as
        // an atomic snapshot; a plain session open does not checkpoint.
        let pfx = "slice0-snapshot-";
        let auth_token = [pfx, "to", "ken"].concat();
        let _auth = EnvVarGuard::set("SYMFORGE_DAEMON_AUTH_TOKEN", &auth_token);
        let daemon = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let opened = daemon
            .state
            .open_project_session(OpenProjectRequest {
                project_root: project.path().display().to_string(),
                client_name: "slice0-snapshot".to_string(),
                pid: Some(std::process::id()),
            })
            .expect("open project");
        let indexed = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/v1/sessions/{}/tools/index_folder",
                daemon.port, opened.session_id
            ))
            .bearer_auth(&auth_token)
            .json(&serde_json::json!({ "path": project.path().display().to_string() }))
            .send()
            .await
            .expect("call daemon index_folder")
            .text()
            .await
            .expect("index_folder body");
        assert!(
            indexed.starts_with("Indexed "),
            "precondition: index_folder must succeed so a checkpoint is written, got: {indexed}"
        );
        let _ = daemon.shutdown_tx.send(());
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;

        // The offline edit: the process is down, so no observer sees this.
        std::fs::write(&tracked, b"pub fn snap_0() -> usize { 424242 }\n").expect("offline edit");

        // Restart: the snapshot is restored and verification is asynchronous.
        let Some(restored) = symforge::live_index::persist::load_snapshot_for_root(project.path())
        else {
            panic!("precondition: opening the project must have written a checkpoint");
        };
        let (live, _signals) =
            symforge::live_index::persist::snapshot_to_live_index_with_code_signals(
                restored,
                project.path(),
            );
        let seeded = symforge::live_index::SharedIndexHandle::shared(live);

        let verify_state = seeded.read().snapshot_verify_state();
        let served = seeded.read().get_file("src/snap_0.rs").is_some();

        assert!(
            !matches!(verify_state, SnapshotVerifyState::Completed(_)),
            "precondition: a freshly restored snapshot must still be awaiting \n             verification, observed {verify_state:?}"
        );
        assert!(
            !served,
            "a snapshot restored but not yet verified ({verify_state:?}) answered a \n             query for src/snap_0.rs, whose bytes on disk had already changed \n             underneath it. A seed must not be query-visible until its identity \n             and completeness are re-proved"
        );
    });
}

/// Design defect 2.5 / SC-025 — process capacity is one conserved domain.
///
/// FR-004 makes catalog-entry limits per-candidate; SC-025 owns
/// `ProcessCapacityPool` and `ProcessIndexRuntime` as the process-wide budget
/// (`ORACLE-CAPACITY-PHYSICAL-OWNERSHIP`, `ORACLE-CAPACITY-RUNTIME-INTEGRATION`
/// in `lifecycle-acceptance-oracles-v11.md`). `SYMFORGE_MAX_INDEX_FILES` bounds
/// each discovery pass independently and must not be mistaken for the pool.
///
/// Part A mirrors `tests/process_capacity_pool_v11.rs::capacity_is_conserved_until_physical_drop`.
/// Part B exercises `process_runtime.rs::ProcessIndexRuntime` surface attach refusal.
/// Part C asserts daemon opens must honor the same budget — not yet wired.
#[test]
#[ignore = "Feature 020 Slice 0 RED control for design defect 2.5 / SC-025. CONTROL-STALE→retargeted-body: ProcessCapacityPool + ProcessIndexRuntime process-wide refusal then daemon integration; still RED until SC-025 wires pool acquisition into open/load"]
fn configured_capacity_bounds_the_process_not_each_load() {
    run_daemon_test(async {
        const CEILING: usize = 10;
        const FILE_BYTES: u64 = 64;
        const PROCESS_BUDGET: u64 = (CEILING as u64) * FILE_BYTES;

        // Part A — ORACLE-CAPACITY-PHYSICAL-OWNERSHIP (mirrors
        // process_capacity_pool_v11.rs, not duplicated): one process pool
        // conserves charges until physical drop.
        let pool = std::sync::Arc::new(ProcessCapacityPool::new());
        let process_owner = pool.root(PROCESS_BUDGET);
        let held = pool
            .redeem(
                pool.reserve(process_owner, PROCESS_BUDGET - FILE_BYTES)
                    .expect("first reservation fits"),
            )
            .expect("redeem first grant");
        assert_eq!(
            pool.reserve(process_owner, FILE_BYTES * 2)
                .expect_err("second reservation exceeds headroom"),
            CapacityRefusal::Exhausted {
                requested: FILE_BYTES * 2,
                available: FILE_BYTES,
            }
        );
        drop(held);
        assert_eq!(
            pool.charged(process_owner),
            0,
            "physical drop refunds exactly once"
        );

        // Part B — ORACLE-CAPACITY-RUNTIME-INTEGRATION seam: surfaces share
        // one ProcessIndexRuntime budget; a second attach beyond headroom refuses.
        let runtime = ProcessIndexRuntime::incarnate(PROCESS_BUDGET);
        runtime
            .attach(SurfaceKind::Daemon, PROCESS_BUDGET - FILE_BYTES)
            .expect("first surface fits");
        assert_eq!(
            runtime
                .attach(SurfaceKind::Stdio, FILE_BYTES * 2)
                .expect_err("second surface exceeds process runtime headroom"),
            RuntimeRefusal::Capacity(CapacityRefusal::ExceedsParent {
                requested: FILE_BYTES * 2,
                available: FILE_BYTES,
            })
        );

        // Part C — product seam: daemon opens must participate in the same pool.
        let first = TempDir::new().expect("project one");
        let second = TempDir::new().expect("project two");
        write_project_files(first.path(), "cap_a", CEILING);
        write_project_files(second.path(), "cap_b", CEILING);
        let _cap = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", &CEILING.to_string());

        let daemon = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let opened: Vec<_> = [first.path(), second.path()]
            .into_iter()
            .enumerate()
            .map(|(index, root)| {
                daemon
                    .state
                    .open_project_session(OpenProjectRequest {
                        project_root: root.display().to_string(),
                        client_name: format!("slice0-capacity-{index}"),
                        pid: Some(std::process::id()),
                    })
                    .expect("open project")
            })
            .collect();

        // Fail-closed: wait until every slot leaves EmptyBootstrap/Loading
        // placeholders — zero file_count during cold start must not satisfy
        // process conservation.
        let settled = wait_for_settled_project_health(
            &daemon.state,
            &opened
                .iter()
                .map(|o| o.project_id.clone())
                .collect::<Vec<_>>(),
            std::time::Duration::from_secs(30),
        )
        .await;

        let ready_admitted: usize = settled
            .iter()
            .filter(|health| health.index_state == "Ready")
            .map(|health| health.file_count)
            .sum();
        let per_load_scout_refusal = settled
            .iter()
            .any(|health| is_typed_catalog_capacity_refusal(&health.freshness));
        let projects = daemon.state.list_projects().len();
        let _ = daemon.shutdown_tx.send(());

        // Success limb (GREEN when SC-025 wires pool into open/load): open-time
        // process refusal, OR Ready totals within the process ceiling. Per-load
        // CatalogEntryCapacityExceeded scout refusal is explicitly NOT process pool.
        assert!(
            projects < opened.len() || ready_admitted <= CEILING,
            "daemon open must honor the ProcessCapacityPool process-wide budget \
             (SC-025): {ready_admitted} Ready-indexed files across {projects} \
             settled slot(s) with no open-time process refusal against a process \
             ceiling of {CEILING}. Per-load scout refusal observed={per_load_scout_refusal}; \
             that is FR-004 catalog isolation, not process capacity."
        );
    });
}

/// Design defect 2.8 / same-path physical-root replacement.
///
/// A root deleted and recreated at the same path is a different physical root,
/// but V10 identifies roots by path alone. Nothing records that the identity
/// under that path changed, so a publication built against the replacement is
/// reported exactly as one built against the original — the classic
/// delete/recreate ABA.
///
/// As with the observer-gap control, the assertion is that the replacement
/// survives a subsequent clean publication: V10's freshness is a pure function
/// of present state, so a transient non-Current proves nothing.
#[test]
fn same_path_root_replacement_is_not_silently_adopted() {
    run_daemon_test(async {
        let parent = TempDir::new().expect("parent dir");
        let root = parent.path().join("project");
        std::fs::create_dir_all(&root).expect("create root");
        write_project_files(&root, "before", 6);
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "3600");

        let index = LiveIndex::load(&root).expect("load original root");
        assert!(
            index.read().get_file("src/before_0.rs").is_some(),
            "precondition: the original root is indexed"
        );

        // Delete the whole root and recreate a different project at the same
        // path: same path, different physical root.
        std::fs::remove_dir_all(&root).expect("remove original root");
        std::fs::create_dir_all(&root).expect("recreate root");
        write_project_files(&root, "after", 6);

        index.reload(&root).expect("reload the replacement");
        let after_replacement = (*index.freshness_status()).clone();
        index.reload(&root).expect("ordinary clean reload");
        let after_clean_publication = (*index.freshness_status()).clone();

        assert!(
            index.read().get_file("src/after_0.rs").is_some(),
            "precondition: the replacement's content must be indexed"
        );
        assert!(
            !matches!(after_clean_publication, FreshnessStatus::Current),
            "a root deleted and recreated at the same path was adopted with no \
             durable record that the identity changed: freshness went from \
             {after_replacement:?} to {after_clean_publication:?}. Same-path \
             replacement must fence the prior incarnation rather than be \
             indistinguishable from a reindex of the same root"
        );
    });
}
