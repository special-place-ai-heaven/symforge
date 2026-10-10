//! The `analyze_file_impact` engine, shared by the MCP tool, the sidecar
//! impact hook and the embedded `QueryRequest::FileImpact` lane. Callers own
//! path normalization, how the file is re-admitted from disk, the symbol cache
//! and any stats or footers; the new-file decision, the pre-edit baseline, the
//! receipt handling, the symbol diff and every rendered line live here once.

use std::path::Path;
use std::sync::Arc;

use crate::domain::{SymbolKind, SymbolRecord};
use crate::live_index::PublishedGeneration;
use crate::live_index::single_file::{ReindexOutcome, ReindexReceipt};
use crate::live_index::store::{IndexedFile, PublicationFence, SharedIndex};

/// Lightweight snapshot of a symbol used to detect pre/post-edit changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolSnapshot {
    pub name: String,
    pub kind: String,
    pub line_range: (u32, u32),
    pub byte_range: (u32, u32),
}

/// Why an impact analysis produced no answer. MCP maps these onto the HTTP
/// statuses its sidecar handlers return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImpactFailure {
    /// The file is absent on disk and the index did not move meanwhile.
    NotFound,
    /// The project generation or publication moved under the analysis.
    Unavailable,
}

/// Which token-stat record the caller owes for this answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImpactStats {
    None,
    /// A new file was indexed.
    Write,
    /// An existing file's symbol diff; `file_bytes` is the larger of the pre
    /// and post content lengths, the savings-footer and stats baseline.
    Edit {
        file_bytes: u64,
    },
}

pub(crate) struct ImpactRender {
    pub text: String,
    /// The publication this answer was computed from.
    pub published: Arc<PublishedGeneration>,
    pub stats: ImpactStats,
}

/// The caller's per-generation symbol cache: the last post-edit symbols an
/// impact analysis recorded for a path, used as the baseline only when neither
/// a pre-update snapshot nor an indexed copy exists.
pub(crate) trait ImpactSymbolCache {
    fn get(&mut self, path: &str) -> Result<Option<Vec<SymbolSnapshot>>, ImpactFailure>;
    fn store(&mut self, path: &str, symbols: Vec<SymbolSnapshot>) -> Result<(), ImpactFailure>;
}

/// Re-admit `path` (already a normalized, safe repository path) through
/// `admit` and report its impact against `baseline`.
///
/// **new_file=true (HOOK-06):** Reads file from disk, parses it, indexes it.
/// Returns: language, symbol kind breakdown, `[Indexed, 0 callers yet]`.
///
/// **default (HOOK-05 edit):** Re-indexes the file from disk, computes pre/post symbol diff.
/// Shows Added/Changed/Removed symbols plus callers for Changed+Removed symbols.
#[allow(clippy::too_many_arguments)]
pub(crate) fn analyze_file_impact(
    shared: &SharedIndex,
    root: &Path,
    path: &str,
    new_file: bool,
    expected_generation: u64,
    baseline: Arc<PublishedGeneration>,
    admit: &mut dyn FnMut(&str, &Path) -> ReindexReceipt,
    cache: &mut dyn ImpactSymbolCache,
) -> Result<ImpactRender, ImpactFailure> {
    if shared.current_project_generation() != expected_generation {
        return Err(ImpactFailure::Unavailable);
    }
    if new_file {
        // HOOK-06: Index a new file from disk.
        return new_file_impact(
            shared,
            root,
            path,
            expected_generation,
            baseline,
            admit,
            cache,
        );
    }

    let should_auto_index_new_file = {
        // AAP-002: `from_path` (not `from_extension`) so extensionless narrative
        // entry points and dotfiles (README, .env, .gitignore) auto-index the
        // same way discovery admits them.
        let is_supported = crate::domain::LanguageId::from_path(path).is_some();
        let indexed = {
            let guard = shared.read();
            guard.get_file(path).is_some()
        };
        is_supported && !indexed && root.join(path).is_file()
    };

    if should_auto_index_new_file {
        return new_file_impact(
            shared,
            root,
            path,
            expected_generation,
            baseline,
            admit,
            cache,
        );
    }

    // HOOK-05: Re-index existing file and compute symbol diff.
    edit_impact(
        shared,
        root,
        path,
        expected_generation,
        baseline,
        admit,
        cache,
    )
}

