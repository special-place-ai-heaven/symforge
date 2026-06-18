//! Co-change coupling store + ranker-signal integration.
//!
//! Populated by a bounded git-history walker; queried at rerank time to
//! promote files and symbols that historically ride with the query's
//! anchor. This module owns storage, cold-build, HEAD-delta updates, lazy
//! preparation, and ranker-facing evidence for co-change signals.

pub mod lifecycle;
pub mod schema;
pub mod store;
pub mod walker;

pub use lifecycle::{
    LazyPrepareOutcome, coupling_prepare_policy_from_env, init_coupling_store,
    open_existing_coupling_store, refresh_on_reconcile_tick, start_lazy_prepare,
};
pub use store::{CouplingRow, CouplingStore};
pub use walker::{DeltaOutcome, WalkerConfig, WalkerStats, apply_head_delta, cold_build};

/// `exp(-delta_secs * ln2 / half_life_secs)`. Shared by the walker and
/// the store's delta routine so rescale math stays consistent.
/// Returns 1.0 for non-positive half-life (guards against misconfig) or
/// non-positive delta (future / simultaneous commit — no decay).
pub(crate) fn decay_factor(delta_secs: i64, half_life_secs: i64) -> f64 {
    if half_life_secs <= 0 || delta_secs <= 0 {
        return 1.0;
    }
    (-(delta_secs as f64) * std::f64::consts::LN_2 / half_life_secs as f64).exp()
}

/// Granularity of a coupling edge. File-level and symbol-level rows coexist
/// in the same table, distinguished by the `AnchorKey` prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
    File,
    Symbol,
}

/// Namespaced key identifying one endpoint of a coupling edge.
///
/// * File endpoint: `"file:<rel-path>"`
/// * Symbol endpoint: `"symbol:<rel-path>#<name>#<kind>"`
///
/// Paths are normalised to forward slashes so Windows and POSIX paths
/// hash to the same row.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorKey(String);

impl AnchorKey {
    pub fn file(rel_path: &str) -> Self {
        Self(format!("file:{}", normalize_path(rel_path)))
    }

    pub fn symbol(rel_path: &str, name: &str, kind: &str) -> Self {
        Self(format!(
            "symbol:{}#{}#{}",
            normalize_path(rel_path),
            name,
            kind
        ))
    }

    pub(crate) fn from_raw(raw: String) -> Self {
        Self(raw)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn granularity(&self) -> Granularity {
        if self.0.starts_with("symbol:") {
            Granularity::Symbol
        } else {
            Granularity::File
        }
    }
}

fn normalize_path(s: &str) -> String {
    s.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_anchor_is_prefixed_and_slash_normalized() {
        let a = AnchorKey::file("src\\live_index\\query.rs");
        assert_eq!(a.as_str(), "file:src/live_index/query.rs");
        assert_eq!(a.granularity(), Granularity::File);
    }

    #[test]
    fn symbol_anchor_embeds_name_and_kind() {
        let a = AnchorKey::symbol("src/live_index/query.rs", "capture_search_files_view", "fn");
        assert_eq!(
            a.as_str(),
            "symbol:src/live_index/query.rs#capture_search_files_view#fn"
        );
        assert_eq!(a.granularity(), Granularity::Symbol);
    }

    #[test]
    fn from_raw_preserves_unknown_prefix_as_file_granularity() {
        // Defensive: DB rows with an unexpected shape still deserialize.
        let a = AnchorKey::from_raw("weird:whatever".to_string());
        assert_eq!(a.as_str(), "weird:whatever");
        assert_eq!(a.granularity(), Granularity::File);
    }
}
