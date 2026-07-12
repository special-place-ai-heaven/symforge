//! In-memory call-graph projection + inbound BFS (Program 015).
//!
//! Started as the SP-0A spike falsifier (p95 < 200ms at depth 5 — GO, see
//! `research.md` § Spike Results) and is now `detect_impact`'s (C-S1A-003)
//! real blast-radius engine. Edges are still **name-based syntactic** Call
//! references (no resolver yet), confidence implicitly 1.0 — the
//! resolver-weighted, incrementally-patched `GraphProjection` lands at
//! C-S2-001; this stays the over-approximating v1 in the meantime.
//!
//! BFS shape follows `data-model.md` Appendix A (`cbm_store_bfs`): inbound uses
//! `in_edges`, depth-capped, node-capped, deterministic ordering.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use super::store::LiveIndex;
use crate::domain::{ReferenceKind, SymbolKind};

/// Stable graph node key derived from the LiveIndex (see `data-model.md`
/// Appendix A). Equality over all three fields; hash/order fold the `SymbolKind`
/// to its discriminant so this needs no extra derives on the domain enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolId {
    pub path: String,
    pub name: String,
    pub kind: SymbolKind,
}

impl Hash for SymbolId {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path.hash(state);
        self.name.hash(state);
        (self.kind as u8).hash(state);
    }
}

impl Ord for SymbolId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.path
            .cmp(&other.path)
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| (self.kind as u8).cmp(&(other.kind as u8)))
    }
}

impl PartialOrd for SymbolId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Inbound adjacency over Call edges: `in_edges[callee]` = sorted, deduped
/// callers. Outbound is not needed for SP-0A (inbound BFS only).
pub struct GraphProjection {
    in_edges: HashMap<SymbolId, Vec<SymbolId>>,
    node_count: usize,
    edge_count: usize,
}

impl GraphProjection {
    /// Build the projection from a frozen index snapshot. Nodes are every
    /// symbol definition; edges are caller -> callee for each `Call` reference.
    ///
    /// Callee resolution is ambiguity-scoped (019 detect_impact correctness):
    /// a `Call` whose bare name maps to exactly ONE definition links that def
    /// (bare-name resolution is correct and unambiguous). A name that maps to
    /// MULTIPLE defs is NOT fanned out to all of them — the old v1 behavior
    /// linked every same-name def, so a call to `run()` became an edge to every
    /// `run` in the repo, exploding `detect_impact`'s blast radius with wrong
    /// callers. Instead we try to disambiguate via the call site's
    /// `qualified_name` module segment; if that does not pick exactly one def,
    /// the edge is dropped rather than inventing N wrong edges.
    pub fn from_index(index: &LiveIndex) -> Self {
        // Pass 1: collect every definition keyed by name.
        let mut defs_by_name: HashMap<&str, Vec<SymbolId>> = HashMap::new();
        let mut node_count = 0usize;
        for (path, file) in &index.files {
            for sym in &file.symbols {
                node_count += 1;
                defs_by_name
                    .entry(sym.name.as_str())
                    .or_default()
                    .push(SymbolId {
                        path: path.clone(),
                        name: sym.name.clone(),
                        kind: sym.kind,
                    });
            }
        }

        // Pass 2: build inbound adjacency from Call references.
        let mut in_edges: HashMap<SymbolId, Vec<SymbolId>> = HashMap::new();
        for (path, file) in &index.files {
            for r in &file.references {
                if r.kind != ReferenceKind::Call {
                    continue;
                }
                // A caller must have an enclosing definition; module-level calls
                // have no caller node and are skipped (ponytail: file pseudo-node
                // if module-level edges matter at C-S2-001).
                let Some(enc_idx) = r.enclosing_symbol_index else {
                    continue;
                };
                let Some(caller_sym) = file.symbols.get(enc_idx as usize) else {
                    continue;
                };
                let caller = SymbolId {
                    path: path.clone(),
                    name: caller_sym.name.clone(),
                    kind: caller_sym.kind,
                };
                let Some(callees) = defs_by_name.get(r.name.as_str()) else {
                    continue;
                };
                // Ambiguity-scoped callee resolution (SAME principle as Item 1's
                // ambiguity gate): one def -> link it; many defs -> disambiguate
                // by the call site's module qualifier, else drop the edge.
                let resolved: Option<&SymbolId> = match callees.as_slice() {
                    [only] => Some(only),
                    many => resolve_ambiguous_callee(many, r.qualified_name.as_deref()),
                };
                let Some(callee) = resolved else {
                    // ponytail: an ambiguous bare-name call with no resolving
                    // qualifier is dropped (0 edges) rather than fanned out to
                    // all N same-name defs. A real name resolver (C-S2-001)
                    // would recover the true edge; until then, dropping keeps
                    // detect_impact's blast radius honest instead of confidently
                    // wrong. Ceiling: cross-file overloads that share a name and
                    // carry no module qualifier lose their (single) true edge.
                    continue;
                };
                if *callee == caller {
                    continue; // skip self-recursion edges
                }
                in_edges
                    .entry(callee.clone())
                    .or_default()
                    .push(caller.clone());
            }
        }

        // Determinism: sort + dedup each adjacency list.
        let mut edge_count = 0usize;
        for callers in in_edges.values_mut() {
            callers.sort();
            callers.dedup();
            edge_count += callers.len();
        }

        Self {
            in_edges,
            node_count,
            edge_count,
        }
    }

