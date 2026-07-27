//! Frozen Gate I `search_knowledge` extraction, ranking, safety, and formatting.
//!
//! Every response is derived from one caller-captured [`PublishedGeneration`].
//! This module never reloads the live index while formatting a result.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Range;
use std::sync::Arc;

use crate::domain::{CoverageStatus, FreshnessStatus, LanguageId, SourceLocation};
use crate::knowledge::{guard_hit, guard_query, project_markdown_sections};
use crate::live_index::knowledge_authority::{KnowledgeAuthorityRecord, KnowledgeVoice};
use crate::live_index::knowledge_bridge::{
    BridgeEvidenceKind, BridgeResolution, CodeAnchorId, DerivedCoverage, KnowledgeAnchor,
};
use crate::live_index::{PublishedGeneration, PublishedIndexStatus, PublishedSourceSet};

use super::search_tools::{KnowledgeAuthorityScope, KnowledgeSourceScope, SearchKnowledgeInput};

const DEFAULT_LIMIT: usize = 10;
const MAX_LIMIT: usize = 100;
const MIN_PROVENANCE_TOKENS: u64 = 64;
const MAX_IDS_PER_HIT: usize = 8;
const MAX_BRIDGE_PREVIEWS_PER_HIT: usize = 4;
/// Excerpt bound in Unicode CHARACTERS, not bytes (SIFT-WS1).
const EXCERPT_MAX_CHARS: usize = 240;
/// Shortest digest prefix ever displayed; extended on collision.
const DIGEST_PREFIX_MIN: usize = 12;

/// Bound a matched line to a readable window around the match.
///
/// SIFT-WS1 (T024). The excerpt used to be the whole matched line with no cap:
/// dogfood captured a 1.5 KB Markdown table row as one hit's excerpt.
///
/// Operates on `char_indices` of the ORIGINAL line. The tempting
/// implementation — lowercase the line, `find()` the match, slice the original
/// at that byte offset — is wrong twice: `to_lowercase()` is not
/// length-preserving for all Unicode, and a byte offset can land inside a
/// multi-byte character and panic. Match location is therefore resolved in
/// CHARACTER space and every cut lands on a character boundary.
fn window_excerpt(line: &str, phrase: &str, terms: &[String]) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= EXCERPT_MAX_CHARS {
        return line.to_string();
    }

    // Locate the match in character space via a lowercased char vector, so the
    // index maps back onto `chars` one-for-one.
    let lower: Vec<char> = line.chars().flat_map(|c| c.to_lowercase()).collect();
    let lower_str: String = lower.iter().collect();
    let find_chars = |needle: &str| -> Option<usize> {
        if needle.is_empty() {
            return None;
        }
        lower_str
            .find(needle)
            .map(|byte| lower_str[..byte].chars().count())
    };
    // `to_lowercase` can change char count (e.g. 'İ'), so a char index derived
    // from the lowered string is only a hint. Clamp it into range.
    let hint = find_chars(phrase)
        .or_else(|| terms.iter().find_map(|term| find_chars(term)))
        .unwrap_or(0)
        .min(chars.len().saturating_sub(1));

    let match_len = phrase.chars().count().max(1);
    // Center the window on the match, then clamp to the line.
    let half = EXCERPT_MAX_CHARS.saturating_sub(match_len) / 2;
    let mut start = hint.saturating_sub(half);
    let mut end = (start + EXCERPT_MAX_CHARS).min(chars.len());
    start = end.saturating_sub(EXCERPT_MAX_CHARS);

    // Snap outward-in to whitespace so a cut never splits a word.
    if start > 0 {
        let limit = (start + 32).min(end);
        if let Some(offset) = (start..limit).find(|index| chars[*index].is_whitespace()) {
            // Never snap past the match itself.
            if offset < hint {
                start = offset + 1;
            }
        }
    }
    if end < chars.len() {
        let floor = end.saturating_sub(32).max(hint + match_len);
        if let Some(offset) = (floor..end)
            .rev()
            .find(|index| chars[*index].is_whitespace())
        {
            end = offset;
        }
    }

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(chars[start..end].iter());
    if end < chars.len() {
        out.push('…');
    }
    out
}

/// Per-response identifier abbreviation table (SIFT-WS1, T025).
///
/// Provenance vectors mix opaque content digests with SEMANTIC rule/policy IDs
/// (`authority-history-v1`, `temporal-coverage-incomplete`). Abbreviating
/// indiscriminately would corrupt the semantic ones, and a fixed 48-bit prefix
/// is not collision-safe across thousands of units — so digests abbreviate to
/// the shortest prefix, at least [`DIGEST_PREFIX_MIN`], that is unique across
/// EVERY digest in the response. One length for the whole response, so the same
/// digest never renders two ways in one answer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DisplayIds {
    digest_len: usize,
}

