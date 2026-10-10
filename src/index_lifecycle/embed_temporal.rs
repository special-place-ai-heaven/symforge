//! Git temporal data (churn, ownership, co-change) for embedded queries.
//! The history producer and the aggregator are MCP's own
//! (`GitTemporalIndex::compute_from_repo`, the walk behind
//! `spawn_git_temporal_computation`); this lane only decides when to run them.
//! It runs over the source root's own repository, only when the host permits
//! derived-state preparation, under the query's cancellation and deadline.

use std::sync::{Arc, Mutex};

use crate::embed::parity::{QueryPolicy, QueryRefusalKind};
use crate::live_index::git_temporal::{GitTemporalIndex, GitTemporalState};

use super::embed_query::{Budget, EmbeddedQuerySnapshot};

/// The last index a binding computed and the HEAD commit its walk started at.
/// ponytail: one entry per binding keyed by HEAD only, so `days_ago` ages
/// until HEAD moves; key on the publication too if that drift ever matters.
pub(super) type TemporalCache = Mutex<Option<(git2::Oid, Arc<GitTemporalIndex>)>>;

/// The temporal index a query renders from. A publication that already
/// carries Ready or Unavailable data (a restored snapshot) is used as is.
/// Otherwise the history is walked when policy permits; when it does not, the
/// state is reported Unavailable with the reason, never as still loading,
/// because no embedded computation is pending.
pub(super) fn temporal(
    snapshot: &EmbeddedQuerySnapshot,
    policy: QueryPolicy,
    budget: &Budget,
) -> Result<Arc<GitTemporalIndex>, QueryRefusalKind> {
    let published = &snapshot.generation.code_signals.temporal;
    if matches!(
        published.state,
        GitTemporalState::Ready | GitTemporalState::Unavailable(_)
    ) {
        return Ok(Arc::clone(published));
    }
    if !policy.allow_derived_state_preparation {
        return Ok(Arc::new(GitTemporalIndex::unavailable(
            "host policy does not permit derived-state preparation".to_string(),
        )));
    }
    let Some(repo) = super::embed_changes::open_repository(&snapshot.root) else {
        return Ok(Arc::new(GitTemporalIndex::unavailable(
            "the source root is not a git work tree".to_string(),
        )));
    };
    let head = repo.resolve_ref_commit("HEAD");
    if let Some(head) = head
        && let Some((cached_head, cached)) = snapshot
            .temporal_cache
            .lock()
            .expect("embedded temporal cache mutex")
            .as_ref()
        && *cached_head == head
    {
        return Ok(Arc::clone(cached));
    }
    let computed = Arc::new(GitTemporalIndex::compute_from_repo(&repo, &mut || {
        budget.check().is_err()
    }));
    budget.check()?;
    if let Some(head) = head {
        *snapshot
            .temporal_cache
            .lock()
            .expect("embedded temporal cache mutex") = Some((head, Arc::clone(&computed)));
    }
    Ok(computed)
}