    pub fn node_count(&self) -> usize {
        self.node_count
    }

    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// The `n` symbols with the most inbound Call edges (most-depended-on),
    /// deterministically ordered. Used to pick representative BFS roots.
    pub fn top_inbound_targets(&self, n: usize) -> Vec<SymbolId> {
        let mut ranked: Vec<(&SymbolId, usize)> = self
            .in_edges
            .iter()
            .map(|(id, callers)| (id, callers.len()))
            .collect();
        // Highest in-degree first; tie-break on the stable SymbolId order.
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        ranked
            .into_iter()
            .take(n)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Inbound BFS: symbols that transitively reach (call) `start`, up to
    /// `max_depth` hops, capped at `max_nodes` results, each tagged with its
    /// hop distance from `start`. Deterministic order (BFS traversal order —
    /// callers are visited in sorted order per `from_index`, so ties break the
    /// same way every run). Empty graph or unknown `start` -> empty result,
    /// never panics.
    pub fn inbound_bfs(
        &self,
        start: &SymbolId,
        max_depth: u32,
        max_nodes: usize,
    ) -> Vec<(SymbolId, u32)> {
        let mut visited: HashSet<SymbolId> = HashSet::new();
        visited.insert(start.clone());
        let mut queue: VecDeque<(SymbolId, u32)> = VecDeque::new();
        queue.push_back((start.clone(), 0));
        let mut results: Vec<(SymbolId, u32)> = Vec::new();

        while let Some((node, depth)) = queue.pop_front() {
            if results.len() >= max_nodes {
                break;
            }
            if depth > 0 {
                results.push((node.clone(), depth));
            }
            if depth >= max_depth {
                continue;
            }
            if let Some(callers) = self.in_edges.get(&node) {
                for caller in callers {
                    if visited.insert(caller.clone()) {
                        queue.push_back((caller.clone(), depth + 1));
                    }
                }
            }
        }

        results
    }
}

/// Safety cap on per-seed BFS traversal size, matching the SP-0A spike's own
/// exercised bound. Not caller-tunable — `detect_impact`'s frozen contract has
/// no such input; this only guards against a pathological in-degree blowup on
/// a single seed. Response-size pagination is the tool layer's job.
const INBOUND_BFS_SAFETY_CAP: usize = 100_000;

/// Risk tier assigned to a blast-radius node, by hop distance from the
/// nearest changed symbol (contracts/detect-impact.md § Risk tiers).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiskTier {
    Critical,
    High,
    Medium,
    Low,
}

