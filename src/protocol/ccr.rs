//! Compress-Cache-Retrieve (CCR-lite) for bulk discovery tool output.
//!
//! ponytail: v1 in-memory per session only; disk spill under `.symforge/session-blobs/`
//! is the upgrade path when serve long-lived sessions need restart survival.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

use crate::live_index::search::{TextLineMatch, TextSearchResult};

const SEARCH_LINES_PER_FILE: usize = 10;
const SEARCH_MAX_FILES: usize = 20;

/// Per-tool output shaping rules (011).
#[derive(Clone, Copy, Debug)]
pub struct ToolOutputProfile {
    pub tool_name: &'static str,
    pub ccr_eligible: bool,
    pub default_max_tokens: u64,
}

pub const TOOL_OUTPUT_PROFILES: &[ToolOutputProfile] = &[
    ToolOutputProfile {
        tool_name: "search_text",
        ccr_eligible: true,
        default_max_tokens: 8_000,
    },
    ToolOutputProfile {
        tool_name: "search_symbols",
        ccr_eligible: true,
        default_max_tokens: 8_000,
    },
    ToolOutputProfile {
        tool_name: "find_references",
        ccr_eligible: true,
        default_max_tokens: 8_000,
    },
    ToolOutputProfile {
        tool_name: "explore",
        ccr_eligible: true,
        default_max_tokens: 12_000,
    },
    ToolOutputProfile {
        tool_name: "get_repo_map",
        ccr_eligible: true,
        default_max_tokens: 16_000,
    },
];

pub fn profile_for_tool(tool_name: &str) -> Option<&'static ToolOutputProfile> {
    TOOL_OUTPUT_PROFILES
        .iter()
        .find(|p| p.tool_name == tool_name)
}

/// Resolve `max_tokens` from agent override or tool profile default.
pub fn resolve_tool_max_tokens(tool_name: &str, agent_max: Option<u64>) -> Option<u64> {
    agent_max.or_else(|| profile_for_tool(tool_name).map(|p| p.default_max_tokens))
}

/// Stored formatted output for reversible compression.
#[derive(Clone, Debug)]
pub struct CcrBlob {
    pub handle: String,
    pub tool_name: String,
    pub formatted_bytes: String,
    pub created_at: Instant,
}

/// Per-session CCR economics counters (011 US5, heuristic).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct CcrEconomics {
    pub offloads: u32,
    pub bytes_stored: u64,
    pub retrieves: u32,
    pub bytes_retrieved: u64,
}

/// Combined session compression counters for economics surfaces (011 US5).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
pub struct SessionCompressionHeuristic {
    pub cache_hits: u32,
    pub ccr_offloads: u32,
    pub ccr_bytes_stored: u64,
    pub ccr_bytes_retrieved: u64,
}

impl SessionCompressionHeuristic {
    pub fn from_parts(cache_hits: u32, ccr: CcrEconomics) -> Self {
        Self {
            cache_hits,
            ccr_offloads: ccr.offloads,
            ccr_bytes_stored: ccr.bytes_stored,
            ccr_bytes_retrieved: ccr.bytes_retrieved,
        }
    }
}

/// Per-session CCR blob store.
#[derive(Debug, Default)]
pub struct CcrStore {
    blobs: HashMap<String, CcrBlob>,
    total_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
    economics: CcrEconomics,
}

impl CcrStore {
    pub fn new() -> Self {
        Self {
            blobs: HashMap::new(),
            total_bytes: 0,
            max_bytes: 32 * 1024 * 1024,
            max_entries: 256,
            economics: CcrEconomics::default(),
        }
    }

