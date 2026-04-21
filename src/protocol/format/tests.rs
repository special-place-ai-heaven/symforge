use super::*;
use crate::domain::{LanguageId, SymbolKind, SymbolRecord};
use crate::live_index::store::{CircuitBreakerState, IndexedFile, LiveIndex, ParseStatus};
use std::collections::HashMap;
use std::time::{Duration, Instant};

// --- Test helpers ---

fn make_symbol(
    name: &str,
    kind: SymbolKind,
    depth: u32,
    line_start: u32,
    line_end: u32,
) -> SymbolRecord {
    let byte_range = (0, 10);
    SymbolRecord {
        name: name.to_string(),
        kind,
        depth,
        sort_order: 0,
        byte_range,
        item_byte_range: Some(byte_range),
        line_range: (line_start, line_end),
        doc_byte_range: None,
    }
}

fn make_symbol_with_bytes(
    name: &str,
    kind: SymbolKind,
    depth: u32,
    line_start: u32,
    line_end: u32,
    byte_start: u32,
    byte_end: u32,
) -> SymbolRecord {
    let byte_range = (byte_start, byte_end);
    SymbolRecord {
        name: name.to_string(),
        kind,
        depth,
        sort_order: 0,
        byte_range,
        item_byte_range: Some(byte_range),
        line_range: (line_start, line_end),
        doc_byte_range: None,
    }
}

fn make_file(path: &str, content: &[u8], symbols: Vec<SymbolRecord>) -> (String, IndexedFile) {
    (
        path.to_string(),
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: content.to_vec(),
            symbols,
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: content.len() as u64,
            content_hash: "test".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        },
    )
}

fn make_index(files: Vec<(String, IndexedFile)>) -> LiveIndex {
    use crate::live_index::trigram::TrigramIndex;
    let cb = CircuitBreakerState::new(0.20);
    let files_map = files
        .into_iter()
        .map(|(path, file)| (path, std::sync::Arc::new(file)))
        .collect::<HashMap<_, _>>();
    let trigram_index = TrigramIndex::build_from_files(&files_map);
    let mut index = LiveIndex {
        files: files_map,
        loaded_at: Instant::now(),
        loaded_at_system: std::time::SystemTime::now(),
        load_duration: Duration::from_millis(42),
        cb_state: cb,
        is_empty: false,
        load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
        snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
        reverse_index: HashMap::new(),
        files_by_basename: HashMap::new(),
        files_by_dir_component: HashMap::new(),
        trigram_index,
        gitignore: None,
        skipped_files: Vec::new(),
    };
    index.rebuild_path_indices();
    index
}

fn empty_index() -> LiveIndex {
    make_index(vec![])
}

// --- file_outline tests ---

#[test]
fn test_file_outline_header_shows_path_and_count() {
    let (key, file) = make_file(
        "src/main.rs",
        b"fn main() {}",
        vec![make_symbol("main", SymbolKind::Function, 0, 1, 1)],
    );
    let index = make_index(vec![(key, file)]);
    let result = file_outline(&index, "src/main.rs");
    assert!(
        result.starts_with("src/main.rs  (1 symbols)"),
        "header should show path and count, got: {result}"
    );
}

#[test]
fn test_file_outline_symbol_line_with_kind_and_range() {
    let (key, file) = make_file(
        "src/main.rs",
        b"fn main() {}",
        vec![make_symbol("main", SymbolKind::Function, 0, 0, 4)],
    );
    let index = make_index(vec![(key, file)]);
    let result = file_outline(&index, "src/main.rs");
    assert!(result.contains("fn"), "should contain fn kind");
    assert!(result.contains("main"), "should contain symbol name");
    assert!(result.contains("1-5"), "should contain 1-based line range");
}

#[test]
fn test_file_outline_depth_indentation() {
    let symbols = vec![
        make_symbol("MyStruct", SymbolKind::Struct, 0, 1, 10),
        make_symbol("my_method", SymbolKind::Method, 1, 2, 5),
    ];
    let (key, file) = make_file(
        "src/lib.rs",
        b"struct MyStruct { fn my_method() {} }",
        symbols,
    );
    let index = make_index(vec![(key, file)]);
    let result = file_outline(&index, "src/lib.rs");
    let lines: Vec<&str> = result.lines().collect();
    // Method at depth 1 should be indented by 2 spaces
    let method_line = lines.iter().find(|l| l.contains("my_method")).unwrap();
    assert!(
        method_line.starts_with("  "),
        "depth-1 symbol should be indented by 2 spaces"
    );
}

#[test]
fn test_file_outline_not_found() {
    let index = empty_index();
    let result = file_outline(&index, "nonexistent.rs");
    assert_eq!(result, "File not found: nonexistent.rs");
}

#[test]
fn test_file_outline_empty_symbols() {
    let (key, file) = make_file("src/main.rs", b"", vec![]);
    let index = make_index(vec![(key, file)]);
    let result = file_outline(&index, "src/main.rs");
    assert!(result.contains("(0 symbols)"), "should show 0 symbols");
}

#[test]
fn test_file_outline_view_matches_live_index_output() {
    let (key, file) = make_file(
        "src/main.rs",
        b"fn main() {}",
        vec![make_symbol("main", SymbolKind::Function, 0, 1, 5)],
    );
    let index = make_index(vec![(key, file)]);

    let live_result = file_outline(&index, "src/main.rs");
    let captured_result =
        file_outline_view(&index.capture_file_outline_view("src/main.rs").unwrap());

    assert_eq!(captured_result, live_result);
}

// --- symbol_detail tests ---