impl RiskTier {
    /// Lowercase wire label used by `detect_impact`'s JSON output.
    pub fn as_str(self) -> &'static str {
        match self {
            RiskTier::Critical => "critical",
            RiskTier::High => "high",
            RiskTier::Medium => "medium",
            RiskTier::Low => "low",
        }
    }

    /// Severity ordering for ranking blast entries (higher = more severe):
    /// Critical > High > Medium > Low. Used by `detect_impact` to keep the most
    /// severe nodes when the returned list is capped.
    pub fn severity_rank(self) -> u8 {
        match self {
            RiskTier::Critical => 3,
            RiskTier::High => 2,
            RiskTier::Medium => 1,
            RiskTier::Low => 0,
        }
    }

    /// Tier for a blast node at `hop` hops from the nearest changed symbol,
    /// promoted to `Critical` when the node is an entry point at hop 1
    /// (contracts/detect-impact.md § Risk tiers: hop 1 = High, hop 2 = Medium,
    /// hop 3+ = Low, entry_point at hop 1 = Critical).
    fn for_hop(hop: u32, is_entry_point: bool) -> Self {
        if hop <= 1 {
            if hop == 1 && is_entry_point {
                RiskTier::Critical
            } else {
                RiskTier::High
            }
        } else if hop == 2 {
            RiskTier::Medium
        } else {
            RiskTier::Low
        }
    }
}

/// One node in a `detect_impact` blast radius (always hop >= 1 — the changed
/// symbols themselves are hop 0 and never appear here).
#[derive(Clone, Debug)]
pub struct BlastNode {
    pub symbol: SymbolId,
    pub hop: u32,
    pub risk: RiskTier,
}

/// Disambiguate a `Call` to an ambiguous bare name (>1 same-name def) using the
/// call site's `qualified_name` module segment (e.g. `"a::run"` -> module hint
/// `"a"`). Returns `Some(def)` only when exactly ONE candidate's path matches
/// the hint (file stem or a path component); otherwise `None`, so the caller
/// drops the edge rather than fanning out to all N candidates.
///
/// ponytail: the hint match is a syntactic path-segment heuristic, not real
/// name resolution. It recovers the common `mod::name` case (the qualified
/// module-call fixture) without inventing wrong edges; the resolver at
/// C-S2-001 supersedes it. When the hint is absent or matches zero/multiple
/// candidates, we return `None` (drop) — safety over recall.
fn resolve_ambiguous_callee<'a>(
    candidates: &'a [SymbolId],
    qualified_name: Option<&str>,
) -> Option<&'a SymbolId> {
    // Module hint = the segment before the last `::` (last `::` separates the
    // module path from the called name). No `::` -> no hint -> drop.
    let qn = qualified_name?;
    let module_hint = qn.rsplit_once("::").map(|(head, _)| head)?;
    // Use the innermost module segment (e.g. `crate::a` -> `a`).
    let hint = module_hint.rsplit("::").next().unwrap_or(module_hint);
    if hint.is_empty() {
        return None;
    }
    let mut matched: Option<&SymbolId> = None;
    for cand in candidates {
        let path_matches = std::path::Path::new(&cand.path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|stem| stem == hint)
            .unwrap_or(false)
            || cand
                .path
                .split(['/', '\\'])
                .any(|component| component == hint);
        if path_matches {
            if matched.is_some() {
                return None; // hint matched more than one candidate -> ambiguous
            }
            matched = Some(cand);
        }
    }
    matched
}

/// A `fn main` is the conventional program entry point across every language
/// this repo indexes.
///
/// ponytail: minimal S1a heuristic for the frozen contract's one required
/// entry-point case. The full `GraphNode.is_entry_point` signal (HTTP routes,
/// CLI commands — data-model.md) lands at S2+ once `GraphProjection` gains
/// outbound edges and route metadata.
fn is_entry_point(id: &SymbolId) -> bool {
    id.name == "main" && matches!(id.kind, SymbolKind::Function | SymbolKind::Method)
}

