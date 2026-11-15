//! Issue #876: resident memory and timings of the daemon's publication
//! workload under a real global allocator.
//!
//! `tests/publication_memory_876.rs` installs a counting allocator, so it can
//! not measure another allocator. This program runs the same workload with
//! whichever allocator its features select: the system allocator by default,
//! `--features mimalloc`, or `--features jemalloc` (not on MSVC).
//!
//! Workload: copy this repository's tracked files (minus `tests/fixtures/`),
//! load an index, publish `ALLOC876_CHANGES` single-file edits through the
//! watcher's update seam holding the last `ALLOC876_PINNED` generations,
//! release them, then drop the index. `ALLOC876_TRIM=1` calls `malloc_trim(0)`
//! after the release and after the unload (glibc system allocator only).
//! Allocator options are set by their own environment variables
//! (`MIMALLOC_*`, `_RJEM_MALLOC_CONF`, `GLIBC_TUNABLES`) so the binary is the
//! same for every configuration of one allocator.
#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::path::Path;
use std::time::{Duration, Instant};

use symforge::live_index::LiveIndex;
use symforge::live_index::single_file::{ReindexResult, update_file_from_disk};

#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc_safe::MiMalloc = mimalloc_safe::MiMalloc;

#[cfg(all(feature = "jemalloc", not(target_env = "msvc")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

const ALLOCATOR: &str = if cfg!(feature = "mimalloc") {
    "mimalloc"
} else if cfg!(all(feature = "jemalloc", not(target_env = "msvc"))) {
    "jemalloc"
} else {
    "system"
};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// (resident, peak resident) in bytes: Linux `VmRSS`/`VmHWM`, Windows working set.
fn memory() -> Option<(u64, u64)> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let field = |name: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
                .map(|kib| kib * 1024)
        };
        Some((field("VmRSS:")?, field("VmHWM:")?))
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
        Some((
            u64::try_from(counters.WorkingSetSize).ok()?,
            u64::try_from(counters.PeakWorkingSetSize).ok()?,
        ))
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

fn rss() -> String {
    memory().map_or_else(
        || "unmeasured".to_string(),
        |(rss, _)| format!("{:.0}", rss as f64 / 1048576.0),
    )
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn trim() -> bool {
    // SAFETY: malloc_trim takes a pad size and touches only allocator state.
    unsafe { libc::malloc_trim(0) };
    true
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn trim() -> bool {
    false
}

/// Copy the tracked, non-fixture files of `repo` under `dest`.
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
        let from = repo.join(rel);
        if rel.starts_with("tests/fixtures/") || !from.is_file() {
            continue;
        }
        let to = dest.join(rel);
        std::fs::create_dir_all(to.parent().expect("parent")).expect("mkdir");
        std::fs::copy(&from, &to).expect("copy");
        copied += 1;
    }
    copied
}

fn main() {
    let changes = env_usize("ALLOC876_CHANGES", 20);
    let pinned = env_usize("ALLOC876_PINNED", 4);
    let want_trim = std::env::var("ALLOC876_TRIM").is_ok_and(|v| v == "1");
    // Idle wait before a "settled" reading, so time-based purging can run.
    let settle = Duration::from_secs(env_usize("ALLOC876_SETTLE_SECS", 5) as u64);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let copied = copy_corpus(&repo, root);

    let rss_start = rss();
    let started = Instant::now();
    let shared = LiveIndex::load(root).expect("load corpus");
    let load_ms = started.elapsed().as_secs_f64() * 1e3;
    let rss_loaded = rss();
    std::thread::sleep(settle);
    let rss_loaded_settled = rss();

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

    let mut held = VecDeque::new();
    let mut times = Vec::new();
    for (i, rel) in targets.iter().enumerate() {
        let path = root.join(rel);
        let mut bytes = std::fs::read(&path).expect("read target");
        bytes.extend_from_slice(format!("\n// issue 876 allocator edit {i}\n").as_bytes());
        std::fs::write(&path, bytes).expect("write target");
        let started = Instant::now();
        let outcome = update_file_from_disk(&shared, root, rel);
        times.push(started.elapsed().as_secs_f64() * 1e3);
        assert!(
            matches!(outcome, ReindexResult::Reindexed),
            "{rel}: {outcome:?}"
        );
        held.push_back(shared.published_generation());
        if held.len() > pinned {
            held.pop_front();
        }
    }
    let rss_pinned = rss();
    drop(held);
    let rss_released = rss();
    std::thread::sleep(settle);
    let rss_released_settled = rss();
    let rss_released_trim = if want_trim && trim() {
        rss()
    } else {
        "-".to_string()
    };
    drop(shared);
    let rss_unloaded = rss();
    std::thread::sleep(settle);
    let rss_unloaded_settled = rss();
    let rss_unloaded_trim = if want_trim && trim() {
        rss()
    } else {
        "-".to_string()
    };
    let peak = memory().map_or_else(
        || "unmeasured".to_string(),
        |(_, peak)| format!("{:.0}", peak as f64 / 1048576.0),
    );

    let mean = times.iter().sum::<f64>() / times.len().max(1) as f64;
    times.sort_by(f64::total_cmp);
    let median = times.get(times.len() / 2).copied().unwrap_or(0.0);
    println!(
        "allocator={ALLOCATOR} os={} files={copied} changes={} pinned={pinned} trim={want_trim} \
         settle={}s GLIBC_TUNABLES={} MIMALLOC_PURGE_DELAY={} _RJEM_MALLOC_CONF={}",
        std::env::consts::OS,
        times.len(),
        settle.as_secs(),
        std::env::var("GLIBC_TUNABLES").unwrap_or_default(),
        std::env::var("MIMALLOC_PURGE_DELAY").unwrap_or_default(),
        std::env::var("_RJEM_MALLOC_CONF").unwrap_or_default(),
    );
    println!(
        "RESULT|{ALLOCATOR}|start {rss_start}|loaded {rss_loaded}|loaded+settle {rss_loaded_settled}|pinned {rss_pinned}\
         |released {rss_released}|released+settle {rss_released_settled}|released+trim {rss_released_trim}\
         |unloaded {rss_unloaded}|unloaded+settle {rss_unloaded_settled}|unloaded+trim {rss_unloaded_trim}\
         |peak {peak}|load_ms {load_ms:.0}|edit_mean_ms {mean:.0}|edit_median_ms {median:.0}"
    );
}
