//! Investigation mode: structured multi-step exploration with gap analysis.
//! Builds on SessionContext to suggest what the LLM hasn't looked at yet.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::domain::{ReferenceKind, SymbolKind};
use crate::live_index::store::LiveIndex;
use crate::protocol::session::SessionContext;

/// Analyze session context and suggest unexplored symbols/files.
pub fn suggest_next_steps(
    index: &LiveIndex,
    session: &SessionContext,
    focus: Option<&str>,
) -> String {
    let snap = session.snapshot();
    let mut lines = vec!["── Investigation Suggestions ──".to_string()];
    let fetched_symbols: Vec<_> = snap
        .fetched_symbols
        .iter()
        .filter(|(path, _, tokens)| !path.is_empty() && *tokens > 0)
        .collect();

    // Check the unfiltered snapshot (fetched_symbols local var filters tokens > 0,
    // but a 0-token fetch still means the session has activity).
    let has_any_data = !snap.fetched_symbols.is_empty()
        || !snap.fetched_files.is_empty()
        || !snap.listed_symbols.is_empty()
        || !snap.listed_files.is_empty()
        || !snap.summary_outputs.is_empty();

    if !has_any_data {
        lines.push("No symbols or files fetched yet. Start with:".to_string());
        lines.push("  - get_repo_map(detail=\"compact\") for project overview".to_string());
        lines.push("  - explore(query=\"<topic>\") for concept discovery".to_string());
        lines.push("  - search_symbols(query=\"<name>\") to find specific symbols".to_string());
        return lines.join("\n");
    }

    lines.push(format!(
        "Session: {} symbol bodies, {} file bodies, {} listed symbols, {} listed files, {} summaries (~{} tokens).",
        fetched_symbols.len(),
        snap.fetched_files.len(),
        snap.listed_symbols.len(),
        snap.listed_files.len(),
        snap.summary_outputs.len(),
        snap.total_tokens
    ));

    // Find project-defined symbols referenced by fetched symbol bodies but not yet fetched.
    let loaded_symbol_names: HashSet<&str> = fetched_symbols
        .iter()
        .map(|(_, name, _)| name.as_str())
        .collect();
    let project_symbols = project_defined_symbols(index);

    let mut suggested_symbols: Vec<(String, String, &str)> = Vec::new(); // (path, name, reason)

    for (path, name, _) in fetched_symbols {
        // Find callees of loaded symbols that aren't loaded themselves
        if let Some(file) = index.get_file(path) {
            for sym in &file.symbols {
                if sym.name == *name {
                    // Look at references within this symbol's range
                    for reference in &file.references {
                        if reference.line_range.0 >= sym.line_range.0
                            && reference.line_range.1 <= sym.line_range.1
                            && matches!(reference.kind, ReferenceKind::Call)
                            && !loaded_symbol_names.contains(reference.name.as_str())
                            && is_actionable_suggestion_name(&reference.name)
                            && !is_external_reference(reference.qualified_name.as_deref())
                        {
                            let Some(definition_paths) =
                                project_symbols.get(reference.name.as_str())
                            else {
                                continue;
                            };
                            let Some(definition_path) = definition_paths.first() else {
                                continue;
                            };
                            suggested_symbols.push((
                                definition_path.clone(),
                                reference.name.clone(),
                                "called by loaded symbol",
                            ));
                        }
                    }
                }
            }
        }
    }

    // Deduplicate and limit
    suggested_symbols.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    suggested_symbols.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);

    // Apply focus filter if provided
    if let Some(focus_term) = focus {
        let focus_lower = focus_term.to_ascii_lowercase();
        suggested_symbols.retain(|(path, name, _)| {
            path.to_ascii_lowercase().contains(&focus_lower)
                || name.to_ascii_lowercase().contains(&focus_lower)
        });
    }

    if !suggested_symbols.is_empty() {
        lines.push(String::new());
        lines.push("Symbols referenced but not yet loaded:".to_string());
        for (path, name, reason) in suggested_symbols.iter().take(10) {
            lines.push(format!("  {name} ({path}) — {reason}"));
        }
        if suggested_symbols.len() > 10 {
            lines.push(format!("  ... and {} more", suggested_symbols.len() - 10));
        }
        lines.push(String::new());
        lines.push("To investigate, call:".to_string());
        if let Some((path, name, _)) = suggested_symbols.first() {
            lines.push(format!(
                "  get_symbol_context(name=\"{name}\", path=\"{path}\", verbosity=\"compact\")"
            ));
        }
    } else if !snap.listed_symbols.is_empty() {
        // Symbols appeared in search results but bodies weren't loaded yet.
        lines.push(String::new());
        lines.push("Symbols seen in search results but not yet loaded:".to_string());
        for (path, name, _) in snap.listed_symbols.iter().take(10) {
            if path.is_empty() {
                lines.push(format!("  {name} — seen in search results"));
            } else {
                lines.push(format!("  {name} ({path}) — seen in search results"));
            }
        }
        if snap.listed_symbols.len() > 10 {
            lines.push(format!("  ... and {} more", snap.listed_symbols.len() - 10));
        }
        lines.push(String::new());
        lines.push("To load a symbol's full context, call:".to_string());
        if let Some((path, name, _)) = snap.listed_symbols.first() {
            if path.is_empty() {
                lines.push(format!(
                    "  get_symbol_context(name=\"{name}\", verbosity=\"compact\")"
                ));
            } else {
                lines.push(format!(
                    "  get_symbol_context(name=\"{name}\", path=\"{path}\", verbosity=\"compact\")"
                ));
            }
        }
    } else {
        lines.push(String::new());
        lines.push("No obvious gaps in loaded context. You may want to:".to_string());
        lines.push("  - search_text(query=\"TODO\") for outstanding work".to_string());
        lines.push("  - what_changed(uncommitted=true) for recent modifications".to_string());
        lines.push("  - find_dependents on a key file to check impact radius".to_string());
    }

    lines.join("\n")
}

