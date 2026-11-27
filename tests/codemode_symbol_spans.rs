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
        assert_eq!(&source[start as usize..end as usize], "const cells = 4;");
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
    for (name, selector, own_text, sibling_text, shown) in [
        ("first", 2, "red", "blue", "first = 'red'"),
        ("second", 3, "blue", "red", "second = 'blue'"),
    ] {
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
        assert_eq!(row["source"], shown);
        assert_eq!(row["start_line"], selector);
        assert_eq!(row["end_line"], selector);
        assert_eq!(row["symbol_line"], selector);
        assert_eq!(
            symbol_detail_view(&view, name, None),
            format!(
                "{shown}\n[const, lines {selector}-{selector}, {} bytes]",
                shown.len()
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

const TS_ITEMS: &str = "/** Adds one. */\nexport function addOne(n: number): number {\n    return n + 1;\n}\n\nexport const alpha = (n: number): number => {\n    return n * 2;\n};\n\nconst plain = 3;\n\n/** Two values. */\nexport const beta = 1, gamma = (n: number) => n * 20;\n\nexport class Box {\n    size(): number {\n        return 1;\n    }\n}\n";
const JS_ITEMS: &str = "/** Adds one. */\nexport function addOne(n) {\n    return n + 1;\n}\n\nexport const alpha = (n) => {\n    return n * 2;\n};\n\nconst plain = 3;\n\n/** Two values. */\nexport const beta = 1, gamma = (n) => n * 20;\n\nexport class Box {\n    size() {\n        return 1;\n    }\n}\n";

fn item_server(root: &TempDir, index: &SharedIndex) -> SymForgeServer {
    SymForgeServer::new(
        index.clone(),
        "symbol-spans".to_owned(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root.path().into()),
        None,
    )
}

async fn call(server: &SymForgeServer, tool: &str, args: Value) -> (bool, String) {
    let result = server
        .dispatch_tool_result_for_tests(tool, args)
        .await
        .unwrap();
    let text = result
        .content
        .iter()
        .filter_map(|content| content.as_text().map(|text| text.text.clone()))
        .collect::<Vec<_>>()
        .join("\n");
    (result.is_error == Some(true), text)
}

#[tokio::test]
async fn displayed_js_items_paste_back_into_replace_unchanged() {
    for (path, source) in [("items.ts", TS_ITEMS), ("items.js", JS_ITEMS)] {
        for (name, shown_start, shown_end) in [
            (
                "addOne",
                "/** Adds one. */\nexport function addOne",
                "return n + 1;\n}",
            ),
            ("alpha", "export const alpha", "return n * 2;\n};"),
            ("plain", "const plain = 3;", "const plain = 3;"),
            ("beta", "beta = 1", "beta = 1"),
            ("gamma", "gamma = (n", "n * 20"),
            ("size", "size()", "return 1;\n    }"),
        ] {
            let (root, index) = fixture(path, source);
            let backend = LocalCodeBackend::new(
                index.clone(),
                Some(root.path().into()),
                "symbol-spans",
                None,
            );
            let rows = read(&backend, json!({"path": path, "name": name}));
            let shown = rows[0]["source"].as_str().unwrap().to_owned();
            assert!(
                shown.starts_with(shown_start) && shown.ends_with(shown_end),
                "{path} {name}: {shown:?}"
            );
            let server = item_server(&root, &index);
            let (error, text) = call(
                &server,
                "symforge_edit",
                json!({"path": path, "symbol": name, "op": "replace", "body": shown, "apply": true}),
            )
            .await;
            assert!(!error, "{path} {name}: {text}");
            assert_eq!(
                std::fs::read_to_string(root.path().join(path)).unwrap(),
                source,
                "{path} {name}: {text}"
            );
        }
    }
}

#[tokio::test]
async fn edits_to_one_declarator_never_touch_its_sibling() {
    for (path, source) in [("items.ts", TS_ITEMS), ("items.js", JS_ITEMS)] {
        let gamma = if path.ends_with(".ts") {
            "gamma = (n: number) => n * 20"
        } else {
            "gamma = (n) => n * 20"
        };
        let cases = [
            (
                "symforge_edit",
                json!({"path": path, "symbol": "beta", "op": "replace", "body": "beta = 2", "apply": true}),
                source.replacen("beta = 1", "beta = 2", 1),
            ),
            (
                "symforge_edit",
                json!({"path": path, "symbol": "gamma", "op": "replace", "body": gamma.replace("20", "30"), "apply": true}),
                source.replacen("n * 20", "n * 30", 1),
            ),
            (
                "delete_symbol",
                json!({"path": path, "name": "beta"}),
                source.replacen("beta = 1, ", "", 1),
            ),
            (
                "delete_symbol",
                json!({"path": path, "name": "gamma"}),
                source.replacen(&format!(", {gamma}"), "", 1),
            ),
            (
                "insert_symbol",
                json!({"path": path, "name": "beta", "position": "after", "content": "const extra = 1;"}),
                source.to_owned(),
            ),
        ];
        for (tool, args, expected) in cases {
            let (root, index) = fixture(path, source);
            let server = item_server(&root, &index);
            let (_, text) = call(&server, tool, args.clone()).await;
            assert_eq!(
                std::fs::read_to_string(root.path().join(path)).unwrap(),
                expected,
                "{path} {tool} {args}: {text}"
            );
        }
    }
}

#[tokio::test]
async fn insert_after_a_sole_declarator_lands_after_its_semicolon() {
    for (path, source) in [("items.ts", TS_ITEMS), ("items.js", JS_ITEMS)] {
        let (root, index) = fixture(path, source);
        let server = item_server(&root, &index);
        let (error, text) = call(
            &server,
            "insert_symbol",
            json!({"path": path, "name": "alpha", "position": "after", "content": "const extra = 1;"}),
        )
        .await;
        assert!(!error, "{path}: {text}");
        let written = std::fs::read_to_string(root.path().join(path)).unwrap();
        let alpha_end =
            written.find("    return n * 2;\n};").unwrap() + "    return n * 2;\n};".len();
        let extra = written.find("const extra = 1;").unwrap();
        assert!(extra > alpha_end, "{path}: {written}");
        assert_eq!(written.matches(";;").count(), 0, "{path}: {written}");
    }
}

#[tokio::test]
async fn an_edit_that_breaks_the_syntax_is_refused_and_not_written() {
    for (path, source) in [("items.ts", TS_ITEMS), ("items.js", JS_ITEMS)] {
        for (tool, args) in [
            (
                "symforge_edit",
                json!({"path": path, "symbol": "alpha", "op": "replace", "body": "export const alpha = (;", "apply": true}),
            ),
            (
                "edit_within_symbol",
                json!({"path": path, "name": "plain", "old_text": "= 3", "new_text": "= (3"}),
            ),
            (
                "insert_symbol",
                json!({"path": path, "name": "plain", "position": "after", "content": "const broken = ;"}),
            ),
        ] {
            let (root, index) = fixture(path, source);
            let server = item_server(&root, &index);
            let (_, text) = call(&server, tool, args.clone()).await;
            assert_eq!(
                std::fs::read_to_string(root.path().join(path)).unwrap(),
                source,
                "{path} {tool}: {text}"
            );
            assert!(
                text.contains("syntax error at line ")
                    && text.contains("nothing was written")
                    && !text.contains("Write mode: committed"),
                "{path} {tool}: {text}"
            );
        }
    }
}

#[tokio::test]
async fn declarator_siblings_come_from_the_parse_in_every_mcp_path() {
    let comma_first = "const a = 1\n  , b = 2;\n";
    let commented = "const a = 1 /* x */, b = 2;\n";
    let doc_second = "const a = 1, /** d */ b = 2;\n";
    let split = "export const a = 1,\n    b = 2;\n";
    let replace = |name: &str, body: &str| json!({"name": name, "op": "replace", "body": body});
    let cases: Vec<(&str, &str, Value, String)> = vec![
        (
            comma_first,
            "symforge_edit",
            replace("a", "a = 9"),
            "const a = 9\n  , b = 2;\n".into(),
        ),
        (
            comma_first,
            "delete_symbol",
            json!({"name": "a"}),
            "const b = 2;\n".into(),
        ),
        (
            comma_first,
            "delete_symbol",
            json!({"name": "b"}),
            "const a = 1;\n".into(),
        ),
        (
            comma_first,
            "insert_symbol",
            json!({"name": "a", "position": "after", "content": "let z = 0;"}),
            comma_first.into(),
        ),
        (
            commented,
            "symforge_edit",
            replace("a", "a = 9"),
            "const a = 9 /* x */, b = 2;\n".into(),
        ),
        (
            commented,
            "delete_symbol",
            json!({"name": "a"}),
            "const b = 2;\n".into(),
        ),
        (
            commented,
            "delete_symbol",
            json!({"name": "b"}),
            "const a = 1 /* x */;\n".into(),
        ),
        (
            doc_second,
            "delete_symbol",
            json!({"name": "b"}),
            "const a = 1;\n".into(),
        ),
        (
            doc_second,
            "symforge_edit",
            replace("b", "/** d */ b = 3"),
            "const a = 1, /** d */ b = 3;\n".into(),
        ),
        (
            split,
            "delete_symbol",
            json!({"name": "b"}),
            "export const a = 1;\n".into(),
        ),
        (
            split,
            "delete_symbol",
            json!({"name": "a"}),
            "export const b = 2;\n".into(),
        ),
        (
            split,
            "insert_symbol",
            json!({"name": "b", "position": "before", "content": "let z = 0;"}),
            split.into(),
        ),
        (
            split,
            "batch_edit",
            json!({"edits": [{"name": "b", "operation": {"type": "delete"}}]}),
            "export const a = 1;\n".into(),
        ),
        (
            comma_first,
            "batch_edit",
            json!({"edits": [{"name": "a", "operation": {"type": "replace", "new_body": "a = 9"}}]}),
            "const a = 9\n  , b = 2;\n".into(),
        ),
        (
            split,
            "batch_insert",
            json!({"position": "before", "content": "let z = 0;", "targets": [{"name": "b"}]}),
            split.into(),
        ),
    ];
    for path in ["shapes.ts", "shapes.js"] {
        for (source, tool, args, expected) in &cases {
            let mut args = args.clone();
            if tool == &"symforge_edit" {
                args["path"] = json!(path);
                args["symbol"] = args["name"].take();
                args.as_object_mut().unwrap().remove("name");
                args["apply"] = json!(true);
            } else if tool == &"batch_edit" {
                args["edits"][0]["path"] = json!(path);
            } else if tool == &"batch_insert" {
                args["targets"][0]["path"] = json!(path);
            } else {
                args["path"] = json!(path);
            }
            let (root, index) = fixture(path, source);
            let server = item_server(&root, &index);
            let (_, text) = call(&server, tool, args.clone()).await;
            assert_eq!(
                &std::fs::read_to_string(root.path().join(path)).unwrap(),
                expected,
                "{path} {tool} {args}: {text}"
            );
        }
    }
}

#[tokio::test]
async fn the_syntax_check_names_the_error_and_spares_broken_files() {
    let cases = [
        (
            "lib.rs",
            "fn alpha() -> u32 {\n    1\n}\n\nfn beta() {}\n",
            "fn alpha() -> u32 {\n    (1\n}",
            "fn alpha() -> u32 {\n    2\n}",
            "fn beta() {}",
            "fn beta( {}",
        ),
        (
            "lib.py",
            "def alpha():\n    return 1\n\n\ndef beta():\n    return 2\n",
            "def alpha():\n    return (1",
            "def alpha():\n    return 3",
            "def beta():\n    return 2\n",
            "def beta(:\n    return 2\n",
        ),
    ];
    for (path, source, broken, fixed, from, to) in cases {
        let (root, index) = fixture(path, source);
        let server = item_server(&root, &index);
        let replace = |body: &str, dry_run: bool| json!({"path": path, "name": "alpha", "new_body": body, "dry_run": dry_run});
        let (_, preview) = call(&server, "replace_symbol_body", replace(broken, true)).await;
        let (_, refused) = call(&server, "replace_symbol_body", replace(broken, false)).await;
        for text in [&preview, &refused] {
            assert!(
                text.contains("syntax error at line ")
                    && text.contains("nothing was written")
                    && text.contains("plain text edit"),
                "{path}: {text}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(root.path().join(path)).unwrap(),
            source
        );
        let (_, preview) = call(&server, "replace_symbol_body", replace(fixed, true)).await;
        assert!(
            preview.contains("[DRY RUN] Would replace"),
            "{path}: {preview}"
        );
        let (error, applied) = call(&server, "replace_symbol_body", replace(fixed, false)).await;
        assert!(!error, "{path}: {applied}");
        assert!(
            std::fs::read_to_string(root.path().join(path))
                .unwrap()
                .contains(fixed),
            "{path}: {applied}"
        );

        let broken_source = source.replacen(from, to, 1);
        let (root, index) = fixture(path, &broken_source);
        let server = item_server(&root, &index);
        let (error, applied) = call(&server, "replace_symbol_body", replace(fixed, false)).await;
        assert!(!error, "{path}: {applied}");
        assert!(
            std::fs::read_to_string(root.path().join(path))
                .unwrap()
                .contains(fixed),
            "{path}: {applied}"
        );
    }
}