    pub fn insert(&mut self, tool_name: &str, formatted: String) -> String {
        let handle = mint_handle(tool_name, &formatted);
        let byte_len = formatted.len();
        while self.total_bytes.saturating_add(byte_len) > self.max_bytes
            || self.blobs.len() >= self.max_entries
        {
            if !self.evict_oldest() {
                break;
            }
        }
        self.total_bytes = self.total_bytes.saturating_add(byte_len);
        self.blobs.insert(
            handle.clone(),
            CcrBlob {
                handle: handle.clone(),
                tool_name: tool_name.to_string(),
                formatted_bytes: formatted,
                created_at: Instant::now(),
            },
        );
        self.economics.offloads = self.economics.offloads.saturating_add(1);
        self.economics.bytes_stored = self.economics.bytes_stored.saturating_add(byte_len as u64);
        handle
    }

    pub fn economics(&self) -> CcrEconomics {
        self.economics
    }

    /// Fetch blob and record retrieve bytes (US5).
    pub fn retrieve(&mut self, handle: &str) -> Option<String> {
        let blob = self.blobs.get(handle)?;
        let bytes = blob.formatted_bytes.len() as u64;
        self.economics.retrieves = self.economics.retrieves.saturating_add(1);
        self.economics.bytes_retrieved = self.economics.bytes_retrieved.saturating_add(bytes);
        Some(blob.formatted_bytes.clone())
    }

    pub fn get(&self, handle: &str) -> Option<&CcrBlob> {
        self.blobs.get(handle)
    }

    fn evict_oldest(&mut self) -> bool {
        let oldest = self
            .blobs
            .iter()
            .min_by_key(|(_, b)| b.created_at)
            .map(|(h, b)| (h.clone(), b.formatted_bytes.len()));
        let Some((handle, len)) = oldest else {
            return false;
        };
        self.blobs.remove(&handle);
        self.total_bytes = self.total_bytes.saturating_sub(len);
        true
    }
}

fn mint_handle(tool_name: &str, formatted: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tool_name.hash(&mut hasher);
    formatted.hash(&mut hasher);
    format!("{:012x}", hasher.finish() & 0xFFFF_FFFF_FFFF)
}

/// If `summary` exceeds budget, store `full` and return summary + CCR footer.
pub fn apply_ccr_overflow(
    store: &mut CcrStore,
    tool_name: &str,
    summary: String,
    full: String,
    max_tokens: u64,
) -> String {
    let max_bytes = (max_tokens as usize).saturating_mul(4);
    if full.len() <= max_bytes {
        return full;
    }
    if summary.len() > max_bytes {
        return summary;
    }
    let handle = store.insert(tool_name, full);
    let omitted_note = "full ranked output stored";
    format!(
        "{summary}\n---\nCCR: {omitted_note} · retrieve: symforge_retrieve with hash=\"{handle}\"\n"
    )
}

fn line_is_error_severity(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    ["error", "fatal", "panic", "exception", "failed"]
        .iter()
        .any(|term| lower.contains(term))
}

fn score_line_match(line_match: &TextLineMatch, query: &str) -> i32 {
    let query_lower = query.to_ascii_lowercase();
    let mut score = 0;
    if line_is_error_severity(&line_match.line) {
        score += 5;
    }
    if !query_lower.is_empty() && line_match.line.to_ascii_lowercase().contains(&query_lower) {
        score += 3;
    }
    if let Some(sym) = &line_match.enclosing_symbol
        && sym.name.to_ascii_lowercase().contains(&query_lower)
    {
        score += 2;
    }
    score
}

