//! MCP side of the per-tool edit answer golden shared with
//! `tests/embed_edit_bodies.rs` (`tests/fixtures/edit_parity/tools.json`):
//! every MCP edit tool, previewed and applied, on a fresh copy of the fixture
//! repository. Tee snapshot paths are timestamped, so both sides compare them
//! as `<tee>`. The observed answers are written to
//! `target/edit-body-parity.observed.json`.
#![cfg(feature = "server")]

use std::path::Path;

use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;

/// Replace each tee snapshot path with `<tee>`; the rest of the line stays.
fn normalize_tee(text: &str) -> String {
    text.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            match trimmed
                .strip_prefix("Tee snapshot: `")
                .and_then(|rest| rest.split_once('`'))
            {
                Some((_, tail)) => format!("{indent}Tee snapshot: `<tee>`{tail}"),
                None => line.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn fixture_repo(files: &serde_json::Map<String, serde_json::Value>) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    git2::Repository::init(dir.path()).expect("git init");
    std::fs::write(dir.path().join(".gitignore"), ".symforge/\n").expect("gitignore");
    for (path, content) in files {
        let file = dir.path().join(path);
        std::fs::create_dir_all(file.parent().unwrap()).expect("parent");
        std::fs::write(file, content.as_str().unwrap()).expect("file");
    }
    dir
}

fn server_for_repo(root: &Path) -> SymForgeServer {
    SymForgeServer::new(
        LiveIndex::load(root).expect("index"),
        "edit-body-parity".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.to_path_buf()),
        None,
    )
}

#[tokio::test]
async fn edit_tool_answers_match_embed_parity_golden() {
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/edit_parity/tools.json");
    let mut fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture_path).unwrap()).unwrap();
    let expected = fixture.clone();
    let files = fixture["files"].as_object().unwrap().clone();
    for case in fixture["cases"].as_array_mut().unwrap() {
        let repo = fixture_repo(&files);
        let server = server_for_repo(repo.path());
        let result = server
            .dispatch_tool_result_for_tests(case["tool"].as_str().unwrap(), case["input"].clone())
            .await
            .expect("edit tool dispatch");
        let result = serde_json::to_value(&result).unwrap();
        let text = result["content"][0]["text"].as_str().expect("text answer");
        case["expected"] = serde_json::Value::String(normalize_tee(text));
    }
    let observed =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/edit-body-parity.observed.json");
    let _ = std::fs::write(&observed, serde_json::to_string_pretty(&fixture).unwrap());
    assert_eq!(
        fixture,
        expected,
        "MCP edit answers drifted from the shared golden; see {}",
        observed.display()
    );
}

/// MCP's `batch_edit` grouped files in a hash map and listed them in its
/// iteration order, which differs between calls, so one multi-file batch
/// rendered differently from run to run. It now lists files in path order,
/// as `batch_insert` always has, and as the embedded lane does.
#[tokio::test]
async fn multi_file_batch_edit_lists_files_in_path_order_every_time() {
    let mut files = serde_json::Map::new();
    for name in ["d", "a", "c", "b"] {
        files.insert(
            format!("src/{name}.rs"),
            serde_json::Value::String(format!("pub fn {name}_fn() -> u32 {{\n    1\n}}\n")),
        );
    }
    let edits: Vec<serde_json::Value> = ["d", "a", "c", "b"]
        .iter()
        .map(|name| {
            serde_json::json!({
                "path": format!("src/{name}.rs"),
                "name": format!("{name}_fn"),
                "operation": {"type": "replace", "new_body": format!("pub fn {name}_fn() -> u32 {{\n    2\n}}")}
            })
        })
        .collect();
    for dry_run in [true, false] {
        for _ in 0..8 {
            let repo = fixture_repo(&files);
            let server = server_for_repo(repo.path());
            let result = server
                .dispatch_tool_result_for_tests(
                    "batch_edit",
                    serde_json::json!({"edits": edits, "dry_run": dry_run}),
                )
                .await
                .expect("batch_edit dispatch");
            let result = serde_json::to_value(&result).unwrap();
            let text = result["content"][0]["text"].as_str().unwrap();
            let order: Vec<&str> = ["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs"]
                .into_iter()
                .filter(|path| text.contains(&format!("{path} — replaced")))
                .collect();
            assert_eq!(order.len(), 4, "{text}");
            let positions: Vec<usize> = order
                .iter()
                .map(|path| text.find(&format!("{path} — replaced")).unwrap())
                .collect();
            assert!(
                positions.windows(2).all(|pair| pair[0] < pair[1]),
                "files must be listed in path order:\n{text}"
            );
        }
    }
}
