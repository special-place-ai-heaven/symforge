//! Feature 036 slice A1, independent-review fixes: every continuation a cut answer prints
//! is executed and must return what was cut; the server and embed lanes print the same
//! cut; a cut answer never keeps a `Completeness: full` claim; the zero-hit documentation
//! note names a call that returns the counted matches; `what_changed` counts what it lists.
#![cfg(feature = "server")]

mod reliability_support;

use std::fs;
use std::path::Path;
#[cfg(feature = "embed")]
use std::time::{Duration, Instant};

use reliability_support::{call, committed_fixture_copy, fixture_server, server_for};
use serde_json::{Value, json};
#[cfg(feature = "embed")]
use symforge::embed::parity::{QueryLimits, QueryOutput, QueryPolicy, QueryRequest};
#[cfg(feature = "embed")]
use symforge::embed::{
    EmbeddedSourceHandle, EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase,
};

const CAP: usize = 60_000;

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

/// The calls of the last `Next: ` line, split at `; `.
fn next_calls(text: &str) -> Vec<String> {
    let line = text
        .lines()
        .rev()
        .find(|line| line.contains("Next: "))
        .unwrap_or_else(|| {
            panic!(
                "no `Next:` line\n{}",
                &text[text.len().saturating_sub(800)..]
            )
        });
    line.split_once("Next: ")
        .expect("split")
        .1
        .split("; ")
        .map(str::to_string)
        .collect()
}

/// `tool(key=value, ...)` into the tool name and a JSON argument object.
fn parse_call(call: &str) -> (String, Value) {
    let open = call.find('(').expect("open paren");
    let inner = &call[open + 1..call.rfind(')').expect("close paren")];
    let (mut depth, mut in_string, mut escaped, mut start) = (0i32, false, false, 0usize);
    let mut parts = Vec::new();
    for (index, ch) in inner.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
        } else {
            match ch {
                '"' => in_string = true,
                '[' => depth += 1,
                ']' => depth -= 1,
                ',' if depth == 0 => {
                    parts.push(&inner[start..index]);
                    start = index + 1;
                }
                _ => {}
            }
        }
    }
    parts.push(&inner[start..]);
    let mut args = serde_json::Map::new();
    for part in parts {
        let (key, value) = part.trim().split_once('=').expect("key=value");
        args.insert(
            key.to_string(),
            serde_json::from_str(value.trim()).expect("JSON value"),
        );
    }
    (call[..open].to_string(), Value::Object(args))
}

#[cfg(feature = "embed")]
fn open_embedded(runtime: &ProcessIndexRuntime, root: &Path) -> EmbeddedSourceHandle {
    let handle = runtime
        .open_embedded_source(EmbeddedSourceSpec::current_worktree(root.to_path_buf()))
        .expect("open embedded source");
    let until = Instant::now() + Duration::from_secs(30);
    while handle.runtime_view().phase != SourceRuntimePhase::Current {
        assert!(Instant::now() < until, "source never reached Current");
        std::thread::sleep(Duration::from_millis(5));
    }
    handle
}

#[cfg(feature = "embed")]
fn embedded_output(handle: &EmbeddedSourceHandle, request: QueryRequest) -> QueryOutput {
    let session = handle.new_query_session().expect("session");
    handle
        .query_with_policy(
            &request,
            QueryLimits {
                max_results: 100,
                max_bytes: 1 << 20,
            },
            QueryPolicy {
                allow_derived_state_preparation: true,
            },
            Some(&session),
            None,
        )
        .expect("embedded query")
        .value()
        .clone()
}

fn pad_body(head: &str, marker: &str, lines: usize) -> String {
    let mut text = format!("{head} {{\n    let mut total = 0u32; // {marker}_FIRST\n");
    for n in 0..lines {
        text.push_str(&format!("    total += {n}; // padding padding padding\n"));
    }
    text.push_str(&format!("    total // {marker}_LAST\n}}\n"));
    text
}

