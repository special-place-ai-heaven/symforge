//! Gate I public-contract coverage for `search_knowledge`.

#![cfg(feature = "server")]

use std::fs;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::protocol::SymForgeServer;
use symforge::watcher::WatcherInfo;
use tempfile::TempDir;

struct KnowledgeFixture {
    _dir: TempDir,
    server: SymForgeServer,
}

impl KnowledgeFixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("docs")).expect("docs dir");
        fs::write(
            root.join("docs/recovery.md"),
            "# Recovery\n## Persistence boundaries\nShutdown is not a safe persistence boundary.\n",
        )
        .expect("recovery fixture");
        fs::create_dir_all(root.join("docs/design")).expect("design dir");
        fs::write(
            root.join("docs/design/intent.md"),
            "# Intent\nA future checkpoint may stream snapshots.\n",
        )
        .expect("intent fixture");
        fs::create_dir_all(root.join("docs/archive")).expect("archive dir");
        fs::write(
            root.join("docs/archive/history.md"),
            "# History\nAn earlier snapshot used a shutdown hook.\n",
        )
        .expect("history fixture");
        fs::write(
            root.join("docs/a-tie.md"),
            "# Tie A\nDeterministic ranking is mandatory.\n",
        )
        .expect("first tie fixture");
        fs::write(
            root.join("docs/z-tie.md"),
            "# Tie Z\nDeterministic ranking is mandatory.\n",
        )
        .expect("second tie fixture");
        fs::write(
            root.join("docs/broken.md"),
            "# Broken current implementation\ncode_path = \"src/missing.rs\"\nBudget-failed suppression evidence.\n",
        )
        .expect("suppressed authority fixture");
        for (path, content) in [
            ("docs/rank-exact.md", "# Other\nalpha beta rule\n"),
            (
                "docs/rank-heading.md",
                "# Alpha gamma beta\nheading-ranked evidence\n",
            ),
            (
                "docs/rank-distinct.md",
                "# Other\nalpha separated from beta\n",
            ),
            ("docs/rank-single.md", "# Other\nalpha alone\n"),
        ] {
            fs::write(root.join(path), content).expect("ranking fixture");
        }
        fs::create_dir_all(root.join("src")).expect("source dir");
        fs::write(root.join("src/lib.rs"), "pub fn checkpoint_anchor() {}\n")
            .expect("bridge code fixture");
        fs::write(
            root.join("docs/linked.md"),
            "# Linked recovery\ncode_path = \"src/lib.rs\"\nCall `checkpoint_anchor`.\nLinked checkpoint anchor evidence.\n",
        )
        .expect("bridge knowledge fixture");

        let index = LiveIndex::load(&root).expect("LiveIndex::load knowledge fixture");
        let server = SymForgeServer::new(
            index,
            "search_knowledge_test".to_string(),
            Arc::new(Mutex::new(WatcherInfo::default())),
            Some(root),
            None,
        );
        Self { _dir: dir, server }
    }
}

/// Sub-lines every complete `search_knowledge` hit block carries after
/// SIFT-WS1. A block that shows some but not all of these was cut in half.
const HIT_BLOCK_MARKERS: [&str; 4] = [
    "content_hash=",
    "authority:",
    "finding_ids=",
    "bridge_previews=",
];

/// Split a response into hit blocks: a block starts at `<n>. ` and runs to the
/// next block start (or the CCR footer / end of output).
fn hit_blocks(output: &str) -> Vec<Vec<&str>> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in output.lines() {
        let starts_block = line
            .split_once(". ")
            .is_some_and(|(ordinal, _)| ordinal.parse::<usize>().is_ok());
        if starts_block {
            blocks.push(vec![line]);
        } else if line == "---" || line.starts_with("CCR:") {
            break;
        } else if let Some(current) = blocks.last_mut() {
            current.push(line);
        }
    }
    blocks
}

fn complete_hit_blocks(output: &str) -> usize {
    hit_blocks(output)
        .iter()
        .filter(|block| {
            let text = block.join("\n");
            HIT_BLOCK_MARKERS.iter().all(|marker| text.contains(marker))
        })
        .count()
}

/// Frozen contract test 7: truncation retains COMPLETE provenance. A hit block
/// is atomic — budgeting may withhold it, but must never emit part of it.
fn assert_no_partial_hit_block(output: &str) {
    for block in hit_blocks(output) {
        let text = block.join("\n");
        let present = HIT_BLOCK_MARKERS
            .iter()
            .filter(|marker| text.contains(*marker))
            .count();
        assert!(
            present == 0 || present == HIT_BLOCK_MARKERS.len(),
            "partial hit block escaped budgeting ({present}/{} markers):\n{text}\n--- full ---\n{output}",
            HIT_BLOCK_MARKERS.len()
        );
    }
}

fn assert_budgeted_knowledge_context(output: &str, require_trust: bool) {
    if require_trust {
        assert!(output.contains("Trust:"), "trust header missing: {output}");
    }
    assert!(
        output.contains("source="),
        "source provenance missing: {output}"
    );
    assert!(
        output.contains("publication=") && output.contains("content="),
        "generation provenance missing: {output}"
    );
    assert!(
        output.contains("counts total=")
            && output.contains("overflow=")
            && output.contains("ambiguous=")
            && output.contains("missing="),
        "complete bridge counts missing: {output}"
    );
    assert!(
        output.contains("coverage bridge=") && output.contains("authority="),
        "coverage provenance missing: {output}"
    );
    for line in output.lines().filter(|line| {
        line.trim_start()
            .split_once('.')
            .is_some_and(|(ordinal, _)| ordinal.parse::<usize>().is_ok())
    }) {
        for marker in [
            "bytes=",
            "content_hash=",
            "source=",
            "generation=",
            "link_id=",
            "bridge_index=",
            "resolution=",
            "lifecycle=",
            "voice=",
        ] {
            assert!(
                line.contains(marker),
                "budget emitted a partial knowledge anchor ({marker} missing): {line}\n{output}"
            );
        }
    }
}