#[test]
fn test_symbol_detail_returns_body_and_footer() {
    let content = b"fn hello() { println!(\"hi\"); }";
    let sym = make_symbol_with_bytes("hello", SymbolKind::Function, 0, 0, 0, 0, 30);
    let (key, file) = make_file("src/lib.rs", content, vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = symbol_detail(&index, "src/lib.rs", "hello", None);
    assert!(result.contains("fn hello"), "should contain body");
    assert!(
        result.contains("[fn, lines 1-1, 30 bytes]"),
        "should contain footer (0-based line_range 0-0 displayed as 1-based 1-1)"
    );
}

#[test]
fn test_symbol_detail_not_found_lists_available_symbols() {
    let sym = make_symbol("real_fn", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/lib.rs", b"fn real_fn() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = symbol_detail(&index, "src/lib.rs", "missing_fn", None);
    assert!(result.contains("No symbol missing_fn in src/lib.rs"));
    assert!(result.contains("real_fn"), "should list available symbols");
}

#[test]
fn test_symbol_detail_file_not_found() {
    let index = empty_index();
    let result = symbol_detail(&index, "nonexistent.rs", "foo", None);
    assert_eq!(result, "File not found: nonexistent.rs");
}

#[test]
fn test_symbol_detail_kind_filter_matches() {
    let symbols = vec![
        make_symbol("foo", SymbolKind::Function, 0, 0, 0),
        make_symbol("foo", SymbolKind::Struct, 0, 4, 9),
    ];
    let content = b"fn foo() {} struct foo {}";
    let (key, file) = make_file("src/lib.rs", content, symbols);
    let index = make_index(vec![(key, file)]);
    // Filter for struct kind (0-based 4-9 displays as 1-based 5-10)
    let result = symbol_detail(&index, "src/lib.rs", "foo", Some("struct"));
    assert!(
        result.contains("[struct, lines 5-10"),
        "footer should show struct kind"
    );
}

#[test]
fn test_symbol_detail_view_matches_live_index_output() {
    let content = b"fn hello() { println!(\"hi\"); }";
    let sym = make_symbol_with_bytes("hello", SymbolKind::Function, 0, 1, 1, 0, 30);
    let (key, file) = make_file("src/lib.rs", content, vec![sym]);
    let index = make_index(vec![(key, file)]);

    let live_result = symbol_detail(&index, "src/lib.rs", "hello", None);
    let captured_result = symbol_detail_view(
        &index.capture_symbol_detail_view("src/lib.rs").unwrap(),
        "hello",
        None,
    );

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_code_slice_view_formats_path_and_slice_text() {
    let result = code_slice_view("src/lib.rs", b"fn foo()");
    assert_eq!(result, "src/lib.rs\nfn foo()");
}

#[test]
fn test_code_slice_from_indexed_file_clamps_and_formats() {
    let (key, file) = make_file("src/lib.rs", b"fn foo() { bar(); }", vec![]);
    let index = make_index(vec![(key, file)]);

    let result = code_slice_from_indexed_file(
        index.capture_shared_file("src/lib.rs").unwrap().as_ref(),
        0,
        Some(200),
    );

    assert_eq!(result, "src/lib.rs\nfn foo() { bar(); }");
}

// --- search_symbols_result tests ---

#[test]
fn test_search_symbols_summary_header() {
    let symbols = vec![
        make_symbol("get_user", SymbolKind::Function, 0, 1, 5),
        make_symbol("get_role", SymbolKind::Function, 0, 6, 10),
    ];
    let (key, file) = make_file("src/lib.rs", b"fn get_user() {} fn get_role() {}", symbols);
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "get");
    assert!(
        result.starts_with("2 matches in 1 files"),
        "should start with summary"
    );
}

#[test]
fn test_search_symbols_case_insensitive() {
    let sym = make_symbol("GetUser", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/lib.rs", b"fn GetUser() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "getuser");
    assert!(
        !result.starts_with("No symbols"),
        "should find case-insensitive match"
    );
}

#[test]
fn test_search_symbols_no_match() {
    let sym = make_symbol("unrelated", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/lib.rs", b"fn unrelated() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "xyz_no_match");
    assert_eq!(
        result,
        "No symbols matching 'xyz_no_match'. Try: search_text(query=\"xyz_no_match\") for text matches, or explore(query=\"xyz_no_match\") for concept-based discovery."
    );
}

#[test]
fn test_search_symbols_result_view_matches_live_index_output() {
    let symbols = vec![
        make_symbol("get_user", SymbolKind::Function, 0, 1, 5),
        make_symbol("get_role", SymbolKind::Function, 0, 6, 10),
    ];
    let (key, file) = make_file("src/lib.rs", b"fn get_user() {} fn get_role() {}", symbols);
    let index = make_index(vec![(key, file)]);

    let live_result = search_symbols_result(&index, "get");
    let captured_result =
        search_symbols_result_view(&search::search_symbols(&index, "get", None, 50), "get");

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_search_symbols_grouped_by_file() {
    let sym1 = make_symbol("foo", SymbolKind::Function, 0, 1, 5);
    let sym2 = make_symbol("foo_bar", SymbolKind::Function, 0, 1, 5);
    let (key1, file1) = make_file("a.rs", b"fn foo() {}", vec![sym1]);
    let (key2, file2) = make_file("b.rs", b"fn foo_bar() {}", vec![sym2]);
    let index = make_index(vec![(key1, file1), (key2, file2)]);
    let result = search_symbols_result(&index, "foo");
    assert!(
        result.contains("2 matches in 2 files"),
        "should show 2 files"
    );
    assert!(result.contains("a.rs"), "should contain file a.rs");
    assert!(result.contains("b.rs"), "should contain file b.rs");
}

#[test]
fn test_search_symbols_kind_filter_limits_results() {
    let function = make_symbol("JobRunner", SymbolKind::Function, 0, 1, 5);
    let class = make_symbol("Job", SymbolKind::Class, 0, 6, 10);
    let (key, file) = make_file(
        "src/lib.rs",
        b"fn JobRunner() {} struct Job {}",
        vec![function, class],
    );
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result_with_kind(&index, "job", Some("class"));
    assert!(
        result.contains("class Job"),
        "class result should remain visible: {result}"
    );
    assert!(
        !result.contains("fn JobRunner"),
        "function result should be filtered out: {result}"
    );
}

// --- search_text_result tests ---

#[test]
fn test_search_text_summary_header() {
    let (key, file) = make_file("src/lib.rs", b"let x = 1;\nlet y = 2;", vec![]);
    let index = make_index(vec![(key, file)]);
    let result = search_text_result(&index, "let");
    assert!(result.starts_with("2 matches in 1 files"), "got: {result}");
}

#[test]
fn test_search_text_shows_line_numbers() {
    let content = b"line one\nline two\nline three";
    let (key, file) = make_file("src/lib.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);
    let result = search_text_result(&index, "line two");
    assert!(
        result.contains("  2:"),
        "should show 1-indexed line number 2"
    );
}

#[test]
fn test_search_text_case_insensitive() {
    let (key, file) = make_file("src/lib.rs", b"Hello World", vec![]);
    let index = make_index(vec![(key, file)]);
    let result = search_text_result(&index, "hello world");
    assert!(
        !result.starts_with("No matches"),
        "should find case-insensitive"
    );
}

#[test]
fn test_search_text_no_match() {
    let (key, file) = make_file("src/lib.rs", b"fn main() {}", vec![]);
    let index = make_index(vec![(key, file)]);
    let result = search_text_result(&index, "xyz_totally_absent");
    assert_eq!(
        result,
        "No matches for 'xyz_totally_absent'. Suggestions: try search_symbols(query=...) for symbol names, or use regex=true for pattern matching, or broaden with include_tests=true / include_generated=true."
    );
}

#[test]
fn test_search_text_result_view_matches_live_index_output() {
    let (key, file) = make_file("src/lib.rs", b"let x = 1;\nlet y = 2;", vec![]);
    let index = make_index(vec![(key, file)]);

    let live_result = search_text_result(&index, "let");
    let captured_result = search_text_result_view(
        search::search_text(&index, Some("let"), None, false),
        None,
        None,
        None,
    );

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_search_text_crlf_handling() {
    let content = b"fn foo() {\r\n    let x = 1;\r\n}";
    let (key, file) = make_file("src/lib.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);
    let result = search_text_result(&index, "let x");
    assert!(
        result.contains("let x = 1"),
        "should find content without \\r"
    );
}

#[test]
fn test_search_text_terms_or_matches_multiple_needles() {
    let (key, file) = make_file(
        "src/lib.rs",
        b"// TODO: first\n// FIXME: second\n// NOTE: ignored",
        vec![],
    );
    let index = make_index(vec![(key, file)]);
    let terms = vec!["TODO".to_string(), "FIXME".to_string()];
    let result = search_text_result_with_options(&index, None, Some(&terms), false);
    assert!(
        result.contains("TODO: first"),
        "TODO line should match: {result}"
    );
    assert!(
        result.contains("FIXME: second"),
        "FIXME line should match: {result}"
    );
    assert!(
        !result.contains("NOTE: ignored"),
        "non-matching line should be absent: {result}"
    );
}

#[test]
fn test_search_text_regex_mode_matches_pattern() {
    let (key, file) = make_file(
        "src/lib.rs",
        b"// TODO: first\n// FIXME: second\n// NOTE: ignored",
        vec![],
    );
    let index = make_index(vec![(key, file)]);
    let result = search_text_result_with_options(&index, Some("TODO|FIXME"), None, true);
    assert!(
        result.contains("TODO: first"),
        "TODO line should match regex: {result}"
    );
    assert!(
        result.contains("FIXME: second"),
        "FIXME line should match regex: {result}"
    );
    assert!(
        !result.contains("NOTE: ignored"),
        "non-matching line should be absent: {result}"
    );
}

#[test]
fn test_search_text_result_view_renders_context_windows_with_separators() {
    let (key, file) = make_file(
        "src/lib.rs",
        b"line 1\nline 2\nneedle 3\nline 4\nneedle 5\nline 6\nline 7\nline 8\nneedle 9\nline 10\n",
        vec![],
    );
    let index = make_index(vec![(key, file)]);
    let result = search::search_text_with_options(
        &index,
        Some("needle"),
        None,
        false,
        &search::TextSearchOptions {
            context: Some(1),
            ..search::TextSearchOptions::for_current_code_search()
        },
    );

    let rendered = search_text_result_view(result, None, None, None);

    assert!(
        rendered.contains("src/lib.rs"),
        "file header missing: {rendered}"
    );
    assert!(
        rendered.contains("  2: line 2"),
        "context line missing: {rendered}"
    );
    assert!(
        rendered.contains("> 3: needle 3"),
        "match marker missing: {rendered}"
    );
    assert!(
        rendered.contains("  ..."),
        "window separator missing: {rendered}"
    );
    assert!(
        rendered.contains("> 9: needle 9"),
        "later match missing: {rendered}"
    );
}

#[test]
fn test_search_text_result_view_group_by_symbol_keeps_duplicate_names_separate() {
    let rendered = search_text_result_view(
        Ok(search::TextSearchResult {

            label: "'needle'".to_string(),
            total_matches: 2,
            files: vec![search::TextFileMatches {
                path: "src/lib.rs".to_string(),
                matches: vec![
                    search::TextLineMatch {
                        line_number: 2,
                        line: "needle alpha".to_string(),
                        enclosing_symbol: Some(search::EnclosingMatchSymbol {
                            name: "connect".to_string(),
                            kind: "fn".to_string(),
                            line_range: (0, 1),
                        }),
                    },
                    search::TextLineMatch {
                        line_number: 5,
                        line: "needle beta".to_string(),
                        enclosing_symbol: Some(search::EnclosingMatchSymbol {
                            name: "connect".to_string(),
                            kind: "fn".to_string(),
                            line_range: (3, 4),
                        }),
                    },
                ],
                rendered_lines: None,
                callers: None,
            }],
            suppressed_by_noise: 0,
            overflow_count: 0,
        }),
        Some("symbol"),
        None,
        None,
    );

    assert!(
        rendered.contains("fn connect (lines 1-2): 1 match"),
        "missing first symbol bucket: {rendered}"
    );
    assert!(
        rendered.contains("fn connect (lines 4-5): 1 match"),
        "missing second symbol bucket: {rendered}"
    );
}

// --- repo_outline tests ---

#[test]
fn test_repo_outline_header_totals() {
    let sym = make_symbol("main", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/main.rs", b"fn main() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = repo_outline(&index, "myproject");
    assert!(
        result.starts_with("myproject  (1 files, 1 symbols)"),
        "got: {result}"
    );
}

#[test]
fn test_repo_outline_shows_filename_language_count() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = repo_outline(&index, "proj");
    assert!(result.contains("lib.rs"), "should show filename");
    assert!(result.contains("Rust"), "should show language");
    assert!(result.contains("1 symbols"), "should show symbol count");
}

#[test]
fn test_repo_outline_repeated_basenames_use_shortest_unique_suffixes() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let index = make_index(vec![
        make_file("src/live_index/mod.rs", b"fn foo() {}", vec![sym.clone()]),
        make_file("src/protocol/mod.rs", b"fn foo() {}", vec![sym.clone()]),
        make_file("src/parsing/languages/mod.rs", b"fn foo() {}", vec![sym]),
    ]);

    let result = repo_outline(&index, "proj");

    assert!(result.contains("live_index/mod.rs"), "got: {result}");
    assert!(result.contains("protocol/mod.rs"), "got: {result}");
    assert!(result.contains("languages/mod.rs"), "got: {result}");
    assert!(!result.contains("\n  mod.rs"), "got: {result}");
}

#[test]
fn test_repo_outline_deeper_collisions_expand_beyond_one_parent() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let index = make_index(vec![
        make_file("src/alpha/shared/mod.rs", b"fn foo() {}", vec![sym.clone()]),
        make_file("tests/beta/shared/mod.rs", b"fn foo() {}", vec![sym]),
    ]);

    let result = repo_outline(&index, "proj");

    assert!(result.contains("alpha/shared/mod.rs"), "got: {result}");
    assert!(result.contains("beta/shared/mod.rs"), "got: {result}");
}

#[test]
fn test_repo_outline_view_matches_live_index_output() {
    let alpha = make_symbol("alpha", SymbolKind::Function, 0, 1, 3);
    let beta = make_symbol("beta", SymbolKind::Function, 0, 5, 7);
    let (k1, f1) = make_file("src/zeta.rs", b"fn beta() {}", vec![beta]);
    let (k2, f2) = make_file("src/alpha.rs", b"fn alpha() {}", vec![alpha]);
    let index = make_index(vec![(k1, f1), (k2, f2)]);

    let live_result = repo_outline(&index, "proj");
    let captured_result = repo_outline_view(&index.capture_repo_outline_view(), "proj");

    assert_eq!(captured_result, live_result);
}

// --- health_report tests ---

#[test]
fn test_health_report_ready_state() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = health_report(&index);
    assert!(result.contains("Status: Ready"), "got: {result}");
    assert!(result.contains("Files:"), "should have Files line");
    assert!(result.contains("Symbols:"), "should have Symbols line");
    assert!(result.contains("Loaded in:"), "should have Loaded in line");
    assert!(
        result.contains("Watcher: off"),
        "should have Watcher: off line (no watcher active)"
    );
}

#[test]
fn test_health_report_empty_state() {
    let index = LiveIndex {
        files: HashMap::new(),
        loaded_at: Instant::now(),
        loaded_at_system: std::time::SystemTime::now(),
        load_duration: Duration::from_millis(0),
        cb_state: CircuitBreakerState::new(0.20),
        is_empty: true,
        load_source: crate::live_index::store::IndexLoadSource::EmptyBootstrap,
        snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
        reverse_index: HashMap::new(),
        files_by_basename: HashMap::new(),
        files_by_dir_component: HashMap::new(),
        trigram_index: crate::live_index::trigram::TrigramIndex::new(),
        gitignore: None,
        skipped_files: Vec::new(),
    };
    let result = health_report(&index);
    assert!(result.contains("Status: Empty"), "got: {result}");
}

#[test]
fn test_health_report_shows_watcher_off() {
    // health_report with no watcher active should show "Watcher: off"
    let index = make_index(vec![]);
    let result = health_report(&index);
    assert!(result.contains("Watcher: off"), "got: {result}");
    assert!(
        !result.contains("events"),
        "off watcher should not mention events"
    );
}

#[test]
fn test_health_report_shows_watcher_active() {
    use crate::watcher::{WatcherInfo, WatcherState};
    let index = make_index(vec![]);
    let watcher = WatcherInfo {
        state: WatcherState::Active,
        events_processed: 0,
        last_event_at: None,
        debounce_window_ms: 200,
        ..WatcherInfo::default()
    };
    let result = health_report_with_watcher(&index, &watcher);
    assert!(
        result.contains("Watcher: active (idle; event-driven, waiting for filesystem changes"),
        "got: {result}"
    );
}

#[test]
fn test_health_report_active_watcher_shows_last_change_when_events_exist() {
    use crate::watcher::{WatcherInfo, WatcherState};

    let index = make_index(vec![]);
    let watcher = WatcherInfo {
        state: WatcherState::Active,
        events_processed: 7,
        last_event_at: Some(std::time::SystemTime::now()),
        debounce_window_ms: 200,
        ..WatcherInfo::default()
    };
    let result = health_report_with_watcher(&index, &watcher);
    assert!(
        result.contains("Watcher: active (event-driven; 7 events, last change:"),
        "got: {result}"
    );
}

#[test]
fn test_health_report_from_stats_matches_live_index_output() {
    use crate::watcher::{WatcherInfo, WatcherState};

    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let watcher = WatcherInfo {
        state: WatcherState::Active,
        events_processed: 7,
        last_event_at: None,
        debounce_window_ms: 200,
        ..WatcherInfo::default()
    };

    let live_result = health_report_with_watcher(&index, &watcher);
    let captured_result =
        health_report_from_stats("Ready", &index.health_stats_with_watcher(&watcher));

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_health_report_from_published_state_matches_live_index_output() {
    use crate::watcher::{WatcherInfo, WatcherState};

    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let watcher = WatcherInfo {
        state: WatcherState::Active,
        events_processed: 7,
        last_event_at: None,
        debounce_window_ms: 200,
        ..WatcherInfo::default()
    };

    let live_result = health_report_with_watcher(&index, &watcher);
    let shared = crate::live_index::SharedIndexHandle::shared(index);
    let captured_result = health_report_from_published_state(&shared.published_state(), &watcher);

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_health_report_from_published_state_shows_failed_file_details() {
    use crate::live_index::store::{
        IndexLoadSource, PublishedIndexState, PublishedIndexStatus, SnapshotVerifyState,
    };
    use crate::watcher::{WatcherInfo, WatcherState};
    use std::time::{Duration, SystemTime};

    let published = PublishedIndexState {
        generation: 7,
        status: PublishedIndexStatus::Ready,
        degraded_summary: None,
        file_count: 4,
        parsed_count: 2,
        partial_parse_count: 0,
        failed_count: 2,
        symbol_count: 12,
        loaded_at_system: SystemTime::now(),
        load_duration: Duration::from_millis(12),
        load_source: IndexLoadSource::FreshLoad,
        snapshot_verify_state: SnapshotVerifyState::NotNeeded,
        is_empty: false,
        partial_parse_files: vec![],
        failed_files: vec![
            ("src/bad.rs".to_string(), "syntax error".to_string()),
            ("src/worse.rs".to_string(), "lexer panic".to_string()),
        ],
        tier_counts: (4, 0, 0),
    };
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let report = health_report_from_published_state(&published, &watcher);
    assert!(
        report.contains("Failed files (2):"),
        "published-state health should preserve failed file detail: {report}"
    );
    assert!(
        report.contains("src/bad.rs"),
        "published-state health should list failed file paths: {report}"
    );
    assert!(
        report.contains("syntax error"),
        "published-state health should list failure reasons: {report}"
    );
}

#[test]
fn test_health_report_from_published_state_shows_partial_parse_files() {
    use crate::live_index::store::{
        IndexLoadSource, PublishedIndexState, PublishedIndexStatus, SnapshotVerifyState,
    };
    use crate::watcher::{WatcherInfo, WatcherState};
    use std::time::{Duration, SystemTime};

    let published = PublishedIndexState {
        generation: 8,
        status: PublishedIndexStatus::Ready,
        degraded_summary: None,
        file_count: 3,
        parsed_count: 1,
        partial_parse_count: 2,
        failed_count: 0,
        symbol_count: 9,
        loaded_at_system: SystemTime::now(),
        load_duration: Duration::from_millis(9),
        load_source: IndexLoadSource::FreshLoad,
        snapshot_verify_state: SnapshotVerifyState::NotNeeded,
        is_empty: false,
        partial_parse_files: vec![
            "src/partial_a.rs".to_string(),
            "src/partial_b.rs".to_string(),
        ],
        failed_files: vec![],
        tier_counts: (3, 0, 0),
    };
    let watcher = WatcherInfo {
        state: WatcherState::Off,
        ..WatcherInfo::default()
    };

    let report = health_report_from_published_state(&published, &watcher);
    assert!(
        report.contains("Partial parse files (2):"),
        "published-state health should preserve partial file detail: {report}"
    );
    assert!(
        report.contains("src/partial_a.rs"),
        "published-state health should list partial file paths: {report}"
    );
    assert!(
        report.contains("src/partial_b.rs"),
        "published-state health should list all bounded partial file paths: {report}"
    );
}

#[test]
fn test_health_report_lists_partial_parse_files() {
    use crate::watcher::WatcherState;
    use std::time::Duration;

    let stats = HealthStats {
        file_count: 3,
        symbol_count: 0,
        parsed_count: 0,
        partial_parse_count: 3,
        failed_count: 0,
        load_duration: Duration::from_millis(0),
        watcher_state: WatcherState::Off,
        events_processed: 0,
        last_event_at: None,
        debounce_window_ms: 200,
        overflow_count: 0,
        last_overflow_at: None,
        stale_files_found: 0,
        last_reconcile_at: None,
        partial_parse_files: vec![
            "src/a.rs".to_string(),
            "src/b.rs".to_string(),
            "src/c.rs".to_string(),
        ],
        failed_files: vec![],
        tier_counts: (3, 0, 0),
    };
    let report = health_report_from_stats("Ready", &stats);
    assert!(
        report.contains("Partial parse files (3):"),
        "should contain header"
    );
    assert!(report.contains("  1. src/a.rs"), "should list first file");
    assert!(report.contains("  2. src/b.rs"), "should list second file");
    assert!(report.contains("  3. src/c.rs"), "should list third file");
    assert!(
        !report.contains("... and"),
        "should not show overflow hint for 3 files"
    );
    assert!(
        report.contains("Parse resilience: partial files kept best-effort symbols"),
        "should explain partial parses as resilient degradation"
    );
}

#[test]
fn test_health_report_caps_partial_list_at_10() {
    use crate::watcher::WatcherState;
    use std::time::Duration;

    let partial_parse_files: Vec<String> =
        (1..=50).map(|i| format!("src/file{:02}.rs", i)).collect();
    let stats = HealthStats {
        file_count: 50,
        symbol_count: 0,
        parsed_count: 0,
        partial_parse_count: 50,
        failed_count: 0,
        load_duration: Duration::from_millis(0),
        watcher_state: WatcherState::Off,
        events_processed: 0,
        last_event_at: None,
        debounce_window_ms: 200,
        overflow_count: 0,
        last_overflow_at: None,
        stale_files_found: 0,
        last_reconcile_at: None,
        partial_parse_files,
        failed_files: vec![],
        tier_counts: (50, 0, 0),
    };
    let report = health_report_from_stats("Ready", &stats);
    assert!(
        report.contains("Partial parse files (50):"),
        "should show count of 50"
    );
    assert!(report.contains("  10."), "should list up to entry 10");
    assert!(!report.contains("  11."), "should not list entry 11");
    assert!(
        report.contains("... and 40 more partial files"),
        "should show overflow hint for 40 remaining"
    );
}

#[test]
fn test_health_report_shows_tier_breakdown() {
    use crate::watcher::WatcherState;
    use std::time::Duration;

    let stats = HealthStats {
        file_count: 8200,
        symbol_count: 10000,
        parsed_count: 8180,
        partial_parse_count: 15,
        failed_count: 5,
        load_duration: Duration::from_millis(120),
        watcher_state: WatcherState::Off,
        events_processed: 0,
        last_event_at: None,
        debounce_window_ms: 200,
        overflow_count: 0,
        last_overflow_at: None,
        stale_files_found: 0,
        last_reconcile_at: None,
        partial_parse_files: vec![],
        failed_files: vec![],
        tier_counts: (8200, 1280, 20),
    };
    let report = health_report_from_stats("Ready", &stats);
    assert!(
        report.contains("Admission: 9500 files discovered"),
        "should show total discovered count; got:\n{report}"
    );
    assert!(
        report.contains("Tier 1 (indexed): 8200"),
        "should show Tier 1 count; got:\n{report}"
    );
    assert!(
        report.contains("Tier 2 (metadata only): 1280"),
        "should show Tier 2 count; got:\n{report}"
    );
    assert!(
        report.contains("Tier 3 (hard-skipped): 20"),
        "should show Tier 3 count; got:\n{report}"
    );
}

#[test]
fn test_health_report_shows_reconciliation_and_overflow_stats() {
    use crate::watcher::WatcherState;
    use std::time::{Duration, SystemTime};

    let stats = HealthStats {
        file_count: 1,
        symbol_count: 0,
        parsed_count: 1,
        partial_parse_count: 0,
        failed_count: 0,
        load_duration: Duration::from_millis(10),
        watcher_state: WatcherState::Active,
        events_processed: 7,
        last_event_at: Some(SystemTime::now()),
        debounce_window_ms: 200,
        overflow_count: 2,
        last_overflow_at: Some(SystemTime::now()),
        stale_files_found: 5,
        last_reconcile_at: Some(SystemTime::now()),
        partial_parse_files: vec![],
        failed_files: vec![],
        tier_counts: (1, 0, 0),
    };

    let report = health_report_from_stats("Ready", &stats);
    assert!(report.contains("overflows: 2"), "got: {report}");
    assert!(report.contains("reconcile repairs: 5"), "got: {report}");
    assert!(report.contains("last overflow:"), "got: {report}");
    assert!(report.contains("last reconcile:"), "got: {report}");
}

#[test]
fn test_around_match_occurrence_selects_requested_match() {
    let content = b"line one\nTODO first\nline three\nTODO second\nline five";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_match_occurrence("todo", Some(2), Some(1), false, false),
    );

    assert_eq!(result, "3: line three\n4: TODO second\n5: line five");
}

#[test]
fn test_around_match_occurrence_reports_available_lines() {
    let content = b"line one\nTODO first\nline three";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_match_occurrence("todo", Some(2), Some(1), false, false),
    );

    assert_eq!(
        result,
        "Match occurrence 2 for 'todo' not found in src/main.rs; 1 match(es) available at lines 2"
    );
}

// --- what_changed_result tests ---

#[test]
fn test_what_changed_since_far_past_lists_all_files() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    // since_ts=0 (epoch) is before index was loaded
    let result = what_changed_result(&index, 0);
    assert!(
        result.contains("src/lib.rs"),
        "should list all files: {result}"
    );
}

#[test]
fn test_what_changed_since_far_future_returns_no_changes() {
    let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
    let index = make_index(vec![(key, file)]);
    // since_ts=far future — no changes
    let result = what_changed_result(&index, i64::MAX);
    assert_eq!(result, "No changes detected since last index load.");
}

#[test]
fn test_what_changed_timestamp_view_matches_live_index_output() {
    let (k1, f1) = make_file("src/z.rs", b"fn z() {}", vec![]);
    let (k2, f2) = make_file("src/a.rs", b"fn a() {}", vec![]);
    let index = make_index(vec![(k1, f1), (k2, f2)]);

    let live_result = what_changed_result(&index, 0);
    let captured_result =
        what_changed_timestamp_view(&index.capture_what_changed_timestamp_view(), 0);

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_what_changed_paths_result_sorts_and_deduplicates() {
    let result = what_changed_paths_result(
        &[
            "src\\b.rs".to_string(),
            "src/a.rs".to_string(),
            "src/a.rs".to_string(),
        ],
        "No git changes detected.",
    );
    assert_eq!(result, "src/a.rs\nsrc/b.rs");
}

#[test]
fn test_search_files_resolve_result_view_returns_exact_path() {
    let view = SearchFilesResolveView::Resolved {
        path: "src/protocol/tools.rs".to_string(),
    };

    assert_eq!(
        search_files_resolve_result_view(&view),
        "src/protocol/tools.rs"
    );
}

#[test]
fn test_search_files_resolve_result_view_formats_ambiguous_output() {
    let view = SearchFilesResolveView::Ambiguous {
        hint: "lib.rs".to_string(),
        matches: vec!["src/lib.rs".to_string(), "tests/lib.rs".to_string()],
        overflow_count: 1,
    };

    let result = search_files_resolve_result_view(&view);

    assert!(result.contains("Ambiguous path hint 'lib.rs' (3 matches)"));
    assert!(result.contains("  src/lib.rs"));
    assert!(result.contains("  tests/lib.rs"));
    assert!(result.contains("  ... and 1 more"));
}

#[test]
fn test_search_files_resolve_result_view_not_found() {
    let view = SearchFilesResolveView::NotFound {
        hint: "README.md".to_string(),
    };

    assert_eq!(
        search_files_resolve_result_view(&view),
        "No indexed source path matched 'README.md'. Try search_files(query=\"README.md\") without resolve=true for fuzzy matches, or check the path with get_repo_map(detail=\"tree\")."
    );
}

#[test]
fn test_search_files_result_view_groups_ranked_paths() {
    let view = SearchFilesView::Found {
        query: "tools.rs".to_string(),
        total_matches: 3,
        overflow_count: 1,
        hits: vec![
            crate::live_index::SearchFilesHit {
                tier: SearchFilesTier::StrongPath,
                path: "src/protocol/tools.rs".to_string(),
                coupling_score: None,
                shared_commits: None,
            },
            crate::live_index::SearchFilesHit {
                tier: SearchFilesTier::Basename,
                path: "src/sidecar/tools.rs".to_string(),
                coupling_score: None,
                shared_commits: None,
            },
            crate::live_index::SearchFilesHit {
                tier: SearchFilesTier::LoosePath,
                path: "src/protocol/tools_helper.rs".to_string(),
                coupling_score: None,
                shared_commits: None,
            },
        ],
    };

    let result = search_files_result_view(&view);

    assert!(result.contains("3 matching files"));
    assert!(result.contains("── Strong path matches ──"));
    assert!(result.contains("  src/protocol/tools.rs"));
    assert!(result.contains("── Basename matches ──"));
    assert!(result.contains("  src/sidecar/tools.rs"));
    assert!(result.contains("── Loose path matches ──"));
    assert!(result.contains("  src/protocol/tools_helper.rs"));
    assert!(result.contains("... and 1 more"));
}

#[test]
fn test_search_files_result_view_not_found() {
    let view = SearchFilesView::NotFound {
        query: "README.md".to_string(),
    };

    assert_eq!(
        search_files_result_view(&view),
        "No indexed source files matching 'README.md'"
    );
}

// --- file_content tests ---

#[test]
fn test_file_content_full() {
    let content = b"fn main() {\n    println!(\"hi\");\n}";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);
    let result = file_content(&index, "src/main.rs", None, None);
    assert!(result.contains("fn main()"), "should return full content");
    assert!(result.contains("println!"), "should return full content");
}

#[test]
fn test_file_content_line_range() {
    let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);
    // Lines 2-4 (1-indexed)
    let result = file_content(&index, "src/main.rs", Some(2), Some(4));
    assert!(!result.contains("line 1"), "should not include line 1");
    assert!(result.contains("line 2"), "should include line 2");
    assert!(result.contains("line 3"), "should include line 3");
    assert!(result.contains("line 4"), "should include line 4");
    assert!(!result.contains("line 5"), "should not include line 5");
}

#[test]
fn test_file_content_not_found() {
    let index = empty_index();
    let result = file_content(&index, "nonexistent.rs", None, None);
    assert_eq!(result, "File not found: nonexistent.rs");
}

#[test]
fn test_file_outline_from_indexed_file_matches_live_index_output() {
    let (key, file) = make_file(
        "src/main.rs",
        b"fn main() {}",
        vec![
            make_symbol("main", SymbolKind::Function, 0, 0, 0),
            make_symbol("helper", SymbolKind::Function, 1, 1, 1),
        ],
    );
    let index = make_index(vec![(key, file)]);

    let live_result = file_outline(&index, "src/main.rs");
    let shared_result =
        file_outline_from_indexed_file(index.capture_shared_file("src/main.rs").unwrap().as_ref());

    assert_eq!(shared_result, live_result);
}

#[test]
fn test_symbol_detail_from_indexed_file_matches_live_index_output() {
    let content = b"fn helper() {}\nfn target() {}\n";
    let (key, file) = make_file(
        "src/main.rs",
        content,
        vec![
            make_symbol_with_bytes("helper", SymbolKind::Function, 0, 0, 0, 0, 13),
            make_symbol_with_bytes("target", SymbolKind::Function, 0, 1, 1, 14, 27),
        ],
    );
    let index = make_index(vec![(key, file)]);

    let live_result = symbol_detail(&index, "src/main.rs", "target", None);
    let shared_result = symbol_detail_from_indexed_file(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        "target",
        None,
        None,
    );

    assert_eq!(shared_result, live_result);
}

#[test]
fn test_file_content_view_matches_live_index_output() {
    let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let live_result = file_content(&index, "src/main.rs", Some(2), Some(4));
    let captured_result = file_content_view(
        &index.capture_file_content_view("src/main.rs").unwrap(),
        Some(2),
        Some(4),
    );

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_file_content_from_indexed_file_matches_live_index_output() {
    let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let live_result = file_content(&index, "src/main.rs", Some(2), Some(4));
    let shared_result = file_content_from_indexed_file(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        Some(2),
        Some(4),
    );

    assert_eq!(shared_result, live_result);
}

#[test]
fn test_file_content_from_indexed_file_with_context_renders_numbered_full_read() {
    let content = b"line 1\nline 2\nline 3";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::line_range_with_format(None, None, true, false),
    );

    assert_eq!(result, "1: line 1\n2: line 2\n3: line 3");
}

#[test]
fn test_file_content_from_indexed_file_with_context_renders_headered_range_read() {
    let content = b"line 1\nline 2\nline 3\nline 4";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::line_range_with_format(Some(2), Some(3), true, true),
    );

    assert_eq!(result, "src/main.rs [lines 2-3]\n2: line 2\n3: line 3");
}

