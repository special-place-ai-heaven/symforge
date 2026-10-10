//! Warm embedded startup against one admitted source and state binding.

use std::collections::HashMap;
use std::path::Path;

use crate::domain::StatePlacement;
use crate::live_index::persist;
use crate::live_index::store::{SharedIndex, SharedIndexHandle};

use super::activation::ProjectSourceAuthority;
use super::embedded::AdmittedStateAnchor;

pub(super) struct RestoredEmbeddedSeed {
    pub(super) index: SharedIndex,
    pub(super) mtimes: HashMap<String, u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EmbedRestoreRefusal {
    SourceMoved,
    SourceReadRefused,
    StateMoved,
}

/// The host protects the selected state placement while SQLite and snapshot
/// files use the existing path-based persistence layer. The reopened anchor
/// fences the selected state child before and after that trusted operation;
/// source-byte proof always uses the retained original source capability.
pub(super) fn load_admitted_snapshot(
    root: &Path,
    placement: &StatePlacement,
    state_anchor: Option<&AdmittedStateAnchor>,
    authority: &ProjectSourceAuthority,
) -> Result<Option<RestoredEmbeddedSeed>, EmbedRestoreRefusal> {
    let dismissal_digest = || {
        authority
            .with_source_anchor_read(|lease| {
                super::physical_root::read_regular_beneath(
                    lease,
                    Path::new(".symforge/secret-dismissals.json"),
                    crate::knowledge::secret_dismissals::DISMISSAL_STORE_MAX_BYTES as usize,
                )
            })
            .map(|bytes| bytes.map_or_else(String::new, |bytes| crate::hash::digest_hex(&bytes)))
            .map_err(|_| EmbedRestoreRefusal::SourceMoved)
    };
    authority
        .verify_physical_root_anchor()
        .map_err(|_| EmbedRestoreRefusal::SourceMoved)?;
    let before_dismissals = dismissal_digest()?;
    let state_lease = match placement {
        StatePlacement::MemoryOnly { .. } => return Ok(None),
        StatePlacement::ProjectLocal { .. } | StatePlacement::UserLocal { .. } => {
            let anchor = state_anchor.ok_or(EmbedRestoreRefusal::StateMoved)?;
            Some(
                anchor
                    .lease()
                    .reopened_matching_stable_key(anchor.stable_key())
                    .map_err(|_| EmbedRestoreRefusal::StateMoved)?,
            )
        }
    };
    let snapshot =
        persist::load_snapshot_with_admitted_dismissals(root, placement, &before_dismissals);
    if state_lease
        .as_ref()
        .and_then(|lease| lease.opened_stable_key())
        != state_anchor.map(AdmittedStateAnchor::stable_key)
    {
        return Err(EmbedRestoreRefusal::StateMoved);
    }
    authority
        .verify_physical_root_anchor()
        .map_err(|_| EmbedRestoreRefusal::SourceMoved)?;
    if dismissal_digest()? != before_dismissals {
        return Err(EmbedRestoreRefusal::SourceMoved);
    }
    let Some(snapshot) = snapshot else {
        return Ok(None);
    };
    let mtimes = snapshot
        .files
        .iter()
        .map(|(path, file)| (path.clone(), file.mtime_secs))
        .collect();
    let (index, code_signals) =
        persist::snapshot_to_live_index_with_code_signals_bound(snapshot, root, authority)
            .map_err(|refusal| match refusal {
                persist::BoundRestoreRefusal::SourceMoved => EmbedRestoreRefusal::SourceMoved,
                persist::BoundRestoreRefusal::SourceReadRefused => {
                    EmbedRestoreRefusal::SourceReadRefused
                }
            })?;
    let index = SharedIndexHandle::shared_restored_for_state_placement_with_code_signals(
        index,
        root,
        placement,
        code_signals,
    );
    Ok(Some(RestoredEmbeddedSeed { index, mtimes }))
}
