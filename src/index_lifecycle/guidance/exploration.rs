//! Shared exploration ranking and context enrichment. MCP and embedded hosts
//! consume this same result; transport rendering and session accounting stay outside.

use super::source::{extract_signature, is_noise_line};
use crate::domain::LanguageId;
use crate::live_index::{LiveIndex, search};
use std::collections::{HashMap, HashSet};

pub(crate) struct ExploreRequest {
    pub query: String,
    pub limit: usize,
    pub depth: Option<u32>,
    pub include_noise: bool,
    pub include_vendor: bool,
    pub include_personal_tooling: bool,
    pub language: Option<LanguageId>,
    pub path_prefix: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExploreError {
    EmptyQuery,
    Stopped,
}

pub(crate) type EnrichedSymbol = (String, String, String, Option<String>, Vec<String>);

pub(crate) struct ExploreResult {
    pub display_label: String,
    pub symbol_hits: Vec<(String, String, String)>,
    pub symbol_scores: Vec<f32>,
    pub text_hits: Vec<(String, String, usize)>,
    pub related_files: Vec<(String, usize)>,
    pub enriched_symbols: Vec<EnrichedSymbol>,
    pub symbol_impls: Vec<(String, Vec<String>)>,
    pub symbol_deps: Vec<(String, Vec<String>)>,
    pub derived_cluster: Option<DerivedExploreCluster>,
    pub enriched_imports: Vec<String>,
    pub depth: u32,
    pub noise_hidden: usize,
    #[cfg_attr(not(feature = "embed"), allow(dead_code))] // read only by embed_* consumers
    pub overflow_count: usize,
}

#[derive(Default)]
struct ExploreMatchScore {
    raw_count: usize,
    matched_terms: HashSet<String>,
}

#[derive(Default)]
struct ExploreFileSignal {
    raw_score: u64,
    matched_terms: HashSet<String>,
}

pub(crate) struct DerivedExploreCluster {
    pub seed_terms: Vec<String>,
    pub promoted_symbols: Vec<String>,
    pub seed_files: Vec<String>,
}

pub(crate) fn explore_is_test_like_path(
    path: &str,
    classification: Option<&crate::domain::index::FileClassification>,
) -> bool {
    let path_lower = path.replace('\\', "/").to_ascii_lowercase();
    classification.is_some_and(|c| c.is_test)
        || path_lower.contains("/tests/")
        || path_lower.contains("/test/")
        || path_lower.contains("/__tests__/")
        || path_lower.ends_with("/tests.rs")
        || path_lower.ends_with("/test.rs")
        || path_lower.ends_with("_test.rs")
        || path_lower.ends_with("_spec.rs")
}

pub(crate) fn explore_should_skip_path_boost(
    path: &str,
    classification: &crate::domain::index::FileClassification,
    include_noise: bool,
    include_vendor: bool,
    include_personal_tooling: bool,
) -> bool {
    let suppress_vendor = !(include_noise || include_vendor);
    let suppress_personal = !(include_noise || include_personal_tooling);
    let suppress_other = !include_noise;
    if !suppress_vendor && !suppress_personal && !suppress_other {
        return false;
    }
    if suppress_other && explore_is_test_like_path(path, Some(classification)) {
        return true;
    }
    if suppress_personal && crate::live_index::query::is_personal_tooling_path(path) {
        return true;
    }
    if suppress_vendor && classification.is_vendor {
        return true;
    }
    if suppress_other && (classification.is_generated || classification.is_config) {
        return true;
    }
    false
}

/// Whether a concept `text_query` legitimately matches `line` at a word
/// boundary, rather than as a coincidental substring inside a larger
/// identifier.
///
/// SF-STRESS-013: concept text queries are matched by plain case-insensitive
/// substring, so `"try {"` substring-matches `public string Country { ... }`
/// (`coun-TRY-{`) and floods DTO-property results into "Error Handling". When a
/// query begins with an identifier token, we require the character immediately
/// before the match to be a non-word character (or the line start), so the
/// token sits on a real word boundary. Queries that begin with punctuation
/// (e.g. `.expect(`, `#[derive(Serialize`) are already specific and pass
/// through unchanged. This only *rejects* false positives in explore; it never
/// invents new matches.
pub(crate) fn concept_text_query_matches_on_boundary(line: &str, query: &str) -> bool {
    let line_lower = line.to_ascii_lowercase();
    let query_lower = query.to_ascii_lowercase();

    // Anchor token = the leading run of word characters in the query.
    let anchor: String = query_lower
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if anchor.is_empty() {
        // Punctuation-led query (e.g. `.expect(`); substring containment is the
        // intended semantics and already specific.
        return line_lower.contains(&query_lower);
    }

    // Require at least one occurrence of the full query whose anchor token sits
    // on a left word boundary.
    let anchor_bytes = anchor.as_bytes();
    let mut start = 0usize;
    while let Some(rel) = line_lower[start..].find(&query_lower) {
        let pos = start + rel;
        let left_ok = pos == 0
            || !line_lower.as_bytes()[pos - 1].is_ascii_alphanumeric()
                && line_lower.as_bytes()[pos - 1] != b'_';
        if left_ok {
            // Anchor is a prefix of the query, so the right boundary of the
            // anchor is the char at pos + anchor.len(); for code idioms the
            // query already includes the trailing delimiter (e.g. `unwrap()`),
            // so a left boundary is sufficient to reject identifier-internal
            // hits like `coun|try {`.
            debug_assert!(query_lower.as_bytes().starts_with(anchor_bytes));
            return true;
        }
        // Advance past this occurrence to keep scanning.
        start = pos + 1;
        if start >= line_lower.len() {
            break;
        }
    }
    false
}

pub(crate) fn explore_path_penalty(
    path: &str,
    classification: Option<&crate::domain::index::FileClassification>,
) -> u64 {
    let path_lower = path.replace('\\', "/").to_ascii_lowercase();
    if explore_is_test_like_path(path, classification) {
        return 2;
    }
    if classification.is_some_and(|c| c.is_config)
        || path_lower.ends_with(".md")
        || path_lower.ends_with(".html")
        || path_lower.ends_with(".htm")
        || path_lower.contains("/docs/")
        || path_lower.contains("/doc/")
        || path_lower.contains("/plans/")
        || path_lower.contains("/manual/")
        || path_lower.contains("changelog")
        || path_lower.contains(".planning/")
        || path_lower.contains(".auto-claude")
    {
        return 2;
    }
    if path_lower.contains("/examples/")
        || path_lower.contains("/fixtures/")
        || path_lower.contains("/bench/")
        || path_lower.contains("/benches/")
        || path_lower.contains("/sample/")
        || path_lower.contains("/samples/")
    {
        return 3;
    }
    8
}

// Explore scorer tuning (US2). The score blends a saturating NAME signal with an
// additive concept-PROXIMITY term instead of multiplying correlated name-overlap
// factors. Caps keep both signals bounded so no single symbol can run away and
// pin the max-normalized top to 1.00 while the rest crater. W_NAME >= W_PROX
// guarantees an exact-name query still ranks its target at/near the top.
const EXPLORE_RAW_CAP: u64 = 8;
const EXPLORE_COVERAGE_CAP: u64 = 8;
const EXPLORE_PROX_CAP: u64 = 12;
const EXPLORE_W_NAME: u64 = 3;
const EXPLORE_W_PROX: u64 = 2;

pub(crate) fn explore_fallback_alignment_multiplier(
    query_term_count: usize,
    matched_term_count: usize,
) -> u64 {
    if query_term_count <= 1 {
        return 8;
    }
    match (query_term_count, matched_term_count) {
        (_, 0) => 1,
        (2, 1) => 3,
        (2, _) => 8,
        (3, 1) => 2,
        (3, 2) => 6,
        (3, _) => 8,
        (_, 1) => 1,
        (_, 2) => 4,
        _ => 8,
    }
}

/// Extract query terms that aren't part of the matched concept key,
/// filtering out stopwords and short words.
fn compute_remainder_terms(query: &str, concept_key: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "a", "an", "the", "in", "on", "of", "for", "to", "and", "or", "is", "it", "my", "at", "by",
        "do", "no", "so", "up", "if", "with", "from", "this", "that",
    ];
    let key_words: Vec<&str> = concept_key.split_whitespace().collect();
    query
        .split_whitespace()
        .filter(|w| {
            let lower = w.to_ascii_lowercase();
            !key_words.iter().any(|kw| kw.eq_ignore_ascii_case(w))
                && !STOPWORDS.contains(&lower.as_str())
                && lower.len() >= 3
        })
        .map(|w| w.to_ascii_lowercase())
        .collect()
}