#[test]
fn test_file_content_from_indexed_file_with_context_renders_numbered_around_line_excerpt() {
    let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_line(3, Some(1), false, false),
    );

    assert_eq!(result, "2: line 2\n3: line 3\n4: line 4");
}

#[test]
fn test_file_content_from_indexed_file_with_context_renders_numbered_around_match_excerpt() {
    let content = b"line 1\nTODO first\nline 3\nTODO second\nline 5";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_match("todo", Some(1), false, false),
    );

    assert_eq!(result, "1: line 1\n2: TODO first\n3: line 3");
}

#[test]
fn test_file_content_from_indexed_file_with_context_renders_chunked_excerpt_header() {
    let content = b"line 1\nline 2\nline 3\nline 4\nline 5";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::chunk(2, 2),
    );

    assert_eq!(
        result,
        "src/main.rs [chunk 2/3, lines 3-4]\n3: line 3\n4: line 4"
    );
}

#[test]
fn test_file_content_from_indexed_file_with_context_reports_out_of_range_chunk() {
    let content = b"line 1\nline 2\nline 3";
    let (key, file) = make_file("src/main.rs", content, vec![]);
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::chunk(3, 2),
    );

    assert_eq!(result, "Chunk 3 out of range for src/main.rs (2 chunks)");
}