/// SIFT-WS1 (SC-001/SC-002). The slice's headline claims are "the answer is
/// readable first" and "the envelope stops crowding it out". Both are asserted
/// here rather than left to a manual post-release dogfood check, so they cannot
/// silently regress.
///
/// Pre-slice baseline on this repository (commit 83b6b32, captured in
/// specs/020-repository-knowledge-index/sift/quickstart.md): a 872-byte,
/// 6-line envelope carrying two full 64-hex digests, then one ~700-1200 byte
/// pipe-delimited mega-line per hit with the excerpt buried mid-line.
#[tokio::test]
async fn answer_arrives_before_provenance_and_the_envelope_stays_bounded() {
    let fixture = KnowledgeFixture::new();
    let output = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({
                "query": "shutdown is not a safe persistence boundary",
                "source_scope": "current",
                "limit": 10
            }),
        )
        .await;

    let blocks = hit_blocks(&output);
    assert!(!blocks.is_empty(), "expected at least one hit: {output}");

    // Answer-first: the excerpt is the third line of its block (location,
    // heading, excerpt), never buried behind provenance.
    for block in &blocks {
        let excerpt_line = block
            .iter()
            .position(|line| line.trim_start().starts_with('"'))
            .unwrap_or_else(|| panic!("no excerpt line in block:\n{}", block.join("\n")));
        assert!(
            excerpt_line <= 2,
            "excerpt must arrive within the first 3 lines of its block, found at {excerpt_line}:\n{}",
            block.join("\n")
        );
    }

    // Bounded IDs: no envelope line may carry a full 64-hex digest. The
    // pre-slice `Source:` line was ~300 chars because it printed two of them.
    let envelope_end = output
        .lines()
        .position(|line| {
            line.split_once(". ")
                .is_some_and(|(ordinal, _)| ordinal.parse::<usize>().is_ok())
        })
        .unwrap_or(output.lines().count());
    for line in output.lines().take(envelope_end) {
        let longest_hex = line
            .split(|c: char| !c.is_ascii_hexdigit())
            .map(str::len)
            .max()
            .unwrap_or(0);
        assert!(
            longest_hex < 64,
            "envelope line still carries an unbounded 64-hex digest: {line}"
        );
    }
}

/// SIFT-WS1 (T023). `classify_search_knowledge_output` (tools.rs) keys on the
/// literal `"\nNo match:"` to emit `OutcomeClass::EmptyResult`, and the STEL
/// dependent-chain special case reads that same classification. The seam is a
/// silent coupling: if a reformat moved or renamed it, typed no-match answers
/// would be misclassified with nothing failing. Pin the exact prefix, its
/// leading newline, and its position as the final line.
#[tokio::test]
async fn no_match_seam_keeps_its_exact_prefix_and_position() {
    let fixture = KnowledgeFixture::new();
    let output = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "orbital zebra lattice", "source_scope": "current"}),
        )
        .await;

    assert!(
        output.contains("\nNo match: "),
        "outcome classifier keys on a leading-newline `No match: ` seam: {output}"
    );
    let last = output.lines().last().expect("non-empty response");
    assert!(
        last.starts_with("No match: "),
        "the seam must be the final line so provenance precedes it: {output}"
    );
    assert_eq!(
        output.matches("\nNo match: ").count(),
        1,
        "exactly one seam, or the classifier reads an ambiguous response: {output}"
    );
    // Provenance still precedes it -- a no-match answer is a successful,
    // fully-attributed response, not an error.
    assert!(output.contains("Trust:"), "no-match keeps its envelope: {output}");
    assert!(
        output.contains("Counts: overflow="),
        "no-match keeps its counts: {output}"
    );
}

#[tokio::test]
async fn exact_hit_and_complete_no_match_preserve_captured_provenance() {
    let fixture = KnowledgeFixture::new();
    let hit = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({
                "query": "shutdown is not a safe persistence boundary",
                "path_prefix": "docs/",
                "source_scope": "current",
                "authority_scope": "default",
                "limit": 10,
                "max_tokens": 2500
            }),
        )
        .await;

    assert!(hit.contains("docs/recovery.md:3"), "exact path/line: {hit}");
    assert!(
        hit.contains("Recovery > Persistence boundaries"),
        "heading breadcrumb: {hit}"
    );
    assert!(
        hit.contains("Shutdown is not a safe persistence boundary."),
        "exact excerpt: {hit}"
    );
    for field in [
        "source=current",
        "source_version=",
        "publication=",
        "content=",
        "content_hash=",
        "manifest_digest=",
        "coverage=complete",
        "authority:",
        "finding_ids=",
        "provenance_ids=",
        "bridge_previews=",
    ] {
        assert!(hit.contains(field), "missing `{field}` in: {hit}");
    }

    let no_match = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "orbital zebra lattice", "source_scope": "current"}),
        )
        .await;
    assert!(
        no_match.contains("no_evidence_complete"),
        "complete no-match class: {no_match}"
    );
}