fn explore_symbol_segments(name: &str) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = String::new();
    for ch in name.chars() {
        let is_separator =
            !ch.is_alphanumeric() || matches!(ch, '_' | ':' | '-' | '/' | '\\' | '.');
        if is_separator {
            if !current.is_empty() {
                segments.push(current.to_ascii_lowercase());
                current.clear();
            }
            continue;
        }

        let split_before = ch.is_uppercase()
            && !current.is_empty()
            && current
                .chars()
                .last()
                .is_some_and(|prev| prev.is_lowercase() || prev.is_ascii_digit());
        if split_before {
            segments.push(current.to_ascii_lowercase());
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        segments.push(current.to_ascii_lowercase());
    }

    segments
}

fn explore_fallback_symbol_match(name: &str, term: &str) -> bool {
    if name.eq_ignore_ascii_case(term) {
        return true;
    }

    let segments = explore_symbol_segments(name);
    segments.iter().any(|segment| segment == term)
}

fn explore_terms_related(lhs: &str, rhs: &str) -> bool {
    if lhs.eq_ignore_ascii_case(rhs) {
        return true;
    }

    let lhs = lhs.to_ascii_lowercase();
    let rhs = rhs.to_ascii_lowercase();
    let shared_prefix = lhs
        .chars()
        .zip(rhs.chars())
        .take_while(|(a, b)| a == b)
        .count();
    shared_prefix >= 5
}