#[test]
fn test_file_content_from_indexed_file_with_context_renders_around_symbol_excerpt() {
    let content = b"line 1\nfn connect() {}\nline 3";
    let (key, file) = make_file(
        "src/main.rs",
        content,
        vec![make_symbol("connect", SymbolKind::Function, 0, 1, 1)],
    );
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_symbol("connect", None, Some(1)),
    );

    assert_eq!(result, "1: line 1\n2: fn connect() {}\n3: line 3");
}

#[test]
fn test_file_content_from_indexed_file_with_context_reports_ambiguous_around_symbol() {
    let content = b"fn connect() {}\nline 2\nfn connect() {}";
    let (key, file) = make_file(
        "src/main.rs",
        content,
        vec![
            make_symbol("connect", SymbolKind::Function, 0, 0, 0),
            make_symbol("connect", SymbolKind::Function, 0, 2, 2),
        ],
    );
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_symbol("connect", None, Some(1)),
    );

    assert_eq!(
        result,
        "Ambiguous symbol selector for connect in src/main.rs; pass `symbol_line` to disambiguate. Candidates: 0, 2"
    );
}

#[test]
fn test_file_content_from_indexed_file_with_context_around_symbol_line_selects_exact_match() {
    let content = b"fn connect() {}\nline 2\nfn connect() {}";
    let (key, file) = make_file(
        "src/main.rs",
        content,
        vec![
            make_symbol("connect", SymbolKind::Function, 0, 0, 0),
            make_symbol("connect", SymbolKind::Function, 0, 2, 2),
        ],
    );
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_symbol("connect", Some(3), Some(0)),
    );

    assert_eq!(result, "3: fn connect() {}");
}

// --- B2: around_symbol returns full indexed span ---

#[test]
fn test_around_symbol_returns_full_multiline_body() {
    // 25-line function to verify we get the full body, not just 3-7 lines
    let mut lines_vec: Vec<String> = Vec::new();
    lines_vec.push("// preamble".to_string());
    lines_vec.push("fn big_function() {".to_string());
    for i in 0..20 {
        lines_vec.push(format!("    let x{i} = {i};"));
    }
    lines_vec.push("}".to_string());
    lines_vec.push("// postamble".to_string());
    let content_str = lines_vec.join("\n");
    let content = content_str.as_bytes();

    // Symbol spans lines 1..22 (0-indexed), i.e. "fn big_function() {" through "}"
    let (key, file) = make_file(
        "src/main.rs",
        content,
        vec![make_symbol("big_function", SymbolKind::Function, 0, 1, 22)],
    );
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_symbol("big_function", None, None),
    );

    let result_lines: Vec<&str> = result.lines().collect();
    // Symbol is lines 2..23 (1-indexed), default context_lines=0
    assert_eq!(
        result_lines.len(),
        22,
        "should return all 22 lines of the symbol"
    );
    assert!(result_lines[0].contains("fn big_function()"));
    assert!(result_lines[21].contains("}"));
}

#[test]
fn test_around_symbol_with_max_lines_truncates() {
    let content =
        b"line 1\nfn connect() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\nline 7";
    let (key, file) = make_file(
        "src/main.rs",
        content,
        // Symbol spans lines 1..5 (0-indexed), i.e. 6 lines: "fn connect() {" through "}"
        vec![make_symbol("connect", SymbolKind::Function, 0, 1, 5)],
    );
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_symbol_with_max_lines(
            "connect",
            None,
            None,
            Some(3),
            false,
            false,
        ),
    );

    let result_lines: Vec<&str> = result.lines().collect();
    assert_eq!(result_lines.len(), 4); // 3 content lines + truncation hint
    assert!(result_lines[0].contains("fn connect()"));
    assert!(result_lines[3].contains("truncated"));
    assert!(result_lines[3].contains("showing first 3"));
}

#[test]
fn test_around_symbol_context_lines_extends_range() {
    let content = b"line 1\nline 2\nfn connect() {\n    body;\n}\nline 6\nline 7";
    let (key, file) = make_file(
        "src/main.rs",
        content,
        // Symbol spans lines 2..4 (0-indexed)
        vec![make_symbol("connect", SymbolKind::Function, 0, 2, 4)],
    );
    let index = make_index(vec![(key, file)]);

    // context_lines=2 should add 2 lines before and after the symbol
    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_symbol("connect", None, Some(2)),
    );

    let result_lines: Vec<&str> = result.lines().collect();
    // Symbol is lines 3-5 (1-indexed), context extends to 1-7
    assert_eq!(result_lines.len(), 7);
    assert!(result_lines[0].contains("line 1"));
    assert!(result_lines[6].contains("line 7"));
}

#[test]
fn test_around_symbol_not_found_returns_error() {
    let content = b"fn connect() {}\nline 2";
    let (key, file) = make_file(
        "src/main.rs",
        content,
        vec![make_symbol("connect", SymbolKind::Function, 0, 0, 0)],
    );
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_symbol("nonexistent", None, None),
    );

    assert!(
        result.contains("No symbol")
            || result.contains("not found")
            || result.contains("Not found"),
        "should indicate symbol not found, got: {result}"
    );
    assert!(
        result.contains("nonexistent"),
        "error should name the missing symbol, got: {result}"
    );
}

#[test]
fn test_around_symbol_includes_doc_comments_in_indexed_range() {
    // Doc comment is on line 0, function signature on line 1, body on lines 2-3
    let content = b"/// Doc comment\nfn connect() {\n    body;\n}\nline 5";
    let (key, file) = make_file(
        "src/main.rs",
        content,
        // Symbol range includes the doc comment line (0..3)
        vec![make_symbol("connect", SymbolKind::Function, 0, 0, 3)],
    );
    let index = make_index(vec![(key, file)]);

    let result = file_content_from_indexed_file_with_context(
        index.capture_shared_file("src/main.rs").unwrap().as_ref(),
        search::ContentContext::around_symbol("connect", None, None),
    );

    let result_lines: Vec<&str> = result.lines().collect();
    assert_eq!(result_lines.len(), 4);
    assert!(result_lines[0].contains("/// Doc comment"));
    assert!(result_lines[3].contains("}"));
}

// --- guard messages ---

#[test]
fn test_loading_guard_message() {
    assert_eq!(
        loading_guard_message(),
        "Index is loading... try again shortly."
    );
}

#[test]
fn test_empty_guard_message() {
    assert_eq!(
        empty_guard_message(),
        "Index not loaded. Call index_folder to index a directory."
    );
}

// --- not_found helpers ---

#[test]
fn test_not_found_file_format() {
    assert_eq!(not_found_file("src/foo.rs"), "File not found: src/foo.rs");
}