#[tokio::test]
async fn bom_crlf_and_multibyte_hit_uses_original_one_based_half_open_lines() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    fs::create_dir_all(root.join("docs")).expect("docs dir");
    fs::write(
        root.join("docs/bytes.md"),
        "\u{feff}# Ünicode\r\n## Résumé\r\nβeta persistence boundary",
    )
    .expect("byte fixture");
    let index = LiveIndex::load(&root).expect("load byte fixture");
    let server = SymForgeServer::new(
        index,
        "search_knowledge_bytes".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root),
        None,
    );

    let output = server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "βeta persistence boundary", "source_scope": "current"}),
        )
        .await;
    assert!(output.contains("docs/bytes.md:3"), "exact line: {output}");
    assert!(
        output.contains("line_range=2..4"),
        "enclosing knowledge-unit range must be one-based and half-open: {output}"
    );
    assert!(
        output.contains("Ünicode > Résumé") && output.contains("βeta persistence boundary"),
        "multibyte heading/evidence: {output}"
    );
    assert!(
        !output.contains('\u{feff}'),
        "the retained BOM must not be rendered in a visible hit field: {output}"
    );
}

#[tokio::test]
async fn validation_refuses_empty_traversal_unsupported_scope_and_selector_conflicts() {
    let fixture = KnowledgeFixture::new();

    let empty = fixture
        .server
        .dispatch_tool_for_tests("search_knowledge", json!({"query": "   "}))
        .await;
    assert!(empty.contains("query must be non-empty"), "empty: {empty}");

    let traversal = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "shutdown", "path_prefix": "../outside"}),
        )
        .await;
    assert!(
        traversal.contains("invalid path_prefix"),
        "traversal: {traversal}"
    );

    // `worktrees`/`local_refs`/`all` are supported scopes (Gate L). This
    // single-worktree fixture publishes no P1 sources, so a P1 scope composes
    // to a typed empty-scope readiness result, never a false complete-absence.
    let empty_scope = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "shutdown", "source_scope": "worktrees"}),
        )
        .await;
    assert!(
        empty_scope.contains("no_sources_in_scope"),
        "empty worktrees scope: {empty_scope}"
    );

    let conflict = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({
                "query": "shutdown",
                "project": "one",
                "projects": ["two"]
            }),
        )
        .await;
    assert!(
        conflict.contains("project") && conflict.contains("mutually exclusive"),
        "selector conflict: {conflict}"
    );

    for (field, value) in [("source_scope", "unknown"), ("authority_scope", "future")] {
        let mut request = json!({"query": "shutdown"});
        request[field] = json!(value);
        let invalid = fixture
            .server
            .dispatch_tool_for_tests("search_knowledge", request)
            .await;
        assert!(
            invalid.contains("invalid tool parameters") && invalid.contains("unknown variant"),
            "invalid {field} must be a typed schema error: {invalid}"
        );
    }

    let empty_projects = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "shutdown", "projects": []}),
        )
        .await;
    assert!(
        empty_projects.contains("projects must not be empty"),
        "empty selector set: {empty_projects}"
    );

    let mixed_wildcard = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "shutdown", "projects": ["*", "one"]}),
        )
        .await;
    assert!(
        mixed_wildcard.contains("wildcard must be the sole selector"),
        "mixed wildcard selector: {mixed_wildcard}"
    );

    let tiny_budget = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({
                "query": "orbital zebra lattice",
                "max_tokens": 1
            }),
        )
        .await;
    assert!(
        tiny_budget.contains("max_tokens") && tiny_budget.contains("minimum"),
        "a budget too small for provenance must fail validation: {tiny_budget}"
    );
    assert!(
        !tiny_budget.contains("no_evidence_complete"),
        "a budget failure must never become false complete absence: {tiny_budget}"
    );
}

#[tokio::test]
async fn ranking_is_canonical_and_byte_deterministic_for_one_captured_generation() {
    let fixture = KnowledgeFixture::new();
    let request = json!({
        "query": "deterministic ranking",
        "source_scope": "current",
        "authority_scope": "default"
    });
    let first = fixture
        .server
        .dispatch_tool_for_tests("search_knowledge", request.clone())
        .await;
    let second = fixture
        .server
        .dispatch_tool_for_tests("search_knowledge", request)
        .await;

    assert_eq!(
        first, second,
        "same captured generation must format identically"
    );
    let a = first.find("docs/a-tie.md").expect("canonical a tie");
    let z = first.find("docs/z-tie.md").expect("canonical z tie");
    assert!(a < z, "canonical path must break equal-rank ties: {first}");

    let ranked = fixture
        .server
        .dispatch_tool_for_tests("search_knowledge", json!({"query": "alpha beta"}))
        .await;
    let exact = ranked
        .find("docs/rank-exact.md")
        .expect("exact phrase rank");
    let heading = ranked.find("docs/rank-heading.md").expect("heading rank");
    let distinct = ranked
        .find("docs/rank-distinct.md")
        .expect("distinct-term rank");
    let single = ranked
        .find("docs/rank-single.md")
        .expect("single-term rank");
    assert!(
        exact < heading && heading < distinct && distinct < single,
        "phrase, heading, and distinct-term ranking chain: {ranked}"
    );
}

