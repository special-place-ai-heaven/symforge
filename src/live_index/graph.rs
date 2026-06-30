//! SP-0A spike — in-memory call-graph projection + inbound BFS (Program 015).
//!
//! ponytail: throwaway-grade falsifier code. Edges are **name-based syntactic**
//! Call references (no resolver yet), confidence implicitly 1.0. The real
//! `GraphProjection` (resolver-weighted edges, generation fence, incremental
//! patch) lands at C-S2-001. This module exists only to measure inbound BFS
//! latency on the symforge index and confirm/falsify p95 < 200ms at depth 5.
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
    /// symbol definition; edges are caller -> callee for each `Call` reference,
    /// where the callee is every same-name definition in the repo (syntactic,
    /// name-based — the over-approximation the v1 resolver will later narrow).
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
                for callee in callees {
                    if *callee == caller {
                        continue; // skip self-recursion edges
                    }
                    in_edges
                        .entry(callee.clone())
                        .or_default()
                        .push(caller.clone());
                }
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
    /// `max_depth` hops, capped at `max_nodes` results. Deterministic order.
    /// Empty graph or unknown `start` -> empty result, never panics.
    pub fn inbound_bfs(&self, start: &SymbolId, max_depth: u32, max_nodes: usize) -> Vec<SymbolId> {
        let mut visited: HashSet<SymbolId> = HashSet::new();
        visited.insert(start.clone());
        let mut queue: VecDeque<(SymbolId, u32)> = VecDeque::new();
        queue.push_back((start.clone(), 0));
        let mut results: Vec<SymbolId> = Vec::new();

        while let Some((node, depth)) = queue.pop_front() {
            if results.len() >= max_nodes {
                break;
            }
            if depth > 0 {
                results.push(node.clone());
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
