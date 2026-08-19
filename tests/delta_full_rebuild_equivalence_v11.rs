//! Feature 020 V11 delta/full-rebuild equivalence oracles (T036 / 020:T071).
//!
//! Creating this file arms the `planned_exact` declarations TEST-DELTA
//! (`ORACLE-MIGRATION-DELTA-EQUIVALENCE`) and TEST-KNOWLEDGE
//! (`ORACLE-KNOWLEDGE-CURRENT-PROJECTION`) in
//! `contracts/lifecycle-oracle-traceability-v11.md`. The two frozen test
//! names below are contract identifiers and must not be renamed.
//!
//! WHAT "EQUIVALENT AFTER CANONICAL ORDERING" MEANS HERE. Every projection is
//! rendered to sorted canonical rows built ONLY from content-bound fields:
//! paths, content hashes, byte/line ranges, typed classifications, roles,
//! lifecycles, resolutions. Instance-bound counters are deliberately excluded
//! — `content_generation`/`publication_generation` (a delta instance has
//! advanced N times while a clean rebuild starts fresh), `SourceIdentity`
//! inside per-row anchors (asserted once per instance instead, see the
//! generation-binding section of the knowledge oracle), document timelines
//! (filesystem/git time evidence, environment-bound), and resource-usage
//! telemetry. Excluding a field here is a claim that it is NOT part of the
//! artifact's identity; every exclusion is listed in this paragraph so review
//! can challenge it.
//!
//! FAIRNESS (frozen: "every edit and projection class runs even after a
//! mismatch"): comparisons never assert mid-loop. Mismatches accumulate into
//! a report and the oracle fails once, at the end, with every divergent
//! (edit class, projection) pair listed.
//!
//! The fixture corpus is finite and versioned IN THIS FILE (the frozen
//! bounds): nine file classes — code, prose, policy, manifest, orientation,
//! curation, encoding, sparse, path — and the advertised edit classes the
//! observed-refresh gate measures: add, modify, rename,
//! terminal-classification demotion, delete.

// Server-only integration test: drives protocol tools (frecency split,
// knowledge-scope queries) alongside the lib-level index. File-level gate,
// same as `activation_cut_v11.rs` — the traceability validator's case
// matcher rightly refuses per-case feature cfgs, and every CI job that
// builds integration tests is a server build.
#![cfg(feature = "server")]

use std::path::Path;

use serde_json::json;
use symforge::live_index::LiveIndex;
use symforge::live_index::knowledge_authority::{KnowledgeAuthorityView, PolicyLedgerStatus};
use symforge::live_index::knowledge_bridge::{
    BridgeLimits, BridgeResolution, DerivedCoverage, KnowledgeAnchor, KnowledgeBridge,
    KnowledgeLinkResolution, build_knowledge_bridge,
};
use symforge::live_index::single_file::{remove_file, update_file_from_disk};
use symforge::live_index::store::{PublishedGeneration, SharedIndex};
use symforge::protocol::SymForgeServer;

// ── Fixture corpus: nine file classes, finite and versioned ────────────────

/// Growth past the 4 MiB code threshold demotes a file out of the parsed
/// tier (the terminal-classification edit class).
const TERMINAL_BYTES: usize = 4 * 1024 * 1024 + 4096;

const ALPHA_RS: &str =
    "pub fn alpha_entry() -> u64 {\n    1\n}\n\npub struct AlphaConfig {\n    pub field: u64,\n}\n";
const BETA_RS: &str = "pub fn beta_entry() -> u64 {\n    crate::alpha_entry() + 1\n}\n";
const GAMMA_RS: &str = "pub fn gamma_helper() -> u64 {\n    3\n}\n";
const GUIDE_MD: &str = "# Guide\n\n[alpha](../src/alpha.rs)\n\n`alpha_entry` is the entry point.\n\n## Architecture\n\nThe alpha module owns startup.\n";
const README_MD: &str =
    "# Fixture\n\nOrientation: start at [alpha](src/alpha.rs) and `alpha_entry`.\n";
const DECISIONS_MD: &str = "# Decisions\n\nStatus: Superseded\n\nThe old flow described in [old](old.md) is superseded by `alpha_entry`.\n";
const OLD_MD: &str = "# Old flow\n\nHistorical description of the retired flow.\n";
const CARGO_TOML: &str =
    "[package]\nname = \"delta-equivalence-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