#[tokio::test]
async fn bridge_preview_carries_stable_link_id_without_full_bridge_record() {
    let fixture = KnowledgeFixture::new();
    let first = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({
                "query": "linked checkpoint anchor evidence",
                "authority_scope": "all"
            }),
        )
        .await;
    let second = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({
                "query": "linked checkpoint anchor evidence",
                "authority_scope": "all"
            }),
        )
        .await;
    assert_eq!(first, second, "bridge IDs/previews must be stable");
    assert!(
        first.contains("docs/linked.md:4")
            && first.contains("bridge_previews=[")
            && first.contains("src/lib.rs"),
        "linked hit must carry a bounded exact bridge preview: {first}"
    );
    assert!(
        !first.contains("candidate_code_anchors") && !first.contains("evidence_records"),
        "search output must not duplicate full bridge/evidence arrays: {first}"
    );
}

#[tokio::test]
async fn authority_scopes_are_distinct_and_filtered_matches_are_not_false_absence() {
    let fixture = KnowledgeFixture::new();
    let server = &fixture.server;
    let call = |scope: &'static str| async move {
        server
            .dispatch_tool_for_tests(
                "search_knowledge",
                json!({"query": "snapshot", "authority_scope": scope}),
            )
            .await
    };

    let default = call("default").await;
    assert!(
        default.contains("docs/design/intent.md"),
        "default: {default}"
    );
    assert!(
        !default.contains("docs/archive/history.md"),
        "default excludes history: {default}"
    );

    let current = server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "stream snapshots", "authority_scope": "current"}),
        )
        .await;
    assert!(
        current.contains("evidence_noncurrent"),
        "filtered intent is not complete absence: {current}"
    );

    let intent = call("intent").await;
    assert!(intent.contains("docs/design/intent.md"), "intent: {intent}");
    assert!(
        !intent.contains("docs/archive/history.md"),
        "intent excludes history: {intent}"
    );

    let history = call("history").await;
    assert!(
        history.contains("docs/archive/history.md"),
        "history: {history}"
    );
    assert!(
        !history.contains("docs/design/intent.md"),
        "history excludes intent: {history}"
    );

    let all = call("all").await;
    assert!(all.contains("docs/design/intent.md"), "all intent: {all}");
    assert!(
        all.contains("docs/archive/history.md"),
        "all history: {all}"
    );

    for scope in ["default", "current"] {
        let filtered = server
            .dispatch_tool_for_tests(
                "search_knowledge",
                json!({
                    "query": "budget-failed suppression evidence",
                    "authority_scope": scope
                }),
            )
            .await;
        assert!(
            filtered.contains("filtered_suppressed=1") && !filtered.contains("docs/broken.md:3"),
            "suppressed evidence must stay out of {scope}: {filtered}"
        );
    }
    for scope in ["history", "all"] {
        let visible = server
            .dispatch_tool_for_tests(
                "search_knowledge",
                json!({
                    "query": "budget-failed suppression evidence",
                    "authority_scope": scope
                }),
            )
            .await;
        assert!(
            visible.contains("docs/broken.md:3") && visible.contains("voice=suppressed"),
            "suppressed evidence must remain inspectable in {scope}: {visible}"
        );
    }
}

#[tokio::test]
async fn weak_and_sensitive_queries_are_rejected_without_echo() {
    let fixture = KnowledgeFixture::new();
    let weak = fixture
        .server
        .dispatch_tool_for_tests("search_knowledge", json!({"query": "the and why"}))
        .await;
    assert!(weak.contains("query_too_weak"), "weak query: {weak}");

    let canary = ["runtime", "-", "canary", "-", "segment"].concat();
    let query = format!("token={canary}");
    let rejected = fixture
        .server
        .dispatch_tool_for_tests("search_knowledge", json!({"query": query}))
        .await;
    assert!(
        rejected.contains("sensitive query rejected"),
        "sensitive query must fail closed"
    );
    assert!(
        !rejected.contains(&canary),
        "sensitive query must never be echoed"
    );

    let sensitive_path = format!("docs/token={canary}");
    let rejected_path = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({"query": "checkpoint anchor", "path_prefix": sensitive_path}),
        )
        .await;
    assert!(
        rejected_path.contains("sensitive path_prefix rejected"),
        "sensitive path scope must fail closed"
    );
    assert!(
        !rejected_path.contains(&canary),
        "sensitive path scope must never be echoed"
    );
}