/// The `analyze_file_impact(estimate=true)` answer.
pub(crate) fn estimate_text(include_co_changes: bool, co_changes_limit: usize) -> String {
    let est = 200
        + if include_co_changes {
            co_changes_limit * 13
        } else {
            0
        };
    format!(
        "Estimate for analyze_file_impact: ~{} tokens (include_co_changes={})",
        est, include_co_changes
    )
}

/// Append the `include_co_changes=true` section for `path`.
pub(crate) fn append_co_changes(
    result: &mut String,
    temporal: &crate::live_index::git_temporal::GitTemporalIndex,
    path: &str,
    limit: usize,
) {
    match temporal.state {
        crate::live_index::git_temporal::GitTemporalState::Ready => {
            match temporal.files.get(path) {
                Some(history) => {
                    result.push_str("\n\n");
                    result.push_str(&co_changes_result_view(path, history, limit));
                }
                None => {
                    result.push_str("\n\nNo git co-change data found for this file.");
                }
            }
        }
        crate::live_index::git_temporal::GitTemporalState::Pending
        | crate::live_index::git_temporal::GitTemporalState::Computing => {
            result.push_str("\n\nGit temporal data is still loading. Co-changes unavailable.");
        }
        crate::live_index::git_temporal::GitTemporalState::Unavailable(ref reason) => {
            result.push_str(&format!("\n\nGit temporal data unavailable: {reason}"));
        }
    }
}

/// Format git temporal data for a single file: churn, ownership, co-changes, last commit.
pub fn co_changes_result_view(
    path: &str,
    history: &crate::live_index::git_temporal::GitFileHistory,
    limit: usize,
) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Git temporal data for {path}"));
    lines.push(String::new());

    // Churn
    lines.push(format!(
        "Churn score: {:.2} ({} commits)",
        history.churn_score, history.commit_count
    ));

    // Last commit
    let c = &history.last_commit;
    lines.push(format!(
        "Last commit: {} {} — {} ({})",
        c.hash, c.timestamp, c.message_head, c.author
    ));
    lines.push(String::new());

    // Ownership
    if !history.contributors.is_empty() {
        lines.push("Ownership:".to_string());
        for contrib in &history.contributors {
            lines.push(format!(
                "  {}: {} commits ({:.0}%)",
                contrib.author, contrib.commit_count, contrib.percentage
            ));
        }
        lines.push(String::new());
    }

    // Co-changes
    if history.co_changes.is_empty() {
        lines.push(
            "No high-confidence co-changing files detected (needs at least 2 shared commits and Jaccard >= 0.15)."
                .to_string(),
        );
        if !history.weak_co_changes.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "Low-confidence candidates (top {}):",
                limit.min(history.weak_co_changes.len())
            ));
            for entry in history.weak_co_changes.iter().take(limit) {
                lines.push(format!(
                    "  {:<50} coupling: {:.3}  ({} shared commits)",
                    entry.path, entry.coupling_score, entry.shared_commits
                ));
            }
            lines.push(
                "These missed the strong co-change threshold and are advisory only.".to_string(),
            );
        }
    } else {
        lines.push(format!(
            "Co-changing files (top {}):",
            limit.min(history.co_changes.len())
        ));
        for entry in history.co_changes.iter().take(limit) {
            lines.push(format!(
                "  {:<50} coupling: {:.3}  ({} shared commits)",
                entry.path, entry.coupling_score, entry.shared_commits
            ));
        }
    }

    lines.join("\n")
}

