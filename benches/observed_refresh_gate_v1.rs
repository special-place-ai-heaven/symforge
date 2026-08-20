//! `ObservedRefreshGateV1` (Feature 020 Slice 4, T068/T033 — frozen
//! registration `criterion_group:observed_refresh_gate_v1_group->
//! observed_refresh_gate_v1`).
//!
//! Measures the OBSERVED refresh: from an exact completed write (or
//! SymForge mutation commit) to the FIRST in-process observation of the
//! published index carrying that byte identity (`IndexedFile::content` ==
//! the written bytes). Campaigns map onto the three real ingress lanes plus
//! the embed contract:
//!
//! * `delivered_event`  — the managed-observer lane: the real watcher
//!   (`run_watcher_with_stop`: notify + debounce + event batches) delivers
//!   the change; daemon/stdio/serve all ride this lane.
//! * `need_rescan`      — the gap lane: changes land while NO observer
//!   runs; a fresh watcher's mandatory fresh-instance reconciliation must
//!   repair them (the rescan the gap/overflow path performs).
//! * `suppressed_notification` — no watcher delivery at all; the read
//!   ingress (`get_symbol` freshen-on-read) observes staleness itself.
//! * `embed_mutation_commit`   — the embed contract: the synchronous
//!   `update_file_from_disk` facade commit.
//!
//! Fixed workloads per campaign: add, modify, delete, rename,
//! terminal-classification (growth past the 4 MiB code threshold demotes to
//! metadata-only), and a 24-file modify burst measured to FULL convergence.
//!
//! Controls: a deterministic corpus whose digest is pinned by
//! `tests/fixtures/observed-refresh-v1/corpus.json`; host identity,
//! warm-up (initial cold load + one untimed pass) and quiescence (watcher
//! settle polling) recorded; per-campaign completion receipts counted; the
//! pre-granted capacity vector (per-surface and process dark budgets)
//! recorded; and CLEAN-REBUILD EQUIVALENCE — after every campaign's
//! incremental refreshes, a from-disk rebuild must agree file-for-file on
//! content hashes.
//!
//! The bench emits the code-owned receipt
//! `target/observed-refresh-gate-v1/receipt.json` (consumed by release
//! materialization validation and by T070's baseline comparison).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};

use symforge::live_index::LiveIndex;
use symforge::live_index::single_file::update_file_from_disk;
use symforge::live_index::store::SharedIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::{WatcherInfo, run_watcher_with_stop};

// ── Deterministic corpus ───────────────────────────────────────────────────

const CORPUS_FILES: usize = 40;
const BURST_FILES: usize = 24;
/// Growth past the 4 MiB code metadata-only threshold demotes a file.
const TERMINAL_BYTES: usize = 4 * 1024 * 1024 + 4096;

fn corpus_file_body(index: usize, revision: usize) -> String {
    format!(
        "//! Fixture file {index}, revision {revision}.\n\
         pub fn fixture_{index}_symbol() -> u64 {{\n    {index} + {revision}\n}}\n\
         pub struct Fixture{index} {{\n    pub field: u64,\n}}\n"
    )
}

fn write_corpus(root: &Path) -> String {
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    let mut digest_input = String::new();
    for index in 0..CORPUS_FILES {
        let body = corpus_file_body(index, 0);
        digest_input.push_str(&body);
        std::fs::write(root.join(format!("src/fixture_{index}.rs")), &body).expect("corpus file");
    }
    symforge::hash::digest_hex(digest_input.as_bytes())
}

// ── Observation: the first lease of the byte identity ──────────────────────