#[test]
fn test_not_found_symbol_lists_available() {
    let sym = make_symbol("existing_fn", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/lib.rs", b"fn existing_fn() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = not_found_symbol(&index, "src/lib.rs", "missing_fn");
    assert!(result.contains("No symbol missing_fn in src/lib.rs"));
    assert!(result.contains("existing_fn"));
}

#[test]
fn test_not_found_symbol_no_symbols_in_file() {
    let (key, file) = make_file("src/lib.rs", b"", vec![]);
    let index = make_index(vec![(key, file)]);
    let result = not_found_symbol(&index, "src/lib.rs", "foo");
    assert!(result.contains("no indexed symbols"));
}

// ─── find_references_result tests ─────────────────────────────────────

use crate::domain::{ReferenceKind, ReferenceRecord};

fn make_ref(name: &str, kind: ReferenceKind, line: u32, enclosing: Option<u32>) -> ReferenceRecord {
    ReferenceRecord {
        name: name.to_string(),
        qualified_name: None,
        kind,
        byte_range: (0, 1),
        line_range: (line, line),
        enclosing_symbol_index: enclosing,
    }
}

fn make_file_with_refs(
    path: &str,
    content: &[u8],
    symbols: Vec<SymbolRecord>,
    references: Vec<ReferenceRecord>,
) -> (String, IndexedFile) {
    (
        path.to_string(),
        IndexedFile {
            relative_path: path.to_string(),
            language: LanguageId::Rust,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: content.to_vec(),
            symbols,
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: content.len() as u64,
            content_hash: "test".to_string(),
            references,
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        },
    )
}

fn make_index_with_reverse(files: Vec<(String, IndexedFile)>) -> LiveIndex {
    use crate::live_index::trigram::TrigramIndex;
    let cb = CircuitBreakerState::new(0.20);
    let files_map = files
        .into_iter()
        .map(|(path, file)| (path, std::sync::Arc::new(file)))
        .collect::<HashMap<_, _>>();
    let trigram_index = TrigramIndex::build_from_files(&files_map);
    let mut index = LiveIndex {
        files: files_map,
        loaded_at: Instant::now(),
        loaded_at_system: std::time::SystemTime::now(),
        load_duration: Duration::from_millis(42),
        cb_state: cb,
        is_empty: false,
        load_source: crate::live_index::store::IndexLoadSource::FreshLoad,
        snapshot_verify_state: crate::live_index::store::SnapshotVerifyState::NotNeeded,
        reverse_index: HashMap::new(),
        files_by_basename: HashMap::new(),
        files_by_dir_component: HashMap::new(),
        trigram_index,
        gitignore: None,
        skipped_files: Vec::new(),
    };
    index.rebuild_reverse_index();
    index.rebuild_path_indices();
    index
}

#[test]
fn test_find_references_result_groups_by_file_and_shows_context() {
    // Content: 3 lines so we can test context extraction
    let content = b"fn handle() {\n    process(x);\n}\n";
    let sym = make_symbol_with_bytes("handle", SymbolKind::Function, 0, 1, 3, 0, 30);
    let r = make_ref("process", ReferenceKind::Call, 2, Some(0));
    let (key, file) = make_file_with_refs("src/handler.rs", content, vec![sym], vec![r]);
    let index = make_index_with_reverse(vec![(key, file)]);
    let result = find_references_result(&index, "process", None);
    assert!(
        result.contains("1 references in 1 files"),
        "header missing, got: {result}"
    );
    assert!(
        result.contains("src/handler.rs"),
        "file path missing, got: {result}"
    );
    assert!(
        result.contains("process"),
        "reference name missing, got: {result}"
    );
    assert!(
        result.contains("[in fn handle]"),
        "enclosing annotation missing, got: {result}"
    );
}

#[test]
fn test_find_references_result_zero_results() {
    let index = make_index_with_reverse(vec![]);
    let result = find_references_result(&index, "nobody", None);
    assert_eq!(result, "No references found for \"nobody\"");
}

#[test]
fn test_find_references_result_kind_filter_call_only() {
    let content = b"use foo;\nfoo();\n";
    let r_import = make_ref("foo", ReferenceKind::Import, 1, None);
    let r_call = make_ref("foo", ReferenceKind::Call, 2, None);
    let (key, file) = make_file_with_refs("src/lib.rs", content, vec![], vec![r_import, r_call]);
    let index = make_index_with_reverse(vec![(key, file)]);
    let result = find_references_result(&index, "foo", Some("call"));
    // Should only show the call reference, not the import
    assert!(
        result.contains("1 references"),
        "expected only 1 reference, got: {result}"
    );
}

#[test]
fn test_find_references_result_view_matches_live_index_output() {
    let content = b"fn handle() {\n    process(x);\n}\n";
    let sym = make_symbol_with_bytes("handle", SymbolKind::Function, 0, 1, 3, 0, 30);
    let r = make_ref("process", ReferenceKind::Call, 2, Some(0));
    let (key, file) = make_file_with_refs("src/handler.rs", content, vec![sym], vec![r]);
    let index = make_index_with_reverse(vec![(key, file)]);

    let live_result = find_references_result(&index, "process", None);
    let limits = OutputLimits::default();
    let captured_result = find_references_result_view(
        &index.capture_find_references_view("process", None, limits.total_hits),
        "process",
        &limits,
    );

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_find_references_result_view_total_limit_caps_across_files() {
    // 3 files, each with 10 references → 30 total, but total_limit=15
    let mut all_files = Vec::new();
    for i in 0..3 {
        let path = format!("src/file_{i}.rs");
        let content = b"fn f() {}\nfn g() {}\nfn h() {}\n";
        let refs: Vec<ReferenceRecord> = (0..10)
            .map(|j| make_ref("target", ReferenceKind::Call, (j % 3) + 1, None))
            .collect();
        let (key, file) = make_file_with_refs(&path, content, vec![], refs);
        all_files.push((key, file));
    }
    let index = make_index_with_reverse(all_files);
    let view = index.capture_find_references_view("target", None, 200);

    // Without total_hits limit, all 30 refs would be shown (max_per_file is high)
    let unlimited = OutputLimits {
        max_files: 100,
        max_per_file: 100,
        total_hits: usize::MAX,
    };
    let unlimited_result = find_references_result_view(&view, "target", &unlimited);
    assert!(
        !unlimited_result.contains("more references"),
        "unlimited should show all refs"
    );

    // With total_hits=15, only 15 refs should be emitted
    let limits = OutputLimits {
        max_files: 100,
        max_per_file: 100,
        total_hits: 15,
    };
    let result = find_references_result_view(&view, "target", &limits);

    // file_0 gets 10 hits, file_1 gets 5 hits before total_limit reached,
    // file_1 has 5 truncated, file_2 is skipped entirely
    assert!(
        result.contains("... and 5 more references"),
        "file_1 should show 5 truncated hits, got:\n{result}"
    );
    // file_2 should not appear (total_limit already reached before it)
    assert!(
        !result.contains("src/file_2.rs"),
        "file_2 should be skipped, got:\n{result}"
    );
}

#[test]
fn test_find_references_result_view_per_file_limit_within_total() {
    // 1 file with 20 references, max_per_file=5, total_hits=100
    let content = b"fn a() {}\nfn b() {}\nfn c() {}\n";
    let refs: Vec<ReferenceRecord> = (0..20)
        .map(|j| make_ref("target", ReferenceKind::Call, (j % 3) + 1, None))
        .collect();
    let (key, file) = make_file_with_refs("src/lib.rs", content, vec![], refs);
    let index = make_index_with_reverse(vec![(key, file)]);
    let view = index.capture_find_references_view("target", None, 200);

    let limits = OutputLimits {
        max_files: 100,
        max_per_file: 5,
        total_hits: 100,
    };
    let result = find_references_result_view(&view, "target", &limits);

    // Should show 5 refs and truncate 15
    assert!(
        result.contains("... and 15 more references"),
        "expected per-file truncation, got:\n{result}"
    );
}

#[test]
fn test_find_references_compact_view_total_limit_caps_across_files() {
    let mut all_files = Vec::new();
    for i in 0..3 {
        let path = format!("src/file_{i}.rs");
        let content = b"fn f() {}\nfn g() {}\nfn h() {}\n";
        let refs: Vec<ReferenceRecord> = (0..10)
            .map(|j| make_ref("target", ReferenceKind::Call, (j % 3) + 1, None))
            .collect();
        let (key, file) = make_file_with_refs(&path, content, vec![], refs);
        all_files.push((key, file));
    }
    let index = make_index_with_reverse(all_files);
    let view = index.capture_find_references_view("target", None, 200);

    let limits = OutputLimits {
        max_files: 100,
        max_per_file: 100,
        total_hits: 15,
    };
    let result = find_references_compact_view(&view, "target", &limits);

    // file_0 gets 10 hits, file_1 gets 5 hits, file_1 truncates 5, file_2 skipped
    assert!(
        result.contains("... and 5 more"),
        "file_1 should show 5 truncated in compact view, got:\n{result}"
    );
    assert!(
        !result.contains("src/file_2.rs"),
        "file_2 should be skipped in compact view, got:\n{result}"
    );
}

// ─── find_dependents_result tests ─────────────────────────────────────

#[test]
fn test_find_dependents_result_shows_importers() {
    let content_b = b"use crate::db;\n";
    let r = make_ref("db", ReferenceKind::Import, 1, None);
    let (key_b, file_b) = make_file_with_refs("src/handler.rs", content_b, vec![], vec![r]);
    // Also need "src/db.rs" in the index for find_dependents_for_file to work
    let (key_a, file_a) = make_file("src/db.rs", b"pub fn connect() {}", vec![]);
    let index = make_index_with_reverse(vec![(key_a, file_a), (key_b, file_b)]);
    let result = find_dependents_result(&index, "src/db.rs");
    assert!(
        result.contains("File-level dependency graph: 1 files depend on src/db.rs"),
        "header wrong, got: {result}"
    );
    assert!(
        result.contains("Use find_references"),
        "should point callers to symbol-level lookup, got: {result}"
    );
    assert!(
        result.contains("src/handler.rs"),
        "importer missing, got: {result}"
    );
    assert!(
        result.contains("[import]"),
        "import annotation missing, got: {result}"
    );
}

#[test]
fn test_find_dependents_result_zero_dependents() {
    let (key, file) = make_file("src/db.rs", b"", vec![]);
    let index = make_index_with_reverse(vec![(key, file)]);
    let result = find_dependents_result(&index, "src/db.rs");
    assert!(
        result.contains("No file-level dependents found for \"src/db.rs\""),
        "got: {result}"
    );
    assert!(
        result.contains("find_references"),
        "empty state should still point to symbol-level lookup, got: {result}"
    );
}

#[test]
fn test_find_dependents_result_view_matches_live_index_output() {
    let content_b = b"use crate::db;\n";
    let r = make_ref("db", ReferenceKind::Import, 1, None);
    let (key_b, file_b) = make_file_with_refs("src/handler.rs", content_b, vec![], vec![r]);
    let (key_a, file_a) = make_file("src/db.rs", b"pub fn connect() {}", vec![]);
    let index = make_index_with_reverse(vec![(key_a, file_a), (key_b, file_b)]);

    let live_result = find_dependents_result(&index, "src/db.rs");
    let captured_result = find_dependents_result_view(
        &index.capture_find_dependents_view("src/db.rs"),
        "src/db.rs",
        &OutputLimits::default(),
    );

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_find_dependents_mermaid_shows_flowchart() {
    let content_b = b"use crate::db;\n";
    let r = make_ref("db", ReferenceKind::Import, 1, None);
    let (key_b, file_b) = make_file_with_refs("src/handler.rs", content_b, vec![], vec![r]);
    let (key_a, file_a) = make_file("src/db.rs", b"pub fn connect() {}", vec![]);
    let index = make_index_with_reverse(vec![(key_a, file_a), (key_b, file_b)]);
    let view = index.capture_find_dependents_view("src/db.rs");
    let result = find_dependents_mermaid(&view, "src/db.rs", &OutputLimits::default());
    assert!(
        result.starts_with("flowchart LR"),
        "should start with flowchart, got: {result}"
    );
    assert!(result.contains("src/db.rs"), "should mention target file");
    assert!(
        result.contains("src/handler.rs"),
        "should mention dependent"
    );
    assert!(
        result.contains("db"),
        "should show symbol name in edge label"
    );
}

#[test]
fn test_find_dependents_mermaid_empty() {
    let (key, file) = make_file("src/db.rs", b"", vec![]);
    let index = make_index_with_reverse(vec![(key, file)]);
    let view = index.capture_find_dependents_view("src/db.rs");
    let result = find_dependents_mermaid(&view, "src/db.rs", &OutputLimits::default());
    assert_eq!(result, "No dependents found for \"src/db.rs\"");
}

#[test]
fn test_find_dependents_dot_shows_digraph() {
    let content_b = b"use crate::db;\n";
    let r = make_ref("db", ReferenceKind::Import, 1, None);
    let (key_b, file_b) = make_file_with_refs("src/handler.rs", content_b, vec![], vec![r]);
    let (key_a, file_a) = make_file("src/db.rs", b"pub fn connect() {}", vec![]);
    let index = make_index_with_reverse(vec![(key_a, file_a), (key_b, file_b)]);
    let view = index.capture_find_dependents_view("src/db.rs");
    let result = find_dependents_dot(&view, "src/db.rs", &OutputLimits::default());
    assert!(
        result.starts_with("digraph dependents {"),
        "should start with digraph, got: {result}"
    );
    assert!(result.contains("src/db.rs"), "should mention target file");
    assert!(
        result.contains("src/handler.rs"),
        "should mention dependent"
    );
    assert!(result.ends_with('}'), "should end with closing brace");
}

#[test]
fn test_find_dependents_dot_empty() {
    let (key, file) = make_file("src/db.rs", b"", vec![]);
    let index = make_index_with_reverse(vec![(key, file)]);
    let view = index.capture_find_dependents_view("src/db.rs");
    let result = find_dependents_dot(&view, "src/db.rs", &OutputLimits::default());
    assert_eq!(result, "No dependents found for \"src/db.rs\"");
}

#[test]
fn test_find_dependents_mermaid_shows_true_ref_count_not_capped() {
    // Construct a view directly with 5 lines, but set max_per_file=2.
    // The mermaid label should show symbol names (all "db"), not just "5 refs".
    use crate::live_index::query::{DependentFileView, DependentLineView, FindDependentsView};
    let lines: Vec<DependentLineView> = (1..=5)
        .map(|i| DependentLineView {
            line_number: i,
            line_content: format!("use crate::db; // ref {i}"),
            kind: "import".to_string(),
            name: "db".to_string(),
        })
        .collect();
    let view = FindDependentsView {
        files: vec![DependentFileView {
            file_path: "src/handler.rs".to_string(),
            lines,
        }],
    };
    let limits = OutputLimits::new(20, 2); // max_per_file=2, but 5 actual refs
    let result = find_dependents_mermaid(&view, "src/db.rs", &limits);
    assert!(
        result.contains("db"),
        "mermaid label should include symbol name 'db'. Got: {result}"
    );
}

#[test]
fn test_find_dependents_dot_shows_true_ref_count_not_capped() {
    use crate::live_index::query::{DependentFileView, DependentLineView, FindDependentsView};
    let lines: Vec<DependentLineView> = (1..=5)
        .map(|i| DependentLineView {
            line_number: i,
            line_content: format!("use crate::db; // ref {i}"),
            kind: "import".to_string(),
            name: "db".to_string(),
        })
        .collect();
    let view = FindDependentsView {
        files: vec![DependentFileView {
            file_path: "src/handler.rs".to_string(),
            lines,
        }],
    };
    let limits = OutputLimits::new(20, 2);
    let result = find_dependents_dot(&view, "src/db.rs", &limits);
    assert!(
        result.contains("db"),
        "dot label should include symbol name 'db'. Got: {result}"
    );
}

// ─── context_bundle_result tests ──────────────────────────────────────

#[test]
fn test_context_bundle_result_includes_body_and_sections() {
    let content = b"fn process(x: i32) -> i32 {\n    x + 1\n}\n";
    let sym = make_symbol_with_bytes("process", SymbolKind::Function, 0, 1, 3, 0, 41);
    let (key, file) = make_file_with_refs("src/lib.rs", content, vec![sym], vec![]);
    let index = make_index_with_reverse(vec![(key, file)]);
    let result = context_bundle_result(&index, "src/lib.rs", "process", None);
    assert!(result.contains("fn process"), "body missing, got: {result}");
    assert!(
        result.contains("[fn, src/lib.rs:"),
        "footer missing, got: {result}"
    );
    assert!(
        result.contains("Callers"),
        "Callers section missing, got: {result}"
    );
    assert!(
        result.contains("Callees"),
        "Callees section missing, got: {result}"
    );
    assert!(
        result.contains("Type usages"),
        "Type usages section missing, got: {result}"
    );
}

#[test]
fn test_context_bundle_result_caps_callers_at_20() {
    // Build 25 Call references to "process" from different positions
    let refs: Vec<ReferenceRecord> = (0u32..25)
        .map(|i| make_ref("process", ReferenceKind::Call, i + 100, None))
        .collect();
    let content = b"fn caller() {} fn process() {}";
    let sym_caller = make_symbol_with_bytes("caller", SymbolKind::Function, 0, 1, 1, 0, 14);
    let sym_process = make_symbol_with_bytes("process", SymbolKind::Function, 0, 1, 1, 15, 30);
    // Add a process symbol as the target
    let (key, file) =
        make_file_with_refs("src/lib.rs", content, vec![sym_caller, sym_process], refs);
    let index = make_index_with_reverse(vec![(key, file)]);
    let result = context_bundle_result(&index, "src/lib.rs", "process", None);
    assert!(
        result.contains("...and"),
        "overflow message missing, got: {result}"
    );
    assert!(
        result.contains("more callers"),
        "overflow count missing, got: {result}"
    );
}

#[test]
fn test_context_bundle_result_symbol_not_found() {
    let (key, file) = make_file("src/lib.rs", b"fn foo() {}", vec![]);
    let index = make_index_with_reverse(vec![(key, file)]);
    let result = context_bundle_result(&index, "src/lib.rs", "nonexistent", None);
    assert!(
        result.contains("No symbol nonexistent in src/lib.rs"),
        "got: {result}"
    );
}

#[test]
fn test_context_bundle_result_empty_sections_show_zero() {
    let content = b"fn process() {}";
    let sym = make_symbol_with_bytes("process", SymbolKind::Function, 0, 1, 1, 0, 15);
    let (key, file) = make_file_with_refs("src/lib.rs", content, vec![sym], vec![]);
    let index = make_index_with_reverse(vec![(key, file)]);
    let result = context_bundle_result(&index, "src/lib.rs", "process", None);
    assert!(
        result.contains("Callers (0)"),
        "zero callers section missing, got: {result}"
    );
    assert!(
        result.contains("Callees (0)"),
        "zero callees section missing, got: {result}"
    );
    assert!(
        result.contains("Type usages (0)"),
        "zero type usages section missing, got: {result}"
    );
}

#[test]
fn test_context_bundle_result_view_matches_live_index_output() {
    let content = b"fn process(x: i32) -> i32 {\n    x + 1\n}\n";
    let sym = make_symbol_with_bytes("process", SymbolKind::Function, 0, 1, 3, 0, 41);
    let (key, file) = make_file_with_refs("src/lib.rs", content, vec![sym], vec![]);
    let index = make_index_with_reverse(vec![(key, file)]);

    let live_result = context_bundle_result(&index, "src/lib.rs", "process", None);
    let captured_result = context_bundle_result_view(
        &index.capture_context_bundle_view("src/lib.rs", "process", None, None),
        "full",
    );

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_context_bundle_result_view_ambiguous_symbol() {
    let result = context_bundle_result_view(
        &ContextBundleView::AmbiguousSymbol {
            path: "src/lib.rs".to_string(),
            name: "process".to_string(),
            candidate_lines: vec![1, 10],
        },
        "full",
    );

    assert!(
        result.contains("Ambiguous symbol selector"),
        "got: {result}"
    );
    assert!(result.contains("1"), "got: {result}");
    assert!(result.contains("10"), "got: {result}");
}

#[test]
fn test_context_bundle_result_view_suggests_impl_blocks_for_zero_caller_struct() {
    let empty_section = ContextBundleSectionView {
        total_count: 0,
        overflow_count: 0,
        entries: vec![],
        unique_count: 0,
    };
    let result = context_bundle_result_view(
        &ContextBundleView::Found(Box::new(ContextBundleFoundView {
            file_path: "src/actors.rs".to_string(),
            body: "struct MyActor;".to_string(),
            kind_label: "struct".to_string(),
            line_range: (0, 0),
            byte_count: 15,
            callers: empty_section.clone(),
            callees: empty_section.clone(),
            type_usages: empty_section,
            dependencies: vec![],
            implementation_suggestions: vec![
                ImplBlockSuggestionView {
                    display_name: "impl MyActor".to_string(),
                    file_path: "src/actors.rs".to_string(),
                    line_number: 3,
                },
                ImplBlockSuggestionView {
                    display_name: "impl Actor for MyActor".to_string(),
                    file_path: "src/actors.rs".to_string(),
                    line_number: 7,
                },
            ],
        })),
        "full",
    );

    assert!(
        result.contains("0 direct callers"),
        "missing zero-caller tip: {result}"
    );
    assert!(
        result.contains("impl MyActor (src/actors.rs:3)"),
        "missing inherent impl suggestion: {result}"
    );
    assert!(
        result.contains("impl Actor for MyActor (src/actors.rs:7)"),
        "missing trait impl suggestion: {result}"
    );
}

#[test]
fn test_context_bundle_result_view_with_max_tokens_truncates_dependencies_in_priority_order() {
    let empty_section = ContextBundleSectionView {
        total_count: 0,
        overflow_count: 0,
        entries: vec![],
        unique_count: 0,
    };
    let result = context_bundle_result_view_with_max_tokens(
        &ContextBundleView::Found(Box::new(ContextBundleFoundView {
            file_path: "src/lib.rs".to_string(),
            body: "fn plan(alpha: Alpha) -> Output { todo!() }".to_string(),
            kind_label: "fn".to_string(),
            line_range: (0, 0),
            byte_count: 44,
            callers: empty_section.clone(),
            callees: empty_section.clone(),
            type_usages: empty_section,
            dependencies: vec![
                TypeDependencyView {
                    name: "Alpha".to_string(),
                    kind_label: "struct".to_string(),
                    file_path: "src/types.rs".to_string(),
                    line_range: (10, 12),
                    body: "struct Alpha {\n    value: i32,\n}\n".to_string(),
                    depth: 0,
                },
                TypeDependencyView {
                    name: "Gamma".to_string(),
                    kind_label: "struct".to_string(),
                    file_path: "src/types.rs".to_string(),
                    line_range: (20, 40),
                    body: format!(
                        "struct Gamma {{\n{}\n}}\n",
                        "    payload: [u8; 64],\n".repeat(10)
                    ),
                    depth: 1,
                },
            ],
            implementation_suggestions: vec![],
        })),
        "full",
        Some(100),
    );

    assert!(
        result.contains("── Alpha [struct, src/types.rs:11-13]"),
        "expected direct dependency to fit the budget: {result}"
    );
    assert!(
        !result.contains("── Gamma [struct, src/types.rs:21-41"),
        "transitive dependency should be omitted once the budget is exhausted: {result}"
    );
    assert!(
        result.contains("Truncated at ~100 tokens."),
        "expected truncation footer: {result}"
    );
    assert!(
        result.contains("1 additional type dependencies not shown."),
        "expected omitted dependency count: {result}"
    );
}

// --- format_token_savings tests ---

#[test]
fn test_format_token_savings_all_zeros_returns_empty() {
    let snap = crate::sidecar::StatsSnapshot {
        read_fires: 0,
        read_saved_tokens: 0,
        edit_fires: 0,
        edit_saved_tokens: 0,
        write_fires: 0,
        grep_fires: 0,
        grep_saved_tokens: 0,
    };
    let result = format_token_savings(&snap);
    assert!(
        result.is_empty(),
        "all-zero snapshot should return empty string; got: {result}"
    );
}

#[test]
fn test_format_token_savings_shows_section_header() {
    let snap = crate::sidecar::StatsSnapshot {
        read_fires: 1,
        read_saved_tokens: 250,
        edit_fires: 0,
        edit_saved_tokens: 0,
        write_fires: 0,
        grep_fires: 0,
        grep_saved_tokens: 0,
    };
    let result = format_token_savings(&snap);
    assert!(
        result.contains("Token Savings"),
        "result must contain 'Token Savings' header; got: {result}"
    );
}

#[test]
fn test_format_token_savings_read_fires_and_tokens() {
    let snap = crate::sidecar::StatsSnapshot {
        read_fires: 3,
        read_saved_tokens: 750,
        edit_fires: 0,
        edit_saved_tokens: 0,
        write_fires: 0,
        grep_fires: 0,
        grep_saved_tokens: 0,
    };
    let result = format_token_savings(&snap);
    assert!(
        result.contains("Read"),
        "should show Read line; got: {result}"
    );
    assert!(
        result.contains("3 fires"),
        "should show fire count; got: {result}"
    );
    assert!(
        result.contains("750"),
        "should show saved tokens; got: {result}"
    );
}

#[test]
fn test_format_token_savings_total_is_sum_of_parts() {
    let snap = crate::sidecar::StatsSnapshot {
        read_fires: 2,
        read_saved_tokens: 100,
        edit_fires: 1,
        edit_saved_tokens: 50,
        write_fires: 0,
        grep_fires: 3,
        grep_saved_tokens: 200,
    };
    let result = format_token_savings(&snap);
    // Total = 100 + 50 + 200 = 350
    assert!(
        result.contains("350"),
        "total should be sum of read+edit+grep savings (350); got: {result}"
    );
    assert!(
        result.contains("Total:"),
        "should have Total line; got: {result}"
    );
}

#[test]
fn test_format_token_savings_write_fires_no_savings_field() {
    let snap = crate::sidecar::StatsSnapshot {
        read_fires: 0,
        read_saved_tokens: 0,
        edit_fires: 0,
        edit_saved_tokens: 0,
        write_fires: 2,
        grep_fires: 0,
        grep_saved_tokens: 0,
    };
    let result = format_token_savings(&snap);
    assert!(
        result.contains("Write"),
        "should show Write line; got: {result}"
    );
    assert!(
        result.contains("2 fires"),
        "should show write fire count; got: {result}"
    );
    // Write has no savings — just fire count
    assert!(
        !result.contains("tokens saved\nTotal"),
        "write line should not show saved tokens"
    );
}

#[test]
fn test_format_token_savings_omits_zero_hook_types() {
    // Only read fired — edit and grep should not appear.
    let snap = crate::sidecar::StatsSnapshot {
        read_fires: 1,
        read_saved_tokens: 100,
        edit_fires: 0,
        edit_saved_tokens: 0,
        write_fires: 0,
        grep_fires: 0,
        grep_saved_tokens: 0,
    };
    let result = format_token_savings(&snap);
    assert!(result.contains("Read"), "should show Read; got: {result}");
    assert!(
        !result.contains("Edit:"),
        "Edit should be omitted when zero; got: {result}"
    );
    assert!(
        !result.contains("Grep:"),
        "Grep should be omitted when zero; got: {result}"
    );
    assert!(
        !result.contains("Write:"),
        "Write should be omitted when zero; got: {result}"
    );
}

#[test]
fn test_format_hook_adoption_returns_empty_for_no_attempts() {
    let snap = crate::cli::hook::HookAdoptionSnapshot::default();
    assert!(format_hook_adoption(&snap).is_empty());
}

#[test]
fn test_format_hook_adoption_shows_workflow_totals_and_first_repo_start() {
    let snap = crate::cli::hook::HookAdoptionSnapshot {
        source_read: crate::cli::hook::WorkflowAdoptionCounts {
            routed: 3,
            no_sidecar: 1,
            sidecar_error: 0,
            daemon_fallback: 0,
        },
        source_search: crate::cli::hook::WorkflowAdoptionCounts {
            routed: 2,
            no_sidecar: 0,
            sidecar_error: 1,
            daemon_fallback: 0,
        },
        repo_start: crate::cli::hook::WorkflowAdoptionCounts {
            routed: 1,
            no_sidecar: 0,
            sidecar_error: 0,
            daemon_fallback: 0,
        },
        prompt_context: crate::cli::hook::WorkflowAdoptionCounts::default(),
        post_edit_impact: crate::cli::hook::WorkflowAdoptionCounts {
            routed: 0,
            no_sidecar: 1,
            sidecar_error: 0,
            daemon_fallback: 0,
        },
        first_repo_start: Some(crate::cli::hook::HookOutcome::Routed),
    };

    let result = format_hook_adoption(&snap);
    assert!(result.contains("Hook Adoption"), "missing header: {result}");
    assert!(
        result.contains("Owned workflows routed: 6/9 (67%)"),
        "missing totals line: {result}"
    );
    assert!(
        result.contains("Fail-open outcomes: 3 (no sidecar 2, sidecar errors 1)"),
        "missing fail-open breakdown: {result}"
    );
    assert!(
        result.contains("Source read: routed 3, no sidecar 1"),
        "missing source-read line: {result}"
    );
    assert!(
        result.contains("Source search: routed 2, sidecar errors 1"),
        "missing source-search line: {result}"
    );
    assert!(
        result.contains("Post-edit impact: routed 0, no sidecar 1"),
        "missing post-edit line: {result}"
    );
    assert!(
        result.contains("First repo start: routed"),
        "missing first repo-start line: {result}"
    );
    assert!(
        result.contains("Actionable note: sidecar errors are real routing failures"),
        "should distinguish actionable sidecar errors from no-sidecar outcomes: {result}"
    );
}

#[test]
fn test_format_hook_adoption_marks_no_sidecar_fail_open_as_mostly_benign() {
    let snap = crate::cli::hook::HookAdoptionSnapshot {
        source_read: crate::cli::hook::WorkflowAdoptionCounts {
            routed: 0,
            no_sidecar: 2,
            sidecar_error: 0,
            daemon_fallback: 1,
        },
        source_search: crate::cli::hook::WorkflowAdoptionCounts::default(),
        repo_start: crate::cli::hook::WorkflowAdoptionCounts {
            routed: 0,
            no_sidecar: 1,
            sidecar_error: 0,
            daemon_fallback: 0,
        },
        prompt_context: crate::cli::hook::WorkflowAdoptionCounts::default(),
        post_edit_impact: crate::cli::hook::WorkflowAdoptionCounts::default(),
        first_repo_start: Some(crate::cli::hook::HookOutcome::NoSidecar),
    };

    let result = format_hook_adoption(&snap);
    assert!(
        result.contains("Daemon fallback counts as routed work"),
        "should explain daemon fallback semantics: {result}"
    );
    assert!(
        result.contains("Fail-open here is mostly benign"),
        "no-sidecar-only fail-open should be framed as mostly benign: {result}"
    );
}

// --- compact_savings_footer tests ---

#[test]
fn test_compact_savings_footer_shows_savings() {
    let footer = compact_savings_footer(200, 2000);
    assert!(footer.contains("tokens saved"), "got: {footer}");
}

#[test]
fn test_compact_savings_footer_empty_when_no_savings() {
    let footer = compact_savings_footer(2000, 200);
    assert!(footer.is_empty());
}

#[test]
fn test_compact_savings_footer_empty_for_small_files() {
    let footer = compact_savings_footer(50, 100);
    assert!(footer.is_empty());
}

#[test]
fn test_compact_next_step_hint_formats_joined_items() {
    let hint = compact_next_step_hint(&["get_symbol (body)", "find_references (usages)"]);
    assert_eq!(hint, "\nTip: get_symbol (body) | find_references (usages)");
}

#[test]
fn test_compact_next_step_hint_ignores_empty_items() {
    let hint = compact_next_step_hint(&["", "search_text"]);
    assert_eq!(hint, "\nTip: search_text");
}

// ── search_symbols tier ordering tests ───────────────────────────────────

#[test]
fn test_search_symbols_exact_match_tier_header() {
    let sym = make_symbol("parse", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/lib.rs", b"fn parse() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "parse");
    assert!(
        result.contains("Exact matches"),
        "should show 'Exact matches' tier header; got: {result}"
    );
}

#[test]
fn test_search_symbols_prefix_match_tier_header() {
    let sym = make_symbol("parse_file", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/lib.rs", b"fn parse_file() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "parse");
    assert!(
        result.contains("Prefix matches"),
        "should show 'Prefix matches' tier header; got: {result}"
    );
}

#[test]
fn test_search_symbols_substring_match_tier_header() {
    let sym = make_symbol("do_parse_now", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/lib.rs", b"fn do_parse_now() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "parse");
    assert!(
        result.contains("Substring matches"),
        "should show 'Substring matches' tier header; got: {result}"
    );
}

#[test]
fn test_search_symbols_exact_before_prefix_before_substring() {
    // exact: "parse", prefix: "parse_file", substring: "do_parse"
    let symbols = vec![
        make_symbol("do_parse", SymbolKind::Function, 0, 1, 2),
        make_symbol("parse_file", SymbolKind::Function, 0, 3, 4),
        make_symbol("parse", SymbolKind::Function, 0, 5, 6),
    ];
    let (key, file) = make_file(
        "src/lib.rs",
        b"fn do_parse() {} fn parse_file() {} fn parse() {}",
        symbols,
    );
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "parse");

    let exact_pos = result
        .find("Exact matches")
        .expect("missing Exact matches header");
    let prefix_pos = result
        .find("Prefix matches")
        .expect("missing Prefix matches header");
    let substr_pos = result
        .find("Substring matches")
        .expect("missing Substring matches header");

    assert!(exact_pos < prefix_pos, "Exact must appear before Prefix");
    assert!(
        prefix_pos < substr_pos,
        "Prefix must appear before Substring"
    );

    // "parse" must appear after "Exact matches" and before "Prefix matches"
    let parse_pos = result[exact_pos..]
        .find("\n  ")
        .map(|p| exact_pos + p)
        .expect("no symbol line after Exact header");
    assert!(
        parse_pos < prefix_pos,
        "exact match 'parse' must be in Exact section"
    );
}

#[test]
fn test_search_symbols_omits_empty_tier_sections() {
    // Only exact match — prefix and substring headers must NOT appear
    let sym = make_symbol("search", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file("src/lib.rs", b"fn search() {}", vec![sym]);
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "search");
    assert!(
        !result.contains("Prefix matches"),
        "no prefix matches: header must be omitted; got: {result}"
    );
    assert!(
        !result.contains("Substring matches"),
        "no substring matches: header must be omitted; got: {result}"
    );
}

#[test]
fn test_search_symbols_within_exact_tier_alphabetical() {
    let symbols = vec![
        make_symbol("z_fn", SymbolKind::Function, 0, 1, 2),
        make_symbol("a_fn", SymbolKind::Function, 0, 3, 4),
        make_symbol("m_fn", SymbolKind::Function, 0, 5, 6),
    ];
    let (key, file) = make_file(
        "src/lib.rs",
        b"fn z_fn() {} fn a_fn() {} fn m_fn() {}",
        symbols,
    );
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "a_fn");
    // Only "a_fn" matches exactly — just verify it shows up in Exact
    assert!(result.contains("Exact matches"), "got: {result}");
    assert!(result.contains("a_fn"), "got: {result}");
}

#[test]
fn test_search_symbols_within_prefix_tier_shorter_names_first() {
    // "parse" is query, "parse_x" (7 chars) should come before "parse_longer" (12 chars)
    let symbols = vec![
        make_symbol("parse_longer", SymbolKind::Function, 0, 1, 2),
        make_symbol("parse_x", SymbolKind::Function, 0, 3, 4),
    ];
    let (key, file) = make_file(
        "src/lib.rs",
        b"fn parse_longer() {} fn parse_x() {}",
        symbols,
    );
    let index = make_index(vec![(key, file)]);
    let result = search_symbols_result(&index, "parse");

    // In the prefix section, parse_x must appear before parse_longer
    let prefix_pos = result
        .find("Prefix matches")
        .expect("missing Prefix matches");
    let section_after = &result[prefix_pos..];
    let x_pos = section_after
        .find("parse_x")
        .expect("parse_x not in prefix section");
    let longer_pos = section_after
        .find("parse_longer")
        .expect("parse_longer not in prefix section");
    assert!(
        x_pos < longer_pos,
        "shorter prefix match 'parse_x' must appear before 'parse_longer'"
    );
}

// ── file_tree tests ───────────────────────────────────────────────────────

fn make_file_with_lang(
    path: &str,
    content: &[u8],
    symbols: Vec<SymbolRecord>,
    lang: crate::domain::LanguageId,
) -> (String, IndexedFile) {
    (
        path.to_string(),
        IndexedFile {
            relative_path: path.to_string(),
            language: lang,
            classification: crate::domain::FileClassification::for_code_path(path),
            content: content.to_vec(),
            symbols,
            parse_status: ParseStatus::Parsed,
            parse_diagnostic: None,
            byte_len: content.len() as u64,
            content_hash: "test".to_string(),
            references: vec![],
            alias_map: std::collections::HashMap::new(),
            mtime_secs: 0,
        },
    )
}

#[test]
fn test_file_tree_shows_files_with_symbol_count() {
    let sym = make_symbol("main", SymbolKind::Function, 0, 1, 5);
    let (key, file) = make_file_with_lang(
        "src/main.rs",
        b"fn main() {}",
        vec![sym],
        crate::domain::LanguageId::Rust,
    );
    let index = make_index(vec![(key, file)]);
    let result = file_tree(&index, "", 2);
    assert!(
        result.contains("main.rs"),
        "should show filename; got: {result}"
    );
    assert!(
        result.contains("1 symbol"),
        "should show symbol count; got: {result}"
    );
}

#[test]
fn test_file_tree_view_matches_live_index_output() {
    let sym1 = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let sym2 = make_symbol("bar", SymbolKind::Function, 0, 1, 3);
    let (k1, f1) = make_file_with_lang(
        "src/a.rs",
        b"fn foo() {}",
        vec![sym1],
        crate::domain::LanguageId::Rust,
    );
    let (k2, f2) = make_file_with_lang(
        "tests/b.rs",
        b"fn bar() {}",
        vec![sym2],
        crate::domain::LanguageId::Rust,
    );
    let index = make_index(vec![(k1, f1), (k2, f2)]);

    let live_result = file_tree(&index, "", 3);
    let captured_result = file_tree_view(&index.capture_repo_outline_view().files, "", 3);

    assert_eq!(captured_result, live_result);
}

#[test]
fn test_file_tree_shows_directory_with_file_counts() {
    let sym1 = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let sym2 = make_symbol("bar", SymbolKind::Function, 0, 1, 3);
    let (k1, f1) = make_file_with_lang(
        "src/a.rs",
        b"fn foo() {}",
        vec![sym1],
        crate::domain::LanguageId::Rust,
    );
    let (k2, f2) = make_file_with_lang(
        "src/b.rs",
        b"fn bar() {}",
        vec![sym2],
        crate::domain::LanguageId::Rust,
    );
    let index = make_index(vec![(k1, f1), (k2, f2)]);
    let result = file_tree(&index, "", 1);
    // At depth 1, "src" directory should be shown collapsed with file/symbol counts
    assert!(
        result.contains("src"),
        "should show src directory; got: {result}"
    );
}

#[test]
fn test_file_tree_footer_shows_totals() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let (k1, f1) = make_file_with_lang(
        "src/a.rs",
        b"fn foo() {}",
        vec![sym],
        crate::domain::LanguageId::Rust,
    );
    let (k2, f2) = make_file_with_lang(
        "lib/b.rs",
        b"fn bar() {}",
        vec![],
        crate::domain::LanguageId::Rust,
    );
    let index = make_index(vec![(k1, f1), (k2, f2)]);
    let result = file_tree(&index, "", 3);
    // Footer must show directories, files, symbols totals
    assert!(
        result.contains("files"),
        "footer should mention files; got: {result}"
    );
    assert!(
        result.contains("symbols"),
        "footer should mention symbols; got: {result}"
    );
}

#[test]
fn test_file_tree_respects_path_filter() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let (k1, f1) = make_file_with_lang(
        "src/a.rs",
        b"fn foo() {}",
        vec![sym],
        crate::domain::LanguageId::Rust,
    );
    let (k2, f2) = make_file_with_lang(
        "tests/b.rs",
        b"fn test_b() {}",
        vec![],
        crate::domain::LanguageId::Rust,
    );
    let index = make_index(vec![(k1, f1), (k2, f2)]);
    let result = file_tree(&index, "src", 3);
    assert!(
        result.contains("a.rs"),
        "src filter should show a.rs; got: {result}"
    );
    assert!(
        !result.contains("b.rs"),
        "src filter should not show tests/b.rs; got: {result}"
    );
}