fn impact_skipped_text(published: &PublishedGeneration, path: &str) -> String {
    use crate::domain::index::AdmissionTier;

    let view = published.live.capture_admission_tier_lookup_view(path);
    let Some(view) = view else {
        return format!(
            "Not indexed: {path} is excluded by repository scope. The admission gate applies to \
             analyze_file_impact the same as bulk load and the watcher (no force-admit)."
        );
    };
    let tier_label = match view.tier {
        AdmissionTier::Normal => "Tier 1",
        AdmissionTier::MetadataOnly => "Tier 2 (metadata only)",
        AdmissionTier::HardSkip => "Tier 3 (hard skip)",
    };
    let reason = view
        .reason
        .map(|reason| reason.to_string())
        .unwrap_or_else(|| "policy".to_string());
    let size_mb = view.size.unwrap_or(0) as f64 / (1024.0 * 1024.0);

    // SF-AAP-002 is scoped to genuinely NON-PARSER files (no code parser exists
    // for the type — the artifact/binary case). A parser-supported file demoted
    // for SIZE is NOT this case: it keeps the honest oversize refusal that
    // impact_admission (a frozen behavioral contract) pins. `from_path` is the
    // same parser-support signal impact_text uses for auto-indexing (and that
    // discovery admits by), so the wording stays truthful in both branches.
    // AAP-002: `from_path` (not `from_extension`) recognizes extensionless
    // Text/Env entry points (README, .env, .gitignore) as parser-supported, so a
    // demoted one keeps the oversize refusal; `.bin` still reads as non-parser.
    let has_code_parser = crate::domain::LanguageId::from_path(path).is_some();

    // Key the recovery sentence on the read gate's predicted verdict (spec-023):
    // "Use get_file_content" must never point at a read the gate will refuse.
    let raw_read_advice =
        if super::read_gate::disk_read_would_refuse(&published.live, path, view.size) {
            "Its contents are withheld by the admission policy — get_file_content will refuse \
         this file."
        } else {
            "Use get_file_content for raw reads."
        };

    if has_code_parser {
        return format!(
            "Not indexed: {path} is {tier_label} — reason: {reason}, size {size_mb:.1} MB. \
             The admission gate applies to analyze_file_impact the same as bulk load \
             and the watcher (no force-admit). {raw_read_advice}"
        );
    }

    let generation = published.project_generation;
    // Reconciled non-parser file: EXISTS in the catalog, analysis simply
    // unsupported. Report truthful existence + generation/Tier evidence and a
    // typed unsupported-analysis outcome — never false absence for a tracked file.
    format!(
        "── Impact: {path} ──\n\
         Status: exists (analysis unsupported — {tier_label}, no code parser)\n\
         exists: true\n\
         Tier: {tier_label} — reason: {reason}, size {size_mb:.1} MB\n\
         Generation: {generation}\n\
         The file IS tracked (metadata only), not absent; impact/symbol analysis is \
         unsupported for this file type. {raw_read_advice}"
    )
}

fn impact_receipt_publication(
    receipt: &ReindexReceipt,
    baseline: &Arc<PublishedGeneration>,
) -> Result<Arc<PublishedGeneration>, ImpactFailure> {
    if let Some(published) = &receipt.published {
        return Ok(Arc::clone(published));
    }
    if PublicationFence::from_published(baseline.as_ref()) == receipt.observed_at {
        return Ok(Arc::clone(baseline));
    }
    Err(ImpactFailure::Unavailable)
}

fn symbol_snapshots(file: &IndexedFile) -> Vec<SymbolSnapshot> {
    file.symbols
        .iter()
        .map(|symbol| SymbolSnapshot {
            name: symbol.name.clone(),
            kind: symbol.kind.to_string(),
            line_range: symbol.line_range,
            byte_range: symbol.byte_range,
        })
        .collect()
}