/// Encoding class: 0xE9 is invalid UTF-8 (bare Latin-1 e-acute). The index
/// must represent this file the same typed way on both build paths.
const LATIN1_RS: &[u8] =
    b"// caf\xE9 fixture: invalid UTF-8 on purpose\npub fn latin1_marker() {}\n";

/// The versioned corpus. `.symforge-knowledge.toml` (policy class) is
/// generated in `write_corpus` because its entry binds the real content hash
/// of `docs/old.md`.
fn write_corpus(root: &Path) {
    let static_files: &[(&str, &[u8])] = &[
        ("src/alpha.rs", ALPHA_RS.as_bytes()),
        ("src/beta.rs", BETA_RS.as_bytes()),
        // path class: a deep nested module path.
        ("src/nested/deep/gamma_module.rs", GAMMA_RS.as_bytes()),
        ("docs/guide.md", GUIDE_MD.as_bytes()),
        // orientation class.
        ("README.md", README_MD.as_bytes()),
        // curation class: lifecycle-bearing decision prose.
        ("docs/decisions.md", DECISIONS_MD.as_bytes()),
        ("docs/old.md", OLD_MD.as_bytes()),
        // manifest class.
        ("Cargo.toml", CARGO_TOML.as_bytes()),
        // sparse class: an empty placeholder.
        ("docs/placeholder.md", b""),
        // encoding class.
        ("src/latin1_encoding.rs", LATIN1_RS),
    ];
    for (rel, bytes) in static_files {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("fixture parent")).expect("fixture dir");
        std::fs::write(&path, bytes).expect("fixture file");
    }
    std::fs::write(root.join(".symforge-knowledge.toml"), policy_toml()).expect("policy file");
}

fn policy_toml() -> String {
    let old_hash = symforge::hash::digest_hex(OLD_MD.as_bytes());
    format!(
        "version = 1\n\n[[entries]]\nentry_id = \"retire-old-flow\"\nlifecycle = \"superseded\"\nauthority_domain = \"historical_record\"\njustification_code = \"replaced-by-alpha\"\n\n[entries.target]\npath = \"docs/old.md\"\ncontent_hash = \"{old_hash}\"\n\n[entries.superseded_by]\npath = \"docs/guide.md\"\ncontent_hash = \"unpinned\"\n"
    )
}

// ── Canonical projections ──────────────────────────────────────────────────

/// A knowledge anchor's content-bound identity. `content_generation` and
/// `source` are excluded (see the module header); the source binding is
/// asserted per instance in the knowledge oracle.
fn anchor_row(anchor: &KnowledgeAnchor) -> String {
    format!(
        "{}|{}|b{}..{}|l{}..{}",
        anchor.path,
        anchor.content_hash,
        anchor.byte_range.start,
        anchor.byte_range.end,
        anchor.line_range.start,
        anchor.line_range.end
    )
}

fn bridge_resolution_row(resolution: &BridgeResolution) -> String {
    match resolution {
        BridgeResolution::ResolvedExact(anchor) => format!(
            "exact:{:?}|{}|l{}..{}",
            anchor.id, anchor.content_hash, anchor.line_range.start, anchor.line_range.end
        ),
        other => format!("{:?}", variant_tag(other)),
    }
}

/// Debug-render only the variant name (payloads may carry instance-bound
/// fields; the resolved-exact payload above is rendered field-by-field).
fn variant_tag<T: std::fmt::Debug>(value: &T) -> String {
    let debug = format!("{value:?}");
    debug
        .split(['(', '{', ' '])
        .next()
        .unwrap_or(&debug)
        .to_string()
}

fn coverage_row(coverage: &DerivedCoverage) -> String {
    match coverage {
        DerivedCoverage::Complete => "coverage:complete".to_string(),
        DerivedCoverage::Truncated { breaches } => {
            let mut kinds: Vec<String> = breaches
                .iter()
                .map(|breach| format!("{:?}", breach.kind))
                .collect();
            kinds.sort();
            format!("coverage:truncated[{}]", kinds.join(","))
        }
    }
}

fn canonical_code(published: &PublishedGeneration) -> Vec<String> {
    let mut rows: Vec<String> = published
        .live
        .all_files()
        .map(|(path, file)| {
            let mut symbols: Vec<String> = file
                .symbols
                .iter()
                .map(|symbol| format!("{}:{:?}@l{}", symbol.name, symbol.kind, symbol.line_range.0))
                .collect();
            symbols.sort();
            let mut references: Vec<String> = file
                .references
                .iter()
                .map(|reference| {
                    format!(
                        "{}:{:?}@l{}",
                        reference.name, reference.kind, reference.line_range.0
                    )
                })
                .collect();
            references.sort();
            format!(
                "{path}|{}|{:?}|parse:{}|sym:[{}]|ref:[{}]",
                file.content_hash,
                file.language,
                variant_tag(&file.parse_status),
                symbols.join(","),
                references.join(",")
            )
        })
        .collect();
    rows.sort();
    rows
}

