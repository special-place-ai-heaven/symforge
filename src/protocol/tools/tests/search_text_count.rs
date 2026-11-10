//! SF-TE-07 / SF-TE-08: `search_text(group_by="count")` and
//! `search_text(group_by="symbol", include_body=true)` on a real on-disk index.
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use rmcp::handler::server::wrapper::Parameters;

use super::super::{SearchTextInput, SymForgeServer};
use crate::live_index::LiveIndex;

const NEEDLE: &str = "needle";

/// More than 50 hits overall and more than 5 per file, so the default display
/// caps (limit 50, max_per_file 5) would cut every count below.
fn write_fixture(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    let mut many = String::from("pub fn many() {\n");
    for i in 0..60 {
        many.push_str(&format!("    let needle_{i} = {i};\n"));
    }
    many.push_str("}\n\n#[cfg(test)]\nmod tests {\n");
    for i in 0..3 {
        many.push_str(&format!("    fn t{i}() {{ let needle = {i}; }}\n"));
    }
    many.push_str("}\n");
    std::fs::write(root.join("src/many.rs"), many).unwrap();
    std::fs::write(
        root.join("src/few.rs"),
        "pub fn first() {\n    let marker_body_only = 0;\n    let needle_a = 1;\n    let needle_b = 2;\n}\n\npub fn second() {\n    let needle_c = 3;\n    let needle_d = 4;\n    let needle_e = 5;\n    let needle_f = 6;\n    let needle_g = 7;\n}\n",
    )
    .unwrap();
    // Capitalised only: a case-sensitive count must not see it.
    std::fs::write(root.join("src/upper.py"), "def f():\n    return 'Needle'\n").unwrap();
    std::fs::write(
        root.join("tests/it_test.rs"),
        "fn a() { let needle = 1; }\nfn b() { let needle = 2; }\nfn c() { let needle = 3; }\nfn d() { let needle = 4; }\n",
    )
    .unwrap();
}

/// Independent oracle with `rg -F -s --count` semantics: matching lines per file.
fn oracle(root: &Path) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for dir in ["src", "tests"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            let text = std::fs::read_to_string(&path).unwrap();
            let n = text.lines().filter(|line| line.contains(NEEDLE)).count();
            if n > 0 {
                let name = path.file_name().unwrap().to_string_lossy();
                counts.insert(format!("{dir}/{name}"), n);
            }
        }
    }
    counts
}

/// `rg --count` itself when it is on PATH; `None` otherwise.
fn ripgrep_counts(root: &Path) -> Option<BTreeMap<String, usize>> {
    let output = crate::process_util::hidden_command("rg")
        .args(["--count", "-F", "-s", "--no-ignore", NEEDLE, "src", "tests"])
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(
        text.lines()
            .filter_map(|line| {
                let (path, n) = line.rsplit_once(':')?;
                Some((path.replace('\\', "/"), n.parse().ok()?))
            })
            .collect(),
    )
}

fn parse_counts(rendered: &str) -> BTreeMap<String, usize> {
    rendered
        .lines()
        .filter_map(|line| {
            let (path, n) = line.rsplit_once(": ")?;
            let path_like = path.contains('/') && !path.contains(' ');
            path_like.then_some(())?;
            Some((path.to_string(), n.trim().parse().ok()?))
        })
        .collect()
}

fn server_for(root: &Path) -> SymForgeServer {
    let index = LiveIndex::load(root).expect("load fixture index");
    SymForgeServer::new(
        index,
        "search-text-count".to_string(),
        Arc::new(parking_lot::Mutex::new(
            crate::watcher::WatcherInfo::default(),
        )),
        Some(root.canonicalize().unwrap()),
        None,
    )
}

fn count_input() -> SearchTextInput {
    SearchTextInput {
        query: Some(NEEDLE.to_string()),
        case_sensitive: Some(true),
        group_by: Some("count".to_string()),
        ..Default::default()
    }
}