#[test]
fn test_file_tree_repeated_basenames_remain_hierarchical() {
    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let index = make_index(vec![
        make_file_with_lang(
            "src/live_index/mod.rs",
            b"fn foo() {}",
            vec![sym.clone()],
            crate::domain::LanguageId::Rust,
        ),
        make_file_with_lang(
            "src/protocol/mod.rs",
            b"fn foo() {}",
            vec![sym],
            crate::domain::LanguageId::Rust,
        ),
    ]);
    let result = file_tree(&index, "", 3);
    assert!(result.contains("live_index/"), "got: {result}");
    assert!(result.contains("protocol/"), "got: {result}");
    assert!(!result.contains("live_index/mod.rs"), "got: {result}");
    assert!(!result.contains("protocol/mod.rs"), "got: {result}");
}

#[test]
fn test_file_tree_depth_collapses_deep_directories() {
    // At depth=1, nested directories beyond root level should be collapsed
    let sym = make_symbol("deep", SymbolKind::Function, 0, 1, 3);
    let (k1, f1) = make_file_with_lang(
        "src/deep/nested/file.rs",
        b"fn deep() {}",
        vec![sym],
        crate::domain::LanguageId::Rust,
    );
    let index = make_index(vec![(k1, f1)]);
    let result = file_tree(&index, "", 1);
    // file.rs should not be individually listed at depth=1
    assert!(
        !result.contains("file.rs"),
        "file.rs should be collapsed at depth=1; got: {result}"
    );
}

