//! Shared reference-aware rename planner for MCP and Embed. The caller supplies
//! a resolved definition from its own admitted source, then this planner applies
//! the same ambiguity, qualifier and overlap rules before any write.

use std::collections::HashMap;

use crate::domain::index::{LanguageId, SymbolRecord};
use crate::live_index::store::IndexedFile;
use crate::live_index::{LiveIndex, qualified_usages};

pub(crate) struct RenamePlanInput {
    pub path: String,
    pub name: String,
    pub code_only: bool,
}

pub(crate) struct RenamePlan {
    pub by_file: HashMap<String, Vec<(u32, u32)>>,
    pub uncertain_lines: Vec<String>,
    pub language: LanguageId,
}

pub(crate) fn validate_rename_ranges(
    ranges: &mut Vec<(u32, u32)>,
    original: &[u8],
    old_name: &str,
    file_path: &str,
) -> Result<(), String> {
    let old_bytes = old_name.as_bytes();

    // Sort descending by (start, end) — current code only sorts by start
    ranges.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    ranges.dedup();

    // Validate bounds and text match; remove ranges that don't match (xref may
    // produce wider ranges for qualified paths like crate::Widget).
    ranges.retain(|&(start, end)| {
        if start >= end || end as usize > original.len() {
            return false;
        }
        let actual = &original[start as usize..end as usize];
        actual == old_bytes
    });

    // Check overlaps: ranges sorted descending, so prev.start >= curr.start
    for window in ranges.windows(2) {
        let prev = window[0]; // higher offset
        let curr = window[1]; // lower offset
        if curr.1 > prev.0 {
            return Err(format!(
                "{file_path}: overlapping ranges ({}, {}) and ({}, {})",
                curr.0, curr.1, prev.0, prev.1
            ));
        }
    }

    Ok(())
}

