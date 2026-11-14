//! Pipeline profile: where load, single-file updates and queries spend time.
//!
//! Ignored by default. Debug timings are meaningless; run it in release:
//!
//! ```text
//! cargo test --release --test pipeline_profile -- --ignored --nocapture
//! ```
//!
//! It copies this repository's tracked files (minus `tests/fixtures/`, as
//! `publication_memory_876.rs` does) into a temporary directory and prints:
//!
//! * A: `LiveIndex::load` wall time, peak heap and peak working set, the
//!   load's own phase log line, and each phase re-run alone from the test.
//! * B: watcher-seam (`update_file_from_disk`) single-file updates on a small,
//!   a mid and the largest Rust file, for a comment append and a body edit,
//!   with test-side replicas of each part and the knowledge rebuild counters.
//! * C: query timings on the loaded index.
//! * D: what-if micro-measurements (incremental parse, parser reuse, retained
//!   tree bytes, serial versus parallel walk, serial versus parallel scan).
//!
//! Phases timed "alone" are replicas run from the test with the same public
//! functions or the same dependencies; each is labelled. Numbers labelled
//! "derived" are differences of two measured numbers, not measurements.
//!
//! `SYMFORGE_PROFILE_BIG_ROOT` names an extra checkout for the D2 walk
//! (read-only; only its file count is printed). `SYMFORGE_PROFILE_REPS`
//! overrides the B repetition count (default 10). `SYMFORGE_PROFILE_PREWARM=1`
//! first times, on separate fresh copies, the first versus second open of
//! freshly written files (serial read, parallel read, scout) and the first
//! versus second parse pass, so a cold first load can be split into OS
//! first-open cost and code cost.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{BTreeSet, HashMap};
use std::ffi::c_void;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rayon::prelude::*;
use symforge::discovery::{SourceExclusions, discover_all_files, scout_repository_with_exclusions};
use symforge::domain::LanguageId;
use symforge::knowledge::SECRET_POLICY_VERSION;
use symforge::live_index::knowledge_authority::{
    AuthorityLimits, AuthorityTemporalIndex, build_knowledge_authority, full_authority_builds,
};
use symforge::live_index::knowledge_bridge::{
    BridgeLimits, BridgeResolution, CodeAnchorId, build_knowledge_bridge, full_bridge_builds,
    partial_bridge_updates,
};
use symforge::live_index::search::{
    TextSearchOptions, search_structural, search_symbols, search_text,
};
use symforge::live_index::single_file::{ReindexResult, update_file_from_disk};
use symforge::live_index::trigram::TrigramIndex;
use symforge::live_index::{IndexedFile, LiveIndex, SharedIndex};

// ── counting allocators ─────────────────────────────────────────────────────

struct Counting;

static ALLOCATED: AtomicU64 = AtomicU64::new(0);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);
/// Live bytes held by tree-sitter's C allocations (trees, parsers, cursors).
static TS_LIVE: AtomicU64 = AtomicU64::new(0);

fn note_alloc(size: usize) {
    ALLOCATED.fetch_add(size as u64, Ordering::Relaxed);
    let live = LIVE.fetch_add(size as u64, Ordering::Relaxed) + size as u64;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

// SAFETY: every call forwards to `System` with the caller's own layout and
// pointer; the counters are side bookkeeping and never touch the memory.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            note_alloc(layout.size());
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
            note_alloc(layout.size());
        }
        ptr
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            note_alloc(new_size);
            LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        }
        new
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

// tree-sitter allocates through C `malloc`, which a Rust global allocator never
// sees. These shims route it through `GLOBAL` (so the counters above include
// it) with a 16-byte size header, because C `free` carries no size.
const TS_HEADER: usize = 16;

fn ts_layout(size: usize) -> Option<Layout> {
    Layout::from_size_align(size.checked_add(TS_HEADER)?, TS_HEADER).ok()
}

unsafe extern "C" fn ts_malloc(size: usize) -> *mut c_void {
    let Some(layout) = ts_layout(size) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `layout` has a non-zero size; the header is written inside it.
    unsafe {
        let base = std::alloc::alloc(layout);
        if base.is_null() {
            return std::ptr::null_mut();
        }
        base.cast::<usize>().write(size);
        TS_LIVE.fetch_add(size as u64, Ordering::Relaxed);
        base.add(TS_HEADER).cast()
    }
}

unsafe extern "C" fn ts_calloc(count: usize, size: usize) -> *mut c_void {
    let Some(total) = count.checked_mul(size) else {
        return std::ptr::null_mut();
    };
    let Some(layout) = ts_layout(total) else {
        return std::ptr::null_mut();
    };
    // SAFETY: as `ts_malloc`, zeroed.
    unsafe {
        let base = std::alloc::alloc_zeroed(layout);
        if base.is_null() {
            return std::ptr::null_mut();
        }
        base.cast::<usize>().write(total);
        TS_LIVE.fetch_add(total as u64, Ordering::Relaxed);
        base.add(TS_HEADER).cast()
    }
}

unsafe extern "C" fn ts_realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    if ptr.is_null() {
        // SAFETY: realloc(NULL, n) is malloc(n).
        return unsafe { ts_malloc(size) };
    }
    let Some(new_layout) = ts_layout(size) else {
        return std::ptr::null_mut();
    };
    // SAFETY: `ptr` came from these shims, so its header sits `TS_HEADER`
    // bytes before it and records the size the block was allocated with.
    unsafe {
        let base = ptr.cast::<u8>().sub(TS_HEADER);
        let old = base.cast::<usize>().read();
        let old_layout = Layout::from_size_align_unchecked(old + TS_HEADER, TS_HEADER);
        let new = std::alloc::realloc(base, old_layout, new_layout.size());
        if new.is_null() {
            return std::ptr::null_mut();
        }
        new.cast::<usize>().write(size);
        TS_LIVE.fetch_add(size as u64, Ordering::Relaxed);
        TS_LIVE.fetch_sub(old as u64, Ordering::Relaxed);
        new.add(TS_HEADER).cast()
    }
}

