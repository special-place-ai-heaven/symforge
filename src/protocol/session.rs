//! Session context tracking: records what the LLM has fetched this session
//! to enable deduplication hints, cache-hit short-circuits, and context inventory.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;

/// Kind of read recorded for session cache-hit keys (011).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum FetchKind {
    FileContext,
    Symbol,
    FileContent,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct FetchKey {
    kind: FetchKind,
    path: String,
    symbol: String,
    params_hash: u64,
}

/// Prior successful read metadata for cache-hit and dedup hints.
#[derive(Clone, Debug)]
pub struct SessionFetchRecord {
    pub approx_tokens: u32,
    pub fetched_at: Instant,
}

/// Metadata for a session cache-hit response body.
#[derive(Clone, Debug)]
pub struct SessionCacheHitMeta {
    pub kind: &'static str,
    pub path: String,
    pub name: String,
    pub prior_tokens: u32,
    pub session_age_secs: u64,
}

/// Tracks what symbols and files have been served to the LLM this session.
pub struct SessionContext {
    inner: Mutex<SessionInner>,
}

struct SessionInner {
    /// Symbols whose full body/detail was served: (path, name) → approximate tokens served
    fetched_symbols: HashMap<(String, String), u32>,
    /// Symbols that appeared in list/search-style results but whose body was not fetched.
    listed_symbols: HashMap<(String, String), u32>,
    /// Files whose full body/raw content was served: path → approximate tokens served
    fetched_files: HashMap<String, u32>,
    /// Files that appeared in overview/search-style results without their body being fetched.
    listed_files: HashMap<String, u32>,
    /// Aggregate outputs that consumed context without mapping cleanly to one file/symbol.
    summary_outputs: HashMap<String, u32>,
    /// Parameter-aware fetch records for cache-hit (011).
    detailed_fetches: HashMap<FetchKey, SessionFetchRecord>,
    /// Read-path cache-hit short-circuits (011 US5).
    cache_hit_count: u32,
    /// Total tokens served this session
    total_tokens: u64,
    /// Session start time
    started_at: Instant,
}

/// A snapshot of the session context for display.
pub struct SessionSnapshot {
    pub fetched_symbols: Vec<(String, String, u32)>, // (path, name, tokens)
    pub listed_symbols: Vec<(String, String, u32)>,  // (path, name, tokens)
    pub fetched_files: Vec<(String, u32)>,           // (path, tokens)
    pub listed_files: Vec<(String, u32)>,            // (path, tokens)
    pub summary_outputs: Vec<(String, u32)>,         // (label, tokens)
    pub cache_hit_count: u32,
    pub total_tokens: u64,
    pub duration_secs: u64,
}