pub(crate) fn build_rename_plan(
    live: &LiveIndex,
    file: &IndexedFile,
    sym: &SymbolRecord,
    input: &RenamePlanInput,
) -> Result<RenamePlan, String> {
    if input.name.is_empty() {
        return Err("Rename target name is empty".to_string());
    }
    let body = file
        .content
        .get(sym.byte_range.0 as usize..sym.byte_range.1 as usize)
        .ok_or_else(|| "Rename target span is stale".to_string())?;
    let name_offset = body
        .windows(input.name.len())
        .position(|window| window == input.name.as_bytes())
        .ok_or_else(|| {
            format!(
                "Could not locate name `{}` within symbol body at {}:{}-{}",
                input.name, input.path, sym.byte_range.0, sym.byte_range.1
            )
        })?;
    let abs_start = sym.byte_range.0 + name_offset as u32;
    let def_name_range = (abs_start, abs_start + input.name.len() as u32);
    let language = file.language;
    let target_owner = crate::live_index::enclosing_impl_owner(&file.symbols, sym.line_range.0);

    // Phase 2: Find all references across the project. Carry each ref's
    // `qualified_name` (019 recall-recovery): the immediate qualifier lets the
    // ambiguity gate recover `Target::new()` call sites whose qualifier matches
    // the resolved target's owner.
    let ref_sites: Vec<(String, (u32, u32), Option<String>)> = {
        let refs = live.find_references_for_name(&input.name, None, false);
        refs.into_iter()
            .map(|(path, rr)| (path.to_string(), rr.byte_range, rr.qualified_name.clone()))
            .collect()
    };

    // Filter ref_sites by code_only
    let mut ref_sites: Vec<(String, (u32, u32), Option<String>)> = if input.code_only {
        ref_sites
            .into_iter()
            .filter(|(path, _, _)| {
                let ext = path.rsplit('.').next().unwrap_or("");
                match crate::domain::index::LanguageId::from_extension(ext) {
                    None => false,
                    Some(lang) => !crate::parsing::config_extractors::is_config_language(&lang),
                }
            })
            .collect()
    } else {
        ref_sites
    };

    // Phase 2b: Supplemental qualified-path scan with confidence classification.
    // The xref index tracks call targets (e.g. "new" in Widget::new()), not
    // path prefixes. find_qualified_usages catches Type::method() patterns,
    // import paths, and any other qualified usage the xref system doesn't index.
    // Matches are split into confident (code context) and uncertain (comments/strings).
    //
    // We collect file content snapshots under the lock, then run the scan outside it.
    let file_contents: Vec<(String, Vec<u8>)> = {
        live.files
            .iter()
            .filter(|(path, _)| {
                if !input.code_only {
                    return true;
                }
                let ext = path.rsplit('.').next().unwrap_or("");
                match crate::domain::index::LanguageId::from_extension(ext) {
                    None => false,
                    Some(lang) => !crate::parsing::config_extractors::is_config_language(&lang),
                }
            })
            .map(|(path, file)| (path.clone(), file.content.clone()))
            .collect()
    };

    // Collect confident and uncertain supplemental matches separately.
    // Each entry: (file_path, byte_range (start, end))
    let mut qualified_confident: Vec<(String, (u32, u32))> = Vec::new();
    // Uncertain entries also carry the display context string for the warning block.
    let mut qualified_uncertain: Vec<(String, u32, String)> = Vec::new(); // (path, line, context)

    let qualified_inputs =
        file_contents
            .iter()
            .map(|(path, content)| qualified_usages::QualifiedFileContent {
                file_path: path.as_str(),
                content: content.as_slice(),
            });
    for usage in qualified_usages::collect_qualified_usages(&input.name, qualified_inputs) {
        if usage.confident {
            qualified_confident.push((usage.file_path, usage.byte_range));
        } else {
            qualified_uncertain.push((usage.file_path, usage.line, usage.context));
        }
    }

    // Phase 2c: Ambiguity gate (P0 safety) with owner-name recall recovery.
    // Count how many DEFINITIONS the index holds for `input.name`. The bare-name
    // reverse-index refs (`ref_sites`) and the unscoped qualified matches
    // (`qualified_confident`) both key on the leaf name only, so for a name with
    // 2+ definitions they cannot be attributed to the resolved target definition
    // by the leaf alone (e.g. renaming `Target::new` must not rewrite
    // `SomeOther::new`).
    //
    // 019 recall-recovery: a qualified ref whose IMMEDIATE QUALIFIER equals the
    // resolved target's `impl` OWNER (`Target::new()` when renaming Target's
    // `new`) IS attributable and stays writable — BUT ONLY when that owner name
    // is UNIQUE among the ambiguous defs' owners. If two unrelated `impl Target`
    // exist, the qualifier can't disambiguate, so we fall back to demoting. Bare
    // (unqualified) refs and refs whose qualifier != owner still demote.
    let def_count = {
        live.files
            .values()
            .flat_map(|file| file.symbols.iter())
            .filter(|sym| sym.name == input.name)
            .count()
    };
    if def_count >= 2 {
        // Owner-uniqueness guard: count how many defs of `input.name` share the
        // resolved target's owner name. Recovery is sound only when EXACTLY ONE
        // does (mirrors resolve_ambiguous_callee's "matched >1 -> drop"). A `None`
        // target owner (free fn / non-Rust container) never recovers.
        let owner_is_unique = if let Some(owner) = target_owner.as_deref() {
            let same_owner_defs = live
                .files
                .values()
                .flat_map(|file| {
                    file.symbols.iter().filter_map(move |sym| {
                        if sym.name != input.name {
                            return None;
                        }
                        crate::live_index::enclosing_impl_owner(&file.symbols, sym.line_range.0)
                    })
                })
                .filter(|o| o == owner)
                .count();
            same_owner_defs == 1
        } else {
            false
        };

        // Immediate qualifier of a `qualified_name` string: the segment right
        // before the leaf (`Target::new` -> `Target`; `a::b::Foo::new` -> `Foo`).
        let immediate_qualifier = |qn: &str| -> Option<String> {
            let segs: Vec<&str> = qn
                .split(['.', ':'])
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            match segs.len() {
                0 | 1 => None,
                n => Some(segs[n - 2].to_string()),
            }
        };
        // Immediate qualifier for a byte-scanned match: the identifier ending at
        // the `::` immediately before the leaf at `leaf_start`.
        let qualifier_before = |content: &[u8], leaf_start: u32| -> Option<String> {
            let leaf_start = leaf_start as usize;
            if leaf_start < 2
                || leaf_start > content.len()
                || content[leaf_start - 2] != b':'
                || content[leaf_start - 1] != b':'
            {
                return None;
            }
            let end = leaf_start - 2;
            let mut start = end;
            while start > 0 {
                let b = content[start - 1];
                if b == b'_' || b.is_ascii_alphanumeric() {
                    start -= 1;
                } else {
                    break;
                }
            }
            if start == end {
                return None;
            }
            // Guard the byte slice against multi-byte UTF-8 splits before decode.
            while start < end && (content[start] & 0b1100_0000) == 0b1000_0000 {
                start += 1;
            }
            std::str::from_utf8(&content[start..end])
                .ok()
                .map(|s| s.to_string())
        };

        // Keep-writable predicate: recovery only when the owner is unique AND the
        // ref's immediate qualifier equals that owner.
        let recovers = |qualifier: Option<&str>| -> bool {
            owner_is_unique
                && match (qualifier, target_owner.as_deref()) {
                    (Some(q), Some(owner)) => q == owner,
                    _ => false,
                }
        };

        // Convert each demoted (path, byte_range) site into an uncertain
        // (path, line, context) tuple so it flows through the existing
        // uncertain-warning block instead of the confident write set.
        let demote = |path: &str, start: u32, sink: &mut Vec<(String, u32, String)>| {
            let content = file_contents
                .iter()
                .find(|(p, _)| p == path)
                .map(|(_, c)| c.as_slice())
                .unwrap_or(&[]);
            let text = String::from_utf8_lossy(content);
            let start = (start as usize).min(text.len());
            let line = text[..start].bytes().filter(|&b| b == b'\n').count() + 1;
            let context = text.lines().nth(line - 1).unwrap_or("").trim().to_string();
            sink.push((path.to_string(), line as u32, context));
        };

        // Partition ref_sites: keep owner-recovered, demote the rest.
        let mut kept_refs: Vec<(String, (u32, u32), Option<String>)> = Vec::new();
        for (path, range, qn) in ref_sites.drain(..) {
            let qualifier = qn.as_deref().and_then(immediate_qualifier);
            if recovers(qualifier.as_deref()) {
                kept_refs.push((path, range, qn));
            } else {
                demote(&path, range.0, &mut qualified_uncertain);
            }
        }
        ref_sites = kept_refs;

        // Partition qualified_confident the same way, parsing the qualifier out
        // of the scanned file content (byte-scan matches carry no qualified_name).
        let mut kept_qual: Vec<(String, (u32, u32))> = Vec::new();
        for (path, range) in qualified_confident.drain(..) {
            let content = file_contents
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, c)| c.as_slice())
                .unwrap_or(&[]);
            let qualifier = qualifier_before(content, range.0);
            if recovers(qualifier.as_deref()) {
                kept_qual.push((path, range));
            } else {
                demote(&path, range.0, &mut qualified_uncertain);
            }
        }
        qualified_confident = kept_qual;
    }

    // Phase 3: Group rename sites by file.
    // Confident sources: definition site, indexed refs, qualified confident matches.
    // Uncertain matches are NOT applied — only surfaced in output.
    let mut by_file: std::collections::HashMap<String, Vec<(u32, u32)>> =
        std::collections::HashMap::new();
    by_file
        .entry(input.path.clone())
        .or_default()
        .push(def_name_range);
    for (path, range, _qn) in &ref_sites {
        by_file.entry(path.clone()).or_default().push(*range);
    }
    for (path, range) in &qualified_confident {
        by_file.entry(path.clone()).or_default().push(*range);
    }
    // Validate, sort descending, dedup, and check for overlaps.
    for (path, ranges) in by_file.iter_mut() {
        let file = {
            live.capture_shared_file(path)
                .ok_or_else(|| format!("File disappeared: {path}"))?
        };
        validate_rename_ranges(ranges, &file.content, &input.name, path)?;
    }

    // Build uncertain warning lines sorted by file then line, deduped.
    let mut sorted_uncertain = qualified_uncertain.clone();
    sorted_uncertain.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    sorted_uncertain.dedup();
    let uncertain_lines: Vec<String> = sorted_uncertain
        .iter()
        .map(|(path, line, ctx)| format!("  {}:{}  {}", path, line, ctx))
        .collect();

    Ok(RenamePlan {
        by_file,
        uncertain_lines,
        language,
    })
}