#[tokio::test]
async fn search_text_count_equals_ripgrep_and_honours_filters() {
    let dir = tempfile::TempDir::new().unwrap();
    write_fixture(dir.path());
    let expected = oracle(dir.path());
    assert!(expected.values().sum::<usize>() > 50 && expected["src/many.rs"] > 5);
    if let Some(rg) = ripgrep_counts(dir.path()) {
        assert_eq!(rg, expected, "fixture oracle must agree with rg --count");
    }
    let server = server_for(dir.path());

    // Tests included: every file, exact counts, despite limit/max_per_file = 1.
    let all = server
        .search_text(Parameters(SearchTextInput {
            include_tests: Some(true),
            limit: Some(1),
            max_per_file: Some(1),
            ..count_input()
        }))
        .await;
    assert_eq!(parse_counts(&all), expected, "{all}");
    let total: usize = expected.values().sum();
    assert!(
        all.contains(&format!("total: {total} in {} files", expected.len())),
        "{all}"
    );

    // Default (tests excluded): the test file and the #[cfg(test)] lines drop
    // out, and the response says how many and how to get them.
    let default = server.search_text(Parameters(count_input())).await;
    let mut visible = expected.clone();
    visible.remove("tests/it_test.rs");
    *visible.get_mut("src/many.rs").unwrap() -= 3;
    assert_eq!(parse_counts(&default), visible, "{default}");
    assert!(
        default
            .contains("7 more match(es) in test code are excluded; retry with include_tests=true"),
        "{default}"
    );

    // Filters still apply: glob and path_prefix.
    let globbed = server
        .search_text(Parameters(SearchTextInput {
            include_tests: Some(true),
            glob: Some("tests/**".to_string()),
            ..count_input()
        }))
        .await;
    assert_eq!(
        parse_counts(&globbed),
        BTreeMap::from([("tests/it_test.rs".to_string(), 4)]),
        "{globbed}"
    );
    let scoped = server
        .search_text(Parameters(SearchTextInput {
            path_prefix: Some("src/few.rs".to_string()),
            ..count_input()
        }))
        .await;
    assert_eq!(
        parse_counts(&scoped),
        BTreeMap::from([("src/few.rs".to_string(), 7)]),
        "{scoped}"
    );
    assert!(!scoped.contains("more match(es) in test code"), "{scoped}");
}

#[tokio::test]
async fn search_text_include_body_returns_each_enclosing_symbol_once() {
    let dir = tempfile::TempDir::new().unwrap();
    write_fixture(dir.path());
    let server = server_for(dir.path());
    let request = |include_body| SearchTextInput {
        query: Some(NEEDLE.to_string()),
        path_prefix: Some("src/few.rs".to_string()),
        group_by: Some("symbol".to_string()),
        include_body,
        ..Default::default()
    };

    let with_body = server.search_text(Parameters(request(Some(true)))).await;
    // One call yields both bodies, including lines that do not match the query.
    assert_eq!(
        with_body.matches("let marker_body_only = 0;").count(),
        1,
        "{with_body}"
    );
    assert_eq!(
        with_body.matches("let needle_g = 7;").count(),
        1,
        "{with_body}"
    );
    assert!(with_body.contains("pub fn second() {"), "{with_body}");

    let without = server.search_text(Parameters(request(None))).await;
    assert!(!without.contains("marker_body_only"), "{without}");
    assert!(without.contains("second"), "{without}");

    // Compaction keeps 10 ordinary lines per file, best-scored first, plus every
    // error line. `holder`'s first match loses to `needle_rich`'s lines and is
    // dropped; `holder` survives through its error line and must keep its body.
    let mut capped = String::from(
        "pub fn holder() {\n    let needle_plain = 0;\n    let marker_holder_body = 1;\n    panic!(\"needle failed\");\n}\n\npub fn needle_rich() {\n",
    );
    for i in 0..11 {
        capped.push_str(&format!("    let needle_{i} = {i};\n"));
    }
    capped.push_str("}\n");
    std::fs::write(dir.path().join("src/capped.rs"), capped).unwrap();
    let server = server_for(dir.path());
    let compacted = server
        .search_text(Parameters(SearchTextInput {
            path_prefix: Some("src/capped.rs".to_string()),
            max_per_file: Some(50),
            ..request(Some(true))
        }))
        .await;
    assert!(compacted.contains("fn holder"), "{compacted}");
    assert!(
        compacted.contains("let marker_holder_body = 1;"),
        "{compacted}"
    );
}

#[tokio::test]
async fn search_text_count_leads_with_total_and_refuses_dropped_modes() {
    let dir = tempfile::TempDir::new().unwrap();
    write_fixture(dir.path());
    let server = server_for(dir.path());
    let counted = server.search_text(Parameters(count_input())).await;
    let total_at = counted.find("total: ").expect("total line");
    let first_row = counted.find("src/few.rs: ").expect("per-file row");
    assert!(
        total_at < first_row,
        "total must precede per-file rows: {counted}"
    );

    for (input, needle) in [
        (
            SearchTextInput {
                context: Some(1),
                ..count_input()
            },
            "group_by=\"count\" cannot be combined",
        ),
        (
            SearchTextInput {
                query: Some(NEEDLE.to_string()),
                group_by: Some("symbol".to_string()),
                include_body: Some(true),
                context: Some(2),
                ..Default::default()
            },
            "include_body=true requires group_by=\"symbol\"",
        ),
        (
            SearchTextInput {
                query: Some(NEEDLE.to_string()),
                include_body: Some(true),
                ..Default::default()
            },
            "include_body=true requires group_by=\"symbol\"",
        ),
    ] {
        let refused = server.search_text(Parameters(input)).await;
        assert!(refused.contains(needle), "{refused}");
    }
}