#[tokio::test]
async fn ccr_truncation_withholds_partial_hits_and_round_trips_full_safe_output() {
    let fixture = KnowledgeFixture::new();
    let capped = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({
                "query": "deterministic ranking",
                "limit": 10,
                "max_tokens": 120
            }),
        )
        .await;
    let hash = capped
        .split("hash=\"")
        .nth(1)
        .and_then(|suffix| suffix.split('"').next())
        .expect("over-budget knowledge search must emit a CCR hash");

    // SIFT-WS1 (T019). The previous assertion was VACUOUS: it filtered lines
    // containing "docs/" and required each to also contain "authority:", which
    // only held because every hit was one line. At max_tokens=120 the response
    // truncates to header-only, so the filter matched nothing and the loop
    // asserted nothing at all. Now that a hit spans several lines, a naive
    // line-boundary cut could keep `path:line` and drop the excerpt and
    // provenance -- exactly what frozen contract test 7 forbids. Assert on
    // whole BLOCKS instead.
    assert_no_partial_hit_block(&capped);
    assert_eq!(
        complete_hit_blocks(&capped),
        0,
        "max_tokens=120 must return provenance and a handle, with no hit block at all: {capped}"
    );

    // A budget that CAN fit a hit must return whole ones, never a fragment.
    let mid = fixture
        .server
        .dispatch_tool_for_tests(
            "search_knowledge",
            json!({
                "query": "deterministic ranking",
                "limit": 10,
                "max_tokens": 300
            }),
        )
        .await;
    assert!(mid.contains("Trust:"), "header must survive: {mid}");
    assert!(
        mid.contains("hash=\""),
        "a truncated response must carry a retrieval handle: {mid}"
    );
    assert_no_partial_hit_block(&mid);
    assert!(
        complete_hit_blocks(&mid) >= 1,
        "max_tokens=300 must fit at least one COMPLETE hit block: {mid}"
    );

    // Sweep the budget across the whole range where a cut can land inside a
    // block. A single spot-check would pass by luck; the guarantee is that NO
    // budget can split a hit.
    for budget in (120..=900).step_by(15) {
        let swept = fixture
            .server
            .dispatch_tool_for_tests(
                "search_knowledge",
                json!({
                    "query": "deterministic ranking",
                    "limit": 10,
                    "max_tokens": budget
                }),
            )
            .await;
        assert_no_partial_hit_block(&swept);
        assert!(
            swept.contains("Trust:") || swept.contains("hash=\""),
            "budget {budget} must still return provenance or a handle: {swept}"
        );
    }

    let full = fixture
        .server
        .dispatch_tool_for_tests("symforge_retrieve", json!({"hash": hash}))
        .await;
    assert!(
        full.len() > capped.len(),
        "CCR must restore the full output"
    );
    assert!(full.contains("docs/a-tie.md"), "first full hit: {full}");
    assert!(full.contains("docs/z-tie.md"), "second full hit: {full}");
    assert!(full.contains("source_version="), "source version: {full}");
    assert!(full.contains("publication="), "publication: {full}");
    assert!(full.contains("content="), "content generation: {full}");

    let evicted = fixture
        .server
        .dispatch_tool_for_tests("symforge_retrieve", json!({"hash": "000000000000"}))
        .await;
    assert!(
        evicted.contains("stale or expired") && evicted.contains("retry"),
        "an unavailable captured CCR generation must be explicit and retryable: {evicted}"
    );
}

#[tokio::test]
async fn ask_routes_explicit_knowledge_intent_without_stealing_code_intent() {
    let fixture = KnowledgeFixture::new();
    let knowledge = fixture
        .server
        .dispatch_tool_for_tests(
            "ask",
            json!({"query": "what do the docs say about persistence boundaries"}),
        )
        .await;
    assert!(
        knowledge.contains("Chosen tool: search_knowledge"),
        "knowledge route: {knowledge}"
    );
    assert!(
        knowledge.contains("docs/recovery.md:2"),
        "knowledge result: {knowledge}"
    );

    let code = fixture
        .server
        .dispatch_tool_for_tests("ask", json!({"query": "where is search_knowledge defined"}))
        .await;
    assert!(
        code.contains("Chosen tool: search_symbols"),
        "code intent must retain code routing: {code}"
    );
    assert!(
        !code.contains("Chosen tool: search_knowledge"),
        "knowledge route stole code intent: {code}"
    );
}

#[tokio::test]
async fn ask_knowledge_secret_guard_runs_before_route_explanation() {
    let fixture = KnowledgeFixture::new();
    let canary = ["runtime", "-", "canary", "-", "segment"].concat();
    let query = format!("search repository knowledge for token={canary}");
    let rejected = fixture
        .server
        .dispatch_tool_for_tests("ask", json!({"query": query}))
        .await;
    assert!(
        rejected.contains("sensitive query rejected"),
        "ask knowledge safety must fail closed"
    );
    assert!(
        !rejected.contains(&canary),
        "ask route explanation must not echo a sensitive query"
    );
}

#[tokio::test]
async fn ask_routes_repository_orientation_to_the_combined_map_without_prose_invention() {
    let fixture = KnowledgeFixture::new();
    let output = fixture
        .server
        .dispatch_tool_for_tests("ask", json!({"query": "orient me in this repository"}))
        .await;

    assert!(output.contains("Chosen tool: get_repo_map"));
    assert!(output.contains("Invocation: get_repo_map(detail=\"compact\")"));
    assert!(output.contains("Index:"));
    assert!(output.contains("Repository knowledge:"));
    assert!(output.contains("Missing roles:"));
    assert!(!output.contains("A future checkpoint may stream snapshots."));
}

#[tokio::test]
async fn repo_map_preserves_topology_and_appends_one_captured_bounded_knowledge_map() {
    let fixture = KnowledgeFixture::new();
    let first = fixture
        .server
        .dispatch_tool_for_tests("get_repo_map", json!({"detail": "compact"}))
        .await;
    let second = fixture
        .server
        .dispatch_tool_for_tests("get_repo_map", json!({"detail": "compact"}))
        .await;

    assert_eq!(first, second);
    assert!(first.contains("Index:"));
    assert!(first.contains("Repository knowledge:"));
    assert!(first.contains("publication="));
    assert!(first.contains("content="));
    assert!(first.contains("Intent roles:"));
    assert!(first.contains("architecture docs/design/intent.md:1"));
    assert!(first.contains("Missing roles:"));
    assert!(first.contains("Hygiene:"));
    assert!(first.contains("Uncertainty:"));
    assert!(first.contains("Coverage:"));
    assert!(!first.contains("A future checkpoint may stream snapshots."));
}