impl DisplayIds {
    /// A digest is lowercase hex and long enough to be worth shortening.
    /// Anything else — `authority-history-v1`, `role.path.plan-handoff.v1` —
    /// is semantic and renders verbatim.
    fn is_digest(id: &str) -> bool {
        id.len() >= DIGEST_PREFIX_MIN
            && id
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, 'a'..='f'))
    }

    fn for_ids<'a>(ids: impl IntoIterator<Item = &'a str>) -> Self {
        let digests: Vec<&str> = ids.into_iter().filter(|id| Self::is_digest(id)).collect();
        let longest = digests.iter().map(|id| id.len()).max().unwrap_or(0);
        let mut digest_len = DIGEST_PREFIX_MIN;
        while digest_len < longest {
            let mut prefixes: Vec<&str> = digests.iter().map(|id| &id[..digest_len]).collect();
            prefixes.sort_unstable();
            let before = prefixes.len();
            prefixes.dedup();
            if prefixes.len() == before {
                break;
            }
            digest_len += 1;
        }
        Self { digest_len }
    }

    fn render(&self, id: &str) -> String {
        if Self::is_digest(id) && id.len() > self.digest_len {
            id[..self.digest_len].to_string()
        } else {
            id.to_string()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NormalizedQuery {
    phrase: String,
    terms: Vec<String>,
    path_prefix: Option<String>,
    authority_scope: KnowledgeAuthorityScope,
    limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AuthorityDisplay {
    lifecycle: String,
    authority_domain: String,
    code_evidence: String,
    voice: String,
    finding_ids: Vec<String>,
    finding_ids_omitted: usize,
    provenance_ids: Vec<String>,
    provenance_ids_omitted: usize,
    coverage: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct KnowledgeHit {
    /// Owning source's real label (`current` / `worktree:<id>` / `ref:<name>`).
    /// Carried on the hit so global sorting cannot separate a hit from its
    /// provenance; the hit line previously hardcoded `source=current`.
    source_label: String,
    /// Position of the owning source in `select_scoped_sources` order — the
    /// current lane is 0. Ranking tuple position 4 (SIFT-WS0).
    source_precedence: usize,
    path: String,
    line: u32,
    line_range: Range<u32>,
    heading_path: Vec<String>,
    excerpt: String,
    content_hash: String,
    publication_generation: u64,
    content_generation: u64,
    authority: AuthorityDisplay,
    bridge_previews: Vec<String>,
    bridge_previews_omitted: usize,
    exact_phrase: bool,
    heading_match: bool,
    distinct_term_count: usize,
    unit_start: u32,
    unit_len: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FilteredCounts {
    current: usize,
    intent: usize,
    history_only: usize,
    suppressed: usize,
    review_required: usize,
    unknown: usize,
}

pub(crate) fn validate_input(input: &SearchKnowledgeInput) -> Result<NormalizedQuery, String> {
    // Security must run before tokenization, path routing, proxying, analytics,
    // cache lookup, or CCR creation. The rejection never echoes the query.
    guard_query(&input.query)
        .map_err(|_| "Error: sensitive query rejected by repository safety policy.".to_string())?;
    if input
        .path_prefix
        .as_deref()
        .is_some_and(|path| guard_query(path).is_err())
    {
        return Err(
            "Error: sensitive path_prefix rejected by repository safety policy.".to_string(),
        );
    }

    let phrase = input.query.trim();
    if phrase.is_empty() {
        return Err("Error: query must be non-empty.".to_string());
    }
    if input.project.is_some() && input.projects.is_some() {
        return Err("Error: project and projects are mutually exclusive.".to_string());
    }
    if input.projects.as_ref().is_some_and(Vec::is_empty) {
        return Err("Error: projects must not be empty.".to_string());
    }
    if let Some(max_tokens) = input.max_tokens
        && max_tokens < MIN_PROVENANCE_TOKENS
    {
        return Err(format!(
            "Error: max_tokens is too small for provenance; minimum is {MIN_PROVENANCE_TOKENS}."
        ));
    }
    if let Some(projects) = input.projects.as_ref()
        && projects.iter().any(|project| project == "*")
        && projects.len() != 1
    {
        return Err("Error: projects wildcard must be the sole selector.".to_string());
    }
    let path_prefix = normalize_path_prefix(input.path_prefix.as_deref())?;
    let terms = significant_terms(phrase);
    let authority_scope = input
        .authority_scope
        .unwrap_or(KnowledgeAuthorityScope::Default);
    let limit = usize::try_from(input.limit.unwrap_or(DEFAULT_LIMIT as u32))
        .unwrap_or(MAX_LIMIT)
        .clamp(1, MAX_LIMIT);

    Ok(NormalizedQuery {
        phrase: phrase.to_lowercase(),
        terms,
        path_prefix,
        authority_scope,
        limit,
    })
}

/// Select the published generations a source scope addresses, deterministically.
///
/// `current` is the single current-worktree lane. `local_refs`/`worktrees`
/// filter the captured set by source location, excluding the current lane.
/// `all` lists the current lane first (it ranks ahead of a divergent ref but
/// never hides it), then every other lane in `SourceId` order.
pub(crate) fn select_scoped_sources(
    source_set: &PublishedSourceSet,
    scope: KnowledgeSourceScope,
) -> Vec<Arc<PublishedGeneration>> {
    let current_id = &source_set.current_source_id;
    let is_git_ref = |generation: &PublishedGeneration| {
        matches!(
            generation.source.as_deref().map(|source| &source.location),
            Some(SourceLocation::GitRef { .. })
        )
    };
    let is_worktree = |generation: &PublishedGeneration| {
        matches!(
            generation.source.as_deref().map(|source| &source.location),
            Some(SourceLocation::WorkingTree { .. })
        )
    };
    match scope {
        KnowledgeSourceScope::Current => source_set
            .sources
            .get(current_id)
            .map(Arc::clone)
            .into_iter()
            .collect(),
        KnowledgeSourceScope::LocalRefs => source_set
            .sources
            .iter()
            .filter(|(id, generation)| *id != current_id && is_git_ref(generation))
            .map(|(_, generation)| Arc::clone(generation))
            .collect(),
        KnowledgeSourceScope::Worktrees => source_set
            .sources
            .iter()
            .filter(|(id, generation)| *id != current_id && is_worktree(generation))
            .map(|(_, generation)| Arc::clone(generation))
            .collect(),
        KnowledgeSourceScope::All => {
            let mut selected: Vec<Arc<PublishedGeneration>> = source_set
                .sources
                .get(current_id)
                .map(Arc::clone)
                .into_iter()
                .collect();
            selected.extend(
                source_set
                    .sources
                    .iter()
                    .filter(|(id, _)| *id != current_id)
                    .map(|(_, generation)| Arc::clone(generation)),
            );
            selected
        }
    }
}

/// Manifest-declared coverage for one source, defaulting to `Degraded` when no
/// manifest is published so a composed response never claims a false Complete.
pub(crate) fn source_coverage(generation: &PublishedGeneration) -> CoverageStatus {
    generation
        .manifest
        .as_deref()
        .map(|manifest| manifest.coverage)
        .unwrap_or(CoverageStatus::Degraded)
}

/// Overall coverage of a composed response equals the worst included source
/// (L-R06): a single degraded source degrades the whole envelope.
pub(crate) fn worst_source_coverage(selected: &[Arc<PublishedGeneration>]) -> CoverageStatus {
    if selected
        .iter()
        .any(|generation| source_coverage(generation) == CoverageStatus::Degraded)
    {
        CoverageStatus::Degraded
    } else {
        CoverageStatus::Complete
    }
}

/// One source's structured contribution to a composed response (SIFT-WS0).
///
/// The frozen contract ranks and limits across every selected source. This
/// carries a source's UNTRUNCATED hits plus its own counts so
/// [`compose_and_render`] can apply `limit` exactly once, globally. Extraction
/// must never truncate, and composition must never parse rendered text to
/// recover hit boundaries.
struct SourceHits {
    label: String,
    envelope: Option<crate::domain::SourceResponseEnvelope>,
    hits: Vec<KnowledgeHit>,
    withheld_sensitive: usize,
    filtered: FilteredCounts,
    /// Set when this source could not be searched at all. It still appears in
    /// the per-source list and still degrades overall coverage, so a composed
    /// response never silently omits a source it failed to read.
    readiness: Option<String>,
    degraded: bool,
    derived: SourceDerived,
    /// Kept separately from `envelope` so a withheld source still reports its
    /// generations (which are not sensitive) without echoing guarded identity.
    publication_generation: u64,
    content_generation: u64,
}

/// Derived-state versions and coverage captured from one source at extraction
/// time. Held on [`SourceHits`] so formatting reads nothing but already-captured
/// state — the one-capture rule forbids reloading while rendering.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SourceDerived {
    authority_rule_version: u32,
    policy_version: u32,
    secret_policy_version: u32,
    bridge_coverage: &'static str,
    authority_coverage: &'static str,
}

impl SourceDerived {
    fn capture(generation: &PublishedGeneration) -> Self {
        Self {
            authority_rule_version: generation.authority.versions.authority_rule_version,
            policy_version: generation.authority.versions.policy_version,
            secret_policy_version: generation.authority.versions.secret_policy_version,
            bridge_coverage: derived_coverage_label(&generation.bridge.coverage),
            authority_coverage: derived_coverage_label(&generation.authority.coverage),
        }
    }
}

/// The real label for one source: `current` for the current lane, else
/// `worktree:<id>` / `ref:<name>`. Derived once and reused in the scope line,
/// the per-source list, and every hit, so a hit can never be separated from
/// its provenance (the hit line previously hardcoded `source=current`).
fn source_label(generation: &PublishedGeneration, is_current: bool) -> String {
    if is_current {
        return "current".to_string();
    }
    match generation.source.as_deref().map(|source| &source.location) {
        Some(SourceLocation::WorkingTree { worktree_id }) => format!("worktree:{worktree_id}"),
        Some(SourceLocation::GitRef { name }) => format!("ref:{name}"),
        None => "unbound".to_string(),
    }
}

/// Compose `search_knowledge` across the scope-selected sources of one captured
/// source set (Gate L L-G06). A multi-source scope that selects no sources
/// returns a typed empty readiness result rather than a false complete-absence
/// claim.
///
/// SIFT-WS0: every scope — including `current` — flows through the same
/// extract → rank globally → limit once pipeline. The previous implementation
/// rendered each source through `search_current` (which applies `limit` and
/// computes its own overflow/withheld/filtered counts) and concatenated the
/// resulting strings, so a two-source scope returned `2 x limit` hits and two
/// independent count sets.
pub(crate) fn search_scoped(
    source_set: &PublishedSourceSet,
    input: &SearchKnowledgeInput,
) -> String {
    search_scoped_output(source_set, input).rendered
}

/// The complete safe output plus a block-safe summary for budgeting.
///
/// Mirrors `ReviewKnowledgeOutput` so both knowledge tools budget the same way.
/// `rendered` -- never `budget_rendered` -- is what CCR stores, so a retrieval
/// handle always resolves to the full pre-truncation document.
pub(crate) struct SearchKnowledgeOutput {
    pub(crate) rendered: String,
    pub(crate) budget_rendered: String,
}

pub(crate) fn search_scoped_output(
    source_set: &PublishedSourceSet,
    input: &SearchKnowledgeInput,
) -> SearchKnowledgeOutput {
    let rendered = search_scoped_rendered(source_set, input);
    let budget_rendered = budget_summary(&rendered, input.max_tokens);
    SearchKnowledgeOutput {
        rendered,
        budget_rendered,
    }
}

/// Pack the header plus as many COMPLETE hit blocks as fit (SIFT-WS1, T028).
///
/// `apply_ccr_budget_with_summary` still runs `enforce_token_budget` on
/// whatever summary it is handed, and that helper cuts at a line boundary --
/// so a summary that is merely "block-aware" but over budget gets chopped
/// mid-block anyway. The summary must therefore already FIT. It must also
/// leave room for the retrieval footer, which `apply_ccr_overflow` appends
/// AFTER budgeting: hence `CCR_FOOTER_RESERVE_BYTES`.
fn budget_summary(rendered: &str, max_tokens: Option<u64>) -> String {
    let Some(tokens) =
        crate::protocol::ccr::resolve_tool_max_tokens("search_knowledge", max_tokens)
            .filter(|tokens| *tokens > 0)
    else {
        return rendered.to_string();
    };
    let max_bytes = (tokens as usize).saturating_mul(4);
    if rendered.len() <= max_bytes {
        return rendered.to_string();
    }
    let budget = max_bytes.saturating_sub(crate::protocol::ccr::CCR_FOOTER_RESERVE_BYTES);

    // Everything before the first block is the envelope: it carries the
    // MUST-include provenance and is never split.
    let is_block_start = |line: &str| {
        line.split_once(". ")
            .is_some_and(|(ordinal, _)| ordinal.parse::<usize>().is_ok())
    };
    let lines: Vec<&str> = rendered.lines().collect();
    let first_block = lines
        .iter()
        .position(|line| is_block_start(line))
        .unwrap_or(lines.len());
    let header = lines[..first_block].join("\n");
    if header.len() >= budget {
        // Provenance alone exceeds the budget: emit it and let the handle carry
        // the rest. A partial envelope would be worse than a bounded one.
        return header;
    }

    let mut out = header;
    let mut block: Vec<&str> = Vec::new();
    let flush = |block: &mut Vec<&str>, out: &mut String| -> bool {
        if block.is_empty() {
            return true;
        }
        let candidate = format!("\n{}", block.join("\n"));
        block.clear();
        if out.len() + candidate.len() <= budget {
            out.push_str(&candidate);
            true
        } else {
            false
        }
    };
    for line in &lines[first_block..] {
        if is_block_start(line) && !flush(&mut block, &mut out) {
            return out;
        }
        block.push(line);
    }
    flush(&mut block, &mut out);
    out
}

fn search_scoped_rendered(source_set: &PublishedSourceSet, input: &SearchKnowledgeInput) -> String {
    let query = match validate_input(input) {
        Ok(query) => query,
        Err(error) => return error,
    };
    let scope = input.source_scope.unwrap_or(KnowledgeSourceScope::Current);
    let selected = select_scoped_sources(source_set, scope);
    if selected.is_empty() {
        return format!(
            "Readiness: no_sources_in_scope; source_scope '{}' selected no sources.",
            source_scope_label(scope)
        );
    }

    let current_id = &source_set.current_source_id;
    let sources: Vec<SourceHits> = selected
        .iter()
        .map(|generation| {
            let is_current = generation
                .source
                .as_deref()
                .is_some_and(|source| &source.source_id == current_id);
            extract_source(generation, source_label(generation, is_current), &query)
        })
        .collect();

    compose_and_render(sources, input, &query, scope)
}

/// Extract one source's complete, untruncated contribution.
///
/// Readiness, missing-envelope, and withheld-envelope states become
/// [`SourceHits::readiness`] instead of an early-return `String`, so they
/// survive composition instead of being lost to string concatenation.
fn extract_source(
    generation: &PublishedGeneration,
    label: String,
    query: &NormalizedQuery,
) -> SourceHits {
    let derived = SourceDerived::capture(generation);
    let degraded = response_is_degraded(generation);
    // A source that could not be searched always degrades the composed
    // envelope: its absence must never read as complete coverage.
    let empty = |readiness: Option<String>, envelope, withheld| {
        let unreadable = readiness.is_some();
        SourceHits {
            label: label.clone(),
            envelope,
            hits: Vec::new(),
            withheld_sensitive: withheld,
            filtered: FilteredCounts::default(),
            readiness,
            degraded: degraded || unreadable,
            derived: derived.clone(),
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
        }
    };

    match generation.health.status {
        PublishedIndexStatus::Loading => {
            return empty(
                Some(
                    "index_scouting_or_verifying; retry after the current publication completes"
                        .to_string(),
                ),
                None,
                0,
            );
        }
        PublishedIndexStatus::Empty if generation.manifest.is_none() => {
            return empty(
                Some(
                    "no_valid_source; run index_folder to rebuild from repository source"
                        .to_string(),
                ),
                None,
                0,
            );
        }
        _ => {}
    }

    let Some(envelope) = generation.source_response_envelope() else {
        return empty(
            Some(
                "no_valid_source; source envelope is unavailable and no evidence was served"
                    .to_string(),
            ),
            None,
            0,
        );
    };
    if !source_envelope_is_safe(&envelope) {
        // The source identity itself is sensitive: contribute a withheld count
        // and no envelope, never the guarded values.
        return empty(Some("evidence_withheld".to_string()), None, 1);
    }
    if query.terms.is_empty() {
        return empty(None, Some(envelope), 0);
    }

    let headings = heading_paths(generation);
    let mut deduplicated: BTreeMap<(String, u32, String), KnowledgeHit> = BTreeMap::new();
    let mut withheld_sensitive = 0usize;
    let mut filtered = FilteredCounts::default();

    for (record_index, record) in generation.authority.records.iter().enumerate() {
        if !path_in_scope(&record.unit.path, query.path_prefix.as_deref()) {
            continue;
        }
        let Some(file) = generation.live.files.get(&record.unit.path) else {
            continue;
        };
        let Some(unit_bytes) = bounded_slice(
            &file.content,
            record.unit.byte_range.start,
            record.unit.byte_range.end,
        ) else {
            continue;
        };
        let Ok(unit_text) = std::str::from_utf8(unit_bytes) else {
            continue;
        };
        let heading_path = headings
            .get(&(
                record.unit.path.clone(),
                record.unit.byte_range.start,
                record.unit.byte_range.end,
            ))
            .cloned()
            .unwrap_or_default();
        let Some(matched) = match_unit(
            &file.content,
            record.unit.byte_range.start,
            unit_text,
            &heading_path,
            query,
        ) else {
            continue;
        };

        if !voice_allowed(record.voice, query.authority_scope) {
            note_filtered_voice(&mut filtered, record.voice);
            continue;
        }

        let authority = authority_display(generation, record_index, record);
        let (bridge_previews, bridge_previews_omitted) = bridge_previews(generation, &record.unit);
        let candidate = KnowledgeHit {
            source_label: label.clone(),
            // Assigned by `compose_and_render`, which owns source ordering.
            source_precedence: 0,
            path: record.unit.path.clone(),
            line: matched.line,
            line_range: matched.line_range,
            heading_path,
            excerpt: matched.excerpt,
            content_hash: record.unit.content_hash.clone(),
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
            authority,
            bridge_previews,
            bridge_previews_omitted,
            exact_phrase: matched.exact_phrase,
            heading_match: matched.heading_match,
            distinct_term_count: matched.distinct_term_count,
            unit_start: record.unit.byte_range.start,
            unit_len: record
                .unit
                .byte_range
                .end
                .saturating_sub(record.unit.byte_range.start),
        };

        let heading = candidate.heading_path.join(" > ");
        let bridge = candidate.bridge_previews.join(" | ");
        let finding_ids = candidate.authority.finding_ids.join(",");
        let provenance_ids = candidate.authority.provenance_ids.join(",");
        let visible_fields = [
            candidate.path.as_str(),
            heading.as_str(),
            candidate.excerpt.as_str(),
            candidate.content_hash.as_str(),
            finding_ids.as_str(),
            provenance_ids.as_str(),
            bridge.as_str(),
        ];
        if guard_hit(&candidate, &visible_fields).is_err() {
            withheld_sensitive = withheld_sensitive.saturating_add(1);
            continue;
        }

        let key = (
            candidate.path.clone(),
            candidate.line,
            candidate.excerpt.clone(),
        );
        match deduplicated.get(&key) {
            Some(existing)
                if existing.unit_len < candidate.unit_len
                    || (existing.unit_len == candidate.unit_len
                        && existing.heading_path.len() >= candidate.heading_path.len()) => {}
            _ => {
                deduplicated.insert(key, candidate);
            }
        }
    }

    // Locally ordered only so extraction is deterministic; `limit` is NOT
    // applied here. Composition re-sorts across every selected source and
    // truncates exactly once (SIFT-WS0).
    let mut hits: Vec<KnowledgeHit> = deduplicated.into_values().collect();
    hits.sort_by(rank_hits);

    SourceHits {
        label,
        envelope: Some(envelope),
        hits,
        withheld_sensitive,
        filtered,
        readiness: None,
        degraded,
        derived,
        publication_generation: generation.publication_generation,
        content_generation: generation.content_generation,
    }
}

/// The frozen ranking tuple: exact phrase, heading/title, distinct-term
/// coverage, source precedence, then canonical path/line tie-break.
///
/// SIFT-WS0 lifts this out of extraction so the per-source and global orders
/// cannot disagree. `source_precedence` sits in position 4 exactly as the
/// contract specifies — after match quality, before the path tie-break — so a
/// better match in a lower-precedence source still outranks a weaker match in
/// the current lane (contract test 9: current ranks ahead of a divergent ref
/// but never hides it).
fn rank_hits(left: &KnowledgeHit, right: &KnowledgeHit) -> std::cmp::Ordering {
    right
        .exact_phrase
        .cmp(&left.exact_phrase)
        .then_with(|| right.heading_match.cmp(&left.heading_match))
        .then_with(|| right.distinct_term_count.cmp(&left.distinct_term_count))
        .then_with(|| left.source_precedence.cmp(&right.source_precedence))
        .then_with(|| left.path.cmp(&right.path))
        .then_with(|| left.line.cmp(&right.line))
        .then_with(|| left.unit_start.cmp(&right.unit_start))
}

/// Compose every selected source into ONE response: global rank, ONE `limit`,
/// ONE aggregate count set, worst-source coverage (SIFT-WS0).
fn compose_and_render(
    sources: Vec<SourceHits>,
    input: &SearchKnowledgeInput,
    query: &NormalizedQuery,
    scope: KnowledgeSourceScope,
) -> String {
    // Flatten structurally. Rendered text is never parsed to recover hits.
    let mut hits: Vec<KnowledgeHit> = Vec::new();
    let mut withheld_sensitive = 0usize;
    let mut filtered = FilteredCounts::default();
    for (precedence, source) in sources.iter().enumerate() {
        withheld_sensitive = withheld_sensitive.saturating_add(source.withheld_sensitive);
        filtered.current = filtered.current.saturating_add(source.filtered.current);
        filtered.intent = filtered.intent.saturating_add(source.filtered.intent);
        filtered.history_only = filtered
            .history_only
            .saturating_add(source.filtered.history_only);
        filtered.suppressed = filtered
            .suppressed
            .saturating_add(source.filtered.suppressed);
        filtered.review_required = filtered
            .review_required
            .saturating_add(source.filtered.review_required);
        filtered.unknown = filtered.unknown.saturating_add(source.filtered.unknown);
        hits.extend(source.hits.iter().cloned().map(|mut hit| {
            hit.source_precedence = precedence;
            hit
        }));
    }

    // ONE global sort, then ONE truncation. `overflow` counts everything the
    // limit withheld across all sources, not per source.
    hits.sort_by(rank_hits);
    let overflow = hits.len().saturating_sub(query.limit);
    hits.truncate(query.limit);

    // Worst included source wins: one degraded source degrades the envelope.
    let degraded = sources.iter().any(|source| source.degraded);

    let no_match = if hits.is_empty() {
        Some(if query.terms.is_empty() {
            "query_too_weak"
        } else if withheld_sensitive > 0 {
            "evidence_withheld"
        } else if filtered.current > 0
            || filtered.intent > 0
            || filtered.history_only > 0
            || filtered.suppressed > 0
            || filtered.review_required > 0
            || filtered.unknown > 0
        {
            "evidence_noncurrent"
        } else if degraded {
            "no_evidence_degraded"
        } else {
            "no_evidence_complete"
        })
    } else {
        None
    };

    render_response(
        &sources,
        input,
        query,
        scope,
        &hits,
        no_match,
        overflow,
        withheld_sensitive,
        filtered,
        degraded,
    )
}

/// Single-source convenience over the same pipeline (SIFT-WS0: there is no
/// second composition path that could drift). Production callers reach this
/// through `search_scoped`; it is retained for the single-source unit tests
/// that exercise readiness and withheld-provenance behavior directly.
#[cfg(test)]
pub(crate) fn search_current(
    generation: &PublishedGeneration,
    input: &SearchKnowledgeInput,
) -> String {
    let query = match validate_input(input) {
        Ok(query) => query,
        Err(error) => return error,
    };
    let source = extract_source(generation, "current".to_string(), &query);
    compose_and_render(vec![source], input, &query, KnowledgeSourceScope::Current)
}

fn source_envelope_is_safe(envelope: &crate::domain::SourceResponseEnvelope) -> bool {
    let branch = envelope.source_version.branch.as_deref().unwrap_or("");
    let commit = envelope.source_version.commit.as_deref().unwrap_or("");
    guard_hit(
        envelope,
        &[envelope.source.source_id.as_str(), branch, commit],
    )
    .is_ok()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnitMatch {
    line: u32,
    line_range: Range<u32>,
    excerpt: String,
    exact_phrase: bool,
    heading_match: bool,
    distinct_term_count: usize,
}

fn match_unit(
    file_bytes: &[u8],
    unit_start: u32,
    unit_text: &str,
    heading_path: &[String],
    query: &NormalizedQuery,
) -> Option<UnitMatch> {
    let unit_lower = unit_text.to_lowercase();
    let exact_phrase = unit_lower.contains(&query.phrase);
    let distinct_term_count = query
        .terms
        .iter()
        .filter(|term| unit_lower.contains(term.as_str()))
        .count();
    if !exact_phrase && distinct_term_count == 0 {
        return None;
    }

    let heading_lower = heading_path.join(" ").to_lowercase();
    let heading_match = heading_lower.contains(&query.phrase)
        || query.terms.iter().any(|term| heading_lower.contains(term));
    let base_line = 1u32.saturating_add(
        file_bytes
            .get(..usize::try_from(unit_start).ok()?)?
            .iter()
            .filter(|byte| **byte == b'\n')
            .count() as u32,
    );
    let mut best: Option<(bool, usize, usize, String)> = None;
    for (offset, raw_line) in unit_text.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        let lower = line.to_lowercase();
        let phrase_here = lower.contains(&query.phrase);
        let terms_here = query
            .terms
            .iter()
            .filter(|term| lower.contains(term.as_str()))
            .count();
        if !phrase_here && terms_here == 0 {
            continue;
        }
        let replace = best.as_ref().is_none_or(|current| {
            (phrase_here, terms_here, std::cmp::Reverse(offset))
                > (current.0, current.1, std::cmp::Reverse(current.2))
        });
        if replace {
            best = Some((phrase_here, terms_here, offset, line.to_string()));
        }
    }
    let (_, _, offset, excerpt) = best?;
    let line = base_line.saturating_add(u32::try_from(offset).unwrap_or(u32::MAX));
    let unit_line_count = u32::try_from(unit_text.lines().count().max(1)).unwrap_or(u32::MAX);
    Some(UnitMatch {
        line,
        line_range: base_line..base_line.saturating_add(unit_line_count),
        // SIFT-WS1 (T024): bound the raw matched line to a readable window.
        // Dogfood captured a 1.5 KB Markdown table row as one hit's excerpt.
        excerpt: window_excerpt(&excerpt, &query.phrase, &query.terms),
        exact_phrase,
        heading_match,
        distinct_term_count,
    })
}

fn heading_paths(generation: &PublishedGeneration) -> HashMap<(String, u32, u32), Vec<String>> {
    let mut headings = HashMap::new();
    let Some(source) = generation.source.as_deref() else {
        return headings;
    };
    for (path, file) in &generation.live.files {
        if file.language != LanguageId::Markdown {
            continue;
        }
        for unit in project_markdown_sections(source, path, &file.content_hash, &file.symbols) {
            headings.insert(
                (path.clone(), unit.byte_range.start, unit.byte_range.end),
                unit.heading_path,
            );
        }
    }
    headings
}

fn authority_display(
    generation: &PublishedGeneration,
    record_index: usize,
    record: &KnowledgeAuthorityRecord,
) -> AuthorityDisplay {
    let mut finding_ids: Vec<String> = generation
        .authority
        .finding_index
        .iter()
        .filter(|(_, index)| usize::try_from(**index).ok() == Some(record_index))
        .map(|(id, _)| id.clone())
        .collect();
    finding_ids.sort();
    finding_ids.dedup();
    let finding_ids_omitted = finding_ids.len().saturating_sub(MAX_IDS_PER_HIT);
    finding_ids.truncate(MAX_IDS_PER_HIT);

    let mut provenance_ids = BTreeSet::new();
    provenance_ids.extend(record.code_evidence.consistent_rule_ids.iter().cloned());
    provenance_ids.extend(
        record
            .code_evidence
            .deterministic_conflict_ids
            .iter()
            .cloned(),
    );
    provenance_ids.extend(record.code_evidence.suspected_conflict_ids.iter().cloned());
    provenance_ids.extend(record.code_evidence.implementation_gap_ids.iter().cloned());
    provenance_ids.extend(record.code_evidence.review_signal_ids.iter().cloned());
    let mut provenance_ids: Vec<String> = provenance_ids.into_iter().collect();
    let provenance_ids_omitted = provenance_ids.len().saturating_sub(MAX_IDS_PER_HIT);
    provenance_ids.truncate(MAX_IDS_PER_HIT);

    AuthorityDisplay {
        lifecycle: snake_debug(record.lifecycle),
        authority_domain: snake_debug(record.authority_domain),
        code_evidence: snake_debug(record.code_evidence.display),
        voice: snake_debug(record.voice),
        finding_ids,
        finding_ids_omitted,
        provenance_ids,
        provenance_ids_omitted,
        coverage: derived_coverage_label(&record.code_evidence.coverage).to_string(),
    }
}

fn bridge_previews(
    generation: &PublishedGeneration,
    unit: &KnowledgeAnchor,
) -> (Vec<String>, usize) {
    let mut previews: Vec<String> = generation
        .bridge
        .forward
        .iter()
        .filter(|link| {
            link.evidence.source == unit.source
                && link.evidence.path == unit.path
                && link.evidence.content_hash == unit.content_hash
                && unit.byte_range.start <= link.evidence.byte_range.start
                && link.evidence.byte_range.end <= unit.byte_range.end
        })
        .map(|link| {
            format!(
                "{}:{}:{}",
                link.id.0,
                bridge_evidence_kind_label(&link.evidence_kind),
                bridge_resolution_preview(&link.resolution)
            )
        })
        .collect();
    previews.sort();
    previews.dedup();

    // SIFT-WS1 (T026): pack by resolution class, reserving at least one slot
    // for every class that is PRESENT, then fill the remaining global cap in
    // class order. A flat `truncate` could drop an entire class -- and the
    // frozen contract (§Successful response, test 18) requires bounded
    // exact/declared-set/ambiguous/missing previews "when present". Missing and
    // ambiguous anchors are a trust signal (a document's code links are
    // broken), not noise to hide.
    let class_of = |preview: &str| -> usize {
        if preview.contains(":exact:") {
            0
        } else if preview.contains(":declared_set:") {
            1
        } else if preview.contains(":ambiguous:") {
            2
        } else {
            3
        }
    };
    let total = previews.len();
    let mut by_class: [Vec<String>; 4] = Default::default();
    for preview in previews {
        by_class[class_of(&preview)].push(preview);
    }
    let present = by_class.iter().filter(|class| !class.is_empty()).count();
    let mut selected: Vec<String> = Vec::new();
    if present > 0 {
        // Pass 1: one reserved slot per present class.
        for class in by_class.iter_mut() {
            if !class.is_empty() && selected.len() < MAX_BRIDGE_PREVIEWS_PER_HIT {
                selected.push(class.remove(0));
            }
        }
        // Pass 2: fill remaining capacity in class order.
        for class in by_class.iter_mut() {
            while !class.is_empty() && selected.len() < MAX_BRIDGE_PREVIEWS_PER_HIT {
                selected.push(class.remove(0));
            }
        }
    }
    let omitted = total.saturating_sub(selected.len());
    (selected, omitted)
}

/// One per-source identity line. A source whose envelope was withheld or that
/// could not be read still gets a line — its absence must be visible, never
/// silent — but never echoes guarded identity.
fn render_source_line(
    source: &SourceHits,
    prefix: &str,
    overall_coverage: &str,
    ids: &DisplayIds,
) -> String {
    let Some(envelope) = source.envelope.as_ref() else {
        let state = source.readiness.as_deref().unwrap_or("unavailable");
        return format!(
            "{prefix}: source=withheld source_id=withheld source_version=withheld \
             publication={} content={} freshness=withheld coverage={overall_coverage} \
             manifest_digest=withheld readiness={state}",
            source.publication_generation, source.content_generation,
        );
    };
    let source_version = format!(
        "branch={};commit={};working_tree={}",
        envelope
            .source_version
            .branch
            .as_deref()
            .unwrap_or("not_applicable"),
        envelope
            .source_version
            .commit
            .as_deref()
            .unwrap_or("not_applicable"),
        snake_debug(envelope.source_version.working_tree)
    );
    // SIFT-WS1: bounded IDs here too. This line carried two FULL 64-hex digests
    // (~300 chars) and was the single largest item in the envelope, crowding
    // hits out of small budgets entirely. The frozen contract's own example
    // renders `hash=<bounded-id>`; full values stay resolvable through
    // `review_knowledge`.
    format!(
        "{prefix}: source={} source_id={} source_version={source_version} publication={} \
         content={} freshness={} coverage={} manifest_digest={}",
        source.label,
        ids.render(envelope.source.source_id.as_str()),
        envelope.publication_generation,
        envelope.content_generation,
        freshness_label(&envelope.freshness),
        coverage_label(envelope.coverage),
        ids.render(&envelope.manifest_digest),
    )
}

#[allow(clippy::too_many_arguments)]
fn render_response(
    sources: &[SourceHits],
    input: &SearchKnowledgeInput,
    query: &NormalizedQuery,
    scope: KnowledgeSourceScope,
    hits: &[KnowledgeHit],
    no_match: Option<&str>,
    overflow: usize,
    withheld_sensitive: usize,
    filtered: FilteredCounts,
    degraded: bool,
) -> String {
    let _ = query;
    // SIFT-WS1 (T025): one abbreviation table for the WHOLE response, computed
    // from every digest it will display, so a digest never renders two ways in
    // one answer and colliding prefixes extend together.
    let mut all_ids: Vec<&str> = Vec::new();
    for source in sources {
        if let Some(envelope) = source.envelope.as_ref() {
            all_ids.push(envelope.source.source_id.as_str());
            all_ids.push(envelope.manifest_digest.as_str());
        }
    }
    for hit in hits {
        all_ids.push(hit.content_hash.as_str());
        all_ids.extend(hit.authority.finding_ids.iter().map(String::as_str));
        all_ids.extend(hit.authority.provenance_ids.iter().map(String::as_str));
        all_ids.extend(
            hit.bridge_previews
                .iter()
                .filter_map(|preview| preview.split_once(':').map(|(id, _)| id)),
        );
    }
    let ids = &DisplayIds::for_ids(all_ids);

    let overall_coverage = if degraded { "degraded" } else { "complete" };
    let path_scope = input.path_prefix.as_deref().unwrap_or("repository");
    let scope_label = source_scope_label(scope);
    let multi = sources.len() > 1;
    // The current lane is always first in `select_scoped_sources` order.
    let primary = sources.first();

    // Trust/Derived report the primary lane's captured versions; coverage is
    // the worst included source (contract: overall coverage equals the worst).
    let (publication, content) = primary
        .map(|source| (source.publication_generation, source.content_generation))
        .unwrap_or((0, 0));
    let derived = primary
        .map(|source| source.derived.clone())
        .unwrap_or_default();
    let trust_source = if multi {
        scope_label.to_string()
    } else {
        primary
            .map(|source| {
                if source.envelope.is_some() {
                    source.label.clone()
                } else {
                    "withheld".to_string()
                }
            })
            .unwrap_or_else(|| "unbound".to_string())
    };

    let mut output = String::new();
    if multi || !matches!(scope, KnowledgeSourceScope::Current) {
        // Preserved verbatim: `local_ref_scout` pins this line.
        output.push_str(&format!("Source scope searched: {scope_label}\n"));
    }
    output.push_str(&format!(
        "Trust: exact repository knowledge evidence | publication={publication} | \
         content={content} | source={trust_source} | coverage={overall_coverage}\n\
         Secret policy: version {}\n\
         Scope: {} + {path_scope}\n",
        derived.secret_policy_version,
        if multi { scope_label } else { "current" },
    ));
    for (index, source) in sources.iter().enumerate() {
        let prefix = if multi {
            format!("Source[{}]", index + 1)
        } else {
            "Source".to_string()
        };
        output.push_str(&render_source_line(source, &prefix, overall_coverage, ids));
        output.push('\n');
    }
    output.push_str(&format!(
        "Derived: authority_rule_version={} policy_version={} secret_policy_version={} \
         bridge_coverage={} authority_coverage={} overall_coverage={overall_coverage}\n\
         Counts: overflow={overflow} withheld_sensitive={withheld_sensitive} \
         filtered_current={} filtered_intent={} filtered_history_only={} \
         filtered_suppressed={} filtered_review_required={} filtered_unknown={}",
        derived.authority_rule_version,
        derived.policy_version,
        derived.secret_policy_version,
        derived.bridge_coverage,
        derived.authority_coverage,
        filtered.current,
        filtered.intent,
        filtered.history_only,
        filtered.suppressed,
        filtered.review_required,
        filtered.unknown,
    ));

    if let Some(no_match) = no_match {
        // Exact prefix and position: `classify_search_knowledge_output` keys on
        // "\nNo match:" to emit OutcomeClass::EmptyResult.
        output.push_str(&format!("\nNo match: {no_match}"));
        return output;
    }

    for (index, hit) in hits.iter().enumerate() {
        output.push_str(&render_hit_block(index + 1, hit, ids));
    }
    output
}

/// One indivisible, answer-first hit block (SIFT-WS1, T027).
///
/// Ordered so the answer arrives before the provenance chrome: location,
/// heading breadcrumb, excerpt, then evidence. The previous single
/// pipe-delimited mega-line cost ~250 tokens per hit and buried the excerpt
/// mid-line, so reading hit 3 of 10 meant parsing thousands of tokens of
/// provenance first.
///
/// Field NAMES are unchanged from the mega-line (`content_hash=`, `authority:`,
/// `finding_ids=`, `bridge_previews=`, ...). Only the layout changed: agents
/// and tests already parse these tokens, and renaming them would be churn on a
/// frozen surface for no readability gain.
///
/// Budgeting treats this whole block as atomic -- see `budget_summary`.
fn render_hit_block(ordinal: usize, hit: &KnowledgeHit, ids: &DisplayIds) -> String {
    let heading = if hit.heading_path.is_empty() {
        "(no heading)".to_string()
    } else {
        hit.heading_path.join(" > ")
    };
    let render_list = |values: &[String]| -> String {
        values
            .iter()
            .map(|id| ids.render(id))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "\n{ordinal}. {} · {}:{}\n   {heading}\n   \"{}\"\n   source={} content_hash={} publication={} content={} line_range={}..{}\n   authority: lifecycle={} domain={} code={} voice={} coverage={}\n   finding_ids=[{}] omitted={} provenance_ids=[{}] omitted={}\n   bridge_previews=[{}] omitted={}",
        hit.source_label,
        hit.path,
        hit.line,
        hit.excerpt,
        hit.source_label,
        ids.render(&hit.content_hash),
        hit.publication_generation,
        hit.content_generation,
        hit.line_range.start,
        hit.line_range.end,
        hit.authority.lifecycle,
        hit.authority.authority_domain,
        hit.authority.code_evidence,
        hit.authority.voice,
        hit.authority.coverage,
        render_list(&hit.authority.finding_ids),
        hit.authority.finding_ids_omitted,
        render_list(&hit.authority.provenance_ids),
        hit.authority.provenance_ids_omitted,
        hit.bridge_previews
            .iter()
            .map(|preview| abbreviate_preview_ids(preview, ids))
            .collect::<Vec<_>>()
            .join(" | "),
        hit.bridge_previews_omitted,
    )
}

/// Bridge previews are `<link-id>:<kind>:<resolution>` triples whose leading
/// link ID is a digest. Abbreviate only that ID; the kind and resolution are
/// semantic and stay verbatim.
fn abbreviate_preview_ids(preview: &str, ids: &DisplayIds) -> String {
    match preview.split_once(':') {
        Some((id, rest)) => format!("{}:{rest}", ids.render(id)),
        None => preview.to_string(),
    }
}

fn significant_terms(query: &str) -> Vec<String> {
    let mut terms = BTreeSet::new();
    for token in query
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let token = token.to_lowercase();
        if token.len() < 2 || is_stopword(&token) {
            continue;
        }
        terms.insert(token);
        if terms.len() == 32 {
            break;
        }
    }
    terms.into_iter().collect()
}

fn is_stopword(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "for"
            | "from"
            | "how"
            | "in"
            | "is"
            | "it"
            | "not"
            | "of"
            | "on"
            | "or"
            | "that"
            | "the"
            | "this"
            | "to"
            | "was"
            | "what"
            | "when"
            | "where"
            | "which"
            | "why"
            | "with"
    )
}

fn normalize_path_prefix(input: Option<&str>) -> Result<Option<String>, String> {
    let Some(raw) = input.map(str::trim).filter(|raw| !raw.is_empty()) else {
        return Ok(None);
    };
    let replaced = raw.replace('\\', "/");
    if replaced.starts_with('/')
        || replaced.starts_with("//")
        || replaced.as_bytes().get(1) == Some(&b':')
    {
        return Err("Error: invalid path_prefix; expected a normalized repository-relative path without traversal."
            .to_string());
    }
    let mut components = Vec::new();
    for component in replaced.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                return Err("Error: invalid path_prefix; expected a normalized repository-relative path without traversal."
                    .to_string());
            }
            component => components.push(component),
        }
    }
    Ok((!components.is_empty()).then(|| components.join("/")))
}

