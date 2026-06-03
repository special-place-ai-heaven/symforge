// Server-only integration test: depends on a `#[cfg(feature = "server")]`
// module (protocol/daemon/cli/sidecar/watcher/analytics). Gating the whole
// file keeps `--no-default-features --features embed --all-targets` compiling.
#![cfg(feature = "server")]

use symforge::capability::{
    CapabilityCost, CapabilityEvidence, CapabilityFreshness, CapabilityName, CapabilityPolicy,
    CapabilitySafety, CapabilityStatus, CouplingPreparePolicy, FrecencyCollectionPolicy,
    RankingDiagnosticsPolicy, WorktreeRoutingPolicy,
};
use symforge::protocol::format::capability_evidence_line;

#[test]
fn default_policy_is_deterministic_and_call_time_first() {
    let policy = CapabilityPolicy::default();

    assert_eq!(
        policy.frecency_collection,
        FrecencyCollectionPolicy::Session
    );
    assert_eq!(
        policy.coupling_prepare,
        CouplingPreparePolicy::LazyOnRequest
    );
    assert_eq!(
        policy.worktree_routing,
        WorktreeRoutingPolicy::ExplicitCallTime
    );
    assert_eq!(
        policy.ranking_diagnostics,
        RankingDiagnosticsPolicy::CallTimeExplain
    );
}

#[test]
fn policy_can_express_operator_defaults_without_evidence_reading_env() {
    let policy = CapabilityPolicy {
        frecency_collection: FrecencyCollectionPolicy::Persistent,
        coupling_prepare: CouplingPreparePolicy::WarmOnStart,
        worktree_routing: WorktreeRoutingPolicy::Disabled,
        ranking_diagnostics: RankingDiagnosticsPolicy::DefaultOn,
    };

    assert_eq!(policy.frecency_collection.to_string(), "persistent");
    assert_eq!(policy.coupling_prepare.to_string(), "warm on start");
    assert_eq!(policy.worktree_routing.to_string(), "disabled");
    assert_eq!(policy.ranking_diagnostics.to_string(), "default on");

    let disabled = CapabilityPolicy::disabled();
    assert_eq!(
        disabled.frecency_collection,
        FrecencyCollectionPolicy::Disabled
    );
    assert_eq!(disabled.coupling_prepare, CouplingPreparePolicy::Disabled);
    assert_eq!(disabled.worktree_routing, WorktreeRoutingPolicy::Disabled);
    assert_eq!(
        disabled.ranking_diagnostics,
        RankingDiagnosticsPolicy::Disabled
    );
}

#[test]
fn evidence_rendering_is_stable_for_call_time_states() {
    let cases = [
        (
            CapabilityEvidence::new(CapabilityName::FrecencyRanking, CapabilityStatus::Applied)
                .with_freshness(CapabilityFreshness::Current)
                .with_cost(CapabilityCost::Low)
                .with_safety(CapabilitySafety::ReadOnly)
                .with_detail("frecency history used"),
            "Capability: frecency ranking applied - frecency history used.",
        ),
        (
            CapabilityEvidence::new(CapabilityName::FrecencyRanking, CapabilityStatus::Ready)
                .with_detail("session collection available"),
            "Capability: frecency ranking ready - session collection available.",
        ),
        (
            CapabilityEvidence::new(CapabilityName::CoChangeRanking, CapabilityStatus::Preparing)
                .with_freshness(CapabilityFreshness::Unknown)
                .with_cost(CapabilityCost::Bounded)
                .with_safety(CapabilitySafety::ReadOnly)
                .with_detail("coupling store warming; path ranking returned"),
            "Capability: co-change ranking preparing - coupling store warming; path ranking returned.",
        ),
        (
            CapabilityEvidence::new(
                CapabilityName::WorktreeRouting,
                CapabilityStatus::Unavailable,
            )
            .with_safety(CapabilitySafety::WriteRequiresConsent)
            .with_detail("working_directory did not match a known worktree"),
            "Capability: worktree routing unavailable - working_directory did not match a known worktree.",
        ),
        (
            CapabilityEvidence::new(
                CapabilityName::WorktreeRouting,
                CapabilityStatus::DisabledByPolicy,
            )
            .with_safety(CapabilitySafety::WriteRequiresConsent)
            .with_detail("operator disabled write rerouting"),
            "Capability: worktree routing disabled by policy - operator disabled write rerouting.",
        ),
        (
            CapabilityEvidence::new(CapabilityName::RankingDiagnostics, CapabilityStatus::Stale)
                .with_freshness(CapabilityFreshness::Stale)
                .with_detail("score breakdown is older than the current index generation"),
            "Capability: ranking diagnostics stale - score breakdown is older than the current index generation.",
        ),
        (
            CapabilityEvidence::new(
                CapabilityName::CoChangeRanking,
                CapabilityStatus::FallbackUsed,
            )
            .with_detail("path ranking returned"),
            "Capability: co-change ranking fallback used - path ranking returned.",
        ),
    ];

    for (evidence, expected) in cases {
        assert_eq!(capability_evidence_line(&evidence), expected);
    }
}