#[tokio::test]
async fn file_context_knowledge_section_preserves_only_default_and_empty_section_modes() {
    let fixture = KnowledgeFixture::new();
    let knowledge_only = fixture
        .server
        .dispatch_tool_for_tests(
            "get_file_context",
            json!({"path": "src/lib.rs", "sections": ["knowledge"]}),
        )
        .await;
    assert!(knowledge_only.contains("Trust:"));
    assert!(knowledge_only.contains("Knowledge evidence:"));
    assert!(
        knowledge_only.contains("docs/linked.md"),
        "{knowledge_only}"
    );
    assert!(!knowledge_only.contains("Symbols:"));

    let omitted = fixture
        .server
        .dispatch_tool_for_tests("get_file_context", json!({"path": "src/lib.rs"}))
        .await;
    assert!(omitted.contains("Knowledge evidence:"));
    assert!(omitted.contains("checkpoint_anchor"));

    let empty = fixture
        .server
        .dispatch_tool_for_tests(
            "get_file_context",
            json!({"path": "src/lib.rs", "sections": []}),
        )
        .await;
    assert!(empty.contains("Knowledge evidence:"));
    assert!(empty.contains("checkpoint_anchor"));
}

#[tokio::test]
async fn symbol_context_knowledge_section_preserves_only_default_empty_and_bundle_modes() {
    let fixture = KnowledgeFixture::new();
    let knowledge_only = fixture
        .server
        .dispatch_tool_for_tests(
            "get_symbol_context",
            json!({
                "path": "src/lib.rs",
                "name": "checkpoint_anchor",
                "sections": ["knowledge"]
            }),
        )
        .await;
    assert!(knowledge_only.contains("Trust:"));
    assert!(knowledge_only.contains("Knowledge evidence:"));
    assert!(knowledge_only.contains("docs/linked.md"));
    assert!(!knowledge_only.contains("pub fn checkpoint_anchor"));

    let omitted = fixture
        .server
        .dispatch_tool_for_tests(
            "get_symbol_context",
            json!({"path": "src/lib.rs", "name": "checkpoint_anchor"}),
        )
        .await;
    assert!(omitted.contains("pub fn checkpoint_anchor"));
    assert!(omitted.contains("Knowledge evidence:"));

    let empty = fixture
        .server
        .dispatch_tool_for_tests(
            "get_symbol_context",
            json!({"path": "src/lib.rs", "name": "checkpoint_anchor", "sections": []}),
        )
        .await;
    assert!(empty.contains("Knowledge evidence:"));

    let bundle = fixture
        .server
        .dispatch_tool_for_tests(
            "get_symbol_context",
            json!({
                "path": "src/lib.rs",
                "name": "checkpoint_anchor",
                "bundle": true
            }),
        )
        .await;
    assert!(bundle.contains("pub fn checkpoint_anchor"));
    assert!(!bundle.contains("Knowledge evidence:"));
}

#[tokio::test]
async fn knowledge_only_context_budget_preserves_provenance_counts_and_atomic_anchors() {
    let fixture = KnowledgeFixture::new();
    let file = fixture
        .server
        .dispatch_tool_for_tests(
            "get_file_context",
            json!({
                "path": "src/lib.rs",
                "sections": ["knowledge"],
                "max_tokens": 96
            }),
        )
        .await;
    assert_budgeted_knowledge_context(&file, true);

    let symbol = fixture
        .server
        .dispatch_tool_for_tests(
            "get_symbol_context",
            json!({
                "path": "src/lib.rs",
                "name": "checkpoint_anchor",
                "sections": ["knowledge"],
                "max_tokens": 96
            }),
        )
        .await;
    assert_budgeted_knowledge_context(&symbol, true);
}

#[tokio::test]
async fn default_and_empty_context_budgets_keep_atomic_knowledge_provenance() {
    let fixture = KnowledgeFixture::new();
    for request in [
        json!({"path": "src/lib.rs", "max_tokens": 128}),
        json!({"path": "src/lib.rs", "sections": [], "max_tokens": 128}),
    ] {
        let output = fixture
            .server
            .dispatch_tool_for_tests("get_file_context", request)
            .await;
        assert_budgeted_knowledge_context(&output, false);
    }

    for request in [
        json!({
            "path": "src/lib.rs",
            "name": "checkpoint_anchor",
            "max_tokens": 128
        }),
        json!({
            "path": "src/lib.rs",
            "name": "checkpoint_anchor",
            "sections": [],
            "max_tokens": 128
        }),
    ] {
        let output = fixture
            .server
            .dispatch_tool_for_tests("get_symbol_context", request)
            .await;
        assert_budgeted_knowledge_context(&output, false);
    }
}

#[tokio::test]
async fn file_context_repeat_cache_invalidates_after_targeted_publication() {
    let fixture = KnowledgeFixture::new();
    let request = json!({"path": "src/lib.rs", "sections": []});
    let first = fixture
        .server
        .dispatch_tool_for_tests("get_file_context", request.clone())
        .await;
    assert!(first.contains("checkpoint_anchor"), "{first}");

    let path = fixture._dir.path().join("src/lib.rs");
    fs::write(&path, "pub fn replacement_anchor() {}\n").expect("rewrite code fixture");
    filetime::set_file_mtime(
        &path,
        filetime::FileTime::from_system_time(
            std::time::SystemTime::now() + std::time::Duration::from_secs(2),
        ),
    )
    .expect("advance code fixture mtime");

    let second = fixture
        .server
        .dispatch_tool_for_tests("get_file_context", request)
        .await;
    assert!(second.contains("replacement_anchor"), "{second}");
    assert!(!second.contains("checkpoint_anchor"), "{second}");
    assert!(!second.contains("session_repeat_read"), "{second}");
}

