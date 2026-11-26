#![cfg(feature = "server")]

use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};
use symforge::codemode::{CodeReadBackend, CodeReadOp, LocalCodeBackend};
use symforge::live_index::{LiveIndex, SharedIndex};
use symforge::protocol::SymForgeServer;
use symforge::protocol::format::{symbol_detail_from_indexed_file_with, symbol_detail_view};
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
        let (file, view) = {
            let guard = index.read();
            (
                guard.capture_shared_file(path).unwrap(),
                guard.capture_symbol_detail_view(path).unwrap(),
            )
        };
        let backend = LocalCodeBackend::new(index, Some(root.path().into()), "symbol-spans", None);
        let rows = read(
            &backend,
            json!({"targets": [
                {"path": path, "name": "tile", "symbol_line": 4},
                {"path": path, "name": "cells", "symbol_line": 7},
                {"path": path, "name": "local", "symbol_line": 8}
            ]}),
        );
        for (row, name, kind, selector, start, end, expected) in [
            (
                0,
                "tile",
                "fn",
                4,
                2,
                4,
                "/** Render a tile. */\nexport /*\n*/ function tile() {}",
            ),
            (
                1,
                "cells",
                "const",
                7,
                6,
                7,
                "/** Size in cells. */\nexport const cells = 4;",
            ),
            (2, "local", "const", 8, 8, 8, "const local = 2;"),
        ] {
            assert_eq!(rows[row]["source"], expected, "{path}: {name}");
            assert_eq!(rows[row]["start_line"], start);
            assert_eq!(rows[row]["end_line"], end);
            assert_eq!(rows[row]["symbol_line"], selector);
            let footer = format!("[{kind}, lines {start}-{end}, {} bytes]", expected.len());
            let classic = format!("{expected}\n{footer}");
            assert_eq!(
                symbol_detail_from_indexed_file_with(
                    &file,
                    name,
                    None,
                    Some(selector),
                    Some(false)
                ),
                classic,
                "{path}: {name}",
            );
            assert_eq!(
                symbol_detail_view(&view, name, None),
                classic,
                "{path}: {name}"
            );

            let numbered = expected
                .lines()
                .zip(start..=end)
                .map(|(line, number)| format!("{number}: {line}"))
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(
                symbol_detail_from_indexed_file_with(&file, name, None, Some(selector), Some(true)),
                format!("{numbered}\n{footer}"),
                "{path}: {name}",
            );
        }
        let cells = file.symbols.iter().find(|sym| sym.name == "cells").unwrap();
        let (start, end) = cells.core_range();
        assert_eq!(&source[start as usize..end as usize], "cells = 4");
    }
}

#[test]
fn classic_footer_uses_clamped_source_bytes_and_excludes_phantom_line_at_eof() {
    let path = "shapes.rs";
    let source = "\r\nfn tile() { /* 🦀 */ }\r\n";
    let (_root, index) = fixture(path, source);
    let mut view = index.read().capture_symbol_detail_view(path).unwrap();
    let symbol = view
        .symbols
        .iter_mut()
        .find(|sym| sym.name == "tile")
        .unwrap();
    // The renderer clamps stored offsets to the actual bytes it can return.
    symbol.byte_range.1 = u32::MAX;
    let body = &source[2..];
    assert_eq!(
        symbol_detail_view(&view, "tile", None),
        format!("{body}\n[fn, lines 2-2, {} bytes]", body.len()),
    );
}

#[tokio::test]
async fn multi_declarator_display_does_not_widen_edit_within_to_sibling() {
    let path = "palette.ts";
    let declaration = "/** Two colors. */\nexport const first = 'red',\n    second = 'blue';";
    let source = format!("{declaration}\n");
    for (name, selector, own_text, sibling_text) in
        [("first", 2, "red", "blue"), ("second", 3, "blue", "red")]
    {
        let (root, index) = fixture(path, &source);
        let view = index.read().capture_symbol_detail_view(path).unwrap();
        let server = SymForgeServer::new(
            index.clone(),
            "symbol-spans".to_owned(),
            Arc::new(Mutex::new(WatcherInfo::default())),
            Some(root.path().into()),
            None,
        );
        let backend = LocalCodeBackend::new(index, Some(root.path().into()), "symbol-spans", None);
        let rows = read(&backend, json!({"path": path, "name": name}));
        let row = &rows[0];
        assert_eq!(row["source"], declaration);
        assert_eq!(row["start_line"], 1);
        assert_eq!(row["end_line"], 3);
        assert_eq!(row["symbol_line"], selector);
        assert_eq!(
            symbol_detail_view(&view, name, None),
            format!(
                "{declaration}\n[const, lines 1-3, {} bytes]",
                declaration.len()
            ),
        );

        let edit_args = |old_text| json!({"path": path, "name": name, "symbol_line": row["symbol_line"], "old_text": old_text, "new_text": "green"});
        let rejected = server
            .dispatch_tool_result_for_tests("edit_within_symbol", edit_args(sibling_text))
            .await
            .unwrap();
        assert_eq!(rejected.is_error, Some(true), "{name}: {rejected:?}");
        assert!(
            rejected.content[0]
                .as_text()
                .unwrap()
                .text
                .contains(&format!(
                    "`{sibling_text}` not found within symbol `{name}`"
                )),
            "{rejected:?}",
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join(path)).unwrap(),
            source
        );

        let accepted = server
            .dispatch_tool_result_for_tests("edit_within_symbol", edit_args(own_text))
            .await
            .unwrap();
        assert_ne!(accepted.is_error, Some(true), "{name}: {accepted:?}");
        assert_eq!(
            std::fs::read_to_string(root.path().join(path)).unwrap(),
            source.replace(own_text, "green"),
        );
    }
}

#[test]
fn symbol_and_raw_slice_report_returned_line_span_and_utf8_truncation() {
    let path = "shapes.rs";
    let source = "\n/// 🦀 shape docs\nfn tile() {\n    let _size = 3;\n}\n";
    let (root, index) = fixture(path, source);
    let backend = LocalCodeBackend::new(index, Some(root.path().into()), "symbol-spans", None);
    let targets = json!([
        {"path": path, "name": "tile", "symbol_line": 3},
        {"path": path, "start_byte": 1},
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
    assert_eq!(rows[0]["symbol_line"], 3);
    assert!(rows[1].get("symbol_line").is_none());
    for row in rows.as_array().unwrap() {
        assert_eq!(row["start_line"], 2);
        assert_eq!(row["end_line"], 5);
        assert!(row.get("truncated").is_none());
    }
    let rows = read(&backend, json!({"targets": targets, "max_tokens": 1}));
    assert_eq!(rows[0]["symbol_line"], 3);
    assert!(rows[1].get("symbol_line").is_none());
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

    let missing = read(&backend, json!({"path": path, "name": "missing"}));
    assert_eq!(missing[0]["refusal"], "not_found");
    assert!(missing[0].get("symbol_line").is_none());
}