/// Rank and cap search_text matches: preserve error-severity lines (US3).
pub fn compact_text_search_result(result: &mut TextSearchResult, query: &str) {
    let mut omitted = 0usize;
    let mut file_scores: Vec<(usize, i32)> = result
        .files
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let best = file
                .matches
                .iter()
                .map(|m| score_line_match(m, query))
                .max()
                .unwrap_or(0);
            (index, best)
        })
        .collect();
    file_scores.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let keep_file_indices: std::collections::HashSet<usize> = file_scores
        .iter()
        .take(SEARCH_MAX_FILES)
        .map(|(index, _)| *index)
        .collect();

    let mut kept_files = Vec::new();
    for (index, mut file) in result.files.drain(..).enumerate() {
        if !keep_file_indices.contains(&index) {
            omitted = omitted.saturating_add(file.matches.len());
            continue;
        }
        omitted = omitted.saturating_add(cap_file_matches(&mut file.matches, query));
        if !file.matches.is_empty() {
            kept_files.push(file);
        } else {
            omitted = omitted.saturating_add(1);
        }
    }
    result.files = kept_files;
    result.overflow_count = result.overflow_count.saturating_add(omitted);
}

fn cap_file_matches(matches: &mut Vec<TextLineMatch>, query: &str) -> usize {
    if matches.is_empty() {
        return 0;
    }
    let before = matches.len();
    let mut ranked: Vec<(usize, i32, bool)> = matches
        .iter()
        .enumerate()
        .map(|(index, m)| {
            (
                index,
                score_line_match(m, query),
                line_is_error_severity(&m.line),
            )
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let mut keep = vec![false; matches.len()];
    for (index, _, is_error) in &ranked {
        if *is_error {
            keep[*index] = true;
        }
    }
    let mut non_error_kept = 0usize;
    for (index, _, is_error) in ranked {
        if is_error {
            continue;
        }
        if non_error_kept >= SEARCH_LINES_PER_FILE {
            break;
        }
        if !keep[index] {
            keep[index] = true;
            non_error_kept += 1;
        }
    }

    let capped: Vec<_> = matches
        .drain(..)
        .enumerate()
        .filter_map(|(index, m)| keep[index].then_some(m))
        .collect();
    let after = capped.len();
    *matches = capped;
    before.saturating_sub(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ccr_round_trip() {
        let mut store = CcrStore::new();
        let full = "line\n".repeat(5000);
        let summary = "top hits".to_string();
        let out = apply_ccr_overflow(&mut store, "search_text", summary, full.clone(), 100);
        assert!(out.contains("symforge_retrieve"));
        let handle = out
            .split("hash=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("handle");
        let blob = store.get(handle).expect("blob");
        assert_eq!(blob.formatted_bytes, full);
        let econ = store.economics();
        assert_eq!(econ.offloads, 1);
        assert!(econ.bytes_stored > 0);
        let retrieved = store.retrieve(handle).expect("retrieve");
        assert_eq!(retrieved, full);
        assert_eq!(store.economics().retrieves, 1);
        assert_eq!(store.economics().bytes_retrieved, econ.bytes_stored);
    }

    #[test]
    fn compact_text_search_preserves_error_lines() {
        use crate::live_index::search::TextFileMatches;

        let mut result = TextSearchResult {
            label: "test".to_string(),
            total_matches: 52,
            files: vec![TextFileMatches {
                path: "src/log.rs".to_string(),
                matches: (0..50)
                    .map(|i| TextLineMatch {
                        line_number: i + 1,
                        line: format!("info line {i}"),
                        enclosing_symbol: None,
                    })
                    .chain([
                        TextLineMatch {
                            line_number: 51,
                            line: "ERROR: disk full".to_string(),
                            enclosing_symbol: None,
                        },
                        TextLineMatch {
                            line_number: 52,
                            line: "ERROR: retry failed".to_string(),
                            enclosing_symbol: None,
                        },
                    ])
                    .collect(),
                rendered_lines: None,
                callers: None,
            }],
            suppressed_by_noise: 0,
            overflow_count: 0,
        };
        compact_text_search_result(&mut result, "disk");
        let lines: Vec<_> = result.files[0]
            .matches
            .iter()
            .map(|m| m.line.as_str())
            .collect();
        assert!(lines.iter().any(|l| l.contains("ERROR: disk")));
        assert!(lines.iter().any(|l| l.contains("ERROR: retry")));
        assert!(result.overflow_count > 0);
    }
}