fn new_file_impact(
    shared: &SharedIndex,
    root: &Path,
    path: &str,
    expected_generation: u64,
    baseline: Arc<PublishedGeneration>,
    admit: &mut dyn FnMut(&str, &Path) -> ReindexReceipt,
    cache: &mut dyn ImpactSymbolCache,
) -> Result<ImpactRender, ImpactFailure> {
    if shared.current_project_generation() != expected_generation {
        return Err(ImpactFailure::Unavailable);
    }
    let abs_path = root.join(path);
    let receipt = admit(path, &abs_path);

    if shared.current_project_generation() != expected_generation {
        return Err(ImpactFailure::Unavailable);
    }

    match &receipt.outcome {
        ReindexOutcome::Reindexed | ReindexOutcome::HashSkip => {}
        ReindexOutcome::Skipped => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            return Ok(ImpactRender {
                text: impact_skipped_text(published.as_ref(), path),
                published,
                stats: ImpactStats::None,
            });
        }
        ReindexOutcome::PublicationRejected => {
            return Err(ImpactFailure::Unavailable);
        }
        ReindexOutcome::NotFound | ReindexOutcome::Removed => {
            if shared.publication_fence() != receipt.observed_at {
                return Err(ImpactFailure::Unavailable);
            }
            return Err(ImpactFailure::NotFound);
        }
        ReindexOutcome::ReadError(_) => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            return Ok(ImpactRender {
                text: format!(
                    "Not indexed: {path} is temporarily unreadable; last-valid state was retained."
                ),
                published,
                stats: ImpactStats::None,
            });
        }
    }

    // Build symbol kind breakdown.
    let mut kind_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let published = receipt
        .published
        .as_ref()
        .ok_or(ImpactFailure::Unavailable)?;
    if published.project_generation != expected_generation {
        return Err(ImpactFailure::Unavailable);
    }
    let (language, post_symbols) = {
        let file = published
            .live
            .get_file(path)
            .ok_or(ImpactFailure::NotFound)?;
        for symbol in &file.symbols {
            *kind_counts.entry(symbol.kind.to_string()).or_insert(0) += 1;
        }
        (file.language, symbol_snapshots(file))
    };

    let mut kind_parts: Vec<String> = kind_counts
        .iter()
        .map(|(k, v)| format!("{} {}", v, k))
        .collect();
    kind_parts.sort();
    let kinds_str = if kind_parts.is_empty() {
        "0 symbols".to_string()
    } else {
        kind_parts.join(", ")
    };

    if receipt.snapshot_created {
        let replacement = PublicationFence::from_published(published);
        let _ = shared.take_pre_update_snapshot_for_publication_at_generation(
            path,
            expected_generation,
            replacement,
        );
    }
    // The next edit must diff against the newly indexed file, not an empty
    // baseline that would report every existing symbol as added.
    cache.store(path, post_symbols)?;

    let text = format!(
        "Language: {:?}\nSymbols: {}\n[Indexed, 0 callers yet]",
        language, kinds_str,
    );

    Ok(ImpactRender {
        text,
        published: Arc::clone(published),
        stats: ImpactStats::Write,
    })
}

/// Locate the SymbolRecord in an indexed file that corresponds to a
/// pre-recorded SymbolSnapshot.
///
/// Used by analyze_file_impact so it can walk the symbol's parent impl
/// block and type-scope the "Callers to review" list. Matches on the
/// triple (name, kind, byte_range) — overloaded names are common, so
/// name alone is insufficient.
pub(crate) fn find_record_matching_snapshot<'a>(
    file: &'a IndexedFile,
    sym: &SymbolSnapshot,
) -> Option<&'a SymbolRecord> {
    file.symbols.iter().find(|s| {
        s.name == sym.name && s.kind.to_string() == sym.kind && s.byte_range == sym.byte_range
    })
}

fn slice_byte_range(bytes: &[u8], range: (u32, u32)) -> Option<&[u8]> {
    let start = range.0 as usize;
    let end = range.1 as usize;
    (start < end && end <= bytes.len()).then(|| &bytes[start..end])
}

/// True when the matched symbol's core body text changed, not merely shifted byte
/// offsets after a prefix insertion (e.g. top-of-file comment edits).
pub(crate) fn symbol_body_bytes_changed(
    pre_bytes: &[u8],
    post_bytes: &[u8],
    pre: &SymbolSnapshot,
    post: &SymbolSnapshot,
) -> bool {
    match (
        slice_byte_range(pre_bytes, pre.byte_range),
        slice_byte_range(post_bytes, post.byte_range),
    ) {
        (Some(pre_slice), Some(post_slice)) => pre_slice != post_slice,
        _ => pre.line_range != post.line_range || pre.byte_range != post.byte_range,
    }
}

