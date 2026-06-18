//! Compress-Cache-Retrieve (CCR-lite) for bulk discovery tool output.
//!
//! ponytail: v1 in-memory per session only; disk spill under `.symforge/session-blobs/`
//! is the upgrade path when serve long-lived sessions need restart survival.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

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

/// Per-session CCR blob store.
#[derive(Debug, Default)]
pub struct CcrStore {
    blobs: HashMap<String, CcrBlob>,
    total_bytes: usize,
    max_bytes: usize,
    max_entries: usize,
}

impl CcrStore {
    pub fn new() -> Self {
        Self {
            blobs: HashMap::new(),
            total_bytes: 0,
            max_bytes: 32 * 1024 * 1024,
            max_entries: 256,
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
        handle
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
    }
}