impl Default for SessionContext {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionContext {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(SessionInner {
                fetched_symbols: HashMap::new(),
                listed_symbols: HashMap::new(),
                fetched_files: HashMap::new(),
                listed_files: HashMap::new(),
                summary_outputs: HashMap::new(),
                detailed_fetches: HashMap::new(),
                cache_hit_count: 0,
                total_tokens: 0,
                started_at: Instant::now(),
            }),
        }
    }

    /// Reset all session accounting to a fresh state. Called when the index is
    /// retargeted to a new repo root: paths recorded under the old root no longer
    /// belong to this session and would otherwise double-count in context_inventory.
    pub fn reset(&self) {
        let mut inner = self.inner.lock();
        *inner = SessionInner {
            fetched_symbols: HashMap::new(),
            listed_symbols: HashMap::new(),
            fetched_files: HashMap::new(),
            listed_files: HashMap::new(),
            summary_outputs: HashMap::new(),
            detailed_fetches: HashMap::new(),
            cache_hit_count: 0,
            total_tokens: 0,
            started_at: Instant::now(),
        };
    }

    /// Record that a symbol body/detail was served to the LLM.
    pub fn record_symbol(&self, path: &str, name: &str, tokens: u32) {
        let mut inner = self.inner.lock();
        let key = (path.to_string(), name.to_string());
        // Once a symbol body is fetched, it should no longer appear as list-only.
        inner.listed_symbols.remove(&key);
        inner.fetched_symbols.insert(key, tokens);
        inner.total_tokens += tokens as u64;
    }

    /// Record that a symbol appeared in a list/search result without serving its body.
    pub fn record_listed_symbol(&self, path: &str, name: &str) {
        let mut inner = self.inner.lock();
        let key = (path.to_string(), name.to_string());
        if inner.fetched_symbols.contains_key(&key) {
            return;
        }
        inner.listed_symbols.entry(key).or_insert(0);
    }

    /// Record that a file body/raw content was served to the LLM.
    pub fn record_file(&self, path: &str, tokens: u32) {
        let mut inner = self.inner.lock();
        inner.listed_files.remove(path);
        inner.fetched_files.insert(path.to_string(), tokens);
        inner.total_tokens += tokens as u64;
    }

    /// Record that a file appeared in an overview/search result without serving its body.
    pub fn record_listed_file(&self, path: &str, tokens: u32) {
        let mut inner = self.inner.lock();
        if inner.fetched_files.contains_key(path) {
            return;
        }
        inner.listed_files.insert(path.to_string(), tokens);
        inner.total_tokens += tokens as u64;
    }

    /// Record a read-path session cache-hit short-circuit (011 US5).
    pub fn record_cache_hit(&self) {
        let mut inner = self.inner.lock();
        inner.cache_hit_count = inner.cache_hit_count.saturating_add(1);
    }

    /// Record a summary/search-style output that consumed context without mapping to one item.
    pub fn record_summary_output(&self, label: &str, tokens: u32) {
        let mut inner = self.inner.lock();
        *inner.summary_outputs.entry(label.to_string()).or_insert(0) += tokens;
        inner.total_tokens += tokens as u64;
    }

    /// Check if a symbol has already been fetched this session.
    pub fn has_symbol(&self, path: &str, name: &str) -> bool {
        let inner = self.inner.lock();
        inner
            .fetched_symbols
            .contains_key(&(path.to_string(), name.to_string()))
    }

    /// Check if a file has already been fetched this session.
    pub fn has_file(&self, path: &str) -> bool {
        let inner = self.inner.lock();
        inner.fetched_files.contains_key(path)
    }

    /// Prior token cost recorded for a fetched symbol, if any.
    pub fn symbol_prior_tokens(&self, path: &str, name: &str) -> Option<u32> {
        let inner = self.inner.lock();
        inner
            .fetched_symbols
            .get(&(path.to_string(), name.to_string()))
            .copied()
    }

    /// Prior token cost recorded for a fetched file, if any.
    pub fn file_prior_tokens(&self, path: &str) -> Option<u32> {
        let inner = self.inner.lock();
        inner.fetched_files.get(path).copied()
    }

    /// Session age in seconds for cache-hit metadata.
    pub fn session_age_secs(&self) -> u64 {
        self.inner.lock().started_at.elapsed().as_secs()
    }

    fn try_cache_hit(
        &self,
        kind: FetchKind,
        path: &str,
        symbol: &str,
        params_hash: u64,
        force_refresh: bool,
    ) -> Option<SessionCacheHitMeta> {
        if force_refresh {
            return None;
        }
        let inner = self.inner.lock();
        let key = FetchKey {
            kind,
            path: path.to_string(),
            symbol: symbol.to_string(),
            params_hash,
        };
        let record = inner.detailed_fetches.get(&key)?;
        let (kind_label, name) = match kind {
            FetchKind::Symbol => ("symbol", symbol.to_string()),
            FetchKind::FileContext | FetchKind::FileContent => ("file", String::new()),
        };
        Some(SessionCacheHitMeta {
            kind: kind_label,
            path: path.to_string(),
            name,
            prior_tokens: record.approx_tokens,
            session_age_secs: inner.started_at.elapsed().as_secs(),
        })
    }

    fn record_detailed_fetch(
        &self,
        kind: FetchKind,
        path: &str,
        symbol: &str,
        params_hash: u64,
        tokens: u32,
    ) {
        let mut inner = self.inner.lock();
        let key = FetchKey {
            kind,
            path: path.to_string(),
            symbol: symbol.to_string(),
            params_hash,
        };
        inner.detailed_fetches.insert(
            key,
            SessionFetchRecord {
                approx_tokens: tokens,
                fetched_at: Instant::now(),
            },
        );
    }

    /// Prior fetch for dedup hint when `force_refresh` re-serves content.
    pub fn prior_fetch_for_dedup(
        &self,
        kind: FetchKind,
        path: &str,
        symbol: &str,
        params_hash: u64,
    ) -> Option<SessionFetchRecord> {
        let inner = self.inner.lock();
        inner
            .detailed_fetches
            .get(&FetchKey {
                kind,
                path: path.to_string(),
                symbol: symbol.to_string(),
                params_hash,
            })
            .cloned()
    }

    pub fn try_symbol_cache_hit(
        &self,
        path: &str,
        name: &str,
        params_hash: u64,
        force_refresh: bool,
    ) -> Option<SessionCacheHitMeta> {
        self.try_cache_hit(FetchKind::Symbol, path, name, params_hash, force_refresh)
    }

    pub fn try_file_context_cache_hit(
        &self,
        path: &str,
        params_hash: u64,
        force_refresh: bool,
    ) -> Option<SessionCacheHitMeta> {
        self.try_cache_hit(FetchKind::FileContext, path, "", params_hash, force_refresh)
    }

    pub fn try_file_content_cache_hit(
        &self,
        path: &str,
        params_hash: u64,
        force_refresh: bool,
    ) -> Option<SessionCacheHitMeta> {
        self.try_cache_hit(FetchKind::FileContent, path, "", params_hash, force_refresh)
    }

    pub fn record_symbol_fetch(&self, path: &str, name: &str, params_hash: u64, tokens: u32) {
        self.record_symbol(path, name, tokens);
        self.record_detailed_fetch(FetchKind::Symbol, path, name, params_hash, tokens);
    }

    pub fn record_file_context_fetch(&self, path: &str, params_hash: u64, tokens: u32) {
        self.record_detailed_fetch(FetchKind::FileContext, path, "", params_hash, tokens);
    }

    pub fn record_file_content_fetch(&self, path: &str, params_hash: u64, tokens: u32) {
        self.record_file(path, tokens);
        self.record_detailed_fetch(FetchKind::FileContent, path, "", params_hash, tokens);
    }

    /// STEL compact-step cache lookup using JSON args.
    pub fn try_cache_hit_from_stel_step(
        &self,
        tool: &str,
        args: &serde_json::Value,
    ) -> Option<SessionCacheHitMeta> {
        let force_refresh = args
            .get("force_refresh")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        match tool {
            "get_symbol" => {
                let name = args.get("name")?.as_str()?.trim();
                if name.is_empty() {
                    return None;
                }
                let path = args
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim();
                if path.is_empty() {
                    return None;
                }
                let hash = hash_symbol_params_json(args);
                self.try_symbol_cache_hit(path, name, hash, force_refresh)
            }
            "get_file_context" | "get_file_content" => {
                let path = args.get("path")?.as_str()?.trim();
                if path.is_empty() {
                    return None;
                }
                let hash = if tool == "get_file_context" {
                    hash_file_context_params_json(args)
                } else {
                    hash_file_content_params_json(args)
                };
                if tool == "get_file_context" {
                    self.try_file_context_cache_hit(path, hash, force_refresh)
                } else {
                    self.try_file_content_cache_hit(path, hash, force_refresh)
                }
            }
            _ => None,
        }
    }

    /// Take a snapshot for display.
    pub fn snapshot(&self) -> SessionSnapshot {
        let inner = self.inner.lock();
        let mut fetched_symbols: Vec<(String, String, u32)> = inner
            .fetched_symbols
            .iter()
            .map(|((p, n), t)| (p.clone(), n.clone(), *t))
            .collect();
        fetched_symbols.sort_by(|a, b| b.2.cmp(&a.2).then(a.1.cmp(&b.1)));

        let mut listed_symbols: Vec<(String, String, u32)> = inner
            .listed_symbols
            .iter()
            .map(|((p, n), t)| (p.clone(), n.clone(), *t))
            .collect();
        listed_symbols.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        let mut fetched_files: Vec<(String, u32)> = inner
            .fetched_files
            .iter()
            .map(|(p, t)| (p.clone(), *t))
            .collect();
        fetched_files.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        let mut listed_files: Vec<(String, u32)> = inner
            .listed_files
            .iter()
            .map(|(p, t)| (p.clone(), *t))
            .collect();
        listed_files.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        let mut summary_outputs: Vec<(String, u32)> = inner
            .summary_outputs
            .iter()
            .map(|(label, tokens)| (label.clone(), *tokens))
            .collect();
        summary_outputs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

        SessionSnapshot {
            fetched_symbols,
            listed_symbols,
            fetched_files,
            listed_files,
            summary_outputs,
            cache_hit_count: inner.cache_hit_count,
            total_tokens: inner.total_tokens,
            duration_secs: inner.started_at.elapsed().as_secs(),
        }
    }
}