fn canonical_manifest(published: &PublishedGeneration) -> Vec<String> {
    let Some(manifest) = published.manifest.as_deref() else {
        return vec!["manifest:absent".to_string()];
    };
    let mut rows: Vec<String> = manifest
        .entries
        .iter()
        .map(|entry| {
            format!(
                "entry|{:?}|{}|{:?}|{}|{}",
                entry.path,
                entry.size,
                entry.language,
                variant_tag(&entry.disposition),
                entry.content_hash.as_deref().unwrap_or("-")
            )
        })
        .collect();
    rows.sort();
    rows.push(format!("coverage|{:?}", manifest.coverage));
    rows.push(format!("digest|{}", manifest.digest));
    rows.push(format!(
        "versions|{}|{}|{}",
        manifest.schema_version, manifest.policy_version, manifest.secret_policy_version
    ));
    let mut issues: Vec<String> = manifest
        .issues
        .iter()
        .map(|issue| format!("issue|{:?}|{:?}", issue.safe_path, issue.kind))
        .collect();
    issues.sort();
    rows.extend(issues);
    rows
}

fn canonical_knowledge(bridge: &KnowledgeBridge) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    for card in &bridge.cards {
        let mut roles: Vec<String> = card
            .roles
            .iter()
            .map(|(role, evidence)| format!("{role:?}/{}", variant_tag(evidence)))
            .collect();
        roles.sort();
        rows.push(format!(
            "card|{}|roles:[{}]",
            anchor_row(&card.anchor),
            roles.join(",")
        ));
    }
    for link in &bridge.forward {
        rows.push(format!(
            "code-link|{}|{:?}|{}",
            anchor_row(&link.evidence),
            link.evidence_kind,
            bridge_resolution_row(&link.resolution)
        ));
    }
    for link in &bridge.knowledge_links {
        let resolution = match &link.resolution {
            KnowledgeLinkResolution::ResolvedExact(anchor) => {
                format!("exact:{}", anchor_row(anchor))
            }
            other => variant_tag(other),
        };
        rows.push(format!(
            "knowledge-link|{}|{resolution}",
            anchor_row(&link.evidence)
        ));
    }
    rows.sort();
    rows.push(format!(
        "ownership-selectors:{}",
        bridge.ownership_selectors.len()
    ));
    rows.push(coverage_row(&bridge.coverage));
    rows
}

fn canonical_authority(view: &KnowledgeAuthorityView) -> Vec<String> {
    let mut rows: Vec<String> = view
        .records
        .iter()
        .map(|record| {
            let mut evidence_ids: Vec<String> = record
                .code_evidence
                .consistent_rule_ids
                .iter()
                .chain(&record.code_evidence.deterministic_conflict_ids)
                .chain(&record.code_evidence.suspected_conflict_ids)
                .chain(&record.code_evidence.implementation_gap_ids)
                .cloned()
                .collect();
            evidence_ids.sort();
            format!(
                "rec|{}|{:?}|{}|{:?}|{}|{:?}|code:{:?}[{}]|succ:{}",
                anchor_row(&record.unit),
                record.lifecycle,
                variant_tag(&record.lifecycle_evidence),
                record.authority_domain,
                variant_tag(&record.authority_domain_evidence),
                record.voice,
                record.code_evidence.display,
                evidence_ids.join(","),
                record
                    .successor
                    .as_ref()
                    .map(|anchor| anchor.path.clone())
                    .unwrap_or_else(|| "-".to_string())
            )
        })
        .collect();
    rows.sort();
    let mut findings: Vec<String> = view.finding_index.keys().cloned().collect();
    findings.sort();
    rows.push(format!("findings:[{}]", findings.join(",")));
    rows.push(format!("policy-digest:{}", view.policy_digest));
    rows.push(format!("policy-status:{:?}", view.policy_status));
    rows.push(format!("curation-eligible:{}", view.curation_eligible));
    rows.push(format!(
        "skipped-suppressions:[{}]",
        view.skipped_suppression_ids.join(",")
    ));
    rows.push(coverage_row(&view.coverage));
    rows
}