#[tokio::test]
async fn deep_read_after_publication_serves_current_bytes() {
    let fixture = KnowledgeFixture::new();
    let request = json!({
        "path": "docs/recovery.md",
        "start_line": 1,
        "end_line": 3
    });

    let first = fixture
        .server
        .dispatch_tool_for_tests("get_file_content", request.clone())
        .await;
    assert!(
        first.contains("Shutdown is not a safe persistence boundary."),
        "initial deep read: {first}"
    );

    let path = fixture._dir.path().join("docs/recovery.md");
    fs::write(
        &path,
        "# Recovery\n## Persistence boundaries\nCheckpoint completion is the persistence boundary.\n",
    )
    .expect("rewrite knowledge fixture");
    filetime::set_file_mtime(
        &path,
        filetime::FileTime::from_system_time(
            std::time::SystemTime::now() + std::time::Duration::from_secs(2),
        ),
    )
    .expect("advance fixture mtime so targeted freshness publishes the rewrite");

    let second = fixture
        .server
        .dispatch_tool_for_tests("get_file_content", request)
        .await;
    assert!(
        second.contains("Checkpoint completion is the persistence boundary."),
        "deep read after publication must serve current bytes: {second}"
    );
    assert!(
        !second.contains("session_repeat_read"),
        "a prior generation must not satisfy the repeat-read cache: {second}"
    );
}

#[tokio::test]
#[ignore = "manual Gate I acceptance against the complete repository corpus"]
async fn real_repository_corpus_returns_exact_deterministic_non_fixture_pointers() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let index = LiveIndex::load(&root).expect("load real repository for knowledge acceptance");
    let server = SymForgeServer::new(
        index,
        "search_knowledge_real_repository".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root),
        None,
    );
    let queries = [
        "shutdown is not a persistence boundary",
        "repair_index is intentionally retired",
        "compact surface has three tools",
        "byte exact storage line endings",
        "why embeddings are optional",
        "GGUF or safetensors indexing limits",
        "worktree routing and stale generations",
        "FTS5 planned or deferred",
    ];

    for query in queries {
        let request = json!({
            "query": query,
            "source_scope": "current",
            "limit": 3,
            "max_tokens": 2500
        });
        let first = server
            .dispatch_tool_for_tests("search_knowledge", request.clone())
            .await;
        let second = server
            .dispatch_tool_for_tests("search_knowledge", request)
            .await;
        assert_eq!(first, second, "real-repository output drifted for {query}");
        assert!(
            first.contains("Source: source=current")
                && first.contains("source_version=")
                && first.contains("freshness=")
                && first.contains("overall_coverage="),
            "real-repository result omitted captured source trust for {query}: {first}"
        );

        let pointer = first
            .lines()
            .filter(|line| {
                line.trim_start()
                    .split_once(". ")
                    .is_some_and(|(rank, _)| rank.parse::<usize>().is_ok())
            })
            .find(|line| !line.contains("specs/020-repository-knowledge-index/quickstart.md"))
            .unwrap_or_else(|| panic!("no non-acceptance-fixture pointer for {query}: {first}"));
        let estimated_tokens = first.len().saturating_add(3) / 4;
        println!("{query}\t{estimated_tokens}\t{}", pointer.trim());
    }
}

#[tokio::test]
#[ignore = "manual Gate J acceptance against the complete repository corpus"]
async fn real_repository_orientation_and_review_are_repeatable_and_actionable() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let index = LiveIndex::load(&root).expect("load real repository for mental-model acceptance");
    let server = SymForgeServer::new(
        index,
        "knowledge_review_real_repository".to_string(),
        Arc::new(Mutex::new(WatcherInfo::default())),
        Some(root),
        None,
    );

    let map_request = json!({"detail": "compact", "max_tokens": 4000});
    let first_map = server
        .dispatch_tool_for_tests("get_repo_map", map_request.clone())
        .await;
    let second_map = server
        .dispatch_tool_for_tests("get_repo_map", map_request)
        .await;
    assert_eq!(first_map, second_map, "repository orientation drifted");
    for marker in [
        "Repository knowledge:",
        "unit_id=",
        "content_hash=",
        "evidence_anchor=",
        "Uncertainty:",
        "Coverage:",
    ] {
        assert!(
            first_map.contains(marker),
            "map missing {marker}: {first_map}"
        );
    }

    let review_request = json!({
        "mode": "remediation",
        "source_scope": "current",
        "limit": 10,
        "max_tokens": 8000
    });
    let first_review = server
        .dispatch_tool_for_tests("review_knowledge", review_request.clone())
        .await;
    let second_review = server
        .dispatch_tool_for_tests("review_knowledge", review_request)
        .await;
    assert_eq!(first_review, second_review, "repository review drifted");
    for marker in [
        "mode=remediation",
        "top_result_hash=",
        "review_hash=",
        "source_id=",
        "branch=",
        "commit=",
        "working_tree=",
        "manifest_digest=",
        "manifest_coverage=",
        "summary.total=",
        "finding_ids=[",
        "action_id=",
        "dossier unit=",
        "content_hash=",
    ] {
        assert!(
            first_review.contains(marker),
            "review missing {marker}: {first_review}"
        );
    }
}