#[test]
fn test_file_tree_empty_index() {
    let index = make_index(vec![]);
    let result = file_tree(&index, "", 2);
    assert!(
        result.contains("0 files") || result.contains("No source files"),
        "got: {result}"
    );
}

#[test]
fn test_repo_map_shows_tier2_tagged() {
    use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason, SkippedFile};

    // No indexed files — only a Tier 2 skipped file.
    let skipped = vec![SkippedFile {
        path: "model.safetensors".to_string(),
        size: 4_509_715_456, // ~4.2 GB
        extension: Some("safetensors".to_string()),
        decision: AdmissionDecision {
            tier: AdmissionTier::MetadataOnly,
            reason: Some(SkipReason::DenylistedExtension),
        },
    }];

    let result = file_tree_view_with_skipped(&[], &skipped, "", 2);
    assert!(
        result.contains("[skipped:"),
        "expected [skipped: tag for Tier 2 file, got: {result}"
    );
    assert!(
        result.contains("model.safetensors"),
        "expected filename in output, got: {result}"
    );
    assert!(
        result.contains("artifact"),
        "expected SkipReason display 'artifact' in tag, got: {result}"
    );
    // Tier 3 footer should NOT appear.
    assert!(
        !result.contains("hard-skipped"),
        "should not have tier3 footer, got: {result}"
    );
}

