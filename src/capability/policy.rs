use std::fmt;
use std::str::FromStr;

/// Controls how symbol/file access frecency signals are collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrecencyCollectionPolicy {
    Session,
    Persistent,
    Disabled,
}

impl fmt::Display for FrecencyCollectionPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Session => "session",
            Self::Persistent => "persistent",
            Self::Disabled => "disabled",
        };
        f.write_str(value)
    }
}

impl FromStr for FrecencyCollectionPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "session" => Ok(Self::Session),
            "persistent" => Ok(Self::Persistent),
            "disabled" => Ok(Self::Disabled),
            other => Err(format!(
                "unknown frecency collection policy {other:?}; expected 'session', 'persistent', or 'disabled'"
            )),
        }
    }
}

/// Controls when the co-change coupling store is prepared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CouplingPreparePolicy {
    LazyOnRequest,
    WarmOnStart,
    Disabled,
}

impl fmt::Display for CouplingPreparePolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::LazyOnRequest => "lazy on request",
            Self::WarmOnStart => "warm on start",
            Self::Disabled => "disabled",
        };
        f.write_str(value)
    }
}

impl FromStr for CouplingPreparePolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "lazy on request" => Ok(Self::LazyOnRequest),
            "warm on start" => Ok(Self::WarmOnStart),
            "disabled" => Ok(Self::Disabled),
            other => Err(format!(
                "unknown coupling prepare policy {other:?}; expected 'lazy on request', 'warm on start', or 'disabled'"
            )),
        }
    }
}

/// Controls whether requests may route to an explicit call-time worktree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorktreeRoutingPolicy {
    ExplicitCallTime,
    Disabled,
}

impl fmt::Display for WorktreeRoutingPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::ExplicitCallTime => "explicit call-time",
            Self::Disabled => "disabled",
        };
        f.write_str(value)
    }
}

impl FromStr for WorktreeRoutingPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "explicit call-time" => Ok(Self::ExplicitCallTime),
            "disabled" => Ok(Self::Disabled),
            other => Err(format!(
                "unknown worktree routing policy {other:?}; expected 'explicit call-time' or 'disabled'"
            )),
        }
    }
}

/// Controls when ranking score breakdowns and diagnostics are emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RankingDiagnosticsPolicy {
    CallTimeExplain,
    DefaultOn,
    Disabled,
}

impl fmt::Display for RankingDiagnosticsPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::CallTimeExplain => "call-time explain",
            Self::DefaultOn => "default on",
            Self::Disabled => "disabled",
        };
        f.write_str(value)
    }
}

impl FromStr for RankingDiagnosticsPolicy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "call-time explain" => Ok(Self::CallTimeExplain),
            "default on" => Ok(Self::DefaultOn),
            "disabled" => Ok(Self::Disabled),
            other => Err(format!(
                "unknown ranking diagnostics policy {other:?}; expected 'call-time explain', 'default on', or 'disabled'"
            )),
        }
    }
}

/// Aggregated operator policy governing optional ranking and routing capabilities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CapabilityPolicy {
    pub frecency_collection: FrecencyCollectionPolicy,
    pub coupling_prepare: CouplingPreparePolicy,
    pub worktree_routing: WorktreeRoutingPolicy,
    pub ranking_diagnostics: RankingDiagnosticsPolicy,
}

impl CapabilityPolicy {
    /// Constructs a capability policy with explicit settings for all four domains.
    pub const fn new(
        frecency_collection: FrecencyCollectionPolicy,
        coupling_prepare: CouplingPreparePolicy,
        worktree_routing: WorktreeRoutingPolicy,
        ranking_diagnostics: RankingDiagnosticsPolicy,
    ) -> Self {
        Self {
            frecency_collection,
            coupling_prepare,
            worktree_routing,
            ranking_diagnostics,
        }
    }

    /// Returns a policy with every optional capability disabled.
    pub const fn disabled() -> Self {
        Self::new(
            FrecencyCollectionPolicy::Disabled,
            CouplingPreparePolicy::Disabled,
            WorktreeRoutingPolicy::Disabled,
            RankingDiagnosticsPolicy::Disabled,
        )
    }
}

impl Default for CapabilityPolicy {
    fn default() -> Self {
        Self {
            frecency_collection: FrecencyCollectionPolicy::Session,
            coupling_prepare: CouplingPreparePolicy::LazyOnRequest,
            worktree_routing: WorktreeRoutingPolicy::ExplicitCallTime,
            ranking_diagnostics: RankingDiagnosticsPolicy::CallTimeExplain,
        }
    }
}