/// Representative query results: ranked file search and exact references —
/// the discovery answers a consumer of either build path would receive.
fn representative_queries(published: &PublishedGeneration) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let search = published
        .live
        .capture_search_files_view("alpha", 10, None, None);
    rows.push(format!("search-files:{search:?}"));
    match published.live.find_exact_references_for_symbol(
        "src/alpha.rs",
        "alpha_entry",
        None,
        None,
        None,
    ) {
        Ok(references) => {
            let mut reference_rows: Vec<String> = references
                .iter()
                .map(|(path, record)| {
                    format!(
                        "{path}|{}:{:?}@l{}",
                        record.name, record.kind, record.line_range.0
                    )
                })
                .collect();
            reference_rows.sort();
            rows.push(format!("references:[{}]", reference_rows.join(",")));
        }
        Err(error) => rows.push(format!("references-unavailable:{error}")),
    }
    rows
}

fn all_projections(published: &PublishedGeneration) -> Vec<(&'static str, Vec<String>)> {
    vec![
        ("code", canonical_code(published)),
        ("manifest", canonical_manifest(published)),
        ("knowledge", canonical_knowledge(&published.bridge)),
        ("authority", canonical_authority(&published.authority)),
        ("queries", representative_queries(published)),
    ]
}

// ── Comparison harness ─────────────────────────────────────────────────────

fn first_divergence(delta: &[String], clean: &[String]) -> String {
    let mut lines = Vec::new();
    for row in delta.iter().filter(|row| !clean.contains(row)).take(3) {
        lines.push(format!("  delta-only: {row}"));
    }
    for row in clean.iter().filter(|row| !delta.contains(row)).take(3) {
        lines.push(format!("  clean-only: {row}"));
    }
    if lines.is_empty() {
        lines.push("  (same rows, different order or count)".to_string());
    }
    lines.join("\n")
}

/// Compare every canonical projection of the live delta instance against a
/// clean full rebuild from the same on-disk bytes. Never asserts: mismatches
/// accumulate so every edit and projection class still runs (frozen
/// fairness).
fn compare_with_clean_rebuild(
    root: &Path,
    delta: &SharedIndex,
    edit_class: &str,
    mismatches: &mut Vec<String>,
) {
    let clean = LiveIndex::load(root).expect("clean full rebuild");
    let delta_published = delta.published_generation();
    let clean_published = clean.published_generation();
    let clean_rows = all_projections(&clean_published);
    for ((name, delta_rows), (_, clean_rows)) in all_projections(&delta_published)
        .into_iter()
        .zip(clean_rows)
    {
        if delta_rows != clean_rows {
            mismatches.push(format!(
                "[{edit_class}/{name}] delta != clean rebuild\n{}",
                first_divergence(&delta_rows, &clean_rows)
            ));
        }
    }
}

fn refresh(shared: &SharedIndex, root: &Path, rel: &str) {
    let outcome = update_file_from_disk(shared, root, rel);
    let rendered = format!("{outcome:?}");
    assert!(
        rendered.contains("Reindexed") || rendered.contains("Skipped"),
        "observed refresh of {rel} reported neither completion nor typed skip: {rendered}"
    );
}

// ── ORACLE-MIGRATION-DELTA-EQUIVALENCE (TEST-DELTA, T036/020:T071) ─────────