fn path_in_scope(path: &str, prefix: Option<&str>) -> bool {
    let Some(prefix) = prefix else {
        return true;
    };
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn voice_allowed(voice: KnowledgeVoice, scope: KnowledgeAuthorityScope) -> bool {
    match scope {
        KnowledgeAuthorityScope::Default => matches!(
            voice,
            KnowledgeVoice::Current
                | KnowledgeVoice::Intent
                | KnowledgeVoice::NeedsReview
                | KnowledgeVoice::Unknown
        ),
        KnowledgeAuthorityScope::Current => matches!(
            voice,
            KnowledgeVoice::Current | KnowledgeVoice::NeedsReview | KnowledgeVoice::Unknown
        ),
        KnowledgeAuthorityScope::Intent => voice == KnowledgeVoice::Intent,
        KnowledgeAuthorityScope::History => {
            matches!(
                voice,
                KnowledgeVoice::HistoryOnly | KnowledgeVoice::Suppressed
            )
        }
        KnowledgeAuthorityScope::All => true,
    }
}

fn note_filtered_voice(counts: &mut FilteredCounts, voice: KnowledgeVoice) {
    match voice {
        KnowledgeVoice::Current => counts.current = counts.current.saturating_add(1),
        KnowledgeVoice::Intent => counts.intent = counts.intent.saturating_add(1),
        KnowledgeVoice::HistoryOnly => counts.history_only = counts.history_only.saturating_add(1),
        KnowledgeVoice::Suppressed => counts.suppressed = counts.suppressed.saturating_add(1),
        KnowledgeVoice::NeedsReview => {
            counts.review_required = counts.review_required.saturating_add(1)
        }
        KnowledgeVoice::Unknown => counts.unknown = counts.unknown.saturating_add(1),
    }
}

fn bounded_slice(bytes: &[u8], start: u32, end: u32) -> Option<&[u8]> {
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?;
    (start <= end && end <= bytes.len()).then(|| &bytes[start..end])
}

fn response_is_degraded(generation: &PublishedGeneration) -> bool {
    generation
        .manifest
        .as_ref()
        .is_some_and(|manifest| manifest.coverage == CoverageStatus::Degraded)
        || !matches!(generation.freshness.as_ref(), FreshnessStatus::Current)
        || !matches!(generation.bridge.coverage, DerivedCoverage::Complete)
        || !matches!(generation.authority.coverage, DerivedCoverage::Complete)
}

pub(crate) fn source_scope_label(scope: KnowledgeSourceScope) -> &'static str {
    match scope {
        KnowledgeSourceScope::Current => "current",
        KnowledgeSourceScope::Worktrees => "worktrees",
        KnowledgeSourceScope::LocalRefs => "local_refs",
        KnowledgeSourceScope::All => "all",
    }
}