/// The text from the closing `[truncated]` line on.
#[cfg(feature = "embed")]
fn cut_footer(text: &str) -> &str {
    &text[text.rfind("[truncated] Output limit").expect("cut footer")..]
}

fn symbol_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        dir.path(),
        "src/big.rs",
        &pad_body("pub fn huge() -> u32", "HUGE", 2_600),
    );
    // Two same-named methods, each longer than the cap: the selector needs `symbol_line`.
    let mut twins = String::from("pub struct A;\npub struct B;\nimpl A {\n");
    twins.push_str(&pad_body("    pub fn twin() -> u32", "TWIN_A", 1_800));
    twins.push_str("}\nimpl B {\n");
    twins.push_str(&pad_body("    pub fn twin() -> u32", "TWIN_B", 1_800));
    twins.push_str("}\n");
    write(dir.path(), "src/twins.rs", &twins);
    dir
}

/// Review items 3 and 4: a symbol over the cap is cut identically by both lanes, and its
/// continuation returns the whole symbol.
#[tokio::test(flavor = "multi_thread")]
async fn over_cap_symbol_is_cut_the_same_in_both_lanes_and_the_continuation_returns_it() {
    let dir = symbol_fixture();
    let server = server_for(dir.path());
    let args = json!({ "path": "src/big.rs", "name": "huge" });
    let served = call(&server, "get_symbol", args.clone()).await;
    assert!(
        served.len() <= CAP + 200,
        "cut answer far over the cap: {}",
        served.len()
    );
    assert!(served.contains("HUGE_FIRST") && !served.contains("HUGE_LAST"));

    #[cfg(feature = "embed")]
    {
        let runtime = ProcessIndexRuntime::acquire().expect("runtime");
        let handle = open_embedded(&runtime, dir.path());
        let request = serde_json::from_value(args).expect("request");
        let QueryOutput::SymbolRead(read) =
            embedded_output(&handle, QueryRequest::SymbolRead(request))
        else {
            panic!("not a symbol read");
        };
        assert_eq!(
            cut_footer(&served),
            cut_footer(&read.rendered),
            "server and embed lanes print different cuts"
        );
    }

    let calls = next_calls(&served);
    assert_eq!(calls.len(), 1, "{calls:?}");
    let (tool, args) = parse_call(&calls[0]);
    let rest = call(&server, &tool, args).await;
    assert!(
        rest.contains("HUGE_LAST") && !rest.contains("[truncated]"),
        "continuation lost the tail"
    );
}

/// Review item 4: the continuation of an ambiguous name resolves to the same symbol.
#[tokio::test(flavor = "multi_thread")]
async fn continuation_of_an_ambiguous_name_resolves_to_the_same_symbol() {
    let dir = symbol_fixture();
    let server = server_for(dir.path());
    // Line of the second `twin` (1-based).
    let text = fs::read_to_string(dir.path().join("src/twins.rs")).expect("read");
    let lines: Vec<&str> = text.lines().collect();
    let second_line = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.contains("pub fn twin"))
        .nth(1)
        .map(|(index, _)| index + 1)
        .expect("second twin");
    let served = call(
        &server,
        "get_symbol",
        json!({ "path": "src/twins.rs", "name": "twin", "symbol_line": second_line }),
    )
    .await;
    assert!(
        served.contains("TWIN_B_FIRST") && !served.contains("TWIN_B_LAST"),
        "setup: not cut"
    );
    let calls = next_calls(&served);
    let (tool, args) = parse_call(&calls[0]);
    assert_eq!(
        args["symbol_line"],
        json!(second_line),
        "selector dropped: {calls:?}"
    );
    let rest = call(&server, &tool, args).await;
    assert!(
        rest.contains("TWIN_B_LAST") && !rest.contains("TWIN_A_FIRST"),
        "continuation resolved to a different symbol"
    );
}

