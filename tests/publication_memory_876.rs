//! Issue #876 part 2 measurement: what one single-file publication allocates.
//!
//! Ignored by default; run it on purpose and read the printed table:
//!
//! ```text
//! cargo test --release --test publication_memory_876 -- --ignored --nocapture
//! ```
//!
//! It copies this repository's tracked files (minus `tests/fixtures/`) into a
//! temporary directory, loads an index over the copy, then edits `CHANGES`
//! files one at a time and publishes each through `update_file_from_disk`, the
//! same admission and publication seam the watcher uses. A counting global
//! allocator (this test binary only) records the bytes allocated by each
//! publication and the live heap left behind, while the last `PINNED` published
//! generations are held the way open sessions hold theirs.
//!
//! "Allocated" counts every byte handed out, process-wide (rayon workers
//! included), and a `realloc` counts its full new size, so it is an upper
//! bound on fresh memory requested; "live" is allocated minus freed.
//!
//! `SYMFORGE_876_CHANGES` and `SYMFORGE_876_PINNED` override the defaults.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use symforge::knowledge::SECRET_POLICY_VERSION;
use symforge::live_index::LiveIndex;
use symforge::live_index::knowledge_authority::{
    AuthorityLimits, AuthorityTemporalIndex, build_knowledge_authority, full_authority_builds,
};
use symforge::live_index::knowledge_bridge::{
    BridgeLimits, build_knowledge_bridge, full_bridge_builds,
};
use symforge::live_index::single_file::{ReindexResult, update_file_from_disk};

struct Counting;

static ALLOCATED: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);

// SAFETY: every call forwards to `System` with the caller's own layout and
// pointer; the counters are side bookkeeping and never touch the memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            ALLOCATED.fetch_add(layout.size() as u64, Ordering::Relaxed);
            LIVE.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            ALLOCATED.fetch_add(layout.size() as u64, Ordering::Relaxed);
            LIVE.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            ALLOCATED.fetch_add(new_size as u64, Ordering::Relaxed);
            LIVE.fetch_add(new_size as u64, Ordering::Relaxed);
            LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        }
        new
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// Resident memory: Linux `VmRSS`, Windows working set. `None` when unread.
fn resident_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:"))
            .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            .map(|kib| kib * 1024)
    }
    #[cfg(windows)]
    {
        use windows::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows::Win32::System::Threading::GetCurrentProcess;
        let mut counters = PROCESS_MEMORY_COUNTERS::default();
        let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?;
        // SAFETY: a pseudo-handle needing no close, and a writable struct of `size` bytes.
        unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, size) }.ok()?;
        u64::try_from(counters.WorkingSetSize).ok()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

fn rss_label() -> String {
    resident_bytes().map_or_else(
        || "not measured".to_string(),
        |b| format!("{:.1} MiB", mib(b)),
    )
}

/// Copy the tracked, non-fixture files of this repository under `dest`.
fn copy_corpus(repo: &Path, dest: &Path) -> usize {
    let listed = symforge::process_util::hidden_command("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z"])
        .output()
        .expect("git ls-files");
    assert!(listed.status.success(), "git ls-files failed");
    let mut copied = 0;
    for rel in listed.stdout.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let rel = std::str::from_utf8(rel).expect("utf-8 path");
        if rel.starts_with("tests/fixtures/") {
            continue;
        }
        let from = repo.join(rel);
        if !from.is_file() {
            continue;
        }
        let to = dest.join(rel);
        std::fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
        std::fs::copy(&from, &to).expect("copy");
        copied += 1;
    }
    copied
}

