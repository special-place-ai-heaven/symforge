//! In-memory call-graph projection + inbound BFS (Program 015).
//!
//! Started as the SP-0A spike falsifier (p95 < 200ms at depth 5 — GO, see
//! `research.md` § Spike Results) and is now `detect_impact`'s (C-S1A-003)
//! real blast-radius engine. Edges are **language-family-scoped, name-based
//! syntactic** Call references (no compiler resolver yet). Each edge retains
//! source spans and the syntactic resolution rationale for its explanation.
//!
//! BFS shape follows `data-model.md` Appendix A (`cbm_store_bfs`): inbound uses
//! `in_edges`, depth-capped, node-capped, deterministic ordering.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};

use super::store::LiveIndex;
use crate::domain::{LanguageId, ReferenceKind, SymbolKind};

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
/// callers. `edge_evidence` retains each source call site for that edge.
pub struct GraphProjection {
    in_edges: HashMap<SymbolId, Vec<SymbolId>>,
    edge_evidence: HashMap<(SymbolId, SymbolId), Vec<CallEdgeEvidence>>,
    node_count: usize,
    edge_count: usize,
}

/// Why the graph linked a syntactic call to this definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ResolutionReason {
    UniqueName,
    QualifiedModule,
    QualifiedOwner,
}

/// Source-only evidence for one call edge. Byte ranges are half-open; line
/// ranges retain the zero-based inclusive rows stored by `ReferenceRecord`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallEdgeEvidence {
    pub caller: SymbolId,
    pub callee: SymbolId,
    pub call_site_path: String,
    pub byte_range: (u32, u32),
    pub line_range: (u32, u32),
    pub resolution: ResolutionReason,
}

/// A blast-radius row and one complete, shortest call chain explaining it.
#[derive(Clone, Debug)]
pub(crate) struct BlastNodeEvidence {
    pub node: BlastNode,
    /// Ordered from the changed seed to `node.symbol`; never contains snippets.
    pub steps: Vec<CallEdgeEvidence>,
}