fn context_fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut code = String::from("pub fn anchor() {}\n");
    for n in 0..3_000 {
        code.push_str(&format!("pub fn generated_function_number_{n:05}() {{}}\n"));
    }
    write(dir.path(), "src/many.rs", &code);
    for n in 0..4 {
        write(
            dir.path(),
            &format!("docs/link-{n:02}.md"),
            &format!(
                "# Linked {n}\ncode_path = \"src/many.rs\"\nCall `anchor`.\nBacklink evidence {n}.\n"
            ),
        );
    }
    dir
}

/// Review items 2, 3 and 5: a cut `get_file_context` names what it dropped, the emitted
/// calls return it, the cut never keeps `Completeness: full`, and both lanes agree.
#[tokio::test(flavor = "multi_thread")]
async fn cut_file_context_names_what_it_dropped_and_the_calls_return_it() {
    let dir = context_fixture();
    let server = server_for(dir.path());
    let served = call(
        &server,
        "get_file_context",
        json!({ "path": "src/many.rs" }),
    )
    .await;
    assert!(
        served.contains("[truncated] Output limit reached"),
        "setup: not cut"
    );
    assert!(
        served.len() <= CAP + 200,
        "cut answer over the cap: {}",
        served.len()
    );
    for line in served.lines().take(6) {
        assert!(
            !line.contains("Completeness: full") && !line.ends_with("| full"),
            "a cut answer keeps a full-completeness claim: {line}"
        );
    }
    assert!(
        served.contains("knowledge"),
        "the dropped section is not named"
    );

    let calls = next_calls(&served);
    let knowledge_call = calls
        .iter()
        .find(|call| call.contains("\"knowledge\""))
        .unwrap_or_else(|| panic!("no call returns the dropped knowledge: {calls:?}"));
    let (tool, args) = parse_call(knowledge_call);
    let knowledge = call(&server, &tool, args).await;
    assert!(
        knowledge.contains("Knowledge evidence:") && knowledge.contains("docs/link-"),
        "the knowledge call did not return the dropped content\n{knowledge}"
    );
    let rest_call = calls
        .iter()
        .find(|call| call.contains("max_tokens"))
        .unwrap_or_else(|| panic!("no call returns the cut primary: {calls:?}"));
    let (tool, args) = parse_call(rest_call);
    let rest = call(&server, &tool, args).await;
    assert!(
        rest.contains("generated_function_number_02999"),
        "the primary call lost the tail"
    );

    #[cfg(feature = "embed")]
    {
        let runtime = ProcessIndexRuntime::acquire().expect("runtime");
        let handle = open_embedded(&runtime, dir.path());
        let request = serde_json::from_value(json!({ "path": "src/many.rs" })).expect("request");
        let QueryOutput::FileContext(context) =
            embedded_output(&handle, QueryRequest::FileContext(request))
        else {
            panic!("not a file context");
        };
        assert_eq!(
            cut_footer(&served),
            cut_footer(&context.rendered),
            "server and embed lanes print different cuts"
        );
    }
}

