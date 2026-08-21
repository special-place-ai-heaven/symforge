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

use symforge::daemon::{OpenProjectRequest, spawn_daemon};
use symforge::domain::FreshnessStatus;
use symforge::live_index::LiveIndex;
use symforge::live_index::store::SnapshotVerifyState;
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

/// Design defect 2.1 — admission refusal crosses the seam as success.
///
/// `bootstrap_project_index` returns `Result<SharedIndex>`, but a catalog
/// capacity refusal is converted into `Ok(LiveIndex::empty())`
/// (`src/daemon.rs:3539-3557`). The caller cannot tell a verified index from a
/// resource-admission refusal, so `ProjectInstance::activate` registers the
/// project and starts its watcher and Git temporal work for an instance that
/// was never admitted.
///
/// A refusal must be a refusal: no project slot, no watcher, no session.
#[test]
#[ignore = "Feature 020 Slice 0 RED control for design defect 2.1. CONTROL-STALE as of the 2026-08-21 Track A read: V11 answers a refused open with Ok plus a typed SourceRefusal and a non-ready slot, not the Err-plus-zero-slots this body asserts, and FR-004 strict acquisition is the lease. Retarget the body to the typed refusal; do NOT switch production to satisfy the old encoding. Unmeasured residual: activate still starts a watcher (daemon.rs:3398-3403)"]
fn capacity_refused_open_creates_no_slot_and_no_watcher() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        write_project_files(project.path(), "refused", 40);
        // One catalog entry admitted against forty on disk: the scout refuses
        // with CatalogEntryCapacityExceeded, the exact error the conversion
        // above swallows.
        let _cap = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", "1");

        let daemon = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let opened = daemon.state.open_project_session(OpenProjectRequest {
            project_root: project.path().display().to_string(),
            client_name: "slice0-refusal".to_string(),
            pid: Some(std::process::id()),
        });

        let registered = daemon.state.list_projects().len();
        let outcome = opened.map(|response| response.project_id);
        let _ = daemon.shutdown_tx.send(());

        assert!(
            outcome.is_err(),
            "a catalog-capacity refusal must not cross the project-registration \
             seam as a successful open; it returned {outcome:?}"
        );
        assert_eq!(
            registered, 0,
            "a refused admission must leave no registered project behind"
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
#[ignore = "Feature 020 Slice 0 RED control for design defects 2.2/2.3. CODE-WRONG as of the 2026-08-21 Track A read: add_file (live_index/store.rs:2820-2831) has no EmptyBootstrap gate, and the default-suite check at store.rs:6402-6412 papers over it rather than closing it. Keep ignored and fail-closed until a seam owner exists"]
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
///
/// `ProjectSlot::reload_with` stops the old watcher before building the
/// replacement (`src/daemon.rs:3341`). When the build fails, `?` returns before
/// a replacement watcher starts, so last-known-good content stays in memory
/// with its only source-change retry trigger gone: later edits are never
/// observed and nothing retries.
///
/// A failed reload must leave the project as observable as it was before.
///
/// Reaching `reload_with` requires an ALREADY-OPEN slot whose rebuild then
/// fails; a failing first open never gets that far, because the cold load fails
/// in `ensure_project_slot_for_binding` while the old watcher is still running.
/// `index_folder` on an open project is the path that reloads it in place, and
/// a catalog-capacity limit imposed between the two calls is what makes that
/// rebuild fail. (Capacity refusal is only converted to `Ok(empty)` on the cold
/// bootstrap path; on reload it propagates, which is exactly the `?` that skips
/// the watcher restart.)
#[test]
#[ignore = "Feature 020 Slice 0 RED control for design defect 2.10. CODE-WRONG as of the 2026-08-21 Track A read: the path aborts the watcher and then returns via ?, installing no replacement on the Err branch, so the recovery observer is lost. Keep ignored and fail-closed until a seam owner exists"]
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
#[ignore = "Feature 020 Slice 0 RED control for observer replacement gaps. CODE-WRONG as of the 2026-08-21 Track A read: recompute_freshness_locked drops the historical gap and rederives Current, so nothing latches. Keep ignored and fail-closed until a seam owner exists"]
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
#[ignore = "Feature 020 Slice 0 RED control for old-observer delivery after promotion. CODE-WRONG as of the 2026-08-21 Track A read: the same rederive path runs with no stable observer token fencing delivery. Keep ignored and fail-closed until a seam owner exists"]
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

/// Design defect 2.7 / 2.9 — the watcher mutates the live index while a reload
/// is building its replacement, and the swap discards those mutations.
///
/// `reload_for_binding_with_exclusions` builds the replacement OUTSIDE the
/// write lock (`src/live_index/store.rs:2385-2395`) and only then swaps. V10
/// has no candidate isolation, so the watcher keeps mutating the live index
/// throughout that build, and every one of those mutations is destroyed by the
/// swap — silently, with the result still reported as a complete publication.
///
/// A candidate must be isolated: either the watcher cannot mutate it, or the
/// mutations survive promotion. Losing them and reporting success is the one
/// outcome that must not happen.
#[test]
#[ignore = "Feature 020 Slice 0 RED control for design defects 2.7/2.9. CODE-WRONG. The earlier claim that the precondition window was unreachable is FALSE on this tree: live_index/store.rs:2403-2436 still reaches swap_and_publish and IsolatedCandidate appears nowhere in store.rs, so the seam never routes through the candidate pipeline. Keep ignored and fail-closed until a deterministic pause exists at this seam; the official TEST-CANDIDATE case does not retire it"]
fn watcher_mutation_during_candidate_build_is_not_discarded() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        // Large enough that the out-of-lock build stays in flight long enough
        // for a watcher mutation to land inside it.
        write_project_files(project.path(), "candidate", 1_500);
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "3600");

        let index = LiveIndex::load(project.path()).expect("load project");
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = tokio::spawn(run_watcher_with_stop(
            project.path().to_path_buf(),
            index.clone(),
            Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Arc::clone(&stop),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;

        // Start the candidate build, then mutate through the observer while it
        // is still building.
        let reload_index = index.clone();
        let reload_root = project.path().to_path_buf();
        let reload = tokio::task::spawn_blocking(move || reload_index.reload(&reload_root));

        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        std::fs::write(
            project.path().join("src").join("mutated_during_build.rs"),
            b"pub fn mutated_during_build() {}\n",
        )
        .expect("write during candidate build");

        // The mutation must land in the live index BEFORE the swap, or this
        // control is observing an ordinary post-reload edit.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut landed_before_swap = false;
        while std::time::Instant::now() < deadline {
            if reload.is_finished() {
                break;
            }
            if index
                .read()
                .get_file("src/mutated_during_build.rs")
                .is_some()
            {
                landed_before_swap = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let reload_result = reload.await.expect("reload task");
        let survived = index
            .read()
            .get_file("src/mutated_during_build.rs")
            .is_some();
        stop.store(true, Ordering::Release);
        let _ = watcher.await;

        assert!(
            reload_result.is_ok(),
            "precondition: the reload must succeed"
        );
        assert!(
            landed_before_swap,
            "precondition: the observer mutation must land in the live index \
             while the candidate is still building; it did not, so this run \
             cannot distinguish a discarded mutation from a late one"
        );
        assert!(
            survived,
            "an observer mutation applied while the candidate was building was \
             destroyed by the swap, and the publication still reports success. \
             A candidate must be isolated from observer mutation, or carry those \
             mutations through promotion"
        );
    });
}

/// FR-008 / FR-009 / SC-005, `INV-PUBLICATION` — one whole-project immutable
/// root is the sole query-visible publication unit, and partial source
/// generations are never visible.
///
/// The traceability contract reserves the name
/// `whole_project_publication_preserves_latest_siblings` in
/// `tests/project_index_lifecycle_slice0.rs` for `TEST-PUBLICATION` and owns it
/// from T017: prepare a delta for source A, publish source B, resume A, and
/// prove the latest of every sibling survives in exactly one whole-project
/// root store.
///
/// V10 has no whole-project publication unit. A reload rebuilds the entire
/// index from a disk snapshot taken outside the write lock
/// (`src/live_index/store.rs:2385-2395`) and swaps it in wholesale, while the
/// observer keeps publishing sibling updates into the live index. The swap
/// therefore replaces the latest sibling generation with whatever the snapshot
/// happened to contain — a partial publication that reports success.
///
/// Sibling B's latest must survive source A's publication.
#[test]
#[ignore = "Feature 020 Slice 0 RED control for FR-008/FR-009/SC-005. CONTROL-STALE as of the 2026-08-21 Track A read: the frozen oracle is pause A / publish B / rebase / tokens / one store, but this body races V10 LiveIndex::reload against 1500 files in 150ms. Retarget the body to the frozen oracle; making reload win that race is not the property"]
fn whole_project_publication_preserves_latest_siblings() {
    run_daemon_test(async {
        let project = TempDir::new().expect("project dir");
        // Two sibling sources under one project root. A is large enough that
        // its rebuild stays in flight while B's latest is published.
        write_project_files(&project.path().join("source_a"), "a", 1_500);
        write_project_files(&project.path().join("source_b"), "b", 8);
        let _interval = EnvVarGuard::set("SYMFORGE_RECONCILE_INTERVAL", "3600");

        let index = LiveIndex::load(project.path()).expect("load project");
        let stop = Arc::new(AtomicBool::new(false));
        let watcher = tokio::spawn(run_watcher_with_stop(
            project.path().to_path_buf(),
            index.clone(),
            Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
            Arc::clone(&stop),
        ));
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;

        // Prepare source A's delta and start the whole-project rebuild that
        // will carry it.
        std::fs::write(
            project
                .path()
                .join("source_a")
                .join("src")
                .join("a_delta.rs"),
            b"pub fn a_delta() {}\n",
        )
        .expect("write source A delta");
        let reload_index = index.clone();
        let reload_root = project.path().to_path_buf();
        let reload = tokio::task::spawn_blocking(move || reload_index.reload(&reload_root));

        // Publish source B's latest while A's root store is still being built.
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        std::fs::write(
            project
                .path()
                .join("source_b")
                .join("src")
                .join("b_latest.rs"),
            b"pub fn b_latest() {}\n",
        )
        .expect("write source B latest");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut b_published_before_store = false;
        while std::time::Instant::now() < deadline {
            if reload.is_finished() {
                break;
            }
            if index.read().get_file("source_b/src/b_latest.rs").is_some() {
                b_published_before_store = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let reload_result = reload.await.expect("reload task");

        let published = index.read();
        let a_delta = published.get_file("source_a/src/a_delta.rs").is_some();
        let b_latest = published.get_file("source_b/src/b_latest.rs").is_some();
        drop(published);
        stop.store(true, Ordering::Release);
        let _ = watcher.await;

        assert!(
            reload_result.is_ok(),
            "precondition: the reload must succeed"
        );
        assert!(
            b_published_before_store,
            "precondition: sibling B's latest must be published while source A's \
             root store is still building; it was not, so this run cannot \
             distinguish a lost sibling from a late write"
        );
        assert!(
            a_delta,
            "precondition: source A's own delta must be in the resulting store"
        );
        assert!(
            b_latest,
            "source A's publication replaced the whole index and dropped sibling \
             B's latest generation, then reported success. One whole-project \
             immutable root must carry the latest of every sibling, and a partial \
             source generation must never be the query-visible publication"
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
#[ignore = "Feature 020 Slice 0 RED control for design defect 2.11. CODE-WRONG as of the 2026-08-21 Track A read: persist hydrates files immediately, get_file has no Pending gate, and is_ready() is status-only. The SnapshotStore per-entry verify-state wiring remains a recorded open residual. Keep ignored and fail-closed until a seam owner exists"]
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

/// Design defect 2.5 — capacity controls do not reserve process capacity.
///
/// Every load builds its own `InflightByteBudget` whose ceiling is that
/// candidate's own planned bytes, so the configured limit is enforced per
/// project rather than per process. Two projects open together each admit up to
/// the whole ceiling, and the process holds twice what was configured — before
/// counting a retained generation, a replacement candidate, or a watcher
/// backlog.
///
/// A configured ceiling must bound the process, not each load in isolation.
#[test]
#[ignore = "Feature 020 Slice 0 RED control for design defect 2.5. CONTROL-STALE as of the 2026-08-21 Track A read: FR-004 makes capacity a per-candidate catalog and SC-025 owns the ProcessCapacityPool, while SYMFORGE_MAX_INDEX_FILES is per discovery pass. Making that env var process-wide would fight FR-004 and still miss SC-025. Retarget the body at ProcessCapacityPool"]
fn configured_capacity_bounds_the_process_not_each_load() {
    run_daemon_test(async {
        const CEILING: usize = 10;
        let first = TempDir::new().expect("project one");
        let second = TempDir::new().expect("project two");
        write_project_files(first.path(), "cap_a", CEILING);
        write_project_files(second.path(), "cap_b", CEILING);
        let _cap = EnvVarGuard::set("SYMFORGE_MAX_INDEX_FILES", &CEILING.to_string());

        let daemon = spawn_daemon("127.0.0.1").await.expect("spawn daemon");
        let mut opened = Vec::new();
        for (index, root) in [first.path(), second.path()].into_iter().enumerate() {
            opened.push(
                daemon
                    .state
                    .open_project_session(OpenProjectRequest {
                        project_root: root.display().to_string(),
                        client_name: format!("slice0-capacity-{index}"),
                        pid: Some(std::process::id()),
                    })
                    .expect("open project"),
            );
        }

        let admitted: usize = daemon
            .state
            .list_projects()
            .iter()
            .filter_map(|summary| daemon.state.project_health(&summary.project_id))
            .map(|health| health.file_count)
            .sum();
        let projects = daemon.state.list_projects().len();
        let _ = daemon.shutdown_tx.send(());

        // Slice 2 may satisfy the process-wide bound either by admitting less
        // or by refusing the second open outright -- which is exactly what the
        // sibling refusal control demands. Requiring both projects open would
        // keep this RED after a correct fix and misattribute the failure, so a
        // refused second open counts as the bound being honoured.
        let second_open_refused = projects < 2;
        assert!(
            second_open_refused || admitted <= CEILING,
            "two projects admitted {admitted} files against a configured ceiling \
             of {CEILING} while both stayed open. The ceiling is applied per load, \
             so every additional project multiplies what the process actually \
             holds; it must bound the process -- either by admitting within the \
             ceiling or by refusing the second open"
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
#[ignore = "Feature 020 Slice 0 RED control for same-path physical-root replacement. CODE-WRONG as of the 2026-08-21 Track A read: the registry stays path-keyed and publishes Current, so a replaced root at the same path is adopted silently. Slice 1 shipped without discharging it. Keep ignored and fail-closed until a seam owner exists"]
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