fn record_explore_file_signal(
    file_signals: &mut HashMap<String, ExploreFileSignal>,
    path: &str,
    term: &str,
    weight: u64,
) {
    let signal = file_signals.entry(path.to_string()).or_default();
    signal.raw_score += weight;
    signal.matched_terms.insert(term.to_string());
}

fn derive_explore_cluster(
    index: &crate::live_index::LiveIndex,
    query_terms: &[String],
    file_signals: &HashMap<String, ExploreFileSignal>,
    limit: usize,
) -> Option<DerivedExploreCluster> {
    const GENERIC_SYMBOLS: &[&str] = &[
        "build", "create", "error", "get", "handle", "init", "main", "new", "parse", "process",
        "result", "run", "set", "test", "update",
    ];

    if query_terms.len() < 2 {
        return None;
    }

    let mut ranked_files: Vec<(String, u64, usize)> = file_signals
        .iter()
        .filter_map(|(path, signal)| {
            let file = index.get_file(path)?;
            let coverage = signal.matched_terms.len();
            if coverage < 2 {
                return None;
            }
            let path_penalty = explore_path_penalty(path, Some(&file.classification));
            let score =
                (signal.raw_score + ((coverage as u64) * (coverage as u64) * 10)) * path_penalty;
            Some((path.clone(), score, coverage))
        })
        .collect();
    ranked_files.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked_files.truncate(limit.clamp(1, 3));
    if ranked_files.is_empty() {
        return None;
    }

    let mut candidate_scores: HashMap<String, u64> = HashMap::new();
    for (path, file_score, coverage) in &ranked_files {
        let Some(file) = index.get_file(path) else {
            continue;
        };

        for symbol in &file.symbols {
            let lower = symbol.name.to_ascii_lowercase();
            if lower.len() < 4 || GENERIC_SYMBOLS.contains(&lower.as_str()) {
                continue;
            }

            let segments = explore_symbol_segments(&symbol.name);
            let overlap = query_terms
                .iter()
                .filter(|term| {
                    segments
                        .iter()
                        .any(|segment| explore_terms_related(segment, term))
                })
                .count();
            if overlap == 0 {
                continue;
            }

            let kind_bonus = match symbol.kind.to_string().as_str() {
                "struct" | "class" | "trait" | "interface" | "enum" => 5,
                "fn" | "method" => 4,
                "impl" | "mod" | "module" => 3,
                _ => 1,
            } as u64;
            let reverse_hits = index
                .reverse_index
                .get(&symbol.name)
                .map(|hits| hits.len())
                .unwrap_or(0);
            let rarity_bonus = match reverse_hits {
                0 => 8,
                1..=2 => 6,
                3..=5 => 4,
                6..=10 => 2,
                _ => 1,
            } as u64;
            let length_bonus = (symbol.name.len().min(24) / 6) as u64;
            let score = *file_score
                + ((*coverage as u64) * 5)
                + ((overlap as u64) * 12)
                + kind_bonus
                + rarity_bonus
                + length_bonus;

            let entry = candidate_scores.entry(symbol.name.clone()).or_insert(0);
            if score > *entry {
                *entry = score;
            }
        }
    }

    let mut ranked_symbols: Vec<(String, u64)> = candidate_scores.into_iter().collect();
    ranked_symbols.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let promoted_symbols: Vec<String> = ranked_symbols
        .into_iter()
        .take(limit.clamp(1, 4))
        .map(|(name, _)| name)
        .collect();
    if promoted_symbols.is_empty() {
        return None;
    }

    Some(DerivedExploreCluster {
        seed_terms: query_terms.to_vec(),
        promoted_symbols,
        seed_files: ranked_files.into_iter().map(|(path, _, _)| path).collect(),
    })
}