#[tokio::test]
async fn every_edit_matches_clean_full_rebuild() {
    let root = tempfile::tempdir().expect("fixture root");
    let root = root.path();
    write_corpus(root);
    let shared = LiveIndex::load(root).expect("initial load");
    let mut mismatches = Vec::new();

    // Baseline: two loads of the same bytes agree before any edit.
    compare_with_clean_rebuild(root, &shared, "baseline", &mut mismatches);

    // add — code and prose.
    std::fs::write(
        root.join("src/delta_added.rs"),
        "pub fn delta_added_entry() -> u64 {\n    9\n}\n",
    )
    .expect("add code");
    refresh(&shared, root, "src/delta_added.rs");
    std::fs::write(
        root.join("docs/delta_note.md"),
        "# Note\n\n[added](../src/delta_added.rs) documents `delta_added_entry`.\n",
    )
    .expect("add prose");
    refresh(&shared, root, "docs/delta_note.md");
    compare_with_clean_rebuild(root, &shared, "add", &mut mismatches);

    // modify — code symbol change and prose revision.
    std::fs::write(
        root.join("src/alpha.rs"),
        "pub fn alpha_entry() -> u64 {\n    2\n}\n\npub fn alpha_extra() -> u64 {\n    3\n}\n\npub struct AlphaConfig {\n    pub field: u64,\n}\n",
    )
    .expect("modify code");
    refresh(&shared, root, "src/alpha.rs");
    std::fs::write(
        root.join("docs/guide.md"),
        format!("{GUIDE_MD}\nRevision two adds `alpha_extra`.\n"),
    )
    .expect("modify prose");
    refresh(&shared, root, "docs/guide.md");
    compare_with_clean_rebuild(root, &shared, "modify", &mut mismatches);

    // rename — deep path moves; old identity must vanish everywhere.
    std::fs::rename(
        root.join("src/nested/deep/gamma_module.rs"),
        root.join("src/nested/deep/gamma_renamed.rs"),
    )
    .expect("rename");
    refresh(&shared, root, "src/nested/deep/gamma_renamed.rs");
    assert!(
        remove_file(&shared, "src/nested/deep/gamma_module.rs"),
        "the renamed-away path was not removable"
    );
    compare_with_clean_rebuild(root, &shared, "rename", &mut mismatches);

    // terminal classification — growth past 4 MiB demotes out of the parsed
    // tier on both build paths.
    let mut grown = BETA_RS.as_bytes().to_vec();
    grown.resize(TERMINAL_BYTES, b' ');
    std::fs::write(root.join("src/beta.rs"), &grown).expect("grow");
    let demotion = update_file_from_disk(&shared, root, "src/beta.rs");
    let demotion = format!("{demotion:?}");
    compare_with_clean_rebuild(root, &shared, "terminal", &mut mismatches);
    assert!(
        shared.read().get_file("src/beta.rs").is_none(),
        "terminal growth must demote src/beta.rs out of the parsed tier (outcome: {demotion})"
    );

    // delete — both added files go away.
    std::fs::remove_file(root.join("src/delta_added.rs")).expect("delete code");
    assert!(remove_file(&shared, "src/delta_added.rs"));
    std::fs::remove_file(root.join("docs/delta_note.md")).expect("delete prose");
    assert!(remove_file(&shared, "docs/delta_note.md"));
    compare_with_clean_rebuild(root, &shared, "delete", &mut mismatches);

    assert!(
        mismatches.is_empty(),
        "delta/clean-rebuild divergence:\n{}",
        mismatches.join("\n")
    );

    // ── No missing scope is represented as empty (vacuity guards) ─────────
    let published = shared.published_generation();
    let code = canonical_code(&published);
    assert!(
        code.iter().any(|row| row.starts_with("src/alpha.rs|")),
        "code scope lost src/alpha.rs: {code:?}"
    );
    let manifest = canonical_manifest(&published);
    assert!(
        manifest.iter().any(|row| row.contains("Cargo.toml")),
        "manifest scope lost Cargo.toml: {manifest:?}"
    );
    assert!(
        !manifest.contains(&"manifest:absent".to_string()),
        "the canonical manifest is absent"
    );
    let knowledge = canonical_knowledge(&published.bridge);
    assert!(
        knowledge.iter().any(|row| row.starts_with("card|")),
        "knowledge scope produced no cards: {knowledge:?}"
    );
    assert!(
        !published.authority.records.is_empty(),
        "authority scope produced no records"
    );
    // Encoding class: the malformed-encoding fixture is represented with the
    // same typed identity on both paths (its row survived every equivalence
    // comparison above) and its raw bytes are hash-bound, not silently lost.
    let latin1_hash = symforge::hash::digest_hex(LATIN1_RS);
    assert!(
        code.iter()
            .any(|row| row.starts_with("src/latin1_encoding.rs|") && row.contains(&latin1_hash))
            || manifest
                .iter()
                .any(|row| row.contains("src/latin1_encoding.rs")),
        "encoding fixture vanished from both code and manifest scopes"
    );
    // Sparse class: the empty placeholder stays a typed entry.
    assert!(
        manifest
            .iter()
            .any(|row| row.contains("docs/placeholder.md")),
        "sparse placeholder vanished from the manifest"
    );

    // ── A deliberately omitted projection fails equivalence ───────────────
    let clean = LiveIndex::load(root).expect("clean rebuild for omission control");
    let clean_code = canonical_code(&clean.published_generation());
    assert_eq!(code, clean_code, "positive control before omission");
    let mut omitted = code.clone();
    omitted.remove(0);
    assert_ne!(
        omitted, clean_code,
        "the comparator failed to detect an omitted projection row"
    );

    // ── A stale legacy cache fails reachability ────────────────────────────
    // After rename and delete, nothing in the delta instance can serve the
    // retired identities: no file, no symbols, no search hit, no knowledge
    // anchor. A raw cache or second authority would keep answering.
    for stale in [
        "src/nested/deep/gamma_module.rs",
        "src/delta_added.rs",
        "docs/delta_note.md",
    ] {
        assert!(
            shared.read().get_file(stale).is_none(),
            "stale path {stale} still served by the published index"
        );
        assert!(
            !canonical_code(&published)
                .iter()
                .any(|row| row.starts_with(&format!("{stale}|"))),
            "stale path {stale} still in the code projection"
        );
        assert!(
            !canonical_knowledge(&published.bridge)
                .iter()
                .any(|row| row.contains(stale)),
            "stale path {stale} still anchored in the knowledge projection"
        );
    }
    let gamma_search = published
        .live
        .capture_search_files_view("gamma_module", 10, None, None);
    assert!(
        !format!("{gamma_search:?}").contains("gamma_module.rs"),
        "search still ranks the renamed-away path: {gamma_search:?}"
    );

    // ── Search ranking keeps discovery-only calls from creating frecency ──
    frecency_discovery_commitment_split().await;
}