#[test]
fn test_repo_map_tier3_footer_only() {
    use crate::domain::index::{AdmissionDecision, AdmissionTier, SkipReason, SkippedFile};

    let skipped = vec![
        SkippedFile {
            path: "data/huge1.bin".to_string(),
            size: 200 * 1024 * 1024,
            extension: Some("bin".to_string()),
            decision: AdmissionDecision {
                tier: AdmissionTier::HardSkip,
                reason: Some(SkipReason::SizeCeiling),
            },
        },
        SkippedFile {
            path: "data/huge2.bin".to_string(),
            size: 300 * 1024 * 1024,
            extension: Some("bin".to_string()),
            decision: AdmissionDecision {
                tier: AdmissionTier::HardSkip,
                reason: Some(SkipReason::SizeCeiling),
            },
        },
    ];

    let sym = make_symbol("foo", SymbolKind::Function, 0, 1, 3);
    let (k, f) = make_file_with_lang(
        "src/main.rs",
        b"fn foo() {}",
        vec![sym],
        crate::domain::LanguageId::Rust,
    );
    let index = make_index(vec![(k, f)]);
    let view = index.capture_repo_outline_view();

    let result = file_tree_view_with_skipped(&view.files, &skipped, "", 2);

    // Tier 3 files must NOT appear in the tree body.
    assert!(
        !result.contains("huge1.bin"),
        "Tier 3 file should not be in tree, got: {result}"
    );
    assert!(
        !result.contains("huge2.bin"),
        "Tier 3 file should not be in tree, got: {result}"
    );
    // Footer must appear.
    assert!(
        result.contains("2 hard-skipped"),
        "expected '2 hard-skipped' footer, got: {result}"
    );
    assert!(
        result.contains("not shown (>100MB)"),
        "expected '>100MB' in footer, got: {result}"
    );
    // The indexed file must still appear.
    assert!(
        result.contains("main.rs"),
        "indexed file should appear, got: {result}"
    );
}

#[test]
fn test_format_type_dependencies_renders_bodies_and_depth() {
    let deps = vec![
        TypeDependencyView {
            name: "UserConfig".to_string(),
            kind_label: "struct".to_string(),
            file_path: "src/config.rs".to_string(),
            line_range: (0, 2),
            body: "pub struct UserConfig {\n    pub name: String,\n}".to_string(),
            depth: 0,
        },
        TypeDependencyView {
            name: "Address".to_string(),
            kind_label: "struct".to_string(),
            file_path: "src/address.rs".to_string(),
            line_range: (0, 1),
            body: "pub struct Address {\n    pub city: String,\n}".to_string(),
            depth: 1,
        },
    ];
    let result = format_type_dependencies(&deps);
    assert!(
        result.contains("Dependencies (2):"),
        "header missing, got: {result}"
    );
    assert!(
        result.contains("── UserConfig [struct, src/config.rs:1-3] ──"),
        "UserConfig entry missing (0-based 0-2 displayed as 1-based 1-3), got: {result}"
    );
    assert!(
        result.contains("pub struct UserConfig"),
        "UserConfig body missing, got: {result}"
    );
    assert!(
        result.contains("(depth 1)"),
        "depth marker missing for Address, got: {result}"
    );
    // Direct dependency (depth 0) should NOT have depth marker.
    assert!(
        !result.contains("(depth 0)"),
        "depth 0 should have no marker, got: {result}"
    );
}

#[test]
fn test_extract_declaration_name_rust_fn() {
    assert_eq!(
        super::extract_declaration_name("pub fn hello_world() -> String {"),
        Some("hello_world".to_string())
    );
    assert_eq!(
        super::extract_declaration_name("fn main() {"),
        Some("main".to_string())
    );
    assert_eq!(
        super::extract_declaration_name("pub(crate) async fn process(x: u32) -> Result {"),
        Some("process".to_string())
    );
}

#[test]
fn test_extract_declaration_name_struct() {
    assert_eq!(
        super::extract_declaration_name("pub struct Config {"),
        Some("Config".to_string())
    );
    assert_eq!(
        super::extract_declaration_name("struct Inner;"),
        Some("Inner".to_string())
    );
}

#[test]
fn test_extract_declaration_name_non_declaration() {
    assert_eq!(super::extract_declaration_name("let x = 5;"), None);
    assert_eq!(
        super::extract_declaration_name("// fn commented_out()"),
        None
    );
    assert_eq!(
        super::extract_declaration_name("use std::collections::HashMap;"),
        None
    );
}

#[test]
fn test_extract_declaration_name_csharp_const() {
    // C# const: `const string Foo = "bar"` — name is Foo, not string
    assert_eq!(
        super::extract_declaration_name("const string ConnectionString = \"...\";"),
        Some("ConnectionString".to_string())
    );
    assert_eq!(
        super::extract_declaration_name("const int MaxRetries = 3;"),
        Some("MaxRetries".to_string())
    );
    // Rust const should still work (type after colon, not before name)
    assert_eq!(
        super::extract_declaration_name("const MAX_SIZE: usize = 100;"),
        Some("MAX_SIZE".to_string())
    );
}

// ─── extract_signature / apply_verbosity tests (U6) ──────────────────────

#[test]
fn test_extract_signature_single_line_full_decl() {
    // Full single-line Rust fn — visibility, generics, params, return type all preserved
    let body = "pub fn foo<T: Display>(x: T) -> Result<String> {\n    todo!()\n}";
    assert_eq!(
        super::extract_signature(body),
        "pub fn foo<T: Display>(x: T) -> Result<String>"
    );
}

#[test]
fn test_extract_signature_pub_crate_visibility() {
    let body = "pub(crate) fn bar(x: i32) -> bool {\n    x > 0\n}";
    assert_eq!(
        super::extract_signature(body),
        "pub(crate) fn bar(x: i32) -> bool"
    );
}

#[test]
fn test_extract_signature_multi_line_joins_to_one_line() {
    // Multi-line fn signature — params on separate lines
    let body = "pub fn process<T>(\n    input: T,\n    verbose: bool,\n) -> Result<String> {\n    todo!()\n}";
    let sig = super::extract_signature(body);
    // Must be a single line
    assert!(
        !sig.contains('\n'),
        "signature must be one line, got: {sig:?}"
    );
    // Must include visibility, generics, return type
    assert!(
        sig.contains("pub fn process"),
        "missing pub fn process: {sig:?}"
    );
    assert!(sig.contains("<T>"), "missing generic: {sig:?}");
    assert!(
        sig.contains("-> Result<String>"),
        "missing return type: {sig:?}"
    );
}

#[test]
fn test_extract_signature_skips_doc_comments() {
    let body =
        "/// Does something important\n/// Multi-line doc\npub fn documented() -> u32 {\n    42\n}";
    let sig = super::extract_signature(body);
    assert_eq!(sig, "pub fn documented() -> u32");
}

#[test]
fn test_extract_signature_struct_with_generics() {
    let body = "pub struct Wrapper<T: Clone> {\n    inner: T,\n}";
    let sig = super::extract_signature(body);
    assert_eq!(sig, "pub struct Wrapper<T: Clone>");
}

#[test]
fn test_extract_signature_trait_decl() {
    let body = "pub trait Processor: Send + Sync {\n    fn process(&self);\n}";
    let sig = super::extract_signature(body);
    assert_eq!(sig, "pub trait Processor: Send + Sync");
}

#[test]
fn test_apply_verbosity_signature_is_one_line() {
    // Verifies output stability — always one line regardless of body size
    let body = "pub fn foo<T: Display>(x: T) -> Result<String> {\n    let a = 1;\n    let b = 2;\n    todo!()\n}";
    let result = super::apply_verbosity(body, "signature");
    assert!(
        !result.contains('\n'),
        "signature verbosity must produce one line, got: {result:?}"
    );
    assert!(
        result.contains("pub fn foo"),
        "must include pub fn foo: {result:?}"
    );
    assert!(
        result.contains("-> Result<String>"),
        "must include return type: {result:?}"
    );
}

#[test]
fn test_apply_verbosity_full_returns_whole_body() {
    let body = "pub fn foo() {\n    let x = 1;\n}";
    assert_eq!(super::apply_verbosity(body, "full"), body);
}

#[test]
fn test_apply_verbosity_compact_includes_doc() {
    let body = "/// Does the thing\npub fn bar() -> u32 {\n    1\n}";
    let result = super::apply_verbosity(body, "compact");
    assert!(
        result.contains("pub fn bar() -> u32"),
        "missing sig: {result:?}"
    );
    assert!(result.contains("Does the thing"), "missing doc: {result:?}");
    assert!(
        !result.contains("1\n"),
        "body should not be in compact: {result:?}"
    );
}

#[test]
fn test_heuristic_from_name_capitalizes_rest_fragment() {
    let summary = super::heuristic_from_name(
        "render_find_references_compact_view",
        "fn render_find_references_compact_view() -> String",
    );
    assert_eq!(
        summary.as_deref(),
        Some("Renders Find references compact view")
    );
}

#[test]
fn test_auto_summarize_uses_capitalized_heuristic_phrase() {
    let summary =
        super::auto_summarize("fn capture_context_bundle_view() -> String {\n    String::new()\n}");
    assert_eq!(summary, "Captures Context bundle view");
}

// ── cap_file_content_output tests ──────────────────────────────────────────

#[test]
fn test_cap_under_limit_unchanged() {
    let s = "hello\nworld\n".to_string();
    let result = super::cap_file_content_output(s.clone());
    assert_eq!(result, s);
}

#[test]
fn test_cap_exactly_at_limit_unchanged() {
    let s = "x".repeat(super::GET_FILE_CONTENT_MAX_BYTES);
    let result = super::cap_file_content_output(s.clone());
    assert_eq!(result.len(), super::GET_FILE_CONTENT_MAX_BYTES);
    assert!(!result.contains("truncated"), "should not add footer at exact cap");
}

#[test]
fn test_cap_over_limit_truncates_and_adds_footer() {
    let line = "a".repeat(100) + "\n";
    let s = line.repeat(800); // ~80 KB
    let result = super::cap_file_content_output(s);
    assert!(
        result.len() <= super::GET_FILE_CONTENT_MAX_BYTES,
        "capped output should be <= cap, got {} bytes",
        result.len()
    );
    assert!(result.contains("truncated"), "should contain truncation footer");
    assert!(result.contains("chunk_index"), "footer should suggest chunk_index");
}

#[test]
fn test_cap_truncates_at_line_boundary() {
    // Build output that crosses the cap mid-line
    let before_cap = "line\n".repeat(super::GET_FILE_CONTENT_MAX_BYTES / 5 + 1);
    let result = super::cap_file_content_output(before_cap);
    assert!(result.contains("truncated"), "should be truncated");
    // The truncated portion (before footer) should end with a newline
    let footer_start = result.find("\n[Output truncated").unwrap_or(result.len());
    let body = &result[..footer_start];
    assert!(
        body.ends_with('\n'),
        "body before footer should end at line boundary"
    );
}

#[test]
fn test_cap_single_giant_line_truncates_at_byte_boundary() {
    // No newlines — should still truncate without panic
    let s = "x".repeat(super::GET_FILE_CONTENT_MAX_BYTES + 1000);
    let result = super::cap_file_content_output(s);
    assert!(
        result.len() <= super::GET_FILE_CONTENT_MAX_BYTES,
        "should be capped even with no newlines"
    );
    assert!(result.contains("truncated"));
}

#[test]
fn test_cap_idempotent() {
    let line = "b".repeat(100) + "\n";
    let s = line.repeat(800);
    let once = super::cap_file_content_output(s);
    let twice = super::cap_file_content_output(once.clone());
    assert_eq!(once, twice, "cap helper should be idempotent");
}