pub(crate) fn explore(
    guard: &LiveIndex,
    request: &ExploreRequest,
    check: &mut dyn FnMut() -> bool,
) -> Result<ExploreResult, ExploreError> {
    if !check() {
        return Err(ExploreError::Stopped);
    }
    let lang_filter = request.language;
    let limit = request.limit;
    let include_noise = request.include_noise;
    let include_vendor = request.include_vendor;
    let include_personal_tooling = request.include_personal_tooling;
    let suppress_vendor = !(include_noise || include_vendor);
    let suppress_personal = !(include_noise || include_personal_tooling);
    let suppress_other_noise = !include_noise;
    let any_suppression = suppress_vendor || suppress_personal || suppress_other_noise;
    let concept = super::explore::match_concept(&request.query);

    // A single-word concept that matched ONLY via the stemmed fallback is a
    // weak signal: the query never named the concept verbatim. When the query
    // also carries specific terms (computed below) the header should let those
    // terms lead and demote such a concept to a parenthetical hint, rather than
    // collapsing the topic to a label the user did not type. Exact matches and
    // multi-word concept keys keep leading the header.
    let demote_stem_only_concept = matches!(
        concept,
        Some((key, _, super::explore::ConceptMatchKind::Stemmed))
            if key.split_whitespace().count() == 1
    );

    let mut enriched_imports: Vec<String> = Vec::new();
    let (label, symbol_queries, text_queries, remainder_terms) = if let Some((key, c, _)) = concept
    {
        let remainder = compute_remainder_terms(&request.query, key);
        let mut sym_q: Vec<String> = c.symbol_queries.iter().map(|s| s.to_string()).collect();
        // Convention-aware enrichment: add project-specific imports related to the concept.
        let project_imports = super::conventions::extract_top_import_roots(guard, 100);
        let enrichment = super::explore::enrich_concept_with_imports(c, &project_imports);
        enriched_imports = enrichment;
        sym_q.extend(enriched_imports.iter().cloned());
        (
            c.label.to_string(),
            sym_q,
            c.text_queries
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            remainder,
        )
    } else {
        let terms = super::explore::fallback_terms(&request.query);
        if terms.is_empty() {
            return Err(ExploreError::EmptyQuery);
        }
        (format!("'{}'", request.query), terms.clone(), terms, vec![])
    };

    // Phase 1: Symbol search — over-fetch and track both match counts and
    // query-term coverage per symbol so multi-term hits outrank one-term noise.
    let mut match_scores: HashMap<(String, String, String), ExploreMatchScore> = HashMap::new();
    let mut file_signals: HashMap<String, ExploreFileSignal> = HashMap::new();

    // Phase 0: Module-path boosting — symbols from files whose path segment
    // matches a query term get a weight boost. +2 for exact segment match,
    // +1 for substring segment match. Per-directory cap of `limit` symbols.
    // For concept+remainder queries, boost on remainder terms only so path-scoping
    // is driven by the narrowing terms (e.g., "watcher" in "error handling in the watcher").
    let boost_terms = if remainder_terms.is_empty() {
        symbol_queries.clone()
    } else {
        remainder_terms.clone()
    };
    for term in &boost_terms {
        if !check() {
            return Err(ExploreError::Stopped);
        }
        let term_lower = term.to_ascii_lowercase();
        for (file_path, file) in guard.all_files() {
            if !check() {
                return Err(ExploreError::Stopped);
            }
            if explore_should_skip_path_boost(
                file_path,
                &file.classification,
                include_noise,
                include_vendor,
                include_personal_tooling,
            ) {
                continue;
            }
            let segments: Vec<&str> = file_path.split(&['/', '\\'][..]).collect();
            let best_match = segments
                .iter()
                .filter_map(|seg| {
                    let seg_lower = seg.to_ascii_lowercase();
                    let seg_stem = seg_lower
                        .strip_suffix(".rs")
                        .or_else(|| seg_lower.strip_suffix(".py"))
                        .or_else(|| seg_lower.strip_suffix(".ts"))
                        .or_else(|| seg_lower.strip_suffix(".js"))
                        .or_else(|| seg_lower.strip_suffix(".go"))
                        .unwrap_or(&seg_lower);
                    if seg_stem == term_lower {
                        Some(2usize)
                    } else if seg_stem.contains(&*term_lower) {
                        Some(1usize)
                    } else {
                        None
                    }
                })
                .max();
            if let Some(weight) = best_match {
                record_explore_file_signal(
                    &mut file_signals,
                    file_path,
                    &term_lower,
                    weight as u64,
                );
                for (injected, sym) in file.symbols.iter().enumerate() {
                    if injected >= limit {
                        break;
                    }
                    let entry = (sym.name.clone(), sym.kind.to_string(), file_path.clone());
                    let score = match_scores.entry(entry).or_default();
                    if score.raw_count == 0 {
                        // Cap path-boost: only seed symbols that have no prior
                        // matches. This prevents path-matching files from
                        // dominating over content-matching files.
                        score.raw_count = weight.min(1);
                    }
                    score.matched_terms.insert(term_lower.clone());
                }
            }
        }
    }

    // Merge remainder terms into symbol/text queries for Phases 1-2 so that compound
    // queries like "error handling in the watcher" search concept queries AND "watcher".
    let mut all_symbol_queries = symbol_queries.clone();
    all_symbol_queries.extend(remainder_terms.iter().cloned());
    let mut all_text_queries = text_queries.clone();
    all_text_queries.extend(remainder_terms.iter().cloned());

    // Lowercased remainder terms drive "specific term" detection below: a
    // remainder noun that matches an indexed symbol name (e.g. "admission"
    // -> AdmissionTier) must never be outranked by the concept bucket's
    // generic symbol_queries (e.g. "index"), and it should keep the topic
    // visible in the header instead of collapsing it to the concept label.
    let remainder_term_keys: HashSet<String> = remainder_terms
        .iter()
        .map(|t| t.to_ascii_lowercase())
        .collect();
    // Remainder terms that actually resolve to an indexed symbol name via
    // search_symbols' substring semantics. Populated during the symbol
    // search loop so we don't pay for a second index pass.
    let mut specific_terms: HashSet<String> = HashSet::new();

    let fallback_mode = concept.is_none();
    for sq in &all_symbol_queries {
        if !check() {
            return Err(ExploreError::Stopped);
        }
        let term_key = sq.to_ascii_lowercase();
        let result = search::search_symbols(guard, sq, None, limit * 3);
        let is_remainder_term = remainder_term_keys.contains(&term_key);
        let mut seen_paths = HashSet::new();
        for hit in &result.hits {
            if fallback_mode && !explore_fallback_symbol_match(&hit.name, &term_key) {
                continue;
            }
            // A remainder noun that lands on a real symbol name is a
            // load-bearing query term, not generic concept noise.
            if !fallback_mode && is_remainder_term {
                specific_terms.insert(term_key.clone());
            }
            let entry = (hit.name.clone(), hit.kind.clone(), hit.path.clone());
            let score = match_scores.entry(entry).or_default();
            score.raw_count += 1;
            score.matched_terms.insert(term_key.clone());
            if seen_paths.insert(hit.path.clone()) {
                record_explore_file_signal(&mut file_signals, &hit.path, &term_key, 2);
            }
        }
    }

    // Filter Phase 1 results by language and path_prefix
    if lang_filter.is_some() || request.path_prefix.is_some() {
        match_scores.retain(|(_, _, path), _| {
            if let Some(ref prefix) = request.path_prefix
                && !path.starts_with(prefix.as_str())
            {
                return false;
            }
            if let Some(ref lang) = lang_filter {
                let ext = path.rsplit('.').next().unwrap_or("");
                if crate::domain::index::LanguageId::from_extension(ext).as_ref() != Some(lang) {
                    return false;
                }
            }
            true
        });
        file_signals.retain(|path, _| {
            if let Some(ref prefix) = request.path_prefix
                && !path.starts_with(prefix.as_str())
            {
                return false;
            }
            if let Some(ref lang) = lang_filter {
                let ext = path.rsplit('.').next().unwrap_or("");
                if crate::domain::index::LanguageId::from_extension(ext).as_ref() != Some(lang) {
                    return false;
                }
            }
            true
        });
    }

    if any_suppression {
        file_signals.retain(|path, _| {
            let Some(file) = guard.get_file(path) else {
                return false;
            };
            if suppress_other_noise && explore_is_test_like_path(path, Some(&file.classification)) {
                return false;
            }
            if suppress_personal && crate::live_index::query::is_personal_tooling_path(path) {
                return false;
            }
            let class = search::NoisePolicy::classify_path(path, None);
            match class {
                search::NoiseClass::Vendor => !suppress_vendor,
                search::NoiseClass::Generated | search::NoiseClass::Ignored => {
                    !suppress_other_noise
                }
                search::NoiseClass::None => true,
            }
        });
    }

    // Phase 2: Text search — collect text hits and inject enclosing symbols into match_counts
    let mut text_hits: Vec<(String, String, usize)> = Vec::new(); // (path, line, line_number)
    for tq in &all_text_queries {
        if !check() {
            return Err(ExploreError::Stopped);
        }
        let mut options = search::TextSearchOptions {
            total_limit: limit.min(50),
            max_per_file: limit, // need enough matches per file for enclosing symbol extraction
            ..search::TextSearchOptions::for_current_code_search()
        };
        if let Some(ref prefix) = request.path_prefix {
            options.path_scope = search::PathScope::Prefix(prefix.clone());
        }
        if let Some(ref lang) = lang_filter {
            options.language_filter = Some(*lang);
        }
        let result = search::search_text_with_options(guard, Some(tq), None, false, &options);
        if let Ok(r) = result {
            let term_key = tq.to_ascii_lowercase();
            for file in &r.files {
                // SF-STRESS-013: require a word-boundary match so a concept
                // idiom like `try {` does not score off `Country {`. Filter
                // before recording the file signal so coincidental files do
                // not register at all.
                let boundary_matches: Vec<&search::TextLineMatch> = file
                    .matches
                    .iter()
                    .filter(|m| concept_text_query_matches_on_boundary(&m.line, tq))
                    .collect();
                if boundary_matches.is_empty() {
                    continue;
                }
                record_explore_file_signal(&mut file_signals, &file.path, &term_key, 3);
                for m in boundary_matches {
                    if text_hits.len() < limit && !is_noise_line(&m.line) {
                        text_hits.push((file.path.clone(), m.line.clone(), m.line_number));
                    }
                    // Inject enclosing symbol into match_counts.
                    // Weight 2 so content matches outweigh path-only boosts.
                    if let Some(ref enc) = m.enclosing_symbol {
                        let entry = (enc.name.clone(), enc.kind.clone(), file.path.clone());
                        let score = match_scores.entry(entry).or_default();
                        score.raw_count += 2;
                        score.matched_terms.insert(term_key.clone());
                    }
                }
            }
        }
    }

    // Filter text hits by path_prefix (language already handled via TextSearchOptions)
    if let Some(ref prefix) = request.path_prefix {
        text_hits.retain(|(path, _, _)| path.starts_with(prefix.as_str()));
    }

    let derived_cluster = if fallback_mode {
        derive_explore_cluster(guard, &symbol_queries, &file_signals, limit)
    } else {
        None
    };

    if let Some(cluster) = &derived_cluster {
        for derived_query in &cluster.promoted_symbols {
            if !check() {
                return Err(ExploreError::Stopped);
            }
            let term_key = derived_query.to_ascii_lowercase();
            if !all_symbol_queries
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(derived_query))
            {
                let result = search::search_symbols(guard, derived_query, None, limit * 2);
                let mut seen_paths = HashSet::new();
                for hit in &result.hits {
                    let entry = (hit.name.clone(), hit.kind.clone(), hit.path.clone());
                    let score = match_scores.entry(entry).or_default();
                    score.raw_count += 1;
                    score.matched_terms.insert(term_key.clone());
                    if seen_paths.insert(hit.path.clone()) {
                        record_explore_file_signal(&mut file_signals, &hit.path, &term_key, 2);
                    }
                }
            }

            let mut options = search::TextSearchOptions {
                total_limit: limit.min(50),
                max_per_file: limit,
                ..search::TextSearchOptions::for_current_code_search()
            };
            if let Some(ref prefix) = request.path_prefix {
                options.path_scope = search::PathScope::Prefix(prefix.clone());
            }
            if let Some(ref lang) = lang_filter {
                options.language_filter = Some(*lang);
            }

            if let Ok(result) =
                search::search_text_with_options(guard, Some(derived_query), None, false, &options)
            {
                for file in &result.files {
                    if !file.matches.is_empty() {
                        record_explore_file_signal(&mut file_signals, &file.path, &term_key, 2);
                    }
                    for m in &file.matches {
                        if text_hits.len() < limit && !is_noise_line(&m.line) {
                            text_hits.push((file.path.clone(), m.line.clone(), m.line_number));
                        }
                        if let Some(ref enc) = m.enclosing_symbol {
                            let entry = (enc.name.clone(), enc.kind.clone(), file.path.clone());
                            let score = match_scores.entry(entry).or_default();
                            score.raw_count += 1;
                            score.matched_terms.insert(term_key.clone());
                        }
                    }
                }
            }
        }
    }

    // Phase 3: Filter noise, weight by kind and path, sort, truncate to limit.
    // Exclude explore.rs itself (CONCEPT_MAP contains concept keywords in its body).
    match_scores.retain(|(_, _, path), _| {
        !path.ends_with("protocol/explore.rs") && !path.ends_with("guidance/explore.rs")
    });

    // Score each symbol: match_count * kind_weight, penalized for doc/generated files.
    let scored: Vec<((String, String, String), u64)> = match_scores
        .into_iter()
        .map(|((name, kind, path), score_data)| {
            // Kind weight: definition-like symbols rank higher than incidental matches.
            let kind_weight: u64 = match kind.as_str() {
                "fn" | "method" => 4,
                "struct" | "class" | "trait" | "interface" | "enum" => 4,
                "impl" | "mod" | "module" => 3,
                "const" | "type" => 2,
                "variable" | "let" => 1,
                "key" | "section" => 1,
                _ => 2, // "other" (selectors, etc.)
            };
            let classification = guard.get_file(&path).map(|file| &file.classification);
            let path_penalty = explore_path_penalty(&path, classification);
            // Specific remainder terms (query nouns that resolved to a real
            // indexed symbol name, e.g. "admission" -> AdmissionTier) are
            // load-bearing, not generic concept noise. Each one counts as
            // two extra terms toward coverage: enough that a single specific
            // noun outranks a generic two-term concept-bucket hit (the
            // failure this fix targets), but a symbol with genuine
            // three-plus-term co-occurrence still wins. This only affects
            // concept mode; fallback mode never populates the set.
            let specific_match_count = score_data
                .matched_terms
                .iter()
                .filter(|term| specific_terms.contains(*term))
                .count() as u64;
            let effective_terms = score_data.matched_terms.len() as u64 + 2 * specific_match_count;
            let alignment_multiplier = if fallback_mode {
                explore_fallback_alignment_multiplier(
                    symbol_queries.len(),
                    score_data.matched_terms.len(),
                )
            } else {
                8
            };
            // Saturating NAME signal. The old score multiplied three
            // correlated name-overlap factors — raw_count * coverage^2 *
            // alignment — which scaled the query-dependent part like
            // k * k^2 * f(k) (~1:32:216 for 1/2/3-token matches). After the
            // max-normalization below that pinned the best name match to 1.00
            // and cratered everyone else onto a cliff ("lone 1.00, then
            // crater"). Fold raw hit strength and term coverage into ONE
            // additive, saturated value (coverage is now LINEAR, not squared)
            // and scale by the bounded alignment/coverage-quality gate.
            let raw_component = (score_data.raw_count as u64).min(EXPLORE_RAW_CAP);
            let coverage_component = effective_terms.min(EXPLORE_COVERAGE_CAP);
            let name_signal = (raw_component + 3 * coverage_component) * alignment_multiplier;
            // Concept PROXIMITY: a symbol living in a file that >=2 query
            // terms point at (path segment / content / co-located symbol name)
            // earns an additive lift even when its own NAME shares no query
            // token. This consumes `file_signals`, the per-file concept signal
            // that was computed but previously never read by the scorer. Gated
            // on the same >=2-term threshold `derive_explore_cluster` uses, so
            // a coincidental single-term file cannot inflate everything in it.
            let prox = file_signals
                .get(&path)
                .filter(|signal| signal.matched_terms.len() >= 2)
                .map(|signal| signal.raw_score.min(EXPLORE_PROX_CAP))
                .unwrap_or(0);
            // Blend name and proximity ADDITIVELY with W_NAME >= W_PROX so an
            // exact-name query still ranks its target top (no over-correction),
            // while proximity can lift a concept-central symbol out of the
            // crater the multiplicative curve used to bury it in.
            let score =
                kind_weight * path_penalty * (EXPLORE_W_NAME * name_signal + EXPLORE_W_PROX * prox);
            ((name, kind, path), score)
        })
        .collect();

    // Noise filtering: hide vendor / generated / gitignored / personal-tooling
    // by default. include_noise is the umbrella; include_vendor and
    // include_personal_tooling are additive finer-grained overrides per B1.
    let mut noise_hidden: usize = 0;
    let should_hide_path = |path: &str| -> bool {
        let Some(file) = guard.get_file(path) else {
            return true;
        };
        if suppress_other_noise && explore_is_test_like_path(path, Some(&file.classification)) {
            return true;
        }
        if suppress_personal && crate::live_index::query::is_personal_tooling_path(path) {
            return true;
        }
        let class = search::NoisePolicy::classify_path(path, None);
        match class {
            search::NoiseClass::Vendor => suppress_vendor,
            search::NoiseClass::Generated | search::NoiseClass::Ignored => suppress_other_noise,
            search::NoiseClass::None => false,
        }
    };

    let mut ranked = scored;
    // Deterministic order (Constitution IV): score desc, then the full
    // (name, kind, path) key ascending. HashMap iteration order is
    // nondeterministic and the stable sort would otherwise preserve it for
    // equal scores; the key is unique so this is a total order.
    ranked.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(a.0.0.cmp(&b.0.0))
            .then(a.0.1.cmp(&b.0.1))
            .then(a.0.2.cmp(&b.0.2))
    });
    // Filter out weak matches (score < 8 means single text-only hit in a doc file).
    ranked.retain(|(_, score)| *score >= 8);
    // SF-STRESS-013: drop hidden vendor/generated/test symbols BEFORE the
    // limit truncation so they no longer consume the result budget. Running
    // the noise filter after truncate starved real results (e.g. 1 visible
    // symbol while 19 hidden vendor symbols had already eaten the limit).
    if any_suppression {
        ranked.retain(|(key, _)| {
            let hide = should_hide_path(&key.2);
            if hide {
                noise_hidden += 1;
            }
            !hide
        });
    }
    let overflow_count = ranked.len().saturating_sub(limit);
    ranked.truncate(limit);
    let max_score = ranked.first().map(|(_, s)| *s as f32).unwrap_or(1.0);
    let symbol_scores: Vec<f32> = ranked
        .iter()
        .map(|(_, s)| {
            if max_score > 0.0 {
                (*s as f32 / max_score).min(1.0)
            } else {
                0.0
            }
        })
        .collect();
    let symbol_hits: Vec<(String, String, String)> = ranked.into_iter().map(|(k, _)| k).collect();

    // Text hits still pass through the same noise filter so hidden files
    // never leak into the rendered pattern list.
    let text_hits = if any_suppression {
        text_hits
            .into_iter()
            .filter(|(path, _, _)| {
                let hide = should_hide_path(path);
                if hide {
                    noise_hidden += 1;
                }
                !hide
            })
            .collect::<Vec<_>>()
    } else {
        text_hits
    };

    // Count files by symbol/text presence
    let mut file_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for (_, _, path) in &symbol_hits {
        *file_counts.entry(path.clone()).or_default() += 1;
    }
    for (path, _, _) in &text_hits {
        *file_counts.entry(path.clone()).or_default() += 1;
    }
    let mut related_files: Vec<(String, usize)> = file_counts.into_iter().collect();
    sort_related_files(&mut related_files);
    related_files.truncate(limit);

    // Depth 2+: enrich top symbol hits with signatures and dependents
    let depth = request.depth.unwrap_or(1).clamp(1, 3);
    let mut enriched_symbols: Vec<EnrichedSymbol> = Vec::new();
    // (name, kind, path, signature, dependent_files)

    if depth >= 2 {
        let enrich_limit = 5.min(symbol_hits.len());
        for (name, kind, path) in &symbol_hits[..enrich_limit] {
            if !check() {
                return Err(ExploreError::Stopped);
            }
            let signature = guard.get_file(path).and_then(|file| {
                let sym = file
                    .symbols
                    .iter()
                    .find(|s| s.name == *name && s.kind.to_string().eq_ignore_ascii_case(kind))?;
                let body = std::str::from_utf8(
                    &file.content[sym.byte_range.0 as usize..sym.byte_range.1 as usize],
                )
                .ok()?;
                Some(extract_signature(body))
            });

            let dependents = {
                let ref_view = guard.capture_find_references_view(name, None, 3);
                ref_view
                    .files
                    .iter()
                    .take(3)
                    .map(|f| f.file_path.clone())
                    .collect()
            };

            enriched_symbols.push((
                name.clone(),
                kind.clone(),
                path.clone(),
                signature,
                dependents,
            ));
        }
    }

    // Depth 3: gather implementations AND type dependencies for top symbols
    let mut symbol_impls: Vec<(String, Vec<String>)> = Vec::new();
    let mut symbol_deps: Vec<(String, Vec<String>)> = Vec::new();
    if depth >= 3 {
        let impl_limit = 3.min(enriched_symbols.len());
        for (name, _kind, path, _, _) in &enriched_symbols[..impl_limit] {
            if !check() {
                return Err(ExploreError::Stopped);
            }
            // Implementations (trait → implementors)
            let impl_view = guard.capture_implementations_view(name, None);
            let impl_names: Vec<String> = impl_view
                .entries
                .iter()
                .take(5)
                .map(|e| {
                    format!(
                        "{} impl {} ({}:{})",
                        e.implementor, e.trait_name, e.file_path, e.line
                    )
                })
                .collect();
            if !impl_names.is_empty() {
                symbol_impls.push((name.clone(), impl_names));
            }

            // Type dependencies (what types does this symbol reference?)
            let bundle = guard.capture_context_bundle_view(path, name, None, None);
            if let crate::live_index::query::ContextBundleView::Found(found) = bundle {
                let dep_names: Vec<String> = found
                    .dependencies
                    .iter()
                    .take(8)
                    .map(|d| format!("{} {} ({})", d.kind_label, d.name, d.file_path))
                    .collect();
                if !dep_names.is_empty() {
                    symbol_deps.push((name.clone(), dep_names));
                }
            }
        }
    }

    // When a concept matched but the query also named specific terms that
    // resolve to indexed symbols, keep those nouns visible in the header so
    // the topic isn't silently collapsed to the generic concept label.
    // Order by query position (remainder_terms preserves it) for stability.
    let display_label = if !specific_terms.is_empty() {
        let ordered: Vec<&str> = remainder_terms
            .iter()
            .filter(|t| specific_terms.contains(t.as_str()))
            .map(|t| t.as_str())
            .collect();
        if demote_stem_only_concept {
            // The concept matched only via a single-word stem misfire (e.g.
            // "indexed" -> "Indexing") while the query named real terms. Lead
            // with those specific terms and demote the concept to an honest
            // parenthetical hint, instead of letting a label the user never
            // typed front the header.
            format!("{} (+ {label} signals)", ordered.join(", "))
        } else {
            format!("{label} + {}", ordered.join(", "))
        }
    } else {
        label.clone()
    };

    if !check() {
        return Err(ExploreError::Stopped);
    }
    Ok(ExploreResult {
        display_label,
        symbol_hits,
        symbol_scores,
        text_hits,
        related_files,
        enriched_symbols,
        symbol_impls,
        symbol_deps,
        derived_cluster,
        enriched_imports,
        depth,
        noise_hidden,
        overflow_count,
    })
}

fn sort_related_files(files: &mut [(String, usize)]) {
    files.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
}

#[cfg(test)]
mod tests {
    #[test]
    fn related_file_ties_use_path_order() {
        let mut files = vec![
            ("z.rs".into(), 2),
            ("a.rs".into(), 2),
            ("first.rs".into(), 3),
        ];
        super::sort_related_files(&mut files);
        assert_eq!(
            files,
            vec![
                ("first.rs".into(), 3),
                ("a.rs".into(), 2),
                ("z.rs".into(), 2)
            ]
        );
    }
}