fn graph_language_family(language: LanguageId) -> LanguageId {
    match language {
        LanguageId::JavaScript | LanguageId::TypeScript => LanguageId::JavaScript,
        LanguageId::C | LanguageId::Cpp => LanguageId::C,
        LanguageId::Java | LanguageId::Kotlin => LanguageId::Java,
        other => other,
    }
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
        // Pass 1: collect definitions by compatible language family and name.
        let mut defs_by_name: HashMap<(LanguageId, &str), Vec<SymbolId>> = HashMap::new();
        let mut node_count = 0usize;
        for (path, file) in &index.files {
            for sym in &file.symbols {
                node_count += 1;
                defs_by_name
                    .entry((graph_language_family(file.language), sym.name.as_str()))
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
        let mut edge_evidence: HashMap<(SymbolId, SymbolId), Vec<CallEdgeEvidence>> =
            HashMap::new();
        for (path, file) in &index.files {
            let mut symbols_by_start: HashMap<u32, Vec<usize>> = HashMap::new();
            for (idx, sym) in file.symbols.iter().enumerate() {
                symbols_by_start
                    .entry(sym.line_range.0)
                    .or_default()
                    .push(idx);
            }
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
                // The stored enclosing index uses line ranges. A class and its
                // method can begin on the same line, making that index choose
                // the outer class. Resolve the tie using the exact call bytes.
                let caller_sym = symbols_by_start
                    .get(&caller_sym.line_range.0)
                    .and_then(|candidates| {
                        candidates
                            .iter()
                            .map(|&idx| &file.symbols[idx])
                            .filter(|sym| {
                                sym.byte_range.0 <= r.byte_range.0
                                    && r.byte_range.1 <= sym.byte_range.1
                            })
                            .min_by_key(|sym| (sym.byte_range.1 - sym.byte_range.0, sym.sort_order))
                    })
                    .unwrap_or(caller_sym);
                let caller = SymbolId {
                    path: path.clone(),
                    name: caller_sym.name.clone(),
                    kind: caller_sym.kind,
                };
                let language = graph_language_family(file.language);
                let Some(callees) = defs_by_name.get(&(language, r.name.as_str())) else {
                    continue;
                };
                // Ambiguity-scoped callee resolution: one def in the compatible
                // language family links directly; many defs require a unique
                // qualified match or the edge is dropped.
                let resolved: Option<(&SymbolId, ResolutionReason)> = match callees.as_slice() {
                    [only] => Some((only, ResolutionReason::UniqueName)),
                    many => resolve_ambiguous_callee(many, r.qualified_name.as_deref(), index),
                };
                let Some((callee, resolution)) = resolved else {
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
                edge_evidence
                    .entry((caller.clone(), callee.clone()))
                    .or_default()
                    .push(CallEdgeEvidence {
                        caller,
                        callee: callee.clone(),
                        call_site_path: path.clone(),
                        byte_range: r.byte_range,
                        line_range: r.line_range,
                        resolution,
                    });
            }
        }

        // Determinism: sort + dedup each adjacency list.
        let mut edge_count = 0usize;
        for callers in in_edges.values_mut() {
            callers.sort();
            callers.dedup();
            edge_count += callers.len();
        }
        for evidence in edge_evidence.values_mut() {
            evidence.sort_by(|a, b| {
                a.call_site_path
                    .cmp(&b.call_site_path)
                    .then(a.byte_range.cmp(&b.byte_range))
                    .then(a.line_range.cmp(&b.line_range))
                    .then(a.resolution.cmp(&b.resolution))
            });
            evidence.dedup();
        }

        Self {
            in_edges,
            edge_evidence,
            node_count,
            edge_count,
        }
    }

    fn canonical_edge_evidence(
        &self,
        caller: &SymbolId,
        callee: &SymbolId,
    ) -> Option<&CallEdgeEvidence> {
        self.edge_evidence
            .get(&(caller.clone(), callee.clone()))
            .and_then(|evidence| evidence.first())
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
/// call site's `qualified_name` immediate qualifier (e.g. `"a::run"` -> hint
/// `"a"`; `"Target::m"` -> hint `"Target"`). Returns `Some(def)` only when the
/// hint picks exactly ONE candidate; otherwise `None`, so the caller drops the
/// edge rather than fanning out to all N candidates.
///
/// Two disambiguation signals are tried, both gated by the same "matched more
/// than one -> None" discipline (safety over recall):
///
/// 1. **Module-stem match** (original): the hint == a candidate's file stem or a
///    path component recovers the common `mod::name` case.
/// 2. **Owner-name match** (019 recall-recovery): the hint == the target type of
///    a candidate's enclosing `impl` block recovers `Target::method` calls that
///    the module-stem match cannot. Computed from physical defs via
///    [`crate::live_index::enclosing_impl_owner`]; if two physical defs share that owner name
///    (twin `impl Target` in different files), the hint does NOT disambiguate
///    and we drop — mirroring the module-stem uniqueness guard.
///
/// ponytail: both matches are syntactic heuristics, not real name resolution;
/// the resolver at C-S2-001 supersedes them. Owner-name recovery is
/// Rust-`impl`-shaped (see [`crate::live_index::enclosing_impl_owner`]) — non-Rust grammars whose
/// containers aren't `SymbolKind::Impl` yield no owner and keep the drop path.
fn resolve_ambiguous_callee<'a>(
    candidates: &'a [SymbolId],
    qualified_name: Option<&str>,
    index: &LiveIndex,
) -> Option<(&'a SymbolId, ResolutionReason)> {
    // Immediate qualifier = the segment before the last `::` (last `::`
    // separates the qualifier from the called leaf name). No `::` -> no hint.
    let qn = qualified_name?;
    let module_hint = qn.rsplit_once("::").map(|(head, _)| head)?;
    // Use the innermost segment (e.g. `crate::a` -> `a`, `Target` -> `Target`).
    let hint = module_hint.rsplit("::").next().unwrap_or(module_hint);
    if hint.is_empty() {
        return None;
    }

    // Signal 1: module-stem match (file stem or path component == hint).
    let mut matched: Option<&SymbolId> = None;
    let mut stem_ambiguous = false;
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
                stem_ambiguous = true;
                break;
            }
            matched = Some(cand);
        }
    }
    if stem_ambiguous {
        return None; // hint matched more than one candidate path -> ambiguous
    }
    if matched.is_some() {
        return matched.map(|candidate| (candidate, ResolutionReason::QualifiedModule));
    }

    // Signal 2: owner-name match. For each physical def of the callee leaf name,
    // compute its enclosing-`impl` owner type and match it against the hint.
    // Keep the edge only when EXACTLY ONE physical def's owner == hint (the
    // uniqueness guard); a shared owner name (twin `impl Target`) -> drop.
    let leaf = &candidates.first()?.name;
    let mut owner_matched: Option<&SymbolId> = None;
    for cand in candidates {
        let Some(file) = index.files.get(&cand.path) else {
            continue;
        };
        // A candidate `SymbolId` collapses same-name/kind defs in one file, so
        // scan every physical def of the leaf name in this file and check each
        // one's owner independently (twin `impl` in the SAME file is caught).
        let mut file_owner_hits = 0usize;
        for phys in &file.symbols {
            if phys.name != *leaf || phys.kind != cand.kind {
                continue;
            }
            if crate::live_index::enclosing_impl_owner(&file.symbols, phys.line_range.0).as_deref()
                == Some(hint)
            {
                file_owner_hits += 1;
            }
        }
        if file_owner_hits == 0 {
            continue;
        }
        // >1 owner-matching def in one file, or a second file already matched
        // -> the hint can't disambiguate. Drop (soundness over recall).
        if file_owner_hits > 1 || owner_matched.is_some() {
            return None;
        }
        owner_matched = Some(cand);
    }
    owner_matched.map(|candidate| (candidate, ResolutionReason::QualifiedOwner))
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
    compute_impact_with_evidence(graph, changed, max_depth)
        .into_iter()
        .map(|entry| entry.node)
        .collect()
}