fn edit_impact(
    shared: &SharedIndex,
    root: &Path,
    path: &str,
    expected_generation: u64,
    baseline: Arc<PublishedGeneration>,
    admit: &mut dyn FnMut(&str, &Path) -> ReindexReceipt,
    cache: &mut dyn ImpactSymbolCache,
) -> Result<ImpactRender, ImpactFailure> {
    if shared.current_project_generation() != expected_generation {
        return Err(ImpactFailure::Unavailable);
    }
    // Get pre-edit symbols and bytes from the exact index-owned baseline first.
    // The public symbols-only cache is a last-resort compatibility fallback: it
    // cannot prove symbol-body identity, so it must never shadow available
    // content from a pre-update snapshot or the current indexed file.
    //
    // The index pre-update snapshot (`take_pre_update_snapshot`) fixes a race
    // where the watcher re-indexes the file before this hook fires, causing the
    // current index to already contain post-edit symbols/content while the hook
    // still needs the pre-edit baseline for an accurate diff.
    let pre_update = shared.peek_pre_update_snapshot_at_generation(path, expected_generation);
    if shared.current_project_generation() != expected_generation {
        return Err(ImpactFailure::Unavailable);
    }
    let pre_snapshot_replacement = pre_update.as_ref().map(|(_, replacement)| *replacement);
    let (pre_symbols, pre_content): (Vec<SymbolSnapshot>, Option<Vec<u8>>) = {
        if let Some((pre, _)) = pre_update {
            let symbols = pre
                .symbols
                .into_iter()
                .map(|s| SymbolSnapshot {
                    name: s.name,
                    kind: s.kind,
                    line_range: s.line_range,
                    byte_range: s.byte_range,
                })
                .collect();
            (symbols, Some(pre.content))
        } else if let Some(file) = shared.read().get_file(path).cloned() {
            (symbol_snapshots(&file), Some(file.content))
        } else if let Some(cached) = cache.get(path)? {
            (cached, None)
        } else {
            (Vec::new(), None)
        }
    };

    // File byte_len before re-indexing (content baseline comes from `pre_content` above).
    let file_bytes_pre: u64 = pre_content.as_ref().map_or(0, |b| b.len() as u64);

    let abs_path = root.join(path);
    let receipt = admit(path, &abs_path);

    if shared.current_project_generation() != expected_generation {
        return Err(ImpactFailure::Unavailable);
    }

    match &receipt.outcome {
        ReindexOutcome::Reindexed | ReindexOutcome::HashSkip => {}
        ReindexOutcome::Skipped => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            return Ok(ImpactRender {
                text: impact_skipped_text(published.as_ref(), path),
                published,
                stats: ImpactStats::None,
            });
        }
        ReindexOutcome::PublicationRejected => {
            return Err(ImpactFailure::Unavailable);
        }
        ReindexOutcome::ReadError(_) => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            return Ok(ImpactRender {
                text: format!(
                    "── Impact: {path} ──\nStatus: temporarily unreadable — last-valid state retained"
                ),
                published,
                stats: ImpactStats::None,
            });
        }
        ReindexOutcome::NotFound | ReindexOutcome::Removed => {
            let published = impact_receipt_publication(&receipt, &baseline)?;
            // One latency-bounded observation cannot distinguish a durable
            // deletion from delete→recreate disk ABA. Retain last-valid state;
            // the watcher retry/reconciliation path owns confirmed removal.
            let prev_symbol_count = pre_symbols.len();
            let root_display = root.display().to_string();
            let has_index_record = published.live.get_file(path).is_some();
            let (status, detail) = if has_index_record {
                (
                    "last-valid index state retained pending watcher confirmation",
                    format!("Previously indexed symbols: {prev_symbol_count}."),
                )
            } else {
                (
                    "no index record remains; watcher confirmation pending",
                    "No prior symbol count was observed.".to_string(),
                )
            };
            return Ok(ImpactRender {
                text: format!(
                    "── Impact: {path} ──\nStatus: not found under {root_display} — {status}\n{detail}"
                ),
                published,
                stats: ImpactStats::None,
            });
        }
    }

    // Use the immutable generation returned by this request's winning
    // publication seam. Sampling current state here could accidentally adopt a
    // later watcher update and consume that update's snapshot.
    let post_generation = receipt
        .published
        .as_ref()
        .ok_or(ImpactFailure::Unavailable)?;
    if post_generation.project_generation != expected_generation
        || shared.current_project_generation() != expected_generation
    {
        return Err(ImpactFailure::Unavailable);
    }
    let (post_symbols, post_content) = {
        let file = post_generation
            .live
            .get_file(path)
            .ok_or(ImpactFailure::NotFound)?;
        (symbol_snapshots(file), file.content.clone())
    };
    if receipt.snapshot_created {
        let replacement = PublicationFence::from_published(post_generation);
        let _ = shared.take_pre_update_snapshot_for_publication_at_generation(
            path,
            expected_generation,
            replacement,
        );
    } else if matches!(receipt.outcome, ReindexOutcome::HashSkip)
        && let Some(replacement) = pre_snapshot_replacement
    {
        let _ = shared.take_pre_update_snapshot_for_publication_at_generation(
            path,
            expected_generation,
            replacement,
        );
    }
    let file_bytes: u64 = (post_content.len() as u64).max(file_bytes_pre);

    // Compute symbol diff using positional proximity for duplicate name+kind pairs.
    let mut matched_pre = vec![false; pre_symbols.len()];
    let mut matched_post = vec![false; post_symbols.len()];
    let mut changed_post: Vec<usize> = Vec::new();

    for (pi, ps) in post_symbols.iter().enumerate() {
        // Find the closest unmatched pre-symbol with the same name+kind.
        let best = pre_symbols
            .iter()
            .enumerate()
            .filter(|(i, pr)| !matched_pre[*i] && pr.name == ps.name && pr.kind == ps.kind)
            .min_by_key(|(_, pr)| (pr.line_range.0 as i64 - ps.line_range.0 as i64).unsigned_abs());
        if let Some((pri, pr)) = best {
            matched_pre[pri] = true;
            matched_post[pi] = true;
            let body_changed = match pre_content.as_deref() {
                Some(pre_bytes) => symbol_body_bytes_changed(pre_bytes, &post_content, pr, ps),
                None => true,
            };
            if body_changed {
                changed_post.push(pi);
            }
        }
    }

    let added: Vec<&SymbolSnapshot> = post_symbols
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched_post[*i])
        .map(|(_, s)| s)
        .collect();

    let removed: Vec<&SymbolSnapshot> = pre_symbols
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched_pre[*i])
        .map(|(_, s)| s)
        .collect();

    let changed: Vec<&SymbolSnapshot> = changed_post.iter().map(|&i| &post_symbols[i]).collect();

    // Update cache with post-edit snapshot.
    cache.store(path, post_symbols.clone())?;

    // Build response lines.
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("── Impact: {} ──", path));

    if added.is_empty() && changed.is_empty() && removed.is_empty() {
        lines.push(format!(
            "Status: indexed and unchanged\nSymbols: {}\nTip: Use what_changed to see recent modifications.",
            post_symbols.len()
        ));
    } else {
        lines.push("Status: changed on disk since last index".to_string());
        for sym in &added {
            lines.push(format!("  [Added]   {} {}", sym.kind, sym.name));
        }
        for sym in &changed {
            lines.push(format!("  [Changed] {} {}", sym.kind, sym.name));
        }
        for sym in &removed {
            lines.push(format!("  [Removed] {} {}", sym.kind, sym.name));
        }

        // Show callers for Changed + Removed symbols.
        //
        // For CHANGED symbols that live inside an `impl` block, scope the
        // caller list to files that also reference the parent type —
        // prevents `MathMachine::new` from flagging every unrelated `new()`
        // call. Mirrors the filter in protocol::edit::detect_stale_references.
        //
        // REMOVED symbols cannot be type-scoped here: the post-edit file no
        // longer contains the SymbolRecord, so `find_record_matching_snapshot`
        // returns None and the filter short-circuits to name-only matching.
        // Acceptable trade-off: removing a same-named method from one of many
        // types is rare, and carrying parent_type through SymbolSnapshot would
        // widen the schema for a corner case. Revisit if the false positive
        // surfaces in real usage.
        let impacted: Vec<&SymbolSnapshot> =
            changed.iter().chain(removed.iter()).copied().collect();
        if !impacted.is_empty() {
            let guard = post_generation.live.as_ref();
            let post_file = guard.get_file(path);
            let mut callers_lines: Vec<String> = Vec::new();
            for sym in &impacted {
                // Derive the parent impl/class type for this symbol, if any.
                // Look the symbol up in the POST-edit file by name+byte_range so
                // overloaded names do not confuse the walker.
                let parent_type: Option<String> = post_file.as_ref().and_then(|file| {
                    find_record_matching_snapshot(file, sym)
                        .and_then(|record| find_parent_impl_type(file, record))
                });

                // When we know the parent type, collect the set of files that
                // reference it. Only those files could plausibly call
                // `ParentType::method_name()`.
                let type_files: Option<std::collections::HashSet<String>> =
                    parent_type.as_ref().map(|tn| {
                        guard
                            .find_references_for_name(tn, None, false)
                            .into_iter()
                            .map(|(fp, _)| fp.to_string())
                            .collect()
                    });

                let callers = guard.find_references_for_name(&sym.name, None, false);
                let external: Vec<_> = callers
                    .iter()
                    .filter(|(fp, _)| *fp != path)
                    .filter(|(fp, _)| match &type_files {
                        Some(tf) => tf.contains(*fp),
                        None => true,
                    })
                    .take(5)
                    .collect();
                if !external.is_empty() {
                    callers_lines.push(format!("  Callers of {}():", sym.name));
                    for (caller_file, r) in &external {
                        callers_lines.push(format!(
                            "    {}  line {}",
                            caller_file,
                            r.line_range.0 + 1
                        ));
                    }
                }
            }
            if !callers_lines.is_empty() {
                lines.push(String::new());
                lines.push("Callers to review:".to_string());
                lines.extend(callers_lines);
            }
        }
    }

    // Apply budget (150 tokens = 600 bytes).
    let (text, _) = super::read_context::build_with_budget(&lines, 600);

    Ok(ImpactRender {
        text,
        published: Arc::clone(post_generation),
        stats: ImpactStats::Edit { file_bytes },
    })
}

