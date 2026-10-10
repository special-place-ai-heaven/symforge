//! Source-aware question routing shared by MCP and embedded hosts.

use super::smart_query::{self, QueryIntent};

pub(crate) fn resolve(index: &crate::live_index::LiveIndex, question: &str) -> (QueryIntent, bool) {
    let (mut intent, mut matched_prefix) = smart_query::classify_intent_with_match(question);
    if matches!(
        intent,
        QueryIntent::Understand { .. } | QueryIntent::Explore { .. }
    ) {
        if let Some(name) = extract_exact_implementation_understanding_candidate(index, question) {
            intent = QueryIntent::UnderstandImplementations { name };
            matched_prefix = false;
        } else if let Some(symbol) = extract_exact_symbol_understanding_candidate(index, question) {
            intent = QueryIntent::UnderstandSymbol { symbol };
            matched_prefix = false;
        }
    }
    (intent, matched_prefix)
}

fn ask_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for raw in query.split_whitespace() {
        let token = raw
            .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != ':')
            .to_string();
        if token.is_empty() {
            continue;
        }
        if token.len() < 2 {
            continue;
        }
        if tokens
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&token))
        {
            continue;
        }
        tokens.push(token);
    }
    tokens
}

fn ask_symbol_candidate_tokens(query: &str) -> Vec<String> {
    ask_query_tokens(query)
        .into_iter()
        .filter(|token| {
            let is_symbol_like = token.contains('_')
                || token.contains("::")
                || token.chars().skip(1).any(|c| c.is_uppercase());
            is_symbol_like && token.len() >= 4
        })
        .collect()
}