pub fn hash_value(value: &serde_json::Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.to_string().hash(&mut hasher);
    hasher.finish()
}

pub fn hash_file_context_params(max_tokens: Option<u64>, sections: Option<&[String]>) -> u64 {
    hash_value(&serde_json::json!({
        "max_tokens": max_tokens,
        "sections": sections,
    }))
}

pub fn hash_file_context_params_json(args: &serde_json::Value) -> u64 {
    hash_value(&serde_json::json!({
        "max_tokens": args.get("max_tokens"),
        "sections": args.get("sections"),
    }))
}

pub fn hash_symbol_params_json(args: &serde_json::Value) -> u64 {
    hash_value(&serde_json::json!({
        "kind": args.get("kind"),
        "symbol_line": args.get("symbol_line"),
        "max_tokens": args.get("max_tokens"),
    }))
}

pub fn hash_symbol_params(
    kind: Option<&str>,
    symbol_line: Option<u32>,
    max_tokens: Option<u64>,
) -> u64 {
    hash_value(&serde_json::json!({
        "kind": kind,
        "symbol_line": symbol_line,
        "max_tokens": max_tokens,
    }))
}

pub fn hash_file_content_params_json(args: &serde_json::Value) -> u64 {
    hash_value(args)
}

/// Format the session context inventory for display.
pub fn format_context_inventory(
    snap: &SessionSnapshot,
    ccr: crate::protocol::ccr::CcrEconomics,
) -> String {
    let minutes = snap.duration_secs / 60;
    let total_items = snap.fetched_symbols.len()
        + snap.fetched_files.len()
        + snap.listed_symbols.len()
        + snap.listed_files.len()
        + snap.summary_outputs.len();

    let mut lines = vec![format!(
        "Session Context ({} minutes, {} tracked entries, ~{} tokens total)",
        minutes, total_items, snap.total_tokens
    )];

    if !snap.fetched_symbols.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Symbol bodies fetched ({}):",
            snap.fetched_symbols.len()
        ));
        for (path, name, tokens) in snap.fetched_symbols.iter().take(15) {
            lines.push(format!("  {name} ({path}) — ~{tokens} tokens"));
        }
        if snap.fetched_symbols.len() > 15 {
            lines.push(format!(
                "  ... and {} more",
                snap.fetched_symbols.len() - 15
            ));
        }
    }

    if !snap.fetched_files.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "File bodies fetched ({}):",
            snap.fetched_files.len()
        ));
        for (path, tokens) in snap.fetched_files.iter().take(10) {
            lines.push(format!("  {path} — ~{tokens} tokens"));
        }
        if snap.fetched_files.len() > 10 {
            lines.push(format!("  ... and {} more", snap.fetched_files.len() - 10));
        }
    }

    if !snap.listed_symbols.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Symbols listed only in search/summary results ({}):",
            snap.listed_symbols.len()
        ));
        for (path, name, _) in snap.listed_symbols.iter().take(15) {
            if path.is_empty() {
                lines.push(format!("  {name}"));
            } else {
                lines.push(format!("  {name} ({path})"));
            }
        }
        if snap.listed_symbols.len() > 15 {
            lines.push(format!("  ... and {} more", snap.listed_symbols.len() - 15));
        }
    }

    if !snap.listed_files.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Files listed only in search/summary results ({}):",
            snap.listed_files.len()
        ));
        for (path, tokens) in snap.listed_files.iter().take(10) {
            lines.push(format!("  {path} — ~{tokens} tokens of overview"));
        }
        if snap.listed_files.len() > 10 {
            lines.push(format!("  ... and {} more", snap.listed_files.len() - 10));
        }
    }

    if !snap.summary_outputs.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "Summary/search outputs without fetched bodies ({}):",
            snap.summary_outputs.len()
        ));
        for (label, tokens) in snap.summary_outputs.iter().take(10) {
            lines.push(format!("  {label} — ~{tokens} tokens"));
        }
        if snap.summary_outputs.len() > 10 {
            lines.push(format!(
                "  ... and {} more",
                snap.summary_outputs.len() - 10
            ));
        }
    }

    if snap.cache_hit_count > 0 || ccr.offloads > 0 || ccr.retrieves > 0 {
        lines.push(String::new());
        lines.push("Compression economics (heuristic estimates, 011):".to_string());
        lines.push(format!("  cache_hits: {}", snap.cache_hit_count));
        lines.push(format!("  ccr_offloads: {}", ccr.offloads));
        lines.push(format!("  ccr_bytes_stored: {}", ccr.bytes_stored));
        lines.push(format!("  ccr_bytes_retrieved: {}", ccr.bytes_retrieved));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_query() {
        let ctx = SessionContext::new();
        assert!(!ctx.has_symbol("src/lib.rs", "main"));
        ctx.record_listed_symbol("src/lib.rs", "main");
        assert!(
            !ctx.has_symbol("src/lib.rs", "main"),
            "list-only symbols should not count as fetched"
        );
        ctx.record_symbol("src/lib.rs", "main", 100);
        assert!(ctx.has_symbol("src/lib.rs", "main"));
        assert!(!ctx.has_file("src/lib.rs"));
        ctx.record_listed_file("src/lib.rs", 250);
        assert!(
            !ctx.has_file("src/lib.rs"),
            "overview-only files should not count as fetched"
        );
        ctx.record_file("src/lib.rs", 500);
        assert!(ctx.has_file("src/lib.rs"));
    }

    #[test]
    fn test_snapshot() {
        let ctx = SessionContext::new();
        ctx.record_listed_symbol("z.rs", "listed_fn");
        ctx.record_symbol("a.rs", "foo", 100);
        ctx.record_symbol("b.rs", "bar", 200);
        ctx.record_listed_file("outline.rs", 40);
        ctx.record_file("c.rs", 300);
        ctx.record_summary_output("explore", 60);
        let snap = ctx.snapshot();
        assert_eq!(snap.fetched_symbols.len(), 2);
        assert_eq!(snap.listed_symbols.len(), 1);
        assert_eq!(snap.fetched_files.len(), 1);
        assert_eq!(snap.listed_files.len(), 1);
        assert_eq!(snap.summary_outputs.len(), 1);
        assert_eq!(snap.total_tokens, 700);
    }

    #[test]
    fn test_reset_clears_accounting() {
        let ctx = SessionContext::new();
        ctx.record_symbol("old_root/a.rs", "foo", 100);
        ctx.record_file("old_root/b.rs", 200);
        assert!(ctx.has_symbol("old_root/a.rs", "foo"));
        ctx.reset();
        assert!(
            !ctx.has_symbol("old_root/a.rs", "foo"),
            "reset must drop symbols recorded under the previous root"
        );
        assert!(!ctx.has_file("old_root/b.rs"));
        let snap = ctx.snapshot();
        assert_eq!(snap.fetched_symbols.len(), 0);
        assert_eq!(snap.fetched_files.len(), 0);
        assert_eq!(snap.total_tokens, 0);
    }

    #[test]
    fn test_format_inventory() {
        let ctx = SessionContext::new();
        ctx.record_listed_symbol("src/search.rs", "SearchHit");
        ctx.record_symbol("src/lib.rs", "LiveIndex", 500);
        ctx.record_listed_file("src/overview.rs", 120);
        ctx.record_file("src/main.rs", 1000);
        ctx.record_summary_output("explore", 75);
        let snap = ctx.snapshot();
        let output = format_context_inventory(&snap, crate::protocol::ccr::CcrEconomics::default());
        assert!(output.contains("LiveIndex"));
        assert!(output.contains("SearchHit"));
        assert!(output.contains("src/overview.rs"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("explore"));
        assert!(output.contains("1695"));
    }
}
