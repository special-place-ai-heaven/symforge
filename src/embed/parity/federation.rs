//! Cross-source reads over a host-supplied, explicitly admitted source set.
use super::session::QuerySession;
use super::{QueryClaim, QueryPolicy, QueryRefusal, QueryUsage};
use crate::embed::EmbeddedSourceHandle;

pub use crate::embed::lifecycle::embed_federation::query_sources;

/// This list is the complete authority boundary for federation. Construct it
/// from host-granted handles; selectors never discover or open another root.
pub struct AdmittedQuerySource<'a> {
    pub label: &'a str,
    pub handle: &'a EmbeddedSourceHandle,
    pub session: Option<&'a QuerySession>,
    pub policy: QueryPolicy,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum SourceSelection {
    One(String),
    Many(Vec<String>),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FederationRefusalKind {
    InvalidRequest,
    UnknownSource,
    BudgetTooSmall,
    Cancelled,
    DeadlineExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FederationRefusal {
    pub kind: FederationRefusalKind,
    pub canonical_argument_hash: String,
}
impl std::fmt::Display for FederationRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "federated query refused: {:?}", self.kind)
    }
}
impl std::error::Error for FederationRefusal {}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ResolvedQuerySource {
    pub label: String,
    pub binding_identity: String,
    pub producing_runtime_identity: String,
    pub aliases: Vec<String>,
}
#[derive(Debug)]
pub enum FederatedSourceOutcome {
    Claim(QueryClaim),
    Refusal(QueryRefusal),
}

#[derive(Debug)]
pub struct FederatedSourceResult {
    pub source: ResolvedQuerySource,
    /// SHA-256 of the adapted semantic request before its per-source budget and
    /// trusted policy are incorporated into the child operation receipt.
    pub normalized_request_hash: String,
    pub outcome: FederatedSourceOutcome,
    /// Trusted policy used for this source, including any derived-state preparation grant.
    pub policy: QueryPolicy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FederationStop {
    Cancelled,
    DeadlineExceeded,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FederationCompletion {
    Complete,
    Truncated,
    Stopped(FederationStop),
}
#[derive(Debug)]
pub struct FederatedQueryClaim {
    pub canonical_argument_hash: String,
    pub resolved_sources: Vec<ResolvedQuerySource>,
    pub results: Vec<FederatedSourceResult>,
    pub usage: QueryUsage,
    pub truncated: bool,
    pub omitted_sources: u32,
    pub completion: FederationCompletion,
}