/// Review item 6: the call the zero-hit note names returns the counted match; a regex
/// query gets no call; a capped count prints `N+`.
#[tokio::test(flavor = "multi_thread")]
async fn zero_hit_documentation_note_names_a_call_that_returns_the_match() {
    let server = fixture_server();
    let text = call(
        &server,
        "search_text",
        json!({ "query": "zebrafish_quorum" }),
    )
    .await;
    let calls = next_calls(&text);
    let (tool, args) = parse_call(&calls[0]);
    assert_eq!(tool, "search_knowledge");
    assert!(
        args.get("path_prefix").is_none(),
        "a guessed path_prefix is back: {args}"
    );
    let found = call(&server, &tool, args).await;
    assert!(
        found.contains("docs/notes.md"),
        "the named call does not return the match\n{found}"
    );

    let regex = call(
        &server,
        "search_text",
        json!({ "query": "zebrafish_quor.m", "regex": true }),
    )
    .await;
    assert!(
        regex.contains("in documentation"),
        "regex count missing\n{regex}"
    );
    assert!(
        !regex.contains("Next: search_knowledge"),
        "a literal call for a regex\n{regex}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn documentation_match_count_stops_at_the_scan_cap() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "src/lib.rs", "pub fn a() {}\n");
    write(
        dir.path(),
        "docs/big.md",
        &"wombat_token here\n".repeat(1_100),
    );
    let server = server_for(dir.path());
    let text = call(&server, "search_text", json!({ "query": "wombat_token" })).await;
    assert!(text.contains("1000+ matches in documentation"), "{text}");
}

/// Review item 7: counts and listing agree, a directory collapses only inside the prefix,
/// and `code_only` still filters a directory of data files out.
#[tokio::test(flavor = "multi_thread")]
async fn what_changed_counts_what_it_lists() {
    let repo = committed_fixture_copy();
    write(repo.path(), "src/fresh/a.rs", "pub fn a() {}\n");
    write(repo.path(), "src/fresh/b.rs", "pub fn b() {}\n");
    write(repo.path(), "data/x.json", "{}\n");
    let server = server_for(repo.path());

    let scoped = call(
        &server,
        "what_changed",
        json!({ "uncommitted": true, "path_prefix": "src/fresh/a.rs" }),
    )
    .await;
    assert!(scoped.contains("src/fresh/a.rs"), "{scoped}");
    assert!(
        !scoped.lines().any(|l| l.trim() == "src/fresh/"),
        "dir collapsed outside the prefix\n{scoped}"
    );
    assert!(
        scoped.contains("1 of 3 path(s) shown"),
        "label not derived from the listing\n{scoped}"
    );

    let default = call(&server, "what_changed", json!({ "uncommitted": true })).await;
    assert!(
        default.lines().any(|l| l.trim() == "src/fresh/"),
        "{default}"
    );
    assert!(
        !default.contains("data/"),
        "code_only hidden by a collapsed dir\n{default}"
    );
}

/// Review items 8 and 10: `./src` is `src`; a path with no entries says so.
#[tokio::test(flavor = "multi_thread")]
async fn repo_map_normalises_the_path_and_says_when_nothing_is_under_it() {
    let server = fixture_server();
    let plain = call(
        &server,
        "get_repo_map",
        json!({ "path": "src", "depth": 1 }),
    )
    .await;
    let dotted = call(
        &server,
        "get_repo_map",
        json!({ "path": "./src", "depth": 1 }),
    )
    .await;
    assert!(
        dotted.contains("parent"),
        "`./src` returned an empty map\n{dotted}"
    );
    assert_eq!(
        plain.replace("Index under src", ""),
        dotted.replace("Index under src", "")
    );
    let wrong = call(&server, "get_repo_map", json!({ "path": "SRC" })).await;
    assert!(wrong.contains("No entries under SRC"), "{wrong}");
}

/// Delta c: shown can never exceed total, even when `path_prefix` sits inside an untracked
/// directory (the directory then stays expanded in both counts).
#[tokio::test(flavor = "multi_thread")]
async fn what_changed_shown_never_exceeds_total_inside_an_untracked_directory() {
    let repo = committed_fixture_copy();
    for n in 0..5 {
        write(
            repo.path(),
            &format!("src/fresh/f{n}.rs"),
            "pub fn f() {}\n",
        );
    }
    let server = server_for(repo.path());
    let text = call(
        &server,
        "what_changed",
        json!({ "uncommitted": true, "path_prefix": "src/fresh/f" }),
    )
    .await;
    // Equal counts print no label (`full for current scope`); a label must have shown <= total.
    let label = text
        .split('(')
        .find_map(|part| part.strip_suffix(" path(s) shown)"));
    if let Some(label) = label {
        let (shown, total) = label.split_once(" of ").expect("N of M");
        let (shown, total): (usize, usize) = (shown.parse().expect("N"), total.parse().expect("M"));
        assert!(
            shown <= total,
            "{shown} of {total}
{text}"
        );
    } else {
        assert!(text.contains("full for current scope"), "{text}");
        assert_eq!(
            text.lines()
                .filter(|l| l.trim().starts_with("src/fresh/f"))
                .count(),
            5,
            "{text}"
        );
    }
}

/// Delta d: the repo-map note says what was omitted, by kind, and its call returns it.
#[tokio::test(flavor = "multi_thread")]
async fn repo_map_note_counts_what_was_omitted_and_the_call_returns_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    for n in 0..220 {
        write(
            dir.path(),
            &format!("src/module_{n:03}/lib.rs"),
            &format!("pub struct Type{n:03};\n"),
        );
    }
    let server = server_for(dir.path());
    let text = call(&server, "get_repo_map", json!({})).await;
    assert!(!text.contains("lowest-ranked"), "{text}");
    let note = text
        .lines()
        .find(|line| line.starts_with("Omitted at the 1000-token limit"))
        .unwrap_or_else(|| panic!("no omission note\n{text}"));
    assert!(note.contains("directory row(s)"), "{note}");
    let call_text = note.split_once("limit: ").expect("note").1;
    let call_text = &call_text[call_text.find("get_repo_map(").expect("call")..];
    let call_text = &call_text[..=call_text.find(')').expect("close")];
    let (tool, args) = parse_call(call_text);
    let listed = call(&server, &tool, args).await;
    assert!(
        listed.contains("module_219"),
        "the named call lacks the omitted directories\n{listed}"
    );
}