/// Discovery tools must never create frecency; commitment reads must. Driven
/// through the real tool dispatch on a dedicated root so the equivalence
/// corpus above stays byte-stable.
async fn frecency_discovery_commitment_split() {
    use symforge::domain::{RootCandidateSource, RootRequestMode, RootResolution};
    use symforge::live_index::frecency::{FRECENCY_FLAG_ENV, FrecencyStore};

    struct EnvGuard(Option<std::ffi::OsString>);
    #[allow(unsafe_code)] // test-only; the suite runs --test-threads=1.
    impl EnvGuard {
        fn set(value: &str) -> Self {
            let previous = std::env::var_os(FRECENCY_FLAG_ENV);
            // SAFETY: single-threaded test execution (--test-threads=1).
            unsafe { std::env::set_var(FRECENCY_FLAG_ENV, value) };
            Self(previous)
        }
    }
    #[allow(unsafe_code)]
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: see EnvGuard::set.
            match &self.0 {
                Some(value) => unsafe { std::env::set_var(FRECENCY_FLAG_ENV, value) },
                None => unsafe { std::env::remove_var(FRECENCY_FLAG_ENV) },
            }
        }
    }

    let _env = EnvGuard::set("1");
    let dir = tempfile::tempdir().expect("frecency root");
    write_corpus(dir.path());
    let RootResolution::Bound(binding) = symforge::discovery::resolve_root_candidate(
        dir.path(),
        RootCandidateSource::LaunchCwd,
        RootRequestMode::Automatic,
    ) else {
        panic!("frecency fixture root must bind");
    };
    let root = binding.canonical_root.clone();
    let state_placement = symforge::discovery::resolve_state_placement(&binding);
    let project_state = state_placement
        .directory()
        .expect("frecency fixture must have durable state")
        .clone();
    let shared = LiveIndex::load_for_state_placement(&root, &state_placement)
        .expect("frecency fixture load");
    let server = SymForgeServer::new_with_state_placement(
        shared,
        "delta_equivalence_frecency".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root),
        Some(state_placement),
        None,
    );
    let db_path = project_state
        .as_path()
        .join(symforge::paths::FRECENCY_DB_NAME);

    // Discovery-only calls: search must not create frecency.
    server
        .dispatch_tool_for_tests("search_files", json!({ "query": "alpha" }))
        .await;
    server
        .dispatch_tool_for_tests("search_text", json!({ "query": "alpha_entry" }))
        .await;
    let after_discovery = FrecencyStore::open_existing_readonly(&db_path)
        .expect("readonly open")
        .map(|store| store.last_10_bumps().expect("bumps"))
        .unwrap_or_default();
    assert!(
        after_discovery.is_empty(),
        "discovery-only calls created frecency: {after_discovery:?}"
    );

    // Positive control: a commitment read bumps.
    server
        .dispatch_tool_for_tests("get_file_content", json!({ "path": "src/alpha.rs" }))
        .await;
    let after_commitment = FrecencyStore::open_existing_readonly(&db_path)
        .expect("readonly open")
        .map(|store| store.last_10_bumps().expect("bumps"))
        .unwrap_or_default();
    assert!(
        after_commitment
            .iter()
            .any(|entry| entry.path.to_string_lossy().contains("alpha.rs")),
        "the commitment read did not create frecency: {after_commitment:?}"
    );
}

// ── ORACLE-KNOWLEDGE-CURRENT-PROJECTION (TEST-KNOWLEDGE, T036/020:T071) ────