unsafe extern "C" fn ts_free(ptr: *mut c_void) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: as `ts_realloc`: the header records the allocation size.
    unsafe {
        let base = ptr.cast::<u8>().sub(TS_HEADER);
        let old = base.cast::<usize>().read();
        TS_LIVE.fetch_sub(old as u64, Ordering::Relaxed);
        std::alloc::dealloc(
            base,
            Layout::from_size_align_unchecked(old + TS_HEADER, TS_HEADER),
        );
    }
}

// ── small helpers ───────────────────────────────────────────────────────────

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

/// (mean, median) of millisecond samples.
fn stats(samples: &[f64]) -> (f64, f64) {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / sorted.len().max(1) as f64;
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
    (mean, median)
}

fn time<T>(work: impl FnOnce() -> T) -> (T, f64) {
    let started = Instant::now();
    let value = work();
    (value, ms(started.elapsed()))
}

/// (current, peak) working set on Windows, VmRSS/VmHWM on Linux.
fn resident() -> Option<(u64, u64)> {
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

fn resident_label() -> String {
    resident().map_or_else(
        || "not measured".to_string(),
        |(now, peak)| format!("{:.1} MiB now, {:.1} MiB process peak", mib(now), mib(peak)),
    )
}

/// Copy the tracked, non-fixture files of this repository under `dest`.
fn copy_corpus(repo: &Path, dest: &Path) -> (usize, u64) {
    let listed = symforge::process_util::hidden_command("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z"])
        .output()
        .expect("git ls-files");
    assert!(listed.status.success(), "git ls-files failed");
    let (mut copied, mut bytes) = (0, 0);
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
        bytes += std::fs::copy(&from, &to).expect("copy");
        copied += 1;
    }
    (copied, bytes)
}

/// The production grammar mapping (`parsing::tree_sitter_language`, private).
fn ts_language(language: LanguageId, tsx: bool) -> Option<tree_sitter::Language> {
    Some(match language {
        LanguageId::Rust => tree_sitter_rust::LANGUAGE.into(),
        LanguageId::Python => tree_sitter_python::LANGUAGE.into(),
        LanguageId::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        LanguageId::TypeScript if tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        LanguageId::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        LanguageId::Go => tree_sitter_go::LANGUAGE.into(),
        LanguageId::Java => tree_sitter_java::LANGUAGE.into(),
        LanguageId::C => tree_sitter_c::LANGUAGE.into(),
        LanguageId::Cpp => tree_sitter_cpp::LANGUAGE.into(),
        LanguageId::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        LanguageId::Ruby => tree_sitter_ruby::LANGUAGE.into(),
        LanguageId::Php => tree_sitter_php::LANGUAGE_PHP.into(),
        LanguageId::Swift => tree_sitter_swift::LANGUAGE.into(),
        LanguageId::Perl => tree_sitter_perl::LANGUAGE.into(),
        LanguageId::Kotlin => tree_sitter_kotlin_sg::LANGUAGE.into(),
        LanguageId::Dart => tree_sitter_dart::LANGUAGE.into(),
        LanguageId::Elixir => tree_sitter_elixir::LANGUAGE.into(),
        LanguageId::Html => tree_sitter_html::LANGUAGE.into(),
        LanguageId::Css => tree_sitter_css::LANGUAGE.into(),
        LanguageId::Scss => tree_sitter_scss::language(),
        _ => return None,
    })
}

/// What production does per file: a NEW parser, then one full parse.
fn fresh_parse(language: &tree_sitter::Language, source: &str) -> tree_sitter::Tree {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(language).expect("set language");
    parser.parse(source, None).expect("parse")
}