/// Compute the blast radius and one canonical shortest call chain for every
/// returned node. A single multi-source BFS visits each reachable graph node
/// once; sorted seeds and sorted inbound adjacency make equal-length choices
/// deterministic. The caller bounds depth, and the finite graph bounds nodes.
pub(crate) fn compute_impact_with_evidence(
    graph: &GraphProjection,
    changed: &[SymbolId],
    max_depth: u32,
) -> Vec<BlastNodeEvidence> {
    let mut starts = changed.to_vec();
    starts.sort();
    starts.dedup();

    let changed_set: HashSet<SymbolId> = changed.iter().cloned().collect();
    let mut visited: HashSet<SymbolId> = starts.iter().cloned().collect();
    let mut queue: VecDeque<(SymbolId, u32)> = starts.into_iter().map(|start| (start, 0)).collect();
    let mut parent: HashMap<SymbolId, (SymbolId, CallEdgeEvidence)> = HashMap::new();
    let mut best_hop: HashMap<SymbolId, u32> = HashMap::new();

    while let Some((node, depth)) = queue.pop_front() {
        if depth >= max_depth {
            continue;
        }
        let Some(callers) = graph.in_edges.get(&node) else {
            continue;
        };
        for caller in callers {
            if visited.contains(caller) {
                continue;
            }
            let Some(edge) = graph.canonical_edge_evidence(caller, &node) else {
                continue;
            };
            visited.insert(caller.clone());
            parent.insert(caller.clone(), (node.clone(), edge.clone()));
            let hop = depth + 1;
            queue.push_back((caller.clone(), hop));
            if !changed_set.contains(caller) {
                best_hop.insert(caller.clone(), hop);
            }
        }
    }

    let mut entries: Vec<BlastNodeEvidence> = best_hop
        .into_iter()
        .map(|(symbol, hop)| {
            let mut steps = Vec::new();
            let mut current = symbol.clone();
            while let Some((next, edge)) = parent.get(&current) {
                steps.push(edge.clone());
                current = next.clone();
            }
            steps.reverse();
            let risk = RiskTier::for_hop(hop, is_entry_point(&symbol));
            BlastNodeEvidence {
                node: BlastNode { symbol, hop, risk },
                steps,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        a.node
            .hop
            .cmp(&b.node.hop)
            .then_with(|| a.node.symbol.cmp(&b.node.symbol))
    });
    entries
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

    // 019 recall-recovery: an ambiguous callee `m` (2 defs, `impl Target` and
    // `impl Other`) called as `Target::m()` must edge ONLY to Target's `m`
    // (owner-name recovery), not fan out and not drop. The immediate qualifier
    // `Target` == the enclosing-impl owner of exactly ONE candidate def.
    #[test]
    fn ambiguous_call_recovers_edge_via_unique_owner_qualifier() {
        let graph = build_graph(&[
            (
                "a.rs",
                "pub struct Target;\nimpl Target {\n    pub fn m(&self) -> u32 { 1 }\n}\n",
            ),
            (
                "b.rs",
                "pub struct Other;\nimpl Other {\n    pub fn m(&self) -> u32 { 2 }\n}\n",
            ),
            ("caller.rs", "fn drive() -> u32 { Target::m(&Target) }\n"),
        ]);
        let drive = sym("caller.rs", "drive");
        // Target::m() is a method; its SymbolId kind is Method, not Function.
        let target_m = SymbolId {
            path: "a.rs".to_string(),
            name: "m".to_string(),
            kind: SymbolKind::Method,
        };
        let other_m = SymbolId {
            path: "b.rs".to_string(),
            name: "m".to_string(),
            kind: SymbolKind::Method,
        };
        let reaches_target = graph
            .inbound_bfs(&target_m, 2, 100)
            .iter()
            .any(|(id, _)| *id == drive);
        let reaches_other = graph
            .inbound_bfs(&other_m, 2, 100)
            .iter()
            .any(|(id, _)| *id == drive);
        assert!(
            reaches_target,
            "Target::m() must edge to Target's m (owner-name recovery)"
        );
        assert!(
            !reaches_other,
            "Target::m() must NOT edge to Other's m (wrong owner)"
        );
        let evidence = graph
            .edge_evidence
            .get(&(drive, target_m))
            .and_then(|evidence| evidence.first())
            .expect("owner-qualified edge retains its source evidence");
        assert_eq!(evidence.resolution, ResolutionReason::QualifiedOwner);
    }

    // 019 recall-recovery soundness: TWIN OWNERS. Two `impl Target` in distinct
    // files, each with `fn m`. A `Target::m()` call cannot disambiguate between
    // them (both owners are named "Target"), so the edge is DROPPED, not
    // fanned out. This is the uniqueness-guard case.
    #[test]
    fn ambiguous_call_with_twin_owner_qualifier_drops_edge() {
        let graph = build_graph(&[
            (
                "a.rs",
                "pub struct Target;\nimpl Target {\n    pub fn m(&self) -> u32 { 1 }\n}\n",
            ),
            (
                "c.rs",
                "impl Target {\n    pub fn m(&self) -> u32 { 2 }\n}\n",
            ),
            ("caller.rs", "fn drive() -> u32 { Target::m(&Target) }\n"),
        ]);
        let drive = sym("caller.rs", "drive");
        let a_m = SymbolId {
            path: "a.rs".to_string(),
            name: "m".to_string(),
            kind: SymbolKind::Method,
        };
        let c_m = SymbolId {
            path: "c.rs".to_string(),
            name: "m".to_string(),
            kind: SymbolKind::Method,
        };
        let reaches_a = graph
            .inbound_bfs(&a_m, 2, 100)
            .iter()
            .any(|(id, _)| *id == drive);
        let reaches_c = graph
            .inbound_bfs(&c_m, 2, 100)
            .iter()
            .any(|(id, _)| *id == drive);
        let hits = usize::from(reaches_a) + usize::from(reaches_c);
        assert_eq!(
            hits, 0,
            "twin `impl Target` owners cannot disambiguate Target::m() -> \
             edge must be dropped (reaches_a={reaches_a} reaches_c={reaches_c})"
        );
    }

    #[test]
    fn same_name_definitions_resolve_within_compatible_language_family() {
        let graph = build_graph(&[
            ("js.js", "function read_to_string() { return 0; }\n"),
            ("lib.rs", "pub fn read_to_string() -> u32 { 1 }\n"),
            ("caller.rs", "fn drive() -> u32 { read_to_string() }\n"),
        ]);
        let drive = sym("caller.rs", "drive");
        let rust_target = sym("lib.rs", "read_to_string");
        let js_target = sym("js.js", "read_to_string");

        assert!(
            graph
                .inbound_bfs(&rust_target, 1, 100)
                .iter()
                .any(|(id, _)| *id == drive),
            "the Rust caller should resolve to the same-named Rust definition"
        );
        assert!(
            !graph
                .inbound_bfs(&js_target, 1, 100)
                .iter()
                .any(|(id, _)| *id == drive),
            "an unrelated JavaScript definition must not receive the Rust caller edge"
        );
    }

    #[test]
    fn rust_std_call_does_not_resolve_to_unrelated_javascript_definition() {
        let graph = build_graph(&[
            ("js.js", "function read_to_string() { return 0; }\n"),
            (
                "caller.rs",
                "fn load() { std::fs::read_to_string(\"input.txt\"); }\n",
            ),
        ]);
        let caller = sym("caller.rs", "load");
        let js_target = sym("js.js", "read_to_string");

        assert!(
            !graph
                .inbound_bfs(&js_target, 1, 100)
                .iter()
                .any(|(id, _)| *id == caller),
            "Rust's std::fs::read_to_string call must not change JavaScript impact"
        );
    }

    #[test]
    fn javascript_and_typescript_remain_a_compatible_graph_family() {
        let graph = build_graph(&[
            ("library.js", "export function shared() { return 7; }\n"),
            (
                "caller.ts",
                "function drive(): number { return shared(); }\n",
            ),
        ]);
        let caller = sym("caller.ts", "drive");
        let target = sym("library.js", "shared");

        assert!(
            graph
                .inbound_bfs(&target, 1, 100)
                .iter()
                .any(|(id, _)| *id == caller),
            "TypeScript callers should continue to connect to JavaScript definitions"
        );
    }

    #[test]
    fn c_and_cpp_remain_a_compatible_graph_family() {
        let graph = build_graph(&[
            ("library.c", "int shared(void) { return 7; }\n"),
            ("caller.cpp", "int drive() { return shared(); }\n"),
        ]);
        let caller = sym("caller.cpp", "drive");
        let target = sym("library.c", "shared");

        assert!(
            graph
                .inbound_bfs(&target, 1, 100)
                .iter()
                .any(|(id, _)| *id == caller),
            "C++ callers should continue to connect to C definitions"
        );
    }

    #[test]
    fn cpp_qualified_call_keeps_its_disambiguating_evidence() {
        let graph = build_graph(&[
            ("Foo.cpp", "int create() { return 1; }\n"),
            ("Other.cpp", "int create() { return 2; }\n"),
            ("caller.cpp", "int drive() { return Foo::create(); }\n"),
        ]);
        let caller = sym("caller.cpp", "drive");
        let target = sym("Foo.cpp", "create");
        let other = sym("Other.cpp", "create");

        assert!(
            graph
                .inbound_bfs(&target, 1, 100)
                .iter()
                .any(|(id, _)| *id == caller),
            "Foo::create should resolve to the Foo.cpp definition"
        );
        assert!(
            !graph
                .inbound_bfs(&other, 1, 100)
                .iter()
                .any(|(id, _)| *id == caller),
            "the same-named definition in Other.cpp should not receive the call"
        );
        let evidence = graph
            .edge_evidence
            .get(&(caller, target))
            .and_then(|entries| entries.first())
            .expect("qualified C++ edge retains source evidence");
        assert_eq!(evidence.resolution, ResolutionReason::QualifiedModule);
        assert!(evidence.byte_range.0 < evidence.byte_range.1);
    }

    #[test]
    fn csharp_calls_stay_in_the_csharp_namespace() {
        let graph = build_graph(&[
            (
                "Target.cs",
                "public class Target {\n    public void Read() {}\n}\n",
            ),
            (
                "Caller.cs",
                "public class Caller {\n    public void Run() {\n        new Target().Read();\n    }\n}\n",
            ),
            ("rust.rs", "fn Read() {}\nfn rust_caller() { Read(); }\n"),
            (
                "helper.js",
                "function Read() {}\nfunction js_caller() { Read(); }\n",
            ),
        ]);
        let csharp_caller = SymbolId {
            path: "Caller.cs".to_string(),
            name: "Run".to_string(),
            kind: SymbolKind::Method,
        };
        let csharp_target = SymbolId {
            path: "Target.cs".to_string(),
            name: "Read".to_string(),
            kind: SymbolKind::Method,
        };
        let rust_caller = sym("rust.rs", "rust_caller");
        let js_caller = sym("helper.js", "js_caller");

        let inbound = graph.inbound_bfs(&csharp_target, 1, 100);
        assert!(
            inbound.iter().any(|(id, _)| *id == csharp_caller),
            "C# callers should resolve to their C# definition"
        );
        assert!(
            !inbound
                .iter()
                .any(|(id, _)| *id == rust_caller || *id == js_caller),
            "same-named Rust and JavaScript callers must stay in their own namespaces"
        );
    }

    #[test]
    fn same_line_csharp_call_is_attributed_to_the_enclosing_method() {
        let caller_source = "public class Caller { public void Run() { new Target().Read(); } }\n";
        let graph = build_graph(&[
            (
                "Target.cs",
                "public class Target { public void Read() {} }\n",
            ),
            ("Caller.cs", caller_source),
        ]);
        let caller = SymbolId {
            path: "Caller.cs".to_string(),
            name: "Run".to_string(),
            kind: SymbolKind::Method,
        };
        let callee = SymbolId {
            path: "Target.cs".to_string(),
            name: "Read".to_string(),
            kind: SymbolKind::Method,
        };

        assert!(
            graph
                .inbound_bfs(&callee, 1, 100)
                .iter()
                .any(|(id, _)| *id == caller),
            "a same-line method call must belong to Run, not its enclosing class"
        );
        let evidence = graph
            .edge_evidence
            .get(&(caller, callee))
            .and_then(|entries| entries.first())
            .expect("same-line method edge retains the call-site span");
        let (start, end) = evidence.byte_range;
        assert_eq!(&caller_source[start as usize..end as usize], "Read");
    }

    #[test]
    fn java_and_kotlin_remain_a_compatible_graph_family_in_both_directions() {
        let graph = build_graph(&[
            (
                "JavaLibrary.java",
                "public class JavaLibrary {\n    public static int fromJava() { return 1; }\n}\n",
            ),
            (
                "KotlinApi.kt",
                "class KotlinApi {\n    fun fromKotlin(): Int = 2\n}\n",
            ),
            (
                "KotlinCaller.kt",
                "import JavaLibrary.fromJava\nfun callJava(): Int = fromJava()\n",
            ),
            (
                "JavaCaller.java",
                "public class JavaCaller {\n    public static int callKotlin() {\n        return new KotlinApi().fromKotlin();\n    }\n}\n",
            ),
        ]);
        let kotlin_caller = sym("KotlinCaller.kt", "callJava");
        let java_target = SymbolId {
            path: "JavaLibrary.java".to_string(),
            name: "fromJava".to_string(),
            kind: SymbolKind::Method,
        };
        let java_caller = SymbolId {
            path: "JavaCaller.java".to_string(),
            name: "callKotlin".to_string(),
            kind: SymbolKind::Method,
        };
        let kotlin_target = SymbolId {
            path: "KotlinApi.kt".to_string(),
            name: "fromKotlin".to_string(),
            kind: SymbolKind::Function,
        };

        assert!(
            graph
                .inbound_bfs(&java_target, 1, 100)
                .iter()
                .any(|(id, _)| *id == kotlin_caller),
            "Kotlin callers should continue to connect to Java definitions"
        );
        assert!(
            graph
                .inbound_bfs(&kotlin_target, 1, 100)
                .iter()
                .any(|(id, _)| *id == java_caller),
            "Java callers should continue to connect to Kotlin definitions"
        );
    }

    #[test]
    fn graph_edges_keep_each_call_site_with_resolution_reason() {
        let graph = build_graph(&[
            ("lib.rs", "pub fn shared() {}\n"),
            (
                "caller.rs",
                "fn drive() {\n    shared();\n    shared();\n}\n",
            ),
        ]);
        let caller = sym("caller.rs", "drive");
        let callee = sym("lib.rs", "shared");
        let evidence = graph
            .edge_evidence
            .get(&(caller, callee))
            .expect("resolved call edge retains evidence");

        assert_eq!(evidence.len(), 2, "distinct call sites remain distinct");
        assert!(evidence.windows(2).all(|pair| {
            pair[0].call_site_path < pair[1].call_site_path
                || pair[0].byte_range < pair[1].byte_range
        }));
        assert!(evidence.iter().all(|edge| {
            edge.call_site_path == "caller.rs"
                && edge.byte_range.0 < edge.byte_range.1
                && edge.resolution == ResolutionReason::UniqueName
        }));
        assert_eq!(
            evidence
                .iter()
                .map(|edge| edge.line_range.0)
                .collect::<Vec<_>>(),
            vec![1, 2],
            "line spans preserve zero-based source rows"
        );
    }

    #[test]
    fn qualified_module_edge_records_resolution_and_source_site() {
        let graph = build_graph(&[
            ("a.rs", "pub fn run() {}\n"),
            ("b.rs", "pub fn run() {}\n"),
            ("caller.rs", "fn drive() {\n    a::run();\n}\n"),
        ]);
        let caller = sym("caller.rs", "drive");
        let callee = sym("a.rs", "run");
        let evidence = graph
            .edge_evidence
            .get(&(caller, callee))
            .and_then(|evidence| evidence.first())
            .expect("module qualifier selects one same-family definition");

        assert_eq!(evidence.call_site_path, "caller.rs");
        assert_eq!(evidence.line_range.0, 1);
        assert_eq!(evidence.resolution, ResolutionReason::QualifiedModule);
    }

    #[test]
    fn impact_explanation_is_shortest_bounded_and_stable() {
        let files = [
            ("lib.rs", "pub fn core() {}\n"),
            ("a.rs", "pub fn left() { core(); }\n"),
            ("z.rs", "pub fn right() { core(); }\n"),
            ("main.rs", "fn main() { z::right(); a::left(); }\n"),
        ];
        let graph = build_graph(&files);
        let reversed = build_graph(&[files[3], files[2], files[1], files[0]]);
        let changed = [sym("lib.rs", "core")];
        let impact = compute_impact_with_evidence(&graph, &changed, 3);
        let reversed_impact = compute_impact_with_evidence(&reversed, &changed, 3);
        let main = impact
            .iter()
            .find(|entry| entry.node.symbol.name == "main")
            .expect("main is reached through either direct caller");
        let reversed_main = reversed_impact
            .iter()
            .find(|entry| entry.node.symbol.name == "main")
            .expect("reversed fixture still reaches main");

        assert_eq!(main.node.hop, 2);
        assert_eq!(main.steps.len(), 2);
        assert_eq!(main.steps[0].caller.path, "a.rs");
        assert_eq!(main.steps[0].callee.path, "lib.rs");
        assert_eq!(main.steps[1].caller.path, "main.rs");
        assert_eq!(main.steps[1].callee.path, "a.rs");
        assert_eq!(main.steps, reversed_main.steps);
        assert!(
            compute_impact_with_evidence(&graph, &changed, 1)
                .iter()
                .all(|entry| entry.node.symbol.name != "main"),
            "maximum depth bounds both impact rows and explanations"
        );
    }
}