/// Delta a: the embed lane stores the cached trace answer as truncated when the default
/// cap cut it (reporting invariant: the stored flag matches the stored text).
#[cfg(feature = "embed")]
#[test]
fn embed_cached_trace_answer_over_the_cap_is_stored_truncated() {
    let dir = tempfile::tempdir().expect("tempdir");
    // The trace carries the definition body, so a long body takes it over the cap (the
    // caller list itself is bounded).
    let code = pad_body("pub fn target()", "TARGET", 2_600);
    write(dir.path(), "src/lib.rs", &code);
    let runtime = ProcessIndexRuntime::acquire().expect("runtime");
    let handle = open_embedded(&runtime, dir.path());
    let session = handle.new_query_session().expect("session");
    let limits = QueryLimits {
        max_results: 100,
        max_bytes: 1 << 20,
    };
    let request = serde_json::from_value(json!({
        "name": "target", "path": "src/lib.rs", "sections": []
    }))
    .expect("request");
    let claim = handle
        .query_with_session(
            &QueryRequest::SymbolContext(request),
            limits,
            &session,
            None,
        )
        .expect("trace");
    let QueryOutput::SymbolContext(answer) = claim.value() else {
        panic!("not a symbol context");
    };
    assert!(
        answer.rendered.contains("[truncated] Output limit"),
        "setup: the trace is not over the cap ({} bytes)",
        answer.rendered.len()
    );
    let restored = handle
        .query_with_session(
            &QueryRequest::Retrieve {
                handle: claim.retrieve_handle().expect("handle").into(),
                offset: 0,
            },
            limits,
            &session,
            None,
        )
        .expect("retrieve");
    let QueryOutput::RetrievedOutput(restored) = restored.value() else {
        panic!("not a retrieved output");
    };
    let stored: Value = serde_json::from_slice(&restored.bytes).expect("served record");
    assert_eq!(
        stored["truncated"],
        json!(true),
        "cut answer cached as complete"
    );
}

/// Delta h: `./src` names `src` for the tree detail on the embed lane too.
#[cfg(feature = "embed")]
#[test]
fn embed_repo_map_normalises_a_dotted_path_for_the_tree() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "src/parent/leaf.rs", "pub fn leaf() {}\n");
    let runtime = ProcessIndexRuntime::acquire().expect("runtime");
    let handle = open_embedded(&runtime, dir.path());
    let request = serde_json::from_value(json!({
        "detail": "tree", "path": "./src", "depth": 1
    }))
    .expect("request");
    let QueryOutput::RepoMap(map) = embedded_output(&handle, QueryRequest::RepoMap(request)) else {
        panic!("not a repo map");
    };
    assert!(
        map.rendered.contains("parent"),
        "`./src` found nothing\n{}",
        map.rendered
    );
}