/// Log lines `LiveIndex::load` and publication emit, captured verbatim.
#[derive(Clone)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("capture lock")
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn wait_for_knowledge(shared: &SharedIndex) -> f64 {
    let started = Instant::now();
    while format!("{:?}", shared.published_generation().bridge.coverage).contains("Loading") {
        assert!(
            started.elapsed() < Duration::from_secs(300),
            "knowledge never settled"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    ms(started.elapsed())
}

struct CorpusFile {
    rel: String,
    language: LanguageId,
    bytes: Vec<u8>,
}

#[test]
#[ignore = "pipeline profile; run with --release -- --ignored --nocapture"]
fn profile_pipeline() {
    // SAFETY: installed before any tree-sitter object exists in this process
    // (this binary holds one test), so no block is freed by the wrong allocator.
    unsafe {
        tree_sitter::set_allocator(Some(tree_sitter::Allocator {
            malloc: ts_malloc,
            calloc: ts_calloc,
            realloc: ts_realloc,
            free: ts_free,
        }));
    }
    let captured = Capture(Arc::new(Mutex::new(Vec::new())));
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("symforge=info"))
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("install log capture");

    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let (copied, copied_bytes) = copy_corpus(repo, root);
    println!("── pipeline profile ──");
    println!(
        "os={} threads(rayon default)={} corpus: {copied} files, {:.1} MiB",
        std::env::consts::OS,
        rayon::current_num_threads(),
        mib(copied_bytes)
    );

    // Opt-in: split the first load's one-time cost before loading. Leaves the
    // load corpus unread, so its fresh-file read cost stays in the load.
    let _probe_dir = std::env::var_os("SYMFORGE_PROFILE_PREWARM").map(|_| prewarm_probe(repo));

    // ── A: first load ───────────────────────────────────────────────────────
    let resident_before = resident_label();
    let live_before = LIVE.load(Ordering::Relaxed);
    PEAK.store(live_before, Ordering::Relaxed);
    let builds_before = (full_bridge_builds(), full_authority_builds());
    let (shared, load_ms) = time(|| LiveIndex::load(root).expect("load corpus"));
    let settle_ms = wait_for_knowledge(&shared);
    let load_peak_heap = PEAK.load(Ordering::Relaxed) - live_before;
    let load_live = LIVE.load(Ordering::Relaxed) - live_before;
    let resident_loaded = resident_label();
    let builds_load = (
        full_bridge_builds() - builds_before.0,
        full_authority_builds() - builds_before.1,
    );
    let load_log = String::from_utf8_lossy(&captured.0.lock().expect("capture lock")).into_owned();
    captured.0.lock().expect("capture lock").clear();
    let counts = shared.published_generation().memory_counts();

    println!("\n== A. first load ==");
    println!(
        "LiveIndex::load wall {load_ms:.1} ms; knowledge settled {settle_ms:.1} ms later; \
         full builds during load: bridge {} authority {}",
        builds_load.0, builds_load.1
    );
    println!(
        "index: files={} content={:.1} MiB references={} trigram_entries={}",
        counts.files,
        mib(counts.content_bytes as u64),
        counts.references,
        counts.trigram_entries
    );
    println!(
        "heap during load: peak {:.1} MiB above pre-load, {:.1} MiB live after (Rust + tree-sitter C)",
        mib(load_peak_heap),
        mib(load_live)
    );
    println!("resident: before load {resident_before}; after load {resident_loaded}");
    println!("production phase log (verbatim):");
    for line in load_log.lines().filter(|line| {
        [
            "LiveIndex loaded",
            "admission + parse",
            "trigram",
            "derived",
            "bridge",
            "scouted",
        ]
        .iter()
        .any(|needle| line.contains(needle))
    }) {
        let message = line.find("store:").map_or(line, |at| &line[at..]);
        println!("  {message}");
    }

    // Phases alone, serial unless stated (one core; sums of per-file costs).
    println!("phases alone (test-side replicas, warm OS cache):");
    let exclusions = SourceExclusions::default();
    let mut walk = Vec::new();
    let mut scout = Vec::new();
    for _ in 0..5 {
        walk.push(time(|| discover_all_files(root).expect("walk")).1);
        scout.push(time(|| scout_repository_with_exclusions(root, &exclusions).expect("scout")).1);
    }
    let entries = discover_all_files(root).expect("walk");
    println!(
        "  walk + ignore filter (discover_all_files, {} entries): median {:.1} ms",
        entries.len(),
        stats(&walk).1
    );
    println!(
        "  scout (walk + metadata + binary probe, scout_repository_with_exclusions): median {:.1} ms",
        stats(&scout).1
    );
    let (corpus, read_ms) = time(|| {
        entries
            .iter()
            .filter_map(|entry| {
                Some(CorpusFile {
                    rel: entry.relative_path.clone(),
                    language: entry.language?,
                    bytes: std::fs::read(&entry.absolute_path).ok()?,
                })
            })
            .collect::<Vec<_>>()
    });
    let corpus_bytes: u64 = corpus.iter().map(|f| f.bytes.len() as u64).sum();
    println!(
        "  file reads (std::fs::read, {} recognized files, {:.1} MiB): {read_ms:.1} ms",
        corpus.len(),
        mib(corpus_bytes)
    );
    let (_, hash_ms) = time(|| {
        for file in &corpus {
            std::hint::black_box(symforge::hash::digest_hex(&file.bytes));
        }
    });
    println!("  content hash (hash::digest_hex): {hash_ms:.1} ms");
    let (mut ts_ms, mut ts_files, mut pf_ts_ms, mut pf_other_ms) = (0.0, 0, 0.0, 0.0);
    let mut per_language: HashMap<String, (usize, f64)> = HashMap::new();
    for file in &corpus {
        let grammar = ts_language(file.language, file.rel.ends_with(".tsx"));
        let source = String::from_utf8_lossy(&file.bytes);
        if let Some(grammar) = &grammar {
            let (tree, parse) = time(|| fresh_parse(grammar, &source));
            drop(tree);
            ts_ms += parse;
            ts_files += 1;
        }
        let (_, processed) = time(|| {
            std::hint::black_box(symforge::parsing::process_file(
                &file.rel,
                &file.bytes,
                file.language,
            ))
        });
        let slot = per_language
            .entry(format!("{:?}", file.language))
            .or_default();
        slot.0 += 1;
        slot.1 += processed;
        if grammar.is_some() {
            pf_ts_ms += processed;
        } else {
            pf_other_ms += processed;
        }
    }
    println!(
        "  tree-sitter parse alone (new Parser per file, {ts_files} files): {ts_ms:.1} ms serial"
    );
    println!(
        "  process_file (parse + extract + hash) on those files: {pf_ts_ms:.1} ms serial; \
         derived extraction+rest = {:.1} ms",
        pf_ts_ms - ts_ms
    );
    println!(
        "  process_file on non-tree-sitter files (config/markdown/text extractors): {pf_other_ms:.1} ms serial"
    );
    let mut languages: Vec<_> = per_language.into_iter().collect();
    languages.sort_by(|a, b| b.1.1.total_cmp(&a.1.1));
    let top: Vec<String> = languages
        .iter()
        .take(6)
        .map(|(lang, (n, t))| format!("{lang} {n} files {t:.0} ms"))
        .collect();
    println!("  process_file by language: {}", top.join(", "));
    let (_, par_ms) = time(|| {
        corpus.par_iter().for_each(|file| {
            std::hint::black_box(symforge::parsing::process_file(
                &file.rel,
                &file.bytes,
                file.language,
            ));
        });
    });
    println!(
        "  process_file over rayon par_iter (all recognized files): {par_ms:.1} ms wall \
         (serial sum {:.1} ms)",
        pf_ts_ms + pf_other_ms
    );
    admission_replicas(root, &entries, &corpus);
    let (second, second_ms) = time(|| LiveIndex::load(root).expect("second load"));
    drop(second);
    println!("  LiveIndex::load again, same process, files already opened once: {second_ms:.1} ms");
    for line in String::from_utf8_lossy(&captured.0.lock().expect("capture lock"))
        .lines()
        .filter(|line| line.contains("LiveIndex loaded") || line.contains("index publication"))
    {
        let message = line.find("store:").map_or(line, |at| &line[at..]);
        println!("    {message}");
    }
    let loaded = shared.read();
    let files_map: HashMap<String, Arc<IndexedFile>> = loaded
        .all_files()
        .map(|(path, file)| (path.clone(), Arc::new(file.clone())))
        .collect();
    let mut trigram_samples = Vec::new();
    for _ in 0..3 {
        trigram_samples
            .push(time(|| std::hint::black_box(TrigramIndex::build_from_files(&files_map))).1);
    }
    let trigram = TrigramIndex::build_from_files(&files_map);
    println!(
        "  trigram build (TrigramIndex::build_from_files): median {:.1} ms",
        stats(&trigram_samples).1
    );
    let (_, outline_ms) = time(|| std::hint::black_box(loaded.capture_repo_outline_view()));
    println!("  repo outline capture: {outline_ms:.1} ms");
    let (bridge_ms, authority_ms) = measure_knowledge(&shared);
    println!(
        "  knowledge bridge full build: {bridge_ms:.1} ms; knowledge authority full build: {authority_ms:.1} ms"
    );
    println!(
        "  reverse-reference + path indices: not callable from outside \
         (pub(crate)); see 'derived indices' in the production log line above"
    );
    drop(loaded);

    // ── B: single-file updates ──────────────────────────────────────────────
    let reps = std::env::var("SYMFORGE_PROFILE_REPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(10usize);
    let mut rust_files: Vec<(String, usize)> = corpus
        .iter()
        .filter(|f| f.language == LanguageId::Rust && f.rel.starts_with("src/"))
        .map(|f| {
            (
                f.rel.clone(),
                f.bytes.iter().filter(|b| **b == b'\n').count(),
            )
        })
        .collect();
    rust_files.sort();
    let nearest = |target: usize| {
        rust_files
            .iter()
            .filter(|(_, lines)| *lines > 0)
            .min_by_key(|(_, lines)| lines.abs_diff(target))
            .expect("rust file")
            .clone()
    };
    let largest = rust_files
        .iter()
        .max_by_key(|(_, lines)| *lines)
        .expect("rust file")
        .clone();
    let targets = [
        ("small", nearest(100)),
        ("mid", nearest(1500)),
        ("largest", largest),
    ];

    println!("\n== B. single-file update through update_file_from_disk (n={reps} each) ==");
    println!(
        "columns: mean/median ms. total = whole call; parts are replicas on the same bytes; \
         residual (derived) = total - read - hash - process_file - clone - update - outline"
    );
    // Replica of the bridge's symbol selectors: every inline code span in a
    // Markdown file, `()` stripped, last `::` segment (knowledge_bridge.rs
    // `extract_candidates` inline-code lane and `normalize_symbol_selector`).
    let docs: BTreeSet<String> = shared
        .read()
        .all_files()
        .filter(|(path, _)| path.ends_with(".md"))
        .flat_map(|(_, file)| {
            let text = String::from_utf8_lossy(&file.content).into_owned();
            text.split('`')
                .skip(1)
                .step_by(2)
                .map(|span| {
                    let span = span.strip_suffix("()").unwrap_or(span);
                    span.rsplit("::").next().unwrap_or(span).to_string()
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for (label, (rel, lines)) in &targets {
        for kind in [EditKind::Comment, EditKind::Body] {
            profile_updates(&shared, root, label, rel, *lines, kind, reps, &docs);
        }
    }
    // Control: a file no knowledge link resolves to and none of whose symbol
    // names is a selector, so the carry should hold.
    let linked = resolved_link_targets(&shared);
    let doc_named = rust_files
        .iter()
        .filter(|(rel, _)| {
            shared
                .read()
                .get_file(rel)
                .is_some_and(|file| file.symbols.iter().any(|s| docs.contains(&s.name)))
        })
        .count();
    println!(
        "Rust files under src/ that define a doc-named symbol (a static count, not a \
         measurement of rebuilds): {doc_named} of {}",
        rust_files.len()
    );
    let control = rust_files.iter().find(|(rel, lines)| {
        (50..=400).contains(lines)
            && !linked.contains(rel)
            && shared.read().get_file(rel).is_some_and(|file| {
                file.symbols
                    .iter()
                    .all(|symbol| !docs.contains(&symbol.name))
            })
    });
    match control {
        Some((rel, lines)) => {
            profile_updates(
                &shared,
                root,
                "control",
                rel,
                *lines,
                EditKind::Comment,
                reps,
                &docs,
            );
        }
        None => println!("control: no file without any doc-named symbol found"),
    }
    let publication_log =
        String::from_utf8_lossy(&captured.0.lock().expect("capture lock")).into_owned();
    let deferred = publication_log
        .lines()
        .filter(|line| line.contains("deferred"))
        .count();
    println!("publication log lines mentioning a deferred bridge during B: {deferred}");

    // ── C: queries ─────────────────────────────────────────────────────────
    println!("\n== C. queries on the loaded index (n=20 each; all serial loops, see report) ==");
    let index = shared.read();
    let query = |label: &str, work: &dyn Fn() -> usize| {
        let mut samples = Vec::new();
        let mut hits = 0;
        for _ in 0..20 {
            let (found, elapsed) = time(work);
            hits = found;
            samples.push(elapsed);
        }
        let (mean, median) = stats(&samples);
        println!("  {label:<58} mean {mean:>7.2} ms  median {median:>7.2} ms  ({hits} results)");
    };
    query("search_text literal common ('index')", &|| {
        search_text(&index, Some("index"), None, false)
            .map(|r| r.total_matches)
            .unwrap_or(0)
    });
    query("search_text literal rare ('ponytail')", &|| {
        search_text(&index, Some("ponytail"), None, false)
            .map(|r| r.total_matches)
            .unwrap_or(0)
    });
    query("search_text regex ('fn\\s+\\w+_with_options')", &|| {
        search_text(&index, Some(r"fn\s+\w+_with_options"), None, true)
            .map(|r| r.total_matches)
            .unwrap_or(0)
    });
    query("search_symbols prefix ('search_', limit 50)", &|| {
        search_symbols(&index, "search_", None, 50).hits.len()
    });
    query("find_references_for_name ('LiveIndex')", &|| {
        index
            .find_references_for_name("LiveIndex", None, false)
            .len()
    });
    let default_options = TextSearchOptions::for_current_code_search();
    query(
        "search_structural '$X.unwrap()' (default limit 50)",
        &|| {
            search_structural(&index, "$X.unwrap()", &default_options)
                .map(|r| r.total_matches)
                .unwrap_or(0)
        },
    );
    let full_options = TextSearchOptions {
        total_limit: usize::MAX,
        ..TextSearchOptions::for_current_code_search()
    };
    query(
        "search_structural '$X.unwrap()' (no limit, full scan)",
        &|| {
            search_structural(&index, "$X.unwrap()", &full_options)
                .map(|r| r.total_matches)
                .unwrap_or(0)
        },
    );

    // ── D1: incremental parse, parser reuse, retained trees ────────────────
    println!("\n== D1. tree-sitter: fresh parser vs reused parser vs incremental (n=10) ==");
    for (label, (rel, _)) in &targets {
        let source = std::fs::read_to_string(root.join(rel)).expect("read target");
        incremental_parse(label, rel, &source);
    }
    let rust_grammar: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let rust_sources: Vec<String> = corpus
        .iter()
        .filter(|f| f.language == LanguageId::Rust)
        .map(|f| String::from_utf8_lossy(&f.bytes).into_owned())
        .collect();
    let source_total: usize = rust_sources.iter().map(String::len).sum();
    let ts_before = TS_LIVE.load(Ordering::Relaxed);
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&rust_grammar).expect("set language");
    let parser_bytes = TS_LIVE.load(Ordering::Relaxed) - ts_before;
    let (trees, reuse_all_ms) = time(|| {
        rust_sources
            .iter()
            .map(|source| parser.parse(source, None).expect("parse"))
            .collect::<Vec<_>>()
    });
    let held = TS_LIVE.load(Ordering::Relaxed) - ts_before - parser_bytes;
    let (_, fresh_all_ms) = time(|| {
        for source in &rust_sources {
            drop(fresh_parse(&rust_grammar, source));
        }
    });
    println!(
        "  all {} Rust files: fresh parser each {fresh_all_ms:.1} ms, one reused parser \
         {reuse_all_ms:.1} ms (serial)",
        rust_sources.len()
    );
    println!(
        "  retaining every Rust tree: {:.1} MiB ({:.1} KiB per file, {:.2} bytes per source byte \
         over {:.1} MiB source); idle parser {:.1} KiB",
        mib(held),
        held as f64 / 1024.0 / rust_sources.len().max(1) as f64,
        held as f64 / source_total.max(1) as f64,
        mib(source_total as u64),
        parser_bytes as f64 / 1024.0
    );
    drop(trees);

    // ── D2: ignore serial vs parallel walk ─────────────────────────────────
    println!("\n== D2. ignore::WalkBuilder build() vs build_parallel() (median of 5) ==");
    walk_compare("this repository's checkout", repo);
    walk_compare("corpus copy", root);
    if let Ok(big) = std::env::var("SYMFORGE_PROFILE_BIG_ROOT") {
        walk_compare("largest local checkout", Path::new(&big));
    } else {
        println!("  largest local checkout: SYMFORGE_PROFILE_BIG_ROOT unset, not measured");
    }

    // ── D3: serial vs parallel scan ────────────────────────────────────────
    println!("\n== D3. text-scan replica: serial iter vs rayon par_iter (n=20, median) ==");
    scan_compare(&index, &files_map, &trigram);
    drop(index);
}

/// First versus second read of freshly copied files, and first versus second
/// parallel parse + content-policy pass (one-time initialisation), on a second
/// corpus copy. Returns the copy so it outlives the measurement.
fn prewarm_probe(repo: &Path) -> tempfile::TempDir {
    use symforge::domain::IndexTargets;
    let dir = tempfile::tempdir().expect("tempdir");
    copy_corpus(repo, dir.path());
    let entries = discover_all_files(dir.path()).expect("walk");
    let read_all = || {
        entries
            .iter()
            .filter_map(|e| Some((e, std::fs::read(&e.absolute_path).ok()?)))
            .collect::<Vec<_>>()
    };
    let (_, first_read) = time(read_all);
    let (files, second_read) = time(read_all);
    let (_, first_parallel_read) = {
        let other = tempfile::tempdir().expect("tempdir");
        copy_corpus(repo, other.path());
        let entries = discover_all_files(other.path()).expect("walk");
        time(|| {
            entries
                .par_iter()
                .filter_map(|e| std::fs::read(&e.absolute_path).ok())
                .count()
        })
    };
    let pass = || {
        files.par_iter().for_each(|(entry, bytes)| {
            let Some(language) = entry.language else {
                return;
            };
            std::hint::black_box(symforge::knowledge::classify_stable_content_for_root(
                dir.path(),
                &entry.relative_path,
                IndexTargets::Code,
                bytes,
            ));
            std::hint::black_box(symforge::parsing::process_file(
                &entry.relative_path,
                bytes,
                language,
            ));
        });
    };
    let (_, first_pass) = time(pass);
    let (_, second_pass) = time(pass);
    let scouted = tempfile::tempdir().expect("tempdir");
    copy_corpus(repo, scouted.path());
    let exclusions = SourceExclusions::default();
    let scout =
        || time(|| scout_repository_with_exclusions(scouted.path(), &exclusions).expect("scout")).1;
    let (first_scout, second_scout) = (scout(), scout());
    println!("\n== prewarm probe (SYMFORGE_PROFILE_PREWARM, separate copies) ==");
    println!(
        "  scout_repository_with_exclusions on a third fresh copy: first {first_scout:.1} ms, \
         second {second_scout:.1} ms"
    );
    println!(
        "  fresh copy, {} files: first serial read {first_read:.1} ms, second {second_read:.1} ms; \
         first parallel read of another fresh copy {first_parallel_read:.1} ms",
        entries.len()
    );
    println!(
        "  parallel content policy + process_file: first pass in process {first_pass:.1} ms, \
         second {second_pass:.1} ms"
    );
    dir
}

/// The per-file steps of the load's admission closure that are public, alone
/// (serial) and together in the load's work order (largest first, one file
/// per task, rayon default pool).
fn admission_replicas(
    root: &Path,
    entries: &[symforge::discovery::DiscoveredEntry],
    corpus: &[CorpusFile],
) {
    use symforge::discovery::classify_admission;
    use symforge::domain::IndexTargets;
    use symforge::knowledge::{classify_stable_content_for_root, scan_secret_bytes};
    let sizes: HashMap<&str, (&Path, u64)> = entries
        .iter()
        .map(|e| {
            (
                e.relative_path.as_str(),
                (e.absolute_path.as_path(), e.file_size),
            )
        })
        .collect();
    let (_, admission_ms) = time(|| {
        for file in corpus {
            let (abs, size) = sizes[file.rel.as_str()];
            std::hint::black_box(classify_admission(abs, size, None));
            std::hint::black_box(classify_admission(abs, size, Some(&file.bytes)));
        }
    });
    println!("  classify_admission pre + post content (discovery): {admission_ms:.1} ms serial");
    let (_, secret_ms) = time(|| {
        for file in corpus {
            std::hint::black_box(scan_secret_bytes(&file.rel, &file.bytes));
        }
    });
    println!("  secret scan alone (knowledge::scan_secret_bytes): {secret_ms:.1} ms serial");
    let (_, content_ms) = time(|| {
        for file in corpus {
            std::hint::black_box(classify_stable_content_for_root(
                root,
                &file.rel,
                IndexTargets::Code,
                &file.bytes,
            ));
        }
    });
    println!(
        "  content policy (classify_stable_content_for_root: utf-8 + secret scan + dismissals): \
         {content_ms:.1} ms serial"
    );
    let mut order: Vec<&CorpusFile> = corpus.iter().collect();
    order.sort_by_key(|file| std::cmp::Reverse(file.bytes.len()));
    let (_, replica_ms) = time(|| {
        order.into_par_iter().with_max_len(1).for_each(|file| {
            let (abs, size) = sizes[file.rel.as_str()];
            std::hint::black_box(classify_admission(abs, size, None));
            let bytes = std::fs::read(abs).expect("read");
            std::hint::black_box(classify_admission(abs, size, Some(&bytes)));
            std::hint::black_box(classify_stable_content_for_root(
                root,
                &file.rel,
                IndexTargets::Code,
                &bytes,
            ));
            let result = symforge::parsing::process_file(&file.rel, &bytes, file.language);
            std::hint::black_box(IndexedFile::from_parse_result(result, bytes));
        });
    });
    println!(
        "  admission closure replica, parallel, load work order \
         (classify + read + content policy + process_file): {replica_ms:.1} ms wall"
    );
}

fn measure_knowledge(shared: &SharedIndex) -> (f64, f64) {
    let generation = shared.published_generation();
    let (Some(source), Some(version)) = (
        generation.source.as_deref(),
        generation.source_version.as_deref(),
    ) else {
        println!("  knowledge: generation has no source identity, not measured");
        return (0.0, 0.0);
    };
    let (bridge, bridge_ms) = time(|| {
        build_knowledge_bridge(
            &generation.live,
            source,
            generation.content_generation,
            &BridgeLimits::default(),
        )
    });
    let temporal = AuthorityTemporalIndex::from_published(&generation);
    let (_, authority_ms) = time(|| {
        build_knowledge_authority(
            &generation.live,
            source,
            version,
            generation.content_generation,
            &bridge,
            &temporal,
            SECRET_POLICY_VERSION,
            &AuthorityLimits::default(),
        )
    });
    (bridge_ms, authority_ms)
}

#[derive(Clone, Copy, PartialEq)]
enum EditKind {
    Comment,
    Body,
}

/// `original` with a comment appended, or a statement inserted at the top of
/// the first single-line-signature function body; `i` makes each rep unique.
fn edited(original: &str, kind: EditKind, i: usize) -> String {
    match kind {
        EditKind::Comment => format!("{original}\n// pipeline profile edit {i}\n"),
        EditKind::Body => {
            let mut out = String::with_capacity(original.len() + 64);
            let mut done = false;
            for line in original.split_inclusive('\n') {
                out.push_str(line);
                let trimmed = line.trim();
                if !done
                    && trimmed.ends_with('{')
                    && ["fn ", "pub fn ", "pub(crate) fn "]
                        .iter()
                        .any(|prefix| trimmed.starts_with(prefix))
                {
                    let indent = &line[..line.len() - line.trim_start().len()];
                    out.push_str(&format!("{indent}    let _pipeline_profile = {i};\n"));
                    done = true;
                }
            }
            assert!(done, "no function body to edit");
            out
        }
    }
}

fn resolved_link_targets(shared: &SharedIndex) -> BTreeSet<String> {
    shared
        .published_generation()
        .bridge
        .forward
        .iter()
        .filter_map(|link| match &link.resolution {
            BridgeResolution::ResolvedExact(anchor) => Some(match &anchor.id {
                CodeAnchorId::File { path } => path.clone(),
                CodeAnchorId::Symbol { symbol, .. } => symbol.path.clone(),
            }),
            _ => None,
        })
        .collect()
}

#[derive(Default)]
struct Samples {
    total: Vec<f64>,
    read: Vec<f64>,
    hash: Vec<f64>,
    parse: Vec<f64>,
    process: Vec<f64>,
    clone: Vec<f64>,
    update: Vec<f64>,
    outline: Vec<f64>,
    observe: Vec<f64>,
    residual: Vec<f64>,
}

#[allow(clippy::too_many_arguments)]
fn profile_updates(
    shared: &SharedIndex,
    root: &Path,
    label: &str,
    rel: &str,
    lines: usize,
    kind: EditKind,
    reps: usize,
    docs: &BTreeSet<String>,
) {
    let path = root.join(rel);
    let original = std::fs::read_to_string(&path).expect("read target");
    let status_before = format!(
        "{:?}",
        shared.read().get_file(rel).expect("indexed").parse_status
    );
    // Evidence for the carry's `names_referenced` gate: this file's symbol
    // names that the selector replica contains.
    let mut doc_named: Vec<String> = shared
        .read()
        .get_file(rel)
        .expect("indexed")
        .symbols
        .iter()
        .filter(|symbol| docs.contains(&symbol.name))
        .map(|symbol| symbol.name.clone())
        .collect();
    doc_named.sort();
    doc_named.dedup();
    let mut source_moved = 0;
    let grammar: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    let authority =
        symforge::live_index::index_lifecycle::activation::project_source_authority(root);
    let mut samples = Samples::default();
    let (mut bridge_builds, mut authority_builds, mut partials) = (0, 0, 0);
    let mut alloc_total = 0u64;
    for i in 0..reps {
        let bytes = edited(&original, kind, i).into_bytes();
        std::fs::write(&path, &bytes).expect("write target");
        let before = (
            full_bridge_builds(),
            full_authority_builds(),
            partial_bridge_updates(),
        );
        let source_before = shared.published_generation().source.clone();
        let alloc_before = ALLOCATED.load(Ordering::Relaxed);
        let (outcome, total) = time(|| update_file_from_disk(shared, root, rel));
        if shared.published_generation().source != source_before {
            source_moved += 1;
        }
        alloc_total += ALLOCATED.load(Ordering::Relaxed) - alloc_before;
        assert!(
            matches!(outcome, ReindexResult::Reindexed),
            "{rel}: {outcome:?}"
        );
        wait_for_knowledge(shared);
        bridge_builds += full_bridge_builds() - before.0;
        authority_builds += full_authority_builds() - before.1;
        partials += partial_bridge_updates() - before.2;
        let status = format!(
            "{:?}",
            shared.read().get_file(rel).expect("indexed").parse_status
        );
        assert_eq!(
            status, status_before,
            "{rel}: the edit changed the parse status"
        );

        let (read_bytes, read) = time(|| std::fs::read(&path).expect("read"));
        let (_, hash) = time(|| std::hint::black_box(symforge::hash::digest_hex(&read_bytes)));
        let source = String::from_utf8_lossy(&read_bytes).into_owned();
        let (_, parse) = time(|| fresh_parse(&grammar, &source));
        let (result, process) =
            time(|| symforge::parsing::process_file(rel, &read_bytes, LanguageId::Rust));
        let (mut copy, clone) = time(|| (*shared.read()).clone());
        let (_, update) = time(|| {
            copy.update_file(
                rel.to_string(),
                IndexedFile::from_parse_result(result, read_bytes),
            );
        });
        let (_, outline) = time(|| std::hint::black_box(copy.capture_repo_outline_view()));
        drop(copy);
        let (_, observe) = time(|| authority.observe_admission_active(rel));
        samples
            .residual
            .push(total - read - hash - process - clone - update - outline);
        samples.total.push(total);
        samples.read.push(read);
        samples.hash.push(hash);
        samples.parse.push(parse);
        samples.process.push(process);
        samples.clone.push(clone);
        samples.update.push(update);
        samples.outline.push(outline);
        samples.observe.push(observe);
    }
    // Restore the original so later targets and C see the unedited tree.
    std::fs::write(&path, original.as_bytes()).expect("restore target");
    let restored = update_file_from_disk(shared, root, rel);
    assert!(
        matches!(restored, ReindexResult::Reindexed),
        "{rel}: {restored:?}"
    );
    wait_for_knowledge(shared);

    let linked = resolved_link_targets(shared).contains(rel);
    let kind_label = match kind {
        EditKind::Comment => "comment append",
        EditKind::Body => "body edit",
    };
    let fmt = |name: &str, values: &[f64]| {
        let (mean, median) = stats(values);
        format!("{name} {mean:.2}/{median:.2}")
    };
    println!(
        "{label} {rel} ({lines} lines, {kind_label}, parse status {status_before}): \
         full rebuilds bridge {bridge_builds}/{reps} authority {authority_builds}/{reps}; \
         partial bridge updates {partials}/{reps}; \
         file is a resolved knowledge-link target: {linked}; mean {:.1} MiB allocated per call",
        mib(alloc_total / reps.max(1) as u64)
    );
    let examples: Vec<&str> = doc_named.iter().take(4).map(String::as_str).collect();
    println!(
        "    source identity moved in {source_moved}/{reps} publications; {} of its symbol \
         names are Markdown inline-code selectors (e.g. {examples:?})",
        doc_named.len()
    );
    println!(
        "    {} | {} | {} | ts-parse {} | {} | {} | {} | {} | {}",
        fmt("total", &samples.total),
        fmt("read", &samples.read),
        fmt("hash", &samples.hash),
        fmt("", &samples.parse).trim_start(),
        fmt("process_file", &samples.process),
        fmt("clone", &samples.clone),
        fmt("update", &samples.update),
        fmt("outline", &samples.outline),
        fmt("residual", &samples.residual),
    );
    println!(
        "    (observe_admission_active alone, called again after the update: {})",
        fmt("", &samples.observe).trim_start()
    );
}

fn incremental_parse(label: &str, rel: &str, source: &str) {
    let grammar: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    // One-line edit: insert a comment line at the middle line.
    let middle_row = source.matches('\n').count() / 2;
    let start = source
        .match_indices('\n')
        .nth(middle_row.saturating_sub(1))
        .map_or(0, |(at, _)| at + 1);
    let inserted = "// d1 one-line edit\n";
    let new_source = format!("{}{inserted}{}", &source[..start], &source[start..]);
    let edit = tree_sitter::InputEdit {
        start_byte: start,
        old_end_byte: start,
        new_end_byte: start + inserted.len(),
        start_position: tree_sitter::Point::new(middle_row, 0),
        old_end_position: tree_sitter::Point::new(middle_row, 0),
        new_end_position: tree_sitter::Point::new(middle_row + 1, 0),
    };
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&grammar).expect("set language");
    let ts_before = TS_LIVE.load(Ordering::Relaxed);
    let old_tree = parser.parse(source, None).expect("parse");
    let tree_bytes = TS_LIVE.load(Ordering::Relaxed) - ts_before;

    let (mut fresh, mut reused, mut incremental) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..10 {
        fresh.push(time(|| fresh_parse(&grammar, &new_source)).1);
        reused.push(time(|| parser.parse(&new_source, None).expect("parse")).1);
        let mut edited_tree = old_tree.clone();
        incremental.push(
            time(|| {
                edited_tree.edit(&edit);
                parser
                    .parse(&new_source, Some(&edited_tree))
                    .expect("parse")
            })
            .1,
        );
    }
    let full = parser.parse(&new_source, None).expect("parse");
    let mut edited_tree = old_tree.clone();
    edited_tree.edit(&edit);
    let inc = parser
        .parse(&new_source, Some(&edited_tree))
        .expect("parse");
    let same = full.root_node().to_sexp() == inc.root_node().to_sexp();
    let changed = edited_tree.changed_ranges(&inc).count();
    let (f, r, i) = (stats(&fresh), stats(&reused), stats(&incremental));
    println!(
        "  {label} {rel} ({:.0} KiB): fresh {:.2}/{:.2} | reused {:.2}/{:.2} | \
         incremental (edit+parse) {:.2}/{:.2} ms mean/median; incremental tree equals full: {same}; \
         changed ranges {changed}; retained tree {:.1} KiB",
        source.len() as f64 / 1024.0,
        f.0,
        f.1,
        r.0,
        r.1,
        i.0,
        i.1,
        tree_bytes as f64 / 1024.0
    );
}

fn walk_builder(root: &Path) -> ignore::WalkBuilder {
    // Production adds a hard-scope name filter (`.git`, runtime state dirs)
    // and source exclusions; `.git` is the only one present here.
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        .filter_entry(|entry| entry.file_name() != ".git");
    builder
}

fn walk_compare(label: &str, root: &Path) {
    let (mut serial, mut parallel) = (Vec::new(), Vec::new());
    let mut files = 0;
    for _ in 0..5 {
        let (count, elapsed) = time(|| {
            walk_builder(root)
                .build()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
                .count()
        });
        files = count;
        serial.push(elapsed);
        let counter = AtomicUsize::new(0);
        parallel.push(
            time(|| {
                walk_builder(root).build_parallel().run(|| {
                    Box::new(|entry| {
                        if entry
                            .ok()
                            .and_then(|e| e.file_type())
                            .is_some_and(|t| t.is_file())
                        {
                            counter.fetch_add(1, Ordering::Relaxed);
                        }
                        ignore::WalkState::Continue
                    })
                });
            })
            .1,
        );
        assert_eq!(counter.load(Ordering::Relaxed), files, "walks disagree");
    }
    println!(
        "  {label} ({files} files): serial {:.1} ms, parallel {:.1} ms",
        stats(&serial).1,
        stats(&parallel).1
    );
}

fn scan_compare(
    index: &LiveIndex,
    files: &HashMap<String, Arc<IndexedFile>>,
    trigram: &TrigramIndex,
) {
    let contents: Vec<&[u8]> = index
        .all_files()
        .map(|(_, f)| f.content.as_slice())
        .collect();
    let regex = regex::Regex::new(r"fn\s+\w+_with_options").expect("regex");
    let regex_lane = |content: &&[u8]| {
        String::from_utf8_lossy(content)
            .lines()
            .filter(|line| regex.is_match(line))
            .count()
    };
    report_scan("regex lane, all files", &contents, &regex_lane);
    for term in ["index", "ponytail"] {
        let candidates: Vec<&[u8]> = trigram
            .search(term.as_bytes(), files)
            .iter()
            .filter_map(|path| files.get(path))
            .map(|file| file.content.as_slice())
            .collect();
        // Production case-insensitive literal: one `to_lowercase` per line.
        let lowercase_lane = |content: &&[u8]| {
            String::from_utf8_lossy(content)
                .lines()
                .filter(|line| line.to_lowercase().contains(term))
                .count()
        };
        let finder = aho_corasick::AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build([term])
            .expect("automaton");
        let folding_lane = |content: &&[u8]| {
            String::from_utf8_lossy(content)
                .lines()
                .filter(|line| finder.is_match(line))
                .count()
        };
        report_scan(
            &format!("literal '{term}' per-line to_lowercase, trigram candidates"),
            &candidates,
            &lowercase_lane,
        );
        report_scan(
            &format!("literal '{term}' ascii-folding automaton (no per-line alloc)"),
            &candidates,
            &folding_lane,
        );
    }
}

fn report_scan(label: &str, contents: &[&[u8]], lane: &(dyn Fn(&&[u8]) -> usize + Sync)) {
    let (mut serial, mut parallel) = (Vec::new(), Vec::new());
    let mut hits = (0, 0);
    for _ in 0..20 {
        let (s, t) = time(|| contents.iter().map(lane).sum::<usize>());
        serial.push(t);
        let (p, t) = time(|| contents.par_iter().map(lane).sum::<usize>());
        parallel.push(t);
        hits = (s, p);
    }
    assert_eq!(hits.0, hits.1, "{label}: serial and parallel disagree");
    println!(
        "  {label:<64} {} files: serial {:.2} ms, par_iter {:.2} ms ({} matching lines)",
        contents.len(),
        stats(&serial).1,
        stats(&parallel).1,
        hits.0
    );
}