/// Poll the published index until `rel`'s stored content equals `expected`
/// (byte identity), or until the deadline. Returns the observation latency.
fn observe_bytes(shared: &SharedIndex, rel: &str, expected: &[u8], started: Instant) -> Duration {
    let deadline = started + Duration::from_secs(30);
    loop {
        {
            let guard = shared.read();
            if let Some(file) = guard.get_file(rel)
                && file.content == expected
            {
                return started.elapsed();
            }
        }
        assert!(
            Instant::now() < deadline,
            "byte identity for {rel} was never observed within 30s"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Poll until `rel` is ABSENT from the published index (delete/demotion).
fn observe_absent(shared: &SharedIndex, rel: &str, started: Instant) -> Duration {
    let deadline = started + Duration::from_secs(30);
    loop {
        if shared.read().get_file(rel).is_none() {
            return started.elapsed();
        }
        assert!(
            Instant::now() < deadline,
            "absence of {rel} was never observed within 30s"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

// ── Sample collection for the receipt ──────────────────────────────────────

#[derive(Default)]
struct Samples(Vec<(String, Duration)>);

impl Samples {
    fn record(&mut self, campaign: &str, workload: &str, latency: Duration) {
        self.0.push((format!("{campaign}/{workload}"), latency));
    }

    fn quantiles(&self) -> Vec<(String, u64, u64, u64, usize)> {
        use std::collections::BTreeMap;
        let mut by_case: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
        for (case, latency) in &self.0 {
            by_case
                .entry(case.as_str())
                .or_default()
                .push(latency.as_millis() as u64);
        }
        by_case
            .into_iter()
            .map(|(case, mut millis)| {
                millis.sort_unstable();
                let p50 = millis[millis.len() / 2];
                let p95_rank = ((millis.len() as f64) * 0.95).ceil() as usize;
                let p95 = millis[p95_rank.clamp(1, millis.len()) - 1];
                let max = *millis.last().expect("non-empty");
                (case.to_string(), p50, p95, max, millis.len())
            })
            .collect()
    }
}

// ── The campaigns ──────────────────────────────────────────────────────────

struct WatcherFixture {
    root: tempfile::TempDir,
    shared: SharedIndex,
    stop: Arc<AtomicBool>,
    runtime: tokio::runtime::Runtime,
    watcher: Option<tokio::task::JoinHandle<()>>,
}

impl WatcherFixture {
    fn start() -> Self {
        let root = tempfile::tempdir().expect("root");
        write_corpus(root.path());
        let shared = LiveIndex::load(root.path()).expect("cold load");
        let stop = Arc::new(AtomicBool::new(false));
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("runtime");
        let watcher = {
            let shared = shared.clone();
            let info = Arc::new(parking_lot::Mutex::new(WatcherInfo::default()));
            let stop = Arc::clone(&stop);
            let path = root.path().to_path_buf();
            runtime.spawn(async move {
                run_watcher_with_stop(path, shared, info, stop).await;
            })
        };
        let fixture = Self {
            root,
            shared,
            stop,
            runtime,
            watcher: Some(watcher),
        };
        fixture.quiesce();
        fixture
    }

    /// Quiescence control: prove the observer lane is LIVE before timing —
    /// one untimed write must become visible.
    fn quiesce(&self) {
        let body = corpus_file_body(0, 1);
        let started = Instant::now();
        std::fs::write(self.root.path().join("src/fixture_0.rs"), &body).expect("write");
        observe_bytes(&self.shared, "src/fixture_0.rs", body.as_bytes(), started);
    }

    fn stop(mut self) -> (tempfile::TempDir, SharedIndex) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.watcher.take() {
            let _ = self
                .runtime
                .block_on(async { tokio::time::timeout(Duration::from_secs(10), handle).await });
        }
        (self.root, self.shared)
    }
}

/// The managed-observer lane: real watcher delivery for every workload.
fn campaign_delivered_event(samples: &mut Samples) -> usize {
    let fixture = WatcherFixture::start();
    let root = fixture.root.path().to_path_buf();
    let shared = fixture.shared.clone();
    // T070's no-full-rebuild gate: single-path refreshes never bump the
    // PROJECT generation (only a reload does). Captured here, asserted at
    // campaign end.
    let generation = shared.current_project_generation();
    let mut completions = 0usize;

    // modify
    for revision in 2..7 {
        let body = corpus_file_body(1, revision);
        let started = Instant::now();
        std::fs::write(root.join("src/fixture_1.rs"), &body).expect("write");
        let latency = observe_bytes(&shared, "src/fixture_1.rs", body.as_bytes(), started);
        samples.record("delivered_event", "modify", latency);
        completions += 1;
    }
    // add
    for revision in 0..3 {
        let rel = format!("src/added_{revision}.rs");
        let body = corpus_file_body(900 + revision, revision);
        let started = Instant::now();
        std::fs::write(root.join(&rel), &body).expect("write");
        let latency = observe_bytes(&shared, &rel, body.as_bytes(), started);
        samples.record("delivered_event", "add", latency);
        completions += 1;
    }
    // delete
    {
        let started = Instant::now();
        std::fs::remove_file(root.join("src/added_0.rs")).expect("delete");
        let latency = observe_absent(&shared, "src/added_0.rs", started);
        samples.record("delivered_event", "delete", latency);
        completions += 1;
    }
    // rename (observed as: old identity absent AND new identity present)
    {
        let body = corpus_file_body(901, 1);
        std::fs::write(root.join("src/added_1.rs"), &body).expect("rewrite before rename");
        observe_bytes(&shared, "src/added_1.rs", body.as_bytes(), Instant::now());
        let started = Instant::now();
        std::fs::rename(root.join("src/added_1.rs"), root.join("src/renamed_1.rs"))
            .expect("rename");
        let absent = observe_absent(&shared, "src/added_1.rs", started);
        let present = observe_bytes(&shared, "src/renamed_1.rs", body.as_bytes(), started);
        samples.record("delivered_event", "rename", absent.max(present));
        completions += 1;
    }
    // terminal-classification: growth past the code threshold demotes the
    // file out of the parsed tier — observed as absence from Tier 1.
    {
        let mut grown = corpus_file_body(2, 9).into_bytes();
        grown.resize(TERMINAL_BYTES, b' ');
        let started = Instant::now();
        std::fs::write(root.join("src/fixture_2.rs"), &grown).expect("grow");
        let latency = observe_absent(&shared, "src/fixture_2.rs", started);
        samples.record("delivered_event", "terminal_classification", latency);
        completions += 1;
    }
    // burst: 24 files modified back-to-back, measured to FULL convergence.
    {
        let bodies: Vec<(String, String)> = (10..10 + BURST_FILES)
            .map(|index| {
                (
                    format!("src/fixture_{index}.rs"),
                    corpus_file_body(index, 3),
                )
            })
            .collect();
        let started = Instant::now();
        for (rel, body) in &bodies {
            std::fs::write(root.join(rel), body).expect("burst write");
        }
        let mut worst = Duration::ZERO;
        for (rel, body) in &bodies {
            worst = worst.max(observe_bytes(&shared, rel, body.as_bytes(), started));
        }
        samples.record("delivered_event", "burst_24", worst);
        completions += 1;
    }

    assert_eq!(
        shared.current_project_generation(),
        generation,
        "a delivered-event campaign performed a full rebuild (project          generation moved) outside Gap/ScopeDirty"
    );
    let (_root, shared) = fixture.stop();
    drop(shared);
    completions
}

/// The gap lane: changes land with NO observer; a fresh watcher's mandatory
/// fresh-instance reconciliation must repair every one of them.
fn campaign_need_rescan(samples: &mut Samples) -> usize {
    let root = tempfile::tempdir().expect("root");
    write_corpus(root.path());
    let shared = LiveIndex::load(root.path()).expect("cold load");
    let generation = shared.current_project_generation();

    // The gap: mutate under the observer's feet-to-be.
    let mut expectations = Vec::new();
    for index in 0..6 {
        let rel = format!("src/fixture_{index}.rs");
        let body = corpus_file_body(index, 42);
        std::fs::write(root.path().join(&rel), &body).expect("gap write");
        expectations.push((rel, body));
    }

    let stop = Arc::new(AtomicBool::new(false));
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("runtime");
    let started = Instant::now();
    let handle = {
        let shared = shared.clone();
        let info = Arc::new(parking_lot::Mutex::new(WatcherInfo::default()));
        let stop = Arc::clone(&stop);
        let path = root.path().to_path_buf();
        runtime.spawn(async move {
            run_watcher_with_stop(path, shared, info, stop).await;
        })
    };
    let mut worst = Duration::ZERO;
    for (rel, body) in &expectations {
        worst = worst.max(observe_bytes(&shared, rel, body.as_bytes(), started));
    }
    samples.record("need_rescan", "fresh_instance_rescan", worst);
    assert_eq!(
        shared.current_project_generation(),
        generation,
        "the gap rescan performed a full rebuild instead of repairing in place"
    );
    stop.store(true, Ordering::Release);
    let _ = runtime.block_on(async { tokio::time::timeout(Duration::from_secs(10), handle).await });
    1
}

/// The suppressed-notification lane: no watcher at all; the READ ingress
/// observes staleness itself (freshen-on-read) and the response must carry
/// the fresh identity.
fn campaign_suppressed_notification(samples: &mut Samples) -> usize {
    let root = tempfile::tempdir().expect("root");
    write_corpus(root.path());
    let shared = LiveIndex::load(root.path()).expect("cold load");
    let server = SymForgeServer::new(
        shared.clone(),
        "observed-refresh-gate".to_string(),
        Arc::new(parking_lot::Mutex::new(WatcherInfo::default())),
        Some(root.path().to_path_buf()),
        None,
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let generation = shared.current_project_generation();
    let mut completions = 0usize;
    for revision in 1..6 {
        let body = format!("pub fn fixture_3_symbol() -> u64 {{\n    {revision}00\n}}\n");
        std::fs::write(root.path().join("src/fixture_3.rs"), &body).expect("write");
        // Backdate so the mtime comparison always fires (same-second writes).
        let file = std::fs::File::options()
            .write(true)
            .open(root.path().join("src/fixture_3.rs"))
            .expect("open");
        // A DETERMINISTIC absolute mtime per revision: a now-relative
        // backdate collides across revisions whenever one loop pass
        // stretches past a second boundary (observed as a stale
        // freshen-miss under criterion warm-up), while these values are
        // strictly increasing by construction.
        file.set_times(std::fs::FileTimes::new().set_modified(
            std::time::SystemTime::UNIX_EPOCH
                + Duration::from_secs(1_000_000_000 + revision as u64),
        ))
        .expect("backdate");
        let started = Instant::now();
        // `get_file_content` rides the freshen-on-read lane (the C4c
        // targeted-retrieval freshen); `get_symbol` deliberately serves the
        // captured publication and relies on the observer lanes instead.
        // force_refresh keeps the MEASUREMENT pure: it excludes the
        // repeat-read cache's hit fast-path from the freshen-latency
        // samples, so every sample is a real freshen+serve. (The fence
        // itself HOLDS - freshen runs before the cache key, pinned by
        // tests/session_cache_hit.rs::
        // stale_publication_never_satisfies_the_repeat_read_cache. The
        // stale cache_hit this campaign once flushed out of baseline
        // 1521abb0 was fixture-induced: the original now-relative mtime
        // backdate collided across revisions, the freshen legitimately saw
        // an unchanged mtime, and the cache consistently served the
        // unchanged publication.)
        let response = runtime.block_on(server.dispatch_tool_for_tests(
            "get_file_content",
            serde_json::json!({ "path": "src/fixture_3.rs", "force_refresh": true }),
        ));
        let latency = started.elapsed();
        assert!(
            response.contains(&format!("{revision}00")),
            "the read must observe the fresh identity: {response}"
        );
        observe_bytes(&shared, "src/fixture_3.rs", body.as_bytes(), started);
        samples.record("suppressed_notification", "freshen_on_read", latency);
        completions += 1;
    }
    assert_eq!(
        shared.current_project_generation(),
        generation,
        "freshen-on-read performed a full rebuild instead of a single-path refresh"
    );
    completions
}

/// The embed contract: the synchronous mutation-commit facade.
fn campaign_embed_mutation_commit(samples: &mut Samples) -> usize {
    let root = tempfile::tempdir().expect("root");
    write_corpus(root.path());
    let shared = LiveIndex::load(root.path()).expect("cold load");
    let generation = shared.current_project_generation();
    let mut completions = 0usize;
    for revision in 1..6 {
        let body = corpus_file_body(4, revision * 7);
        std::fs::write(root.path().join("src/fixture_4.rs"), &body).expect("write");
        let started = Instant::now();
        let outcome = update_file_from_disk(&shared, root.path(), "src/fixture_4.rs");
        let latency = observe_bytes(&shared, "src/fixture_4.rs", body.as_bytes(), started);
        samples.record("embed_mutation_commit", "modify", latency);
        assert!(
            format!("{outcome:?}").contains("Reindexed"),
            "the facade commit must report the reindex: {outcome:?}"
        );
        completions += 1;
    }
    assert_eq!(
        shared.current_project_generation(),
        generation,
        "the facade commit performed a full rebuild instead of a single-path refresh"
    );
    completions
}

/// Clean-rebuild equivalence: after incremental refreshes, a from-disk
/// rebuild agrees file-for-file on content hashes.
fn clean_rebuild_equivalence() {
    let root = tempfile::tempdir().expect("root");
    write_corpus(root.path());
    let shared = LiveIndex::load(root.path()).expect("cold load");
    for index in 0..8 {
        let rel = format!("src/fixture_{index}.rs");
        std::fs::write(root.path().join(&rel), corpus_file_body(index, 5)).expect("write");
        let outcome = update_file_from_disk(&shared, root.path(), &rel);
        assert!(format!("{outcome:?}").contains("Reindexed"), "{outcome:?}");
    }
    let incremental: Vec<(String, String)> = {
        let guard = shared.read();
        let mut rows: Vec<(String, String)> = guard
            .all_files()
            .map(|(path, file)| (path.clone(), file.content_hash.clone()))
            .collect();
        rows.sort();
        rows
    };
    let rebuilt = LiveIndex::load(root.path()).expect("clean rebuild");
    let clean: Vec<(String, String)> = {
        let guard = rebuilt.read();
        let mut rows: Vec<(String, String)> = guard
            .all_files()
            .map(|(path, file)| (path.clone(), file.content_hash.clone()))
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(
        incremental, clean,
        "incremental refreshes must equal a clean from-disk rebuild"
    );
}

// ── The registered benchmark ───────────────────────────────────────────────

fn observed_refresh_gate_v1(c: &mut Criterion) {
    // Corpus digest control: the fixture pins the generator's output.
    let fixture: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/observed-refresh-v1/corpus.json"
        ))
        .expect("corpus fixture exists"),
    )
    .expect("corpus fixture parses");
    let scratch = tempfile::tempdir().expect("digest scratch");
    let digest = write_corpus(scratch.path());
    assert_eq!(
        fixture["corpus_digest"].as_str().expect("digest field"),
        digest,
        "the deterministic corpus drifted from its pinned digest"
    );
    drop(scratch);

    let samples = std::sync::Mutex::new(Samples::default());
    let receipts = std::sync::Mutex::new(Vec::<(String, usize)>::new());

    let mut group = c.benchmark_group("observed_refresh_gate_v1");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(1));
    group.bench_function("delivered_event", |b| {
        b.iter_custom(|iterations| {
            let started = Instant::now();
            for _ in 0..iterations {
                let mut local = Samples::default();
                let completions = campaign_delivered_event(&mut local);
                receipts
                    .lock()
                    .expect("receipts")
                    .push(("delivered_event".to_string(), completions));
                samples.lock().expect("samples").0.append(&mut local.0);
            }
            started.elapsed()
        })
    });
    group.bench_function("need_rescan", |b| {
        b.iter_custom(|iterations| {
            let started = Instant::now();
            for _ in 0..iterations {
                let mut local = Samples::default();
                let completions = campaign_need_rescan(&mut local);
                receipts
                    .lock()
                    .expect("receipts")
                    .push(("need_rescan".to_string(), completions));
                samples.lock().expect("samples").0.append(&mut local.0);
            }
            started.elapsed()
        })
    });
    group.bench_function("suppressed_notification", |b| {
        b.iter_custom(|iterations| {
            let started = Instant::now();
            for _ in 0..iterations {
                let mut local = Samples::default();
                let completions = campaign_suppressed_notification(&mut local);
                receipts
                    .lock()
                    .expect("receipts")
                    .push(("suppressed_notification".to_string(), completions));
                samples.lock().expect("samples").0.append(&mut local.0);
            }
            started.elapsed()
        })
    });
    group.bench_function("embed_mutation_commit", |b| {
        b.iter_custom(|iterations| {
            let started = Instant::now();
            for _ in 0..iterations {
                let mut local = Samples::default();
                let completions = campaign_embed_mutation_commit(&mut local);
                receipts
                    .lock()
                    .expect("receipts")
                    .push(("embed_mutation_commit".to_string(), completions));
                samples.lock().expect("samples").0.append(&mut local.0);
            }
            started.elapsed()
        })
    });
    group.finish();

    clean_rebuild_equivalence();

    // ── Capacity conservation measurements (T069/C8) ─────────────────────
    // The same identity the frozen oracle asserts, recorded as receipt data:
    // all four surfaces attach (exhausting the process vector exactly), and
    // a 64-source observation burst is sampled for its retained+candidate
    // peak, convergence, and unknown-refund count.
    let capacity_conservation = {
        use symforge::live_index::index_lifecycle::activation::{
            activate_surface, process_index_runtime, project_source_authority,
        };
        use symforge::live_index::index_lifecycle::process_runtime::SurfaceKind;

        for surface in SurfaceKind::ALL {
            activate_surface(surface);
        }
        let runtime = process_index_runtime();
        let root = tempfile::tempdir().expect("capacity root");
        let lane = project_source_authority(root.path());
        let observer = lane.register_observer();
        let (_, pregranted, _, _) = lane.observation_capacity_ledger();
        let mut peak = 0u64;
        for index in 0..64 {
            lane.observe_admission(observer, &format!("src/burst_{index}.rs"))
                .expect("current incarnation");
            let (candidate, _, _, _) = lane.observation_capacity_ledger();
            let (_, retained) = lane.retained_observation_artifacts();
            peak = peak.max(candidate + retained);
        }
        let (converged_charge, _, outstanding, unknown) = lane.observation_capacity_ledger();
        let (retained_sources, retained_bytes) = lane.retained_observation_artifacts();
        serde_json::json!({
            "surfaces_attached": runtime.attached().len(),
            "process_promisable_after_attach": runtime.available(),
            "lane_pregranted_bytes": pregranted,
            "burst_sources": 64,
            "retained_plus_candidate_peak": peak,
            "converged_candidate_charge": converged_charge,
            "outstanding_charges": outstanding,
            "unknown_refunds": unknown,
            "retained_sources": retained_sources,
            "retained_dark_bytes": retained_bytes,
        })
    };

    // ── The code-owned receipt ────────────────────────────────────────────
    let samples = samples.into_inner().expect("samples");
    let quantiles: Vec<serde_json::Value> = samples
        .quantiles()
        .into_iter()
        .map(|(case, p50, p95, max, count)| {
            serde_json::json!({
                "case": case,
                "p50_ms": p50,
                "p95_ms": p95,
                "max_ms": max,
                "samples": count,
            })
        })
        .collect();
    let completions: Vec<serde_json::Value> = receipts
        .into_inner()
        .expect("receipts")
        .into_iter()
        .map(|(campaign, count)| serde_json::json!({ "campaign": campaign, "completions": count }))
        .collect();
    let receipt = serde_json::json!({
        "kind": "symforge-observed-refresh-gate-v1-receipt",
        "schema_version": 1,
        "corpus_digest": digest,
        "corpus_files": CORPUS_FILES,
        "burst_files": BURST_FILES,
        "host": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        },
        "controls": {
            "single_path_no_full_rebuild": "asserted per campaign: the project generation is stable across every campaign (only a reload bumps it), so no single-path refresh fell back to a full rebuild outside Gap/ScopeDirty",
            "cold_load_before_timing": true,
            "quiescence_probe": "one untimed write observed visible before each watcher campaign",
            "clean_rebuild_equivalence": "asserted (content-hash file-for-file)",
        },
        "capacity_vector": {
            "pre_granted_per_surface_bytes": symforge::live_index::index_lifecycle::activation::OBSERVATION_CAPACITY_BYTES,
            "note": "dark budgets until C7/C8 measurements replace them",
        },
        "capacity_conservation": capacity_conservation,
        "latencies": quantiles,
        "campaign_completions": completions,
    });
    let out_dir = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/target/observed-refresh-gate-v1"
    ));
    std::fs::create_dir_all(&out_dir).expect("receipt dir");
    std::fs::write(
        out_dir.join("receipt.json"),
        serde_json::to_string_pretty(&receipt).expect("receipt serializes"),
    )
    .expect("receipt written");
    eprintln!(
        "observed-refresh-gate-v1 receipt: {}",
        out_dir.join("receipt.json").display()
    );
}

criterion_group!(observed_refresh_gate_v1_group, observed_refresh_gate_v1);
criterion_main!(observed_refresh_gate_v1_group);