/// Compute the blast radius for a set of changed symbols
/// (contracts/detect-impact.md § Risk tiers). `changed` symbols are hop 0 and
/// never appear in the result. When two changed symbols both reach the same
/// node, the nearest hop wins. Sorted by hop then `SymbolId` (deterministic).
/// Empty `changed`, an empty graph, or unknown symbols -> empty result, never
/// panics.
pub fn compute_impact(
    graph: &GraphProjection,
    changed: &[SymbolId],
    max_depth: u32,
) -> Vec<BlastNode> {
    let changed_set: HashSet<&SymbolId> = changed.iter().collect();
    let mut best_hop: HashMap<SymbolId, u32> = HashMap::new();

    for start in changed {
        for (reached, hop) in graph.inbound_bfs(start, max_depth, INBOUND_BFS_SAFETY_CAP) {
            if changed_set.contains(&reached) {
                continue; // hop 0 (itself changed) never appears in the blast list
            }
            best_hop
                .entry(reached)
                .and_modify(|existing| *existing = (*existing).min(hop))
                .or_insert(hop);
        }
    }

    let mut nodes: Vec<BlastNode> = best_hop
        .into_iter()
        .map(|(symbol, hop)| {
            let risk = RiskTier::for_hop(hop, is_entry_point(&symbol));
            BlastNode { symbol, hop, risk }
        })
        .collect();
    nodes.sort_by(|a, b| a.hop.cmp(&b.hop).then_with(|| a.symbol.cmp(&b.symbol)));
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_index::LiveIndex;

    fn build_graph(files: &[(&str, &str)]) -> GraphProjection {
        let dir = tempfile::tempdir().expect("tempdir");
        for (rel, content) in files {
            let path = dir.path().join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dir");
            }
            std::fs::write(&path, content).expect("write fixture file");
        }
        let shared = LiveIndex::load(dir.path()).expect("load index");
        let index = shared.read();
        GraphProjection::from_index(&index)
    }

    fn sym(path: &str, name: &str) -> SymbolId {
        SymbolId {
            path: path.to_string(),
            name: name.to_string(),
            kind: SymbolKind::Function,
        }
    }

    #[test]
    fn compute_impact_excludes_hop_zero_and_is_empty_with_no_callers() {
        let graph = build_graph(&[
            ("lib.rs", "pub fn core() -> u32 { 1 }\n"),
            ("a.rs", "pub fn call_a() -> u32 { core() }\n"),
        ]);
        let changed = vec![sym("a.rs", "call_a")];
        let blast = compute_impact(&graph, &changed, 2);
        assert!(
            blast.is_empty(),
            "call_a has no callers in this fixture: {blast:?}"
        );
    }

    #[test]
    fn compute_impact_risk_tiers_by_hop() {
        let graph = build_graph(&[
            ("lib.rs", "pub fn core() -> u32 { 1 }\n"),
            ("a.rs", "pub fn call_a() -> u32 { core() }\n"),
            ("main.rs", "fn main() { call_a(); }\n"),
        ]);
        let changed = vec![sym("lib.rs", "core")];
        let blast = compute_impact(&graph, &changed, 2);

        let call_a = blast
            .iter()
            .find(|n| n.symbol.name == "call_a")
            .expect("call_a reached at hop 1");
        assert_eq!(call_a.hop, 1);
        assert_eq!(call_a.risk.as_str(), "high");

        let main = blast
            .iter()
            .find(|n| n.symbol.name == "main")
            .expect("main reached at hop 2");
        assert_eq!(main.hop, 2);
        // main() is an entry point, but the Critical promotion only applies at
        // hop 1 (contracts/detect-impact.md § Risk tiers).
        assert_eq!(main.risk.as_str(), "medium");
    }

    #[test]
    fn compute_impact_entry_point_critical_at_hop_one() {
        let graph = build_graph(&[
            ("lib.rs", "pub fn core() -> u32 { 1 }\n"),
            ("main.rs", "fn main() { core(); }\n"),
        ]);
        let changed = vec![sym("lib.rs", "core")];
        let blast = compute_impact(&graph, &changed, 1);
        assert_eq!(blast.len(), 1);
        assert_eq!(blast[0].symbol.name, "main");
        assert_eq!(blast[0].hop, 1);
        assert_eq!(blast[0].risk.as_str(), "critical");
    }

    #[test]
    fn compute_impact_nearest_hop_wins_across_changed_seeds() {
        let graph = build_graph(&[
            ("lib.rs", "pub fn core() -> u32 { 1 }\n"),
            ("a.rs", "pub fn call_a() -> u32 { core() }\n"),
            ("b.rs", "pub fn call_b() -> u32 { core() }\n"),
            ("main.rs", "fn main() { call_a(); call_b(); }\n"),
        ]);
        // From `core` alone, `main` is reached at hop 2 (core -> call_a -> main).
        // Also seeding `call_a` as changed reaches `main` directly at hop 1;
        // the merged result must keep the nearest (smaller) hop.
        let changed = vec![sym("lib.rs", "core"), sym("a.rs", "call_a")];
        let blast = compute_impact(&graph, &changed, 3);
        let main = blast
            .iter()
            .find(|n| n.symbol.name == "main")
            .expect("main reached");
        assert_eq!(main.hop, 1, "nearest hop across changed seeds must win");
    }

    #[test]
    fn compute_impact_reaches_across_qualified_module_call() {
        // Mirrors tests/fixtures/cbm_impact: main.rs calls `a::call_a()`
        // (module-qualified), not a bare `call_a()`.
        let graph = build_graph(&[
            ("lib.rs", "pub fn core() -> u32 { 1 }\n"),
            ("a.rs", "pub fn call_a() -> u32 { core() }\n"),
            ("main.rs", "mod a;\nfn main() { a::call_a(); }\n"),
        ]);
        let changed = vec![sym("a.rs", "call_a")];
        let blast = compute_impact(&graph, &changed, 1);
        assert_eq!(
            blast.len(),
            1,
            "main() calling a::call_a() must resolve as a caller: {blast:?}"
        );
        assert_eq!(blast[0].symbol.name, "main");
    }

    #[test]
    fn compute_impact_empty_changed_set_yields_empty_blast() {
        let graph = build_graph(&[("lib.rs", "pub fn core() -> u32 { 1 }\n")]);
        assert!(compute_impact(&graph, &[], 5).is_empty());
    }

    // PART B (019 detect_impact fix): a bare `run()` call must NOT fan out to
    // every `run` definition in the repo. When the callee name is ambiguous and
    // the call site carries no disambiguating qualifier, the edge is dropped
    // rather than inventing N wrong caller->callee edges. Changing either `run`
    // must therefore reach AT MOST the correctly-scoped one — never both.
    #[test]
    fn ambiguous_bare_call_does_not_fan_out_to_all_same_name_defs() {
        let graph = build_graph(&[
            ("a.rs", "pub fn run() -> u32 { 1 }\n"),
            ("b.rs", "pub fn run() -> u32 { 2 }\n"),
            ("caller.rs", "fn drive() -> u32 { run() }\n"),
        ]);
        // Inspect the raw call edges, not the deduped blast (compute_impact
        // collapses a node reached from two seeds into one entry, which would
        // mask the fan-out). `drive` must be an inbound caller of AT MOST one
        // `run` def, not both.
        let drive = sym("caller.rs", "drive");
        let reaches_a = graph
            .inbound_bfs(&sym("a.rs", "run"), 2, 100)
            .iter()
            .any(|(id, _)| *id == drive);
        let reaches_b = graph
            .inbound_bfs(&sym("b.rs", "run"), 2, 100)
            .iter()
            .any(|(id, _)| *id == drive);
        let hits = usize::from(reaches_a) + usize::from(reaches_b);
        assert!(
            hits <= 1,
            "bare ambiguous run() must not link drive to BOTH run defs \
             (reaches_a={reaches_a} reaches_b={reaches_b})"
        );
    }
}