/// Find the parent impl block's type name for a symbol, if any.
///
/// Walks backward through the file's symbol list to find an `impl` block at a
/// lower depth that encloses the target symbol's byte range. Extracts the
/// concrete type name (e.g. `Foo` from `impl Foo` or `impl Trait for Foo`).
pub(crate) fn find_parent_impl_type(file: &IndexedFile, sym: &SymbolRecord) -> Option<String> {
    if sym.depth == 0 {
        return None; // top-level symbol, not inside an impl block
    }
    // Walk the symbol list to find the enclosing impl block.
    for s in &file.symbols {
        if s.kind != SymbolKind::Impl {
            continue;
        }
        // The impl block must enclose the target symbol.
        if s.byte_range.0 <= sym.byte_range.0 && s.byte_range.1 >= sym.byte_range.1 {
            return extract_impl_type_name(&s.name);
        }
    }
    None
}

/// Extract the concrete type name from an impl block name.
///
/// Handles patterns like:
/// - `impl Foo` -> `Foo`
/// - `impl Trait for Foo` -> `Foo`
/// - `impl<T> Foo<T>` -> `Foo`
/// - `impl<T: Clone> Trait for Foo<T>` -> `Foo`
pub(crate) fn extract_impl_type_name(impl_name: &str) -> Option<String> {
    let name = impl_name.trim();
    // Strip leading "impl" keyword if present (some parsers include it).
    let rest = name.strip_prefix("impl").unwrap_or(name).trim_start();
    // Strip generic parameters from the front: `<T: Clone> Trait for Foo<T>` -> `Trait for Foo<T>`
    let rest = strip_leading_generics(rest);
    // Check for "for" keyword: `Trait for Foo<T>` -> `Foo<T>`
    let type_part = if let Some(pos) = rest.find(" for ") {
        rest[pos + 5..].trim_start()
    } else {
        rest.trim_start()
    };
    // Strip trailing generics: `Foo<T>` -> `Foo`
    let type_name = type_part.split('<').next().unwrap_or(type_part).trim();
    if type_name.is_empty() {
        None
    } else {
        Some(type_name.to_string())
    }
}

/// Strip a leading `<...>` generic parameter list, handling nested angle brackets.
fn strip_leading_generics(s: &str) -> &str {
    let s = s.trim_start();
    if !s.starts_with('<') {
        return s;
    }
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return s[i + 1..].trim_start();
                }
            }
            _ => {}
        }
    }
    s // malformed generics, return as-is
}