fn project_defined_symbols(index: &LiveIndex) -> BTreeMap<String, Vec<String>> {
    let mut by_name: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (path, file) in &index.files {
        if !is_project_source_path(path) {
            continue;
        }

        for symbol in &file.symbols {
            if !is_actionable_symbol_kind(symbol.kind)
                || !is_actionable_suggestion_name(&symbol.name)
            {
                continue;
            }
            by_name
                .entry(symbol.name.clone())
                .or_default()
                .insert(path.clone());
        }
    }

    by_name
        .into_iter()
        .map(|(name, paths)| (name, paths.into_iter().collect()))
        .collect()
}

fn is_project_source_path(path: &str) -> bool {
    !matches!(
        path,
        p if p.starts_with(".claude/")
            || p.starts_with(".git/")
            || p.starts_with(".symforge/")
            || p.starts_with("docs/")
            || p.starts_with("plans/")
            || p.starts_with("target/")
            || p.starts_with("vendor/")
    )
}

fn is_actionable_symbol_kind(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Function
            | SymbolKind::Method
            | SymbolKind::Class
            | SymbolKind::Struct
            | SymbolKind::Enum
            | SymbolKind::Interface
            | SymbolKind::Module
            | SymbolKind::Constant
            | SymbolKind::Type
            | SymbolKind::Trait
    )
}