#[test]
#[ignore = "measurement for issue #876; run with --ignored --nocapture"]
fn measure_single_file_publication_allocation() {
    let changes = env_usize("SYMFORGE_876_CHANGES", 20);
    let pinned = env_usize("SYMFORGE_876_PINNED", 4);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let copied = copy_corpus(repo, root);

    let rss_empty = rss_label();
    let shared = LiveIndex::load(root).expect("load corpus");
    let counts = shared.published_generation().memory_counts();
    let live_loaded = LIVE.load(Ordering::Relaxed);
    let rss_loaded = rss_label();

    // The copy on its own, without the rest of publication.
    let before = ALLOCATED.load(Ordering::Relaxed);
    let started = Instant::now();
    let copy = (*shared.read()).clone();
    let clone_time = started.elapsed();
    let clone_bytes = ALLOCATED.load(Ordering::Relaxed) - before;
    drop(copy);

    let mut targets: Vec<String> = shared
        .read()
        .all_files()
        .map(|(path, _)| path)
        .filter(|path| path.ends_with(".rs"))
        .cloned()
        .collect();
    targets.sort();
    let stride = (targets.len() / changes.max(1)).max(1);
    let targets: Vec<String> = targets.into_iter().step_by(stride).take(changes).collect();

    let builds_before = (full_bridge_builds(), full_authority_builds());
    let mut held = VecDeque::new();
    let mut per_change = Vec::new();
    for (i, rel) in targets.iter().enumerate() {
        let path = root.join(rel);
        let mut bytes = std::fs::read(&path).expect("read target");
        bytes.extend_from_slice(format!("\n// issue 876 measurement edit {i}\n").as_bytes());
        std::fs::write(&path, bytes).expect("write target");

        let alloc_before = ALLOCATED.load(Ordering::Relaxed);
        let live_before = LIVE.load(Ordering::Relaxed);
        let started = Instant::now();
        let outcome = update_file_from_disk(&shared, root, rel);
        let elapsed = started.elapsed();
        let allocated = ALLOCATED.load(Ordering::Relaxed) - alloc_before;
        assert!(
            matches!(outcome, ReindexResult::Reindexed),
            "{rel}: {outcome:?}"
        );
        held.push_back(shared.published_generation());
        if held.len() > pinned {
            held.pop_front();
        }
        let retained = LIVE.load(Ordering::Relaxed) as i64 - live_before as i64;
        per_change.push((allocated, elapsed, retained));
    }
    let builds_after_changes = (full_bridge_builds(), full_authority_builds());
    // Same bytes again, newer mtime: the hash-skip lane skips the parse but
    // still publishes a generation, so this separates parse cost from
    // publication cost. The pause moves `mtime_secs` to a new second, without
    // which a hash-skip publishes nothing.
    let mut hash_skip = Vec::new();
    for rel in &targets {
        let path = root.join(rel);
        let bytes = std::fs::read(&path).expect("read target");
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(&path, bytes).expect("rewrite target");
        let published_before = shared.published_generation().publication_generation;
        let alloc_before = ALLOCATED.load(Ordering::Relaxed);
        let started = Instant::now();
        let outcome = update_file_from_disk(&shared, root, rel);
        let elapsed = started.elapsed();
        let allocated = ALLOCATED.load(Ordering::Relaxed) - alloc_before;
        assert!(
            matches!(outcome, ReindexResult::HashSkip),
            "{rel}: {outcome:?}"
        );
        let published = shared.published_generation().publication_generation != published_before;
        hash_skip.push((allocated, elapsed, published));
    }
    let builds_after_skips = (full_bridge_builds(), full_authority_builds());
    // Attribution: rebuild, off to the side, the derived parts a publication
    // builds from the whole index, using the current generation's inputs.
    let generation = shared.published_generation();
    let measure = |label: &str, work: &dyn Fn()| {
        let before = ALLOCATED.load(Ordering::Relaxed);
        let started = Instant::now();
        work();
        println!(
            "part {label}: {:.1} MiB allocated in {:.1} ms",
            mib(ALLOCATED.load(Ordering::Relaxed) - before),
            started.elapsed().as_secs_f64() * 1e3
        );
    };
    measure("repo outline", &|| {
        let _ = generation.live.capture_repo_outline_view();
    });
    if let (Some(source), Some(version)) = (
        generation.source.as_deref(),
        generation.source_version.as_deref(),
    ) {
        measure("knowledge bridge", &|| {
            let _ = build_knowledge_bridge(
                &generation.live,
                source,
                generation.content_generation,
                &BridgeLimits::default(),
            );
        });
        measure("authority temporal index", &|| {
            let _ = AuthorityTemporalIndex::from_published(&generation);
        });
        let temporal = AuthorityTemporalIndex::from_published(&generation);
        measure("knowledge authority", &|| {
            let _ = build_knowledge_authority(
                &generation.live,
                source,
                version,
                generation.content_generation,
                &generation.bridge,
                &temporal,
                SECRET_POLICY_VERSION,
                &AuthorityLimits::default(),
            );
        });
    } else {
        println!("part knowledge: generation has no source identity, not measured");
    }
    drop(generation);
    let live_pinned = LIVE.load(Ordering::Relaxed);
    let rss_pinned = rss_label();
    drop(held);
    let live_unpinned = LIVE.load(Ordering::Relaxed);
    let rss_unpinned = rss_label();
    let trim = malloc_trim_report();
    // Unload: the whole index goes, as when the daemon drops an idle project.
    drop(shared);
    let live_unloaded = LIVE.load(Ordering::Relaxed);
    let rss_unloaded = rss_label();
    let unload_trim = malloc_trim_report();

    let n = per_change.len().max(1) as u64;
    let total_alloc: u64 = per_change.iter().map(|(a, _, _)| a).sum();
    let total_time: Duration = per_change.iter().map(|(_, t, _)| *t).sum();
    let mut times: Vec<Duration> = per_change.iter().map(|(_, t, _)| *t).collect();
    times.sort();

    println!("── issue #876 publication measurement ──");
    println!(
        "os={} corpus_files_copied={copied} MALLOC_ARENA_MAX={}",
        std::env::consts::OS,
        std::env::var("MALLOC_ARENA_MAX").unwrap_or_else(|_| "unset".to_string())
    );
    println!(
        "index: files={} content={:.1} MiB references={} trigram_entries={}",
        counts.files,
        mib(counts.content_bytes as u64),
        counts.references,
        counts.trigram_entries
    );
    println!(
        "LiveIndex clone alone: {:.1} MiB allocated in {:.1} ms ({:.0}% of content)",
        mib(clone_bytes),
        clone_time.as_secs_f64() * 1e3,
        100.0 * clone_bytes as f64 / counts.content_bytes.max(1) as f64
    );
    println!(
        "per publication (n={}): mean {:.1} MiB allocated ({:.0}% of content), \
         mean {:.1} ms, median {:.1} ms; clone share of allocation {:.0}%",
        per_change.len(),
        mib(total_alloc / n),
        100.0 * (total_alloc / n) as f64 / counts.content_bytes.max(1) as f64,
        total_time.as_secs_f64() * 1e3 / n as f64,
        times[times.len() / 2].as_secs_f64() * 1e3,
        100.0 * clone_bytes as f64 / (total_alloc / n).max(1) as f64
    );
    for (i, (alloc, time, retained)) in per_change.iter().enumerate() {
        println!(
            "  change {i:>3}: allocated {:>7.1} MiB  time {:>7.1} ms  live delta {:>7.1} MiB",
            mib(*alloc),
            time.as_secs_f64() * 1e3,
            *retained as f64 / (1024.0 * 1024.0)
        );
    }
    println!(
        "full knowledge builds: {} bridge and {} authority over {} changes;          {} bridge and {} authority over {} hash-skips",
        builds_after_changes.0 - builds_before.0,
        builds_after_changes.1 - builds_before.1,
        per_change.len(),
        builds_after_skips.0 - builds_after_changes.0,
        builds_after_skips.1 - builds_after_changes.1,
        hash_skip.len()
    );
    let skips = hash_skip.len().max(1) as u64;
    println!(
        "hash-skip publication (n={}, {} published a generation): mean {:.1} MiB allocated, \
         mean {:.1} ms",
        hash_skip.len(),
        hash_skip
            .iter()
            .filter(|(_, _, published)| *published)
            .count(),
        mib(hash_skip.iter().map(|(a, _, _)| a).sum::<u64>() / skips),
        hash_skip
            .iter()
            .map(|(_, t, _)| t.as_secs_f64() * 1e3)
            .sum::<f64>()
            / skips as f64
    );
    println!(
        "live heap: loaded {:.1} MiB, with {pinned} pinned generations {:.1} MiB, unpinned {:.1} MiB",
        mib(live_loaded),
        mib(live_pinned),
        mib(live_unpinned)
    );
    println!(
        "resident: before load {rss_empty}, loaded {rss_loaded}, \
         with {pinned} pinned {rss_pinned}, unpinned {rss_unpinned}"
    );
    println!("after releasing the pinned generations, {trim}");
    println!(
        "after unloading the index: live heap {:.1} MiB, resident {rss_unloaded}; {unload_trim}",
        mib(live_unloaded)
    );
}

/// `malloc_trim(0)` once, after the pinned generations are released: the
/// resident memory it hands back and what it costs. glibc only.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn malloc_trim_report() -> String {
    let before = rss_label();
    let started = Instant::now();
    // SAFETY: malloc_trim takes a pad size and touches only allocator state.
    let released = unsafe { libc::malloc_trim(0) };
    format!(
        "malloc_trim(0): resident {before} -> {} in {:.1} ms (released={released})",
        rss_label(),
        started.elapsed().as_secs_f64() * 1e3
    )
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn malloc_trim_report() -> String {
    "malloc_trim(0): not measured (glibc only)".to_string()
}