#[tokio::test]
async fn review_knowledge_modes_expose_complete_typed_dossiers_and_limit_independent_hashes() {
    let fixture = KnowledgeFixture::new();
    let linked_path = fixture._dir.path().join("docs/linked.md");
    let linked_before = fs::read(&linked_path).expect("linked fixture bytes");
    let policy_path = fixture._dir.path().join(".symforge-knowledge.toml");
    assert!(
        !policy_path.exists(),
        "fixture must start without a policy file"
    );
    let summary = fixture
        .server
        .dispatch_tool_for_tests(
            "review_knowledge",
            json!({"mode": "summary", "source_scope": "current"}),
        )
        .await;
    let document = fixture
        .server
        .dispatch_tool_for_tests(
            "review_knowledge",
            json!({
                "mode": "document",
                "path": "docs/linked.md",
                "source_scope": "current"
            }),
        )
        .await;
    let remediation = fixture
        .server
        .dispatch_tool_for_tests(
            "review_knowledge",
            json!({"mode": "remediation", "source_scope": "current", "limit": 10}),
        )
        .await;
    let limited = fixture
        .server
        .dispatch_tool_for_tests(
            "review_knowledge",
            json!({"mode": "remediation", "source_scope": "current", "limit": 1}),
        )
        .await;

    let field = |output: &str, name: &str| {
        output
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{name}=")))
            .map(str::to_string)
            .unwrap_or_else(|| panic!("missing {name}: {output}"))
    };
    assert!(summary.contains("mode=summary"), "{summary}");
    assert!(summary.contains("summary.total="), "{summary}");
    for required in [
        "finding_ids=[",
        "action_id=",
        "rule_ids=[",
        "code_evidence.consistent_rule_ids=",
        "bridge_records=",
        "timeline.coverage=",
        "exact_code_anchors=[",
        "eligibility.protected_roles=[",
        "proposal.action=",
        "proposal.unmet_preconditions=[",
    ] {
        assert!(
            document.contains(required),
            "missing {required}: {document}"
        );
    }
    assert!(
        !document.contains("Linked checkpoint anchor evidence"),
        "review dossiers must not inline document prose: {document}"
    );
    assert!(remediation.contains("mode=remediation"), "{remediation}");
    assert_eq!(
        field(&remediation, "top_result_hash"),
        field(&limited, "top_result_hash")
    );
    assert_eq!(
        field(&remediation, "review_hash"),
        field(&limited, "review_hash")
    );
    assert!(limited.contains("overflow="), "{limited}");
    assert_eq!(
        fs::read(&linked_path).expect("linked fixture after review"),
        linked_before,
        "read-only review must not mutate repository documents"
    );
    assert!(
        !policy_path.exists(),
        "read-only review must not create repository policy"
    );
}

#[tokio::test]
async fn review_knowledge_tight_budget_returns_only_complete_index_entries_and_degraded_coverage() {
    let fixture = KnowledgeFixture::new();
    let output = fixture
        .server
        .dispatch_tool_for_tests(
            "review_knowledge",
            json!({
                "mode": "remediation",
                "source_scope": "current",
                "limit": 10,
                "max_tokens": 512
            }),
        )
        .await;

    assert!(output.contains("top_result_hash="), "{output}");
    assert!(output.contains("review_hash="), "{output}");
    assert!(output.contains("output_coverage=degraded"), "{output}");
    assert!(output.contains("total_dossiers="), "{output}");
    assert!(output.contains("CCR:"), "{output}");
    assert!(
        !output.lines().any(|line| line.starts_with("dossier ")),
        "a budget summary must not return a partial multi-line dossier: {output}"
    );
    for index in output
        .lines()
        .filter(|line| line.starts_with("review_index "))
    {
        assert!(index.contains("finding_ids=["), "{index}");
        assert!(index.contains("action_id="), "{index}");
        assert!(index.contains("unit="), "{index}");
        assert!(index.contains("evidence_locations=["), "{index}");
    }
}

#[tokio::test]
async fn repository_map_tight_budget_prioritizes_exact_role_cards_and_degraded_coverage() {
    let fixture = KnowledgeFixture::new();
    let output = fixture
        .server
        .dispatch_tool_for_tests(
            "get_repo_map",
            json!({"detail": "compact", "max_tokens": 512}),
        )
        .await;

    assert!(output.contains("output_coverage=degraded"), "{output}");
    assert!(output.contains("Repository knowledge:"), "{output}");
    assert!(output.contains("unit_id="), "{output}");
    assert!(output.contains("content_hash="), "{output}");
    assert!(output.contains("evidence_anchor="), "{output}");
    assert!(output.contains("CCR:"), "{output}");
}

#[tokio::test]
async fn review_knowledge_sensitive_selectors_fail_before_hash_or_echo() {
    let fixture = KnowledgeFixture::new();
    let canary = ["runtime", "-", "canary", "-", "segment"].concat();
    let sensitive_path = format!("docs/token={canary}");
    let rejected = fixture
        .server
        .dispatch_tool_for_tests(
            "review_knowledge",
            json!({"mode": "document", "path": sensitive_path}),
        )
        .await;
    assert!(rejected.contains("sensitive path rejected"), "fail closed");
    assert!(!rejected.contains(&canary), "selector must never be echoed");
    assert!(
        !rejected.contains("review_hash=")
            && !rejected.contains("top_result_hash=")
            && !rejected.contains("hash=\""),
        "rejection must not mint plan or CCR hashes"
    );
}