fn coverage_label(coverage: CoverageStatus) -> &'static str {
    match coverage {
        CoverageStatus::Complete => "complete",
        CoverageStatus::Degraded => "degraded",
    }
}

fn freshness_label(freshness: &FreshnessStatus) -> &'static str {
    match freshness {
        FreshnessStatus::Current => "current",
        FreshnessStatus::Verifying => "verifying",
        FreshnessStatus::Degraded { .. } => "degraded_last_valid",
    }
}

fn derived_coverage_label(coverage: &DerivedCoverage) -> &'static str {
    match coverage {
        DerivedCoverage::Complete => "complete",
        DerivedCoverage::Truncated { .. } => "truncated",
    }
}

fn bridge_evidence_kind_label(kind: &BridgeEvidenceKind) -> &'static str {
    match kind {
        BridgeEvidenceKind::RepositoryLink => "repository_link",
        BridgeEvidenceKind::ExactPathToken => "exact_path",
        BridgeEvidenceKind::ExactCodeSpanSymbol => "exact_code_span",
        BridgeEvidenceKind::DeclaredOwnershipSelector => "declared_set",
        BridgeEvidenceKind::SupportedStructuredValue { .. } => "structured_value",
    }
}

fn bridge_resolution_preview(resolution: &BridgeResolution) -> String {
    match resolution {
        BridgeResolution::ResolvedExact(anchor) => {
            format!("exact:{}", code_anchor_label(&anchor.id))
        }
        BridgeResolution::ResolvedDeclaredSet { matched_count, .. } => {
            format!("declared_set:{matched_count}")
        }
        BridgeResolution::Ambiguous {
            candidate_count,
            bounded_samples,
        } => format!(
            "ambiguous:{candidate_count}:samples={}",
            bounded_samples.len()
        ),
        BridgeResolution::Missing => "missing".to_string(),
    }
}

