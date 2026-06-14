//! Session context tracking: records what the LLM has fetched this session
//! to enable deduplication hints and context inventory.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::Instant;

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
                total_tokens: 0,
                started_at: Instant::now(),
            }),
        }
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
            total_tokens: inner.total_tokens,
            duration_secs: inner.started_at.elapsed().as_secs(),
        }
    }
}

/// Format the session context inventory for display.
pub fn format_context_inventory(snap: &SessionSnapshot) -> String {
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
    fn test_format_inventory() {
        let ctx = SessionContext::new();
        ctx.record_listed_symbol("src/search.rs", "SearchHit");
        ctx.record_symbol("src/lib.rs", "LiveIndex", 500);
        ctx.record_listed_file("src/overview.rs", 120);
        ctx.record_file("src/main.rs", 1000);
        ctx.record_summary_output("explore", 75);
        let snap = ctx.snapshot();
        let output = format_context_inventory(&snap);
        assert!(output.contains("LiveIndex"));
        assert!(output.contains("SearchHit"));
        assert!(output.contains("src/overview.rs"));
        assert!(output.contains("src/main.rs"));
        assert!(output.contains("explore"));
        assert!(output.contains("1695"));
    }
}