fn extract_exact_symbol_understanding_candidate(
    index: &crate::live_index::LiveIndex,
    query: &str,
) -> Option<String> {
    const GENERIC_SYMBOLS: &[&str] = &[
        "build", "create", "get", "handle", "init", "main", "new", "parse", "process", "run",
        "set", "test", "update",
    ];

    /// Score a single (path, symbol) match for prominence. Higher = more canonical.
    fn score_match(path: &str, line_start: u32, line_end: u32) -> i32 {
        let mut score = 0i32;
        if path.starts_with("src/") || path.contains("/src/") {
            score += 10;
        }
        let lower = path.to_ascii_lowercase();
        if !lower.contains("test")
            && !lower.contains("vendor")
            && !lower.contains("example")
            && !lower.contains("bench")
        {
            score += 5;
        }
        let span = line_end.saturating_sub(line_start);
        score += ((span / 10) as i32).min(10);
        score
    }

    /// Search `index` for an exact (case-insensitive) match for `token`.
    /// Returns `Some((canonical_name, best_score))` when 1-5 matches are found.
    fn find_token(index: &crate::live_index::LiveIndex, token: &str) -> Option<(String, i32)> {
        let mut matches: Vec<(String, i32)> = Vec::new(); // (canonical_name, score)
        for (path, file) in index.all_files() {
            for symbol in &file.symbols {
                if symbol.name.eq_ignore_ascii_case(token) {
                    let s = score_match(path.as_str(), symbol.line_range.0, symbol.line_range.1);
                    matches.push((symbol.name.clone(), s));
                }
            }
        }
        if matches.is_empty() || matches.len() > 5 {
            return None;
        }
        // Pick the match with the highest score; stable (first wins on tie).
        let best = matches
            .into_iter()
            .max_by_key(|(_, score)| *score)
            .expect("non-empty; guarded by is_empty check above");
        Some(best)
    }

    // Collect (canonical_name, score) for each qualifying token.
    let mut candidates: Vec<(String, i32)> = Vec::new();
    let tokens = ask_symbol_candidate_tokens(query);
    for token in &tokens {
        if GENERIC_SYMBOLS.contains(&token.to_ascii_lowercase().as_str()) {
            continue;
        }
        if let Some(hit) = find_token(index, token) {
            candidates.push(hit);
        }
    }

    // Compound token joining: try "token_a_token_b" for adjacent pairs when
    // no single-token candidate was found yet.
    if candidates.is_empty() && tokens.len() >= 2 {
        for window in tokens.windows(2) {
            let joined = format!("{}_{}", window[0], window[1]);
            if GENERIC_SYMBOLS.contains(&joined.to_ascii_lowercase().as_str()) {
                continue;
            }
            if let Some(hit) = find_token(index, &joined) {
                candidates.push(hit);
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Deduplicate by canonical name, keeping the entry with the highest score.
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    candidates.dedup_by(|later, first| {
        // `dedup_by` drops `later` when true is returned.  The sort above orders ascending
        // by name, then descending by score within each name group, so `first` always holds
        // the highest score for a given name — exactly the entry we want to keep.
        later.0.eq_ignore_ascii_case(&first.0)
    });

    // Two or more DISTINCT symbols resolved (e.g. "how does X interact with Y").
    // The question is about a relationship, not a single definition — fall through
    // to Understand → explore rather than picking the highest-scoring subject.
    if candidates.len() >= 2 {
        return None;
    }

    // Pick the single candidate with the highest prominence score.
    candidates
        .into_iter()
        .max_by_key(|(_, score)| *score)
        .map(|(name, _)| name)
}

fn extract_exact_implementation_understanding_candidate(
    index: &crate::live_index::LiveIndex,
    query: &str,
) -> Option<String> {
    const IMPLEMENTATION_CUES: &[&str] = &[
        "type",
        "types",
        "implementation",
        "implementations",
        "implementor",
        "implementors",
        "implementer",
        "implementers",
        "implements",
    ];
    const GENERIC_QUERY_WORDS: &[&str] = &[
        "a",
        "all",
        "an",
        "and",
        "are",
        "describe",
        "does",
        "explain",
        "help",
        "how",
        "main",
        "me",
        "of",
        "the",
        "through",
        "tell",
        "type",
        "types",
        "understand",
        "walk",
        "what",
        "work",
    ];
    let tokens = ask_query_tokens(query);
    if !tokens.iter().any(|token| {
        IMPLEMENTATION_CUES
            .iter()
            .any(|cue| token.eq_ignore_ascii_case(cue))
    }) {
        return None;
    }

    let mut candidates = Vec::new();
    for token in tokens {
        let lower = token.to_ascii_lowercase();
        if GENERIC_QUERY_WORDS.contains(&lower.as_str()) {
            continue;
        }

        if let Some(candidate) = exact_trait_like_symbol_candidate(index, &token) {
            candidates.push(candidate);
            continue;
        }

        if lower.ends_with('s') && lower.len() > 4 {
            let singular = &token[..token.len() - 1];
            if let Some(candidate) = exact_trait_like_symbol_candidate(index, singular) {
                candidates.push(candidate);
            }
        }
    }

    candidates.sort();
    candidates.dedup();
    if candidates.len() == 1 {
        candidates.into_iter().next()
    } else {
        None
    }
}

fn exact_trait_like_symbol_candidate(
    index: &crate::live_index::LiveIndex,
    token: &str,
) -> Option<String> {
    let mut exact_match_count = 0usize;
    let mut canonical_name: Option<String> = None;
    for (_path, file) in index.all_files() {
        for symbol in &file.symbols {
            if !symbol.name.eq_ignore_ascii_case(token) {
                continue;
            }
            if !matches!(
                symbol.kind,
                crate::domain::index::SymbolKind::Trait
                    | crate::domain::index::SymbolKind::Interface
                    | crate::domain::index::SymbolKind::Type
            ) {
                continue;
            }
            exact_match_count += 1;
            if canonical_name.is_none() {
                canonical_name = Some(symbol.name.clone());
            }
        }
    }

    if exact_match_count == 1 {
        canonical_name
    } else {
        None
    }
}
