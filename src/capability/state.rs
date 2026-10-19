use std::fmt;

/// Identifies an optional ranking or routing capability reported to callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityName {
    FrecencyRanking,
    CoChangeRanking,
    WorktreeRouting,
    RankingDiagnostics,
}

impl fmt::Display for CapabilityName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::FrecencyRanking => "frecency ranking",
            Self::CoChangeRanking => "co-change ranking",
            Self::WorktreeRouting => "worktree routing",
            Self::RankingDiagnostics => "ranking diagnostics",
        };
        f.write_str(value)
    }
}

/// Call-time execution status of a capability for a specific operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityStatus {
    Applied,
    Ready,
    Preparing,
    Unavailable,
    DisabledByPolicy,
    FallbackUsed,
    /// The capability's output is stale at call time relative to the current index state.
    Stale,
}

impl fmt::Display for CapabilityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Applied => "applied",
            Self::Ready => "ready",
            Self::Preparing => "preparing",
            Self::Unavailable => "unavailable",
            Self::DisabledByPolicy => "disabled by policy",
            Self::FallbackUsed => "fallback used",
            Self::Stale => "stale",
        };
        f.write_str(value)
    }
}

/// Age and readiness of the underlying backing store or index feeding a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityFreshness {
    Current,
    Empty,
    /// The underlying backing store or index age is behind the current generation.
    Stale,
    Unknown,
}

impl fmt::Display for CapabilityFreshness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Current => "current",
            Self::Empty => "empty",
            Self::Stale => "stale",
            Self::Unknown => "unknown",
        };
        f.write_str(value)
    }
}

/// Relative runtime cost class of exercising a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilityCost {
    Free,
    Low,
    Bounded,
    Expensive,
}

impl fmt::Display for CapabilityCost {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Free => "free",
            Self::Low => "low",
            Self::Bounded => "bounded",
            Self::Expensive => "expensive",
        };
        f.write_str(value)
    }
}

/// Mutation and authorization boundary required to exercise a capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CapabilitySafety {
    ReadOnly,
    WriteRequiresConsent,
    OperatorDiagnostics,
}

impl fmt::Display for CapabilitySafety {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::ReadOnly => "read-only",
            Self::WriteRequiresConsent => "write requires consent",
            Self::OperatorDiagnostics => "operator diagnostics",
        };
        f.write_str(value)
    }
}

/// Call-time evidence describing a capability's state, freshness, cost, and safety.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityEvidence {
    pub capability: CapabilityName,
    pub status: CapabilityStatus,
    pub freshness: CapabilityFreshness,
    pub cost: CapabilityCost,
    pub safety: CapabilitySafety,
    pub detail: Option<String>,
}

impl CapabilityEvidence {
    pub fn new(
        capability: CapabilityName,
        status: CapabilityStatus,
        cost: CapabilityCost,
        safety: CapabilitySafety,
    ) -> Self {
        Self {
            capability,
            status,
            freshness: CapabilityFreshness::Unknown,
            cost,
            safety,
            detail: None,
        }
    }

    pub fn with_freshness(mut self, freshness: CapabilityFreshness) -> Self {
        self.freshness = freshness;
        self
    }

    pub fn with_cost(mut self, cost: CapabilityCost) -> Self {
        self.cost = cost;
        self
    }

    pub fn with_safety(mut self, safety: CapabilitySafety) -> Self {
        self.safety = safety;
        self
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Compares the structured capability state fields (`capability`, `status`,
    /// `freshness`, `cost`, `safety`), ignoring free-form `detail`.
    pub fn same_capability_state(&self, other: &Self) -> bool {
        (
            self.capability,
            self.status,
            self.freshness,
            self.cost,
            self.safety,
        ) == (
            other.capability,
            other.status,
            other.freshness,
            other.cost,
            other.safety,
        )
    }
}