fn is_actionable_suggestion_name(name: &str) -> bool {
    const LOW_SIGNAL_NAMES: &[&str] = &[
        "as_mut",
        "as_ref",
        "clone",
        "collect",
        "default",
        "err",
        "expect",
        "from",
        "into",
        "into_iter",
        "iter",
        "iter_mut",
        "len",
        "map",
        "new",
        "none",
        "ok",
        "run",
        "some",
        "str",
        "to_owned",
        "to_string",
        "unwrap",
    ];

    name.len() > 2 && !LOW_SIGNAL_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

fn is_external_reference(qualified_name: Option<&str>) -> bool {
    let Some(qualified_name) = qualified_name else {
        return false;
    };

    [
        "std::",
        "core::",
        "alloc::",
        "tokio::",
        "serde::",
        "serde_json::",
        "anyhow::",
        "notify::",
        "tree_sitter::",
        "rmcp::",
    ]
    .iter()
    .any(|prefix| qualified_name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{FileClassification, LanguageId, ReferenceRecord, SymbolRecord};
    use crate::live_index::store::{
        CircuitBreakerState, IndexLoadSource, IndexedFile, LiveIndex, ParseStatus,
        SnapshotVerifyState,
    };
    use crate::live_index::trigram::TrigramIndex;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn make_symbol(name: &str, kind: SymbolKind, line_start: u32, line_end: u32) -> SymbolRecord {
        let byte_range = (0, 10);
        SymbolRecord {
            name: name.to_string(),
            kind,
            depth: 0,
            sort_order: 0,
            byte_range,
            item_byte_range: Some(byte_range),
            line_range: (line_start, line_end),
            doc_byte_range: None,
        }
    }

    fn make_reference(name: &str, qualified_name: Option<&str>) -> ReferenceRecord {
        ReferenceRecord {
            name: name.to_string(),
            qualified_name: qualified_name.map(ToOwned::to_owned),
            kind: ReferenceKind::Call,
            byte_range: (0, 5),
            line_range: (1, 1),
            enclosing_symbol_index: Some(0),
        }
    }

    fn make_file(
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
                classification: FileClassification::for_code_path(path),
                content: content.to_vec(),
                symbols,
                parse_status: ParseStatus::Parsed,
                parse_diagnostic: None,
                byte_len: content.len() as u64,
                content_hash: "test".to_string(),
                references,
                alias_map: HashMap::new(),
                mtime_secs: 0,
            },
        )
    }

    fn make_index(files: Vec<(String, IndexedFile)>) -> LiveIndex {
        let files_map = files
            .into_iter()
            .map(|(path, file)| (path, Arc::new(file)))
            .collect::<HashMap<_, _>>();
        let trigram_index = TrigramIndex::build_from_files(&files_map);
        let mut index = LiveIndex {
            files: files_map,
            loaded_at: Instant::now(),
            loaded_at_system: std::time::SystemTime::now(),
            load_duration: Duration::from_millis(42),
            cb_state: CircuitBreakerState::new(0.20),
            is_empty: false,
            load_source: IndexLoadSource::FreshLoad,
            snapshot_verify_state: SnapshotVerifyState::NotNeeded,
            reverse_index: HashMap::new(),
            files_by_basename: HashMap::new(),
            files_by_dir_component: HashMap::new(),
            trigram_index,
            gitignore: None,
            skipped_files: Vec::new(),
            local_empty_reason: std::sync::Arc::new(parking_lot::RwLock::new(None)),
        };
        index.rebuild_path_indices();
        index
    }

    #[test]
    fn suggest_next_steps_prefers_project_defined_actionable_symbols() {
        let caller = make_symbol("caller", SymbolKind::Function, 0, 4);
        let helper = make_symbol("helper", SymbolKind::Function, 0, 1);
        let clone_method = make_symbol("clone", SymbolKind::Method, 0, 1);

        let (caller_key, caller_file) = make_file(
            "src/lib.rs",
            b"fn caller() { helper(); clone(); Some(1); }",
            vec![caller],
            vec![
                make_reference("helper", Some("crate::helper")),
                make_reference("clone", Some("std::clone::Clone::clone")),
                make_reference("Some", Some("core::option::Option::Some")),
            ],
        );
        let (helper_key, helper_file) =
            make_file("src/helper.rs", b"pub fn helper() {}", vec![helper], vec![]);
        let (clone_key, clone_file) = make_file(
            "src/daemon.rs",
            b"fn clone() {}",
            vec![clone_method],
            vec![],
        );
        let index = make_index(vec![
            (caller_key, caller_file),
            (helper_key, helper_file),
            (clone_key, clone_file),
        ]);

        let session = SessionContext::new();
        session.record_symbol("src/lib.rs", "caller", 120);

        let output = suggest_next_steps(&index, &session, None);

        assert!(output.contains("helper (src/helper.rs)"));
        assert!(output.contains("get_symbol_context(name=\"helper\", path=\"src/helper.rs\""));
        assert!(!output.contains("clone"));
        assert!(!output.contains("Some"));
    }

    #[test]
    fn suggest_next_steps_zero_token_symbol_is_not_fetched() {
        // A symbol recorded with 0 tokens is filtered out of `fetched_symbols`
        // (the token > 0 filter), so it doesn't count as a "fetched body".
        // But it IS present in the session, so we should NOT see the
        // "No symbols or files fetched yet" empty-state message.
        let caller = make_symbol("caller", SymbolKind::Function, 0, 4);
        let (caller_key, caller_file) =
            make_file("src/lib.rs", b"fn caller() {}", vec![caller], vec![]);
        let index = make_index(vec![(caller_key, caller_file)]);

        let session = SessionContext::new();
        session.record_symbol("src/lib.rs", "caller", 0);

        let output = suggest_next_steps(&index, &session, None);

        // Should NOT show the empty-state message since session has data
        assert!(!output.contains("No symbols or files fetched yet."));
        // Should show session summary
        assert!(output.contains("Session:"));
    }

    #[test]
    fn suggest_next_steps_listed_symbols_shown_when_no_fetched() {
        let helper = make_symbol("helper", SymbolKind::Function, 0, 1);
        let (helper_key, helper_file) =
            make_file("src/helper.rs", b"pub fn helper() {}", vec![helper], vec![]);
        let index = make_index(vec![(helper_key, helper_file)]);

        let session = SessionContext::new();
        session.record_listed_symbol("src/helper.rs", "helper");

        let output = suggest_next_steps(&index, &session, None);

        assert!(!output.contains("No symbols or files fetched yet."));
        assert!(output.contains("Symbols seen in search results but not yet loaded:"));
        assert!(output.contains("helper (src/helper.rs)"));
    }
}
