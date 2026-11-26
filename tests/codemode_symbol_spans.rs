#![cfg(feature = "server")]

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::codemode::{CodeReadBackend, CodeReadOp, LocalCodeBackend};
use symforge::live_index::{LiveIndex, SharedIndex};
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

fn fixture(path: &str, source: &str) -> (TempDir, SharedIndex) {
    let root = TempDir::new().unwrap();
    std::fs::write(root.path().join(path), source).unwrap();
    let index = LiveIndex::load(root.path()).unwrap();
    (root, index)
}

fn read(backend: &LocalCodeBackend, args: Value) -> Value {
    backend
        .execute_op(CodeReadOp::GetSymbol, args.as_object().unwrap().clone())
        .unwrap()
}

#[test]
fn javascript_family_returns_complete_declarations_without_moving_selectors() {
    for path in ["shapes.js", "shapes.ts", "shapes.tsx"] {
        let source = "// unrelated\n/** Render a tile. */\nexport /*\n*/ function tile() {}\n\n/** Size in cells. */\nexport const cells = 4;\nconst local = 2;\n";
        let (root, index) = fixture(path, source);
        let file = index.read().capture_shared_file(path).unwrap();
        let backend = LocalCodeBackend::new(index, Some(root.path().into()), "symbol-spans", None);
        let rows = read(
            &backend,
            json!({"targets": [
                {"path": path, "name": "tile", "symbol_line": 4},
                {"path": path, "name": "cells", "symbol_line": 7},
                {"path": path, "name": "local", "symbol_line": 8}
            ]}),
        );
        for (row, name, start, end, expected) in [
            (
                0,
                "tile",
                2,
                4,
                "/** Render a tile. */\nexport /*\n*/ function tile() {}",
            ),
            (
                1,
                "cells",
                6,
                7,
                "/** Size in cells. */\nexport const cells = 4;",
            ),
            (2, "local", 8, 8, "const local = 2;"),
        ] {
            assert_eq!(rows[row]["source"], expected, "{path}: {name}");
            assert_eq!(rows[row]["start_line"], start);
            assert_eq!(rows[row]["end_line"], end);
            let classic = symforge::protocol::format::symbol_detail_from_indexed_file(
                &file, name, None, None,
            );
            assert!(classic.starts_with(expected), "{classic}");
        }
        let cells = file.symbols.iter().find(|sym| sym.name == "cells").unwrap();
        let (start, end) = cells.core_range();
        assert_eq!(&source[start as usize..end as usize], "cells = 4");
    }
}

#[tokio::test]
async fn multi_declarator_display_does_not_widen_edit_within_to_sibling() {
    let path = "palette.ts";
    let declaration = "/** Two colors. */\nexport const first = 'red',\n    second = 'blue';";
    let source = format!("{declaration}\n");
    let (root, index) = fixture(path, &source);
    let server = SymForgeServer::new(
        index.clone(),
        "symbol-spans".to_owned(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root.path().into()),
        None,
    );
    let backend = LocalCodeBackend::new(index, Some(root.path().into()), "symbol-spans", None);
    let read_row = read(
        &backend,
        json!({"path": path, "name": "second", "symbol_line": 3}),
    );
    let rejected = server
        .dispatch_tool_result_for_tests(
            "edit_within_symbol",
            json!({"path": path, "name": "second", "symbol_line": 3, "old_text": "red", "new_text": "green"}),
        )
        .await
        .unwrap();
    assert_eq!(rejected.is_error, Some(true), "{rejected:?}");
    assert_eq!(
        std::fs::read_to_string(root.path().join(path)).unwrap(),
        source
    );
    let accepted = server
        .dispatch_tool_result_for_tests(
            "edit_within_symbol",
            json!({"path": path, "name": "second", "symbol_line": 3, "old_text": "blue", "new_text": "green"}),
        )
        .await
        .unwrap();
    assert_ne!(accepted.is_error, Some(true), "{accepted:?}");
    assert_eq!(
        std::fs::read_to_string(root.path().join(path)).unwrap(),
        source.replace("blue", "green")
    );
    assert_eq!(read_row[0]["source"], declaration);
    assert_eq!(read_row[0]["start_line"], 1);
    assert_eq!(read_row[0]["end_line"], 3);
}

#[test]
fn symbol_and_raw_slice_report_returned_line_span_and_utf8_truncation() {
    let path = "shapes.rs";
    let source = "\n/// 🦀 shape docs\nfn tile() {\n    let _size = 3;\n}\n";
    let (root, index) = fixture(path, source);
    let backend = LocalCodeBackend::new(index, Some(root.path().into()), "symbol-spans", None);
    let targets = json!([
        {"path": path, "name": "tile", "symbol_line": 3},
        {"path": path, "start_byte": 1}
    ]);
    let invalid = json!({"targets": targets, "max_tokens": 0});
    let refusal = backend
        .execute_op(CodeReadOp::GetSymbol, invalid.as_object().unwrap().clone())
        .unwrap_err();
    assert_eq!(refusal.kind, "invalid_request");
    assert!(refusal.message.contains("max_tokens"), "{refusal}");

    let rows = read(&backend, json!({"targets": targets}));
    assert_eq!(rows[0]["source"], source[1..].trim_end_matches('\n'));
    assert_eq!(rows[1]["source"], &source[1..]);
    for row in rows.as_array().unwrap() {
        assert_eq!(row["start_line"], 2);
        assert_eq!(row["end_line"], 5);
        assert!(row.get("truncated").is_none());
    }
    let rows = read(&backend, json!({"targets": targets, "max_tokens": 1}));
    for row in rows.as_array().unwrap() {
        assert_eq!(row["source"], "/// ");
        assert_eq!(row["start_line"], 2);
        assert_eq!(row["end_line"], 2);
        assert_eq!(row["truncated"], true);
    }
    let rows = read(&backend, json!({"targets": targets, "max_tokens": 2}));
    for row in rows.as_array().unwrap() {
        assert_eq!(row["source"], "/// 🦀");
        assert_eq!(row["end_line"], 2);
    }
    let rows = read(
        &backend,
        json!({"targets": [{"path": path, "start_byte": 0, "end_byte": 1}]}),
    );
    assert_eq!(rows[0]["source"], "\n");
    assert_eq!(rows[0]["start_line"], 1);
    assert_eq!(rows[0]["end_line"], 1);
}