fn code_anchor_label(anchor: &CodeAnchorId) -> String {
    // SIFT-WS1 (T005): one shared rendering, defined next to the type. This
    // previously used `{symbol:?}` and leaked Rust debug syntax into a frozen
    // protocol surface.
    anchor.label()
}

fn snake_debug(value: impl std::fmt::Debug) -> String {
    let debug = format!("{value:?}");
    let mut output = String::with_capacity(debug.len() + 4);
    let mut previous_lowercase = false;
    for character in debug.chars() {
        if character.is_ascii_uppercase() {
            if previous_lowercase {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
            previous_lowercase = false;
        } else {
            previous_lowercase = character.is_ascii_lowercase() || character.is_ascii_digit();
            output.push(character);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::live_index::LiveIndex;

    fn input(query: &str) -> SearchKnowledgeInput {
        SearchKnowledgeInput {
            query: query.to_string(),
            path_prefix: None,
            source_scope: Some(KnowledgeSourceScope::Current),
            authority_scope: Some(KnowledgeAuthorityScope::Default),
            project: None,
            projects: None,
            limit: Some(10),
            max_tokens: None,
        }
    }

    fn bracketed_field(output: &str, field: &str) -> String {
        let marker = format!("{field}=[");
        let start = output.find(&marker).expect("field marker") + marker.len();
        let end = output[start..].find(']').expect("field terminator") + start;
        output[start..end].to_string()
    }

    // ── SIFT-WS0 multi-source composition fixtures ──────────────────────────

    /// Count rendered hit blocks. A hit begins at the start of a line with
    /// `<n>. `, which holds for both the pre-slice mega-line format and the
    /// post-slice block format, so the same helper measures before and after.
    fn hit_count(output: &str) -> usize {
        output
            .lines()
            .filter(|line| {
                line.split_once(". ")
                    .is_some_and(|(ordinal, _)| ordinal.parse::<usize>().is_ok())
            })
            .count()
    }

    fn occurrences(output: &str, needle: &str) -> usize {
        output.matches(needle).count()
    }

    /// Build a project whose docs all match `term`, one unit per file.
    fn corpus(files: &[(&str, &str)]) -> (tempfile::TempDir, Arc<PublishedGeneration>) {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("docs")).expect("docs dir");
        for (name, body) in files {
            std::fs::write(dir.path().join("docs").join(name), body).expect("doc fixture");
        }
        let index = LiveIndex::load(dir.path()).expect("load corpus");
        let generation = index.published_source_set().current_generation();
        (dir, generation)
    }

    /// Relabel a captured generation as a distinct non-current source so one
    /// `PublishedSourceSet` can carry several. Only the source identity is
    /// swapped; all captured evidence is shared by `Arc`, so this cannot
    /// fabricate content.
    fn relabel(generation: &PublishedGeneration, location: SourceLocation) -> PublishedGeneration {
        let source_id = crate::domain::SourceId::new(match &location {
            SourceLocation::WorkingTree { worktree_id } => format!("src-{worktree_id}"),
            SourceLocation::GitRef { name } => format!("src-{name}"),
        });
        let identity = crate::domain::SourceIdentity {
            repository_id: generation
                .source
                .as_deref()
                .map(|source| source.repository_id.clone())
                .expect("fixture generation must be bound"),
            source_id,
            location,
        };
        PublishedGeneration {
            source: Some(Arc::new(identity)),
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
            project_generation: generation.project_generation,
            source_version: generation.source_version.clone(),
            freshness: Arc::clone(&generation.freshness),
            manifest: generation.manifest.clone(),
            code_signals: Arc::clone(&generation.code_signals),
            bridge: Arc::clone(&generation.bridge),
            authority: Arc::clone(&generation.authority),
            live: Arc::clone(&generation.live),
            health: Arc::clone(&generation.health),
            outline: Arc::clone(&generation.outline),
        }
    }

    fn source_set(
        current: &Arc<PublishedGeneration>,
        others: Vec<PublishedGeneration>,
    ) -> PublishedSourceSet {
        let current_id = current
            .source
            .as_deref()
            .expect("current must be bound")
            .source_id
            .clone();
        let mut sources: std::collections::BTreeMap<_, _> = std::collections::BTreeMap::new();
        sources.insert(current_id.clone(), Arc::clone(current));
        for other in others {
            let id = other
                .source
                .as_deref()
                .expect("other must be bound")
                .source_id
                .clone();
            sources.insert(id, Arc::new(other));
        }
        PublishedSourceSet {
            registry_generation: 1,
            current_source_id: current_id,
            sources,
        }
    }

    fn scoped_input(query: &str, limit: u32) -> SearchKnowledgeInput {
        SearchKnowledgeInput {
            source_scope: Some(KnowledgeSourceScope::All),
            limit: Some(limit),
            ..input(query)
        }
    }

    // ── SIFT-WS1 excerpt windowing ──────────────────────────────────────────

    /// SIFT-WS1 (T020). Excerpts were the raw matched line with no cap:
    /// dogfood captured a 1.5 KB Markdown table row as one hit's excerpt.
    #[test]
    fn excerpt_is_bounded_and_keeps_the_match_in_window() {
        let row = format!(
            "| {} | persistence boundary | {} |",
            "filler ".repeat(120),
            "tail ".repeat(120)
        );
        let windowed = window_excerpt(&row, "persistence boundary", &[]);
        let chars = windowed.chars().count();
        assert!(
            chars <= EXCERPT_MAX_CHARS + 8,
            "excerpt must be bounded, got {chars} chars"
        );
        assert!(
            windowed.contains("persistence boundary"),
            "the match must survive the window: {windowed}"
        );
        assert!(
            windowed.len() < row.len(),
            "a long row must actually shrink"
        );
    }

    /// SIFT-WS1 (T020). Cuts land on CHARACTER boundaries. Byte offsets taken
    /// from a lowercased copy are the landmine here: `to_lowercase()` is not
    /// length-preserving and a byte index can split a multi-byte character.
    #[test]
    fn excerpt_cuts_on_character_boundaries_for_cjk_emoji_and_combining_marks() {
        for filler in ["日本語テキスト", "🙂🚀🎉", "éécombining", "İIıi"] {
            let line = format!(
                "{} persistence boundary {}",
                filler.repeat(60),
                filler.repeat(60)
            );
            // Must not panic, and must remain valid UTF-8 by construction.
            let windowed = window_excerpt(&line, "persistence boundary", &[]);
            assert!(
                windowed.chars().count() <= EXCERPT_MAX_CHARS + 8,
                "bounded for {filler}: {windowed}"
            );
            assert!(
                windowed.contains("persistence boundary"),
                "match kept for {filler}: {windowed}"
            );
        }
    }

    /// SIFT-WS1 (T020). A short line is returned untouched — no ellipsis, no
    /// churn on the common case.
    #[test]
    fn excerpt_leaves_short_lines_untouched() {
        let line = "Shutdown is not a safe persistence boundary.";
        assert_eq!(window_excerpt(line, "persistence boundary", &[]), line);
    }

    /// SIFT-WS1 (T020). A window that had to cut snaps to whitespace rather
    /// than slicing a word in half.
    #[test]
    fn excerpt_snaps_to_whitespace_when_it_cuts() {
        let line = format!(
            "{}persistence boundary{}",
            "alpha ".repeat(80),
            " omega".repeat(80)
        );
        let windowed = window_excerpt(&line, "persistence boundary", &[]);
        let trimmed = windowed.trim_start_matches('…').trim_end_matches('…');
        assert!(
            !trimmed.starts_with("lpha") && !trimmed.starts_with("pha"),
            "leading cut must snap to a word boundary: {windowed}"
        );
    }

    // ── SIFT-WS1 type-aware ID abbreviation ─────────────────────────────────

    /// SIFT-WS1 (T021). Provenance vectors mix 64-hex digests with SEMANTIC
    /// rule IDs (`authority-history-v1`, `temporal-coverage-incomplete`).
    /// Abbreviating indiscriminately would corrupt the semantic ones.
    #[test]
    fn semantic_ids_render_verbatim_and_only_digests_abbreviate() {
        let digest_a = "a".repeat(64);
        let digest_b = "b".repeat(64);
        let ids = [
            digest_a.clone(),
            digest_b.clone(),
            "authority-history-v1".to_string(),
            "temporal-coverage-incomplete".to_string(),
            "role.path.plan-handoff.v1".to_string(),
        ];
        let display = DisplayIds::for_ids(ids.iter().map(String::as_str));

        assert_eq!(
            display.render("authority-history-v1"),
            "authority-history-v1"
        );
        assert_eq!(
            display.render("temporal-coverage-incomplete"),
            "temporal-coverage-incomplete"
        );
        assert_eq!(
            display.render("role.path.plan-handoff.v1"),
            "role.path.plan-handoff.v1"
        );
        assert_eq!(display.render(&digest_a), "a".repeat(12));
        assert_eq!(display.render(&digest_b), "b".repeat(12));
    }

    /// SIFT-WS1 (T021). A forced 12-hex collision must extend until unique --
    /// a fixed 48-bit prefix is not collision-safe across thousands of units.
    #[test]
    fn forced_digest_prefix_collision_extends_until_unique() {
        let shared = "c".repeat(12);
        let left = format!("{shared}0{}", "d".repeat(51));
        let right = format!("{shared}1{}", "e".repeat(51));
        let display = DisplayIds::for_ids([left.as_str(), right.as_str()]);

        let rendered_left = display.render(&left);
        let rendered_right = display.render(&right);
        assert_ne!(
            rendered_left, rendered_right,
            "colliding digests must not render identically"
        );
        assert!(
            rendered_left.len() > 12,
            "prefix must EXTEND past 12 on collision, got {rendered_left}"
        );
        assert_eq!(
            rendered_left.len(),
            rendered_right.len(),
            "one length for the whole response, so a digest never renders two ways"
        );
        assert!(left.starts_with(&rendered_left));
        assert!(right.starts_with(&rendered_right));
    }

    /// SIFT-WS0 (T007). The frozen contract ranks and limits across the
    /// selected sources; `search_scoped` instead rendered each source through
    /// `search_current` -- which applies `limit` and computes its own
    /// overflow/withheld/filtered counts -- and concatenated the strings. With
    /// two sources and `limit=10` that returns up to 20 hits and two
    /// independent count sets.
    #[test]
    fn global_limit_and_counts_apply_once_across_sources() {
        let body = |n: usize| format!("# Doc {n}\nalpha beta persistence boundary evidence {n}.\n");
        let files: Vec<(String, String)> = (0..8).map(|n| (format!("d{n}.md"), body(n))).collect();
        let refs: Vec<(&str, &str)> = files
            .iter()
            .map(|(name, body)| (name.as_str(), body.as_str()))
            .collect();

        let (_a, current) = corpus(&refs);
        let (_b, other) = corpus(&refs);
        let other = relabel(
            &other,
            SourceLocation::WorkingTree {
                worktree_id: "wt1".to_string(),
            },
        );
        let set = source_set(&current, vec![other]);

        let output = search_scoped(&set, &scoped_input("alpha beta persistence boundary", 10));

        assert_eq!(
            hit_count(&output),
            10,
            "limit must apply ONCE globally, not per source:\n{output}"
        );
        assert_eq!(
            occurrences(&output, "overflow="),
            1,
            "counts must be a single aggregate, not one set per source:\n{output}"
        );
        assert_eq!(
            occurrences(&output, "withheld_sensitive="),
            1,
            "withheld count must be a single aggregate:\n{output}"
        );
    }

    /// SIFT-WS0 (T008). Ranking is global: an exact-phrase hit in a
    /// lower-precedence source must outrank a term-only hit in the current
    /// source. Per-source concatenation puts every current-source hit first
    /// regardless of match quality.
    #[test]
    fn global_ranking_interleaves_sources_by_frozen_tuple() {
        let (_a, current) = corpus(&[("weak.md", "# Weak\nalpha appears here alone.\n")]);
        let (_b, other) = corpus(&[(
            "strong.md",
            "# Strong\nalpha beta gamma exact phrase lives here.\n",
        )]);
        let other = relabel(
            &other,
            SourceLocation::WorkingTree {
                worktree_id: "wt1".to_string(),
            },
        );
        let set = source_set(&current, vec![other]);

        let output = search_scoped(&set, &scoped_input("alpha beta gamma", 10));
        let strong = output.find("strong.md").expect("strong hit present");
        let weak = output.find("weak.md").expect("weak hit present");
        assert!(
            strong < weak,
            "exact-phrase hit in a non-current source must outrank a term-only current hit:\n{output}"
        );
    }

    /// SIFT-WS0 (T009). Each hit carries its own source label. The hit line
    /// previously hardcoded `source=current` for every source in scope.
    #[test]
    fn hits_carry_real_source_labels_not_hardcoded_current() {
        let (_a, current) = corpus(&[("c.md", "# C\nalpha persistence boundary in current.\n")]);
        let (_b, other) = corpus(&[("w.md", "# W\nalpha persistence boundary in worktree.\n")]);
        let other = relabel(
            &other,
            SourceLocation::WorkingTree {
                worktree_id: "wt1".to_string(),
            },
        );
        let (_c, refsrc) = corpus(&[("r.md", "# R\nalpha persistence boundary in ref.\n")]);
        let refsrc = relabel(
            &refsrc,
            SourceLocation::GitRef {
                name: "refs/heads/other".to_string(),
            },
        );
        let set = source_set(&current, vec![other, refsrc]);

        let output = search_scoped(&set, &scoped_input("alpha persistence boundary", 10));

        // Assert on the HIT LINE, not on any section banner: the banner already
        // carried the real label, while the hit itself hardcoded
        // `source=current` for every source in scope.
        let hit_line = |needle: &str| -> String {
            output
                .lines()
                .find(|line| {
                    line.contains(needle)
                        && line
                            .split_once(". ")
                            .is_some_and(|(ordinal, _)| ordinal.parse::<usize>().is_ok())
                })
                .unwrap_or_else(|| panic!("no hit line for {needle}:\n{output}"))
                .to_string()
        };

        let worktree_hit = hit_line("w.md");
        assert!(
            worktree_hit.contains("worktree:wt1"),
            "worktree hit must carry its real label, not `source=current`:\n{worktree_hit}"
        );
        let ref_hit = hit_line("r.md");
        assert!(
            ref_hit.contains("ref:refs/heads/other"),
            "ref hit must carry its real label, not `source=current`:\n{ref_hit}"
        );
    }

    fn with_readiness(
        generation: &PublishedGeneration,
        status: PublishedIndexStatus,
        freshness: FreshnessStatus,
        keep_manifest: bool,
    ) -> PublishedGeneration {
        let mut health = generation.health.as_ref().clone();
        health.status = status;
        PublishedGeneration {
            publication_generation: generation.publication_generation,
            content_generation: generation.content_generation,
            project_generation: generation.project_generation,
            source: generation.source.clone(),
            source_version: generation.source_version.clone(),
            freshness: Arc::new(freshness),
            manifest: keep_manifest.then(|| generation.manifest.clone()).flatten(),
            code_signals: Arc::clone(&generation.code_signals),
            bridge: Arc::clone(&generation.bridge),
            authority: Arc::clone(&generation.authority),
            live: Arc::clone(&generation.live),
            health: Arc::new(health),
            outline: Arc::clone(&generation.outline),
        }
    }

    #[test]
    fn captured_generation_stays_coherent_across_later_publication() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        let document = project.path().join("docs").join("recovery.md");
        std::fs::write(
            &document,
            "# Recovery\nOld persistence boundary remains visible.\n",
        )
        .expect("old document");

        let shared = LiveIndex::load(project.path()).expect("load old generation");
        let captured = shared.published_source_set().current_generation();

        std::fs::write(
            &document,
            "# Recovery\nNew persistence boundary is now visible.\n",
        )
        .expect("new document");
        shared
            .reload(project.path())
            .expect("publish new generation");
        let next = shared.published_source_set().current_generation();
        assert_ne!(
            captured.publication_generation, next.publication_generation,
            "fixture must publish a distinct generation"
        );

        let old_output = search_current(&captured, &input("persistence boundary"));
        assert!(
            old_output.contains("Old persistence boundary"),
            "old: {old_output}"
        );
        assert!(
            !old_output.contains("New persistence boundary"),
            "captured response mixed a later publication: {old_output}"
        );

        let new_output = search_current(&next, &input("persistence boundary"));
        assert!(
            new_output.contains("New persistence boundary"),
            "new: {new_output}"
        );
        assert!(
            !new_output.contains("Old persistence boundary"),
            "next call served the prior publication: {new_output}"
        );
    }

    #[test]
    fn stable_finding_and_provenance_ids_survive_derived_only_republication() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        std::fs::write(
            project.path().join("docs").join("decision.md"),
            "# Decision\nCurrent boundary evidence.\n",
        )
        .expect("document");
        std::fs::write(
            project.path().join(".symforge-knowledge.toml"),
            "version = 1\n[[entries]]\nentry_id = \"stale-entry\"\nlifecycle = \"archived\"\njustification_code = \"stale\"\n[entries.target]\npath = \"docs/decision.md\"\ncontent_hash = \"stale-hash\"\n",
        )
        .expect("policy");

        let shared = LiveIndex::load(project.path()).expect("load generation");
        let before = shared.published_source_set().current_generation();
        let before_output = search_current(&before, &input("boundary evidence"));
        let finding_ids = bracketed_field(&before_output, "finding_ids");
        let provenance_ids = bracketed_field(&before_output, "provenance_ids");
        assert!(
            !finding_ids.is_empty(),
            "fixture must emit a finding: {before_output}"
        );
        assert!(
            !provenance_ids.is_empty(),
            "fixture must emit provenance: {before_output}"
        );

        let prepared = shared.prepare_authority_rebuild();
        assert!(
            shared.publish_prepared_authority(prepared),
            "derived-only publication must be accepted"
        );
        let after = shared.published_source_set().current_generation();
        assert_eq!(before.content_generation, after.content_generation);
        assert_ne!(before.publication_generation, after.publication_generation);

        let after_output = search_current(&after, &input("boundary evidence"));
        assert_eq!(finding_ids, bracketed_field(&after_output, "finding_ids"));
        assert_eq!(
            provenance_ids,
            bracketed_field(&after_output, "provenance_ids")
        );
    }

    #[test]
    fn readiness_and_degraded_last_valid_never_claim_complete_absence() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        std::fs::write(
            project.path().join("docs").join("recovery.md"),
            "# Recovery\nRetained evidence remains available.\n",
        )
        .expect("document");
        let shared = LiveIndex::load(project.path()).expect("load generation");
        let current = shared.published_source_set().current_generation();

        let loading = with_readiness(
            &current,
            PublishedIndexStatus::Loading,
            FreshnessStatus::Verifying,
            true,
        );
        let loading_output = search_current(&loading, &input("missing evidence"));
        assert!(
            loading_output.contains("index_scouting_or_verifying")
                && !loading_output.contains("no_evidence_complete"),
            "loading readiness must not masquerade as complete absence: {loading_output}"
        );

        let no_valid = with_readiness(
            &current,
            PublishedIndexStatus::Empty,
            FreshnessStatus::Verifying,
            false,
        );
        let no_valid_output = search_current(&no_valid, &input("missing evidence"));
        assert!(
            no_valid_output.contains("no_valid_source")
                && !no_valid_output.contains("no_evidence_complete"),
            "an absent valid snapshot must return recovery guidance: {no_valid_output}"
        );

        let verifying = with_readiness(
            &current,
            PublishedIndexStatus::Ready,
            FreshnessStatus::Verifying,
            true,
        );
        let degraded_miss = search_current(&verifying, &input("orbital zebra lattice"));
        assert!(
            degraded_miss.contains("freshness=verifying")
                && degraded_miss.contains("overall_coverage=degraded")
                && degraded_miss.contains("no_evidence_degraded")
                && !degraded_miss.contains("no_evidence_complete"),
            "verifying last-valid no-match must remain explicitly degraded: {degraded_miss}"
        );
        let degraded_hit = search_current(&verifying, &input("retained evidence"));
        assert!(
            degraded_hit.contains("docs/recovery.md:2")
                && degraded_hit.contains("freshness=verifying")
                && degraded_hit.contains("overall_coverage=degraded"),
            "last-valid evidence may be served only with degraded provenance: {degraded_hit}"
        );
    }

    #[test]
    fn sensitive_source_version_is_withheld_before_any_hit_is_formatted() {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("docs")).expect("docs");
        std::fs::write(
            project.path().join("docs").join("recovery.md"),
            "# Recovery\nSafe checkpoint evidence.\n",
        )
        .expect("document");
        let shared = LiveIndex::load(project.path()).expect("load generation");
        let current = shared.published_source_set().current_generation();
        let canary = ["runtime", "-", "canary", "-", "segment"].concat();
        let mut source_version = current
            .source_version
            .as_ref()
            .expect("source version")
            .as_ref()
            .clone();
        source_version.branch = Some(format!("token={canary}"));
        let unsafe_generation = PublishedGeneration {
            publication_generation: current.publication_generation,
            content_generation: current.content_generation,
            project_generation: current.project_generation,
            source: current.source.clone(),
            source_version: Some(Arc::new(source_version)),
            freshness: Arc::clone(&current.freshness),
            manifest: current.manifest.clone(),
            code_signals: Arc::clone(&current.code_signals),
            bridge: Arc::clone(&current.bridge),
            authority: Arc::clone(&current.authority),
            live: Arc::clone(&current.live),
            health: Arc::clone(&current.health),
            outline: Arc::clone(&current.outline),
        };

        let output = search_current(&unsafe_generation, &input("checkpoint evidence"));
        assert!(
            output.contains("evidence_withheld"),
            "unsafe source provenance must fail closed"
        );
        assert!(
            !output.contains(&canary),
            "unsafe source provenance must never be echoed"
        );
        assert!(
            !output.contains("docs/recovery.md"),
            "no hit may be formatted under unsafe source provenance"
        );
    }
}