/// The MCP dry-run presentation, reusable by the embedded preview after its
/// own output-safety and byte-budget checks.
pub(crate) fn render_rename_preview(
    live: &LiveIndex,
    plan: &RenamePlan,
    old_name: &str,
    new_name: &str,
) -> String {
    const MAX_PREVIEW_SITES_PER_FILE: usize = 10;
    let total_confident: usize = plan.by_file.values().map(Vec::len).sum();
    let mut lines = vec![format!("Dry run: `{old_name}` → `{new_name}`")];
    lines.push(format!(
        "\n── Confident matches (will be applied) — {} site(s) across {} file(s) ──",
        total_confident,
        plan.by_file.len(),
    ));
    let mut sorted_files: Vec<_> = plan.by_file.iter().collect();
    sorted_files.sort_by_key(|(path, _)| (*path).clone());
    for (path, ranges) in sorted_files {
        lines.push(format!("  {} ({} site(s))", path, ranges.len()));
        if let Some(file) = live.capture_shared_file(path) {
            let content = String::from_utf8_lossy(&file.content);
            let mut ascending = ranges.clone();
            ascending.sort_by_key(|(start, _)| *start);
            for (start, _) in ascending.iter().take(MAX_PREVIEW_SITES_PER_FILE) {
                let line_no = file.content[..(*start as usize).min(file.content.len())]
                    .iter()
                    .filter(|&&byte| byte == b'\n')
                    .count()
                    + 1;
                let src_line = content.lines().nth(line_no - 1).unwrap_or("").trim();
                lines.push(format!("    L{line_no}: {src_line}"));
            }
            let overflow = ranges.len().saturating_sub(MAX_PREVIEW_SITES_PER_FILE);
            if overflow > 0 {
                lines.push(format!("    … and {overflow} more"));
            }
        }
    }
    if !plan.uncertain_lines.is_empty() {
        lines.push(format!(
            "\n── Uncertain matches (NOT applied — review manually) — {} site(s) ──",
            plan.uncertain_lines.len(),
        ));
        lines.extend(plan.uncertain_lines.iter().cloned());
    }
    lines.join("\n")
}