#[tokio::test]
async fn knowledge_artifacts_match_clean_full_rebuild() {
    let root = tempfile::tempdir().expect("fixture root");
    let root = root.path();
    write_corpus(root);
    let shared = LiveIndex::load(root).expect("initial load");
    let mut mismatches = Vec::new();

    compare_with_clean_rebuild(root, &shared, "baseline", &mut mismatches);

    // add — a new knowledge note.
    std::fs::write(
        root.join("docs/new_note.md"),
        "# New note\n\n[alpha](../src/alpha.rs) and `alpha_entry` again.\n",
    )
    .expect("add note");
    refresh(&shared, root, "docs/new_note.md");
    compare_with_clean_rebuild(root, &shared, "knowledge-add", &mut mismatches);

    // modify — orientation and guide prose move together.
    std::fs::write(
        root.join("README.md"),
        format!("{README_MD}\nRevision two.\n"),
    )
    .expect("modify orientation");
    refresh(&shared, root, "README.md");
    compare_with_clean_rebuild(root, &shared, "knowledge-modify", &mut mismatches);

    // rename — curation prose moves; the old anchor identity must vanish.
    std::fs::rename(
        root.join("docs/decisions.md"),
        root.join("docs/decisions_v2.md"),
    )
    .expect("rename decisions");
    refresh(&shared, root, "docs/decisions_v2.md");
    assert!(remove_file(&shared, "docs/decisions.md"));
    compare_with_clean_rebuild(root, &shared, "knowledge-rename", &mut mismatches);

    // policy replace — a second entry lands.
    std::fs::write(
        root.join(".symforge-knowledge.toml"),
        format!(
            "{}\n[[entries]]\nentry_id = \"note-historical\"\nlifecycle = \"historical\"\njustification_code = \"note-kept\"\n\n[entries.target]\npath = \"docs/new_note.md\"\ncontent_hash = \"unpinned\"\n",
            policy_toml()
        ),
    )
    .expect("replace policy");
    refresh(&shared, root, ".symforge-knowledge.toml");
    compare_with_clean_rebuild(root, &shared, "policy-replace", &mut mismatches);

    // policy malformed — typed evidence rather than false completion, on
    // BOTH build paths.
    std::fs::write(root.join(".symforge-knowledge.toml"), "version = [").expect("malform policy");
    refresh(&shared, root, ".symforge-knowledge.toml");
    compare_with_clean_rebuild(root, &shared, "policy-malformed", &mut mismatches);
    assert_eq!(
        shared.published_generation().authority.policy_status,
        PolicyLedgerStatus::Malformed,
        "a malformed policy must be typed evidence"
    );
    // The `policy-malformed-v1` global rule lands as a policy-ledger review
    // signal on every authority record; finding ids are stable digests over
    // anchor + rule, so the presence of signals — not a literal rule name —
    // is the typed observable.
    let malformed_authority = shared.published_generation();
    assert!(
        malformed_authority
            .authority
            .records
            .iter()
            .any(|record| !record.code_evidence.review_signal_ids.is_empty()),
        "the malformed policy raised no typed review signal on any record"
    );
    assert!(
        !malformed_authority.authority.finding_index.is_empty(),
        "the malformed policy left the finding index empty"
    );

    // policy delete — a missing scope is typed Absent, not empty success.
    std::fs::remove_file(root.join(".symforge-knowledge.toml")).expect("delete policy");
    assert!(remove_file(&shared, ".symforge-knowledge.toml"));
    compare_with_clean_rebuild(root, &shared, "policy-delete", &mut mismatches);
    assert_eq!(
        shared.published_generation().authority.policy_status,
        PolicyLedgerStatus::Absent,
        "a missing policy must be typed Absent"
    );

    // delete — the note goes away.
    std::fs::remove_file(root.join("docs/new_note.md")).expect("delete note");
    assert!(remove_file(&shared, "docs/new_note.md"));
    compare_with_clean_rebuild(root, &shared, "knowledge-delete", &mut mismatches);

    assert!(
        mismatches.is_empty(),
        "knowledge delta/clean-rebuild divergence:\n{}",
        mismatches.join("\n")
    );

    // ── Knowledge is a generation-bound projection of the same root ───────
    let published = shared.published_generation();
    let source = published
        .source
        .as_deref()
        .expect("the publication carries a source identity");
    assert!(
        !published.bridge.cards.is_empty(),
        "the generation-binding check needs cards"
    );
    for card in &published.bridge.cards {
        assert_eq!(
            card.anchor.content_generation, published.content_generation,
            "card {} is not bound to the published generation",
            card.anchor.path
        );
        assert_eq!(
            &card.anchor.source, source,
            "card {} is not bound to the published source",
            card.anchor.path
        );
        let live_hash = published
            .live
            .get_file(&card.anchor.path)
            .map(|file| file.content_hash.clone())
            .unwrap_or_default();
        assert_eq!(
            card.anchor.content_hash, live_hash,
            "card {} is anchored to bytes the published root does not hold",
            card.anchor.path
        );
    }
    assert_eq!(
        published.authority.content_generation, published.content_generation,
        "the authority view is not bound to the published generation"
    );

    // ── Every knowledge scope answers through one pinned Current lease ────
    let server = SymForgeServer::new(
        shared.clone(),
        "delta_equivalence_knowledge".to_string(),
        std::sync::Arc::new(parking_lot::Mutex::new(
            symforge::watcher::WatcherInfo::default(),
        )),
        Some(root.to_path_buf()),
        None,
    );
    for scope in ["current", "worktrees", "local_refs", "all"] {
        let params = json!({ "query": "alpha entry point", "source_scope": scope });
        let first = server
            .dispatch_tool_for_tests("search_knowledge", params.clone())
            .await;
        let second = server
            .dispatch_tool_for_tests("search_knowledge", params)
            .await;
        assert_eq!(
            first, second,
            "search_knowledge scope {scope} is not deterministic"
        );
        assert!(
            !first.trim().is_empty(),
            "search_knowledge scope {scope} answered with silence"
        );
    }
    let review_first = server
        .dispatch_tool_for_tests("review_knowledge", json!({ "mode": "summary" }))
        .await;
    let review_second = server
        .dispatch_tool_for_tests("review_knowledge", json!({ "mode": "summary" }))
        .await;
    assert_eq!(
        review_first, review_second,
        "review_knowledge is not deterministic"
    );

    // ── A stale projection cannot answer as Current ────────────────────────
    // Production seam: the publication fence refuses a bridge prepared
    // against a superseded publication.
    let prepared_stale = shared.prepare_bridge_rebuild();
    std::fs::write(
        root.join("docs/guide.md"),
        format!("{GUIDE_MD}\nRevision three moves the fence.\n"),
    )
    .expect("move fence");
    refresh(&shared, root, "docs/guide.md");
    assert!(
        !shared.publish_prepared_bridge(prepared_stale),
        "a stale prepared bridge was published as Current"
    );
    // Positive control: a fresh preparation publishes.
    let prepared_fresh = shared.prepare_bridge_rebuild();
    assert!(
        shared.publish_prepared_bridge(prepared_fresh),
        "a current prepared bridge was refused"
    );

    // ── An incomplete projection is typed and fails equivalence ───────────
    let published = shared.published_generation();
    let complete_rows = canonical_knowledge(&published.bridge);
    let truncated = build_knowledge_bridge(
        &published.live,
        published.source.as_deref().expect("source"),
        published.content_generation,
        &BridgeLimits {
            max_cards: 1,
            ..BridgeLimits::default()
        },
    );
    assert!(
        matches!(truncated.coverage, DerivedCoverage::Truncated { .. }),
        "the bounded rebuild did not report typed truncation"
    );
    assert_ne!(
        canonical_knowledge(&truncated),
        complete_rows,
        "an incomplete projection passed equivalence"
    );

    // ── A foreign projection cannot answer as Current ──────────────────────
    let foreign_dir = tempfile::tempdir().expect("foreign root");
    write_corpus(foreign_dir.path());
    let foreign = LiveIndex::load(foreign_dir.path()).expect("foreign load");
    let foreign_published = foreign.published_generation();
    let foreign_source = foreign_published.source.as_deref().expect("foreign source");
    assert_ne!(
        foreign_source,
        published.source.as_deref().expect("source"),
        "two distinct roots share a source identity"
    );
    for card in &foreign_published.bridge.cards {
        assert_ne!(
            &card.anchor.source,
            published.source.as_deref().expect("source"),
            "a foreign card is bound to this root's source identity"
        );
    }

    // ── A missing projection fails equivalence ─────────────────────────────
    let mut omitted = complete_rows.clone();
    let removed = omitted
        .iter()
        .position(|row| row.starts_with("card|"))
        .expect("a card row to omit");
    omitted.remove(removed);
    assert_ne!(
        omitted, complete_rows,
        "the comparator failed to detect a missing knowledge projection"
    );
}
