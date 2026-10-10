//! Actionable refusal metadata (`_meta["symforge/withheld"]`) for feature 034.
//!
//! Peers: `result_status`, `project_evidence`, `repeat_notice`. Secret bytes
//! MUST never appear in these types or their serialization.

use rmcp::model::{JsonObject, MetaObject};
use std::future::Future;

#[cfg(test)]
use crate::knowledge::SecretFindingDescriptor;

pub use crate::index_lifecycle::guidance::withheld::*;

tokio::task_local! {
    /// Pending withheld meta for the tool result currently being built.
    /// Set on admission refusal lanes that already hold finding evidence;
    /// drained when building [`CallToolResult`] so `refuse_by_policy` stays
    /// syscall-free.
    static PENDING_WITHHELD: std::cell::RefCell<Option<WithheldMeta>>;
}

/// Bind the withheld stash for one tools/call dispatch (nested under project evidence).
pub async fn with_withheld_scope<F, T>(future: F) -> T
where
    F: Future<Output = T>,
{
    PENDING_WITHHELD
        .scope(std::cell::RefCell::new(None), future)
        .await
}

pub fn record_pending_withheld(meta: WithheldMeta) {
    let _ = PENDING_WITHHELD.try_with(|cell| *cell.borrow_mut() = Some(meta));
}

pub fn take_pending_withheld() -> Option<WithheldMeta> {
    PENDING_WITHHELD
        .try_with(|cell| cell.borrow_mut().take())
        .ok()
        .flatten()
}

pub fn attach_withheld_meta(meta: &mut Option<MetaObject>, withheld: WithheldMeta) {
    let meta = meta.get_or_insert_with(|| MetaObject(JsonObject::new()));
    if let Ok(value) = serde_json::to_value(&withheld) {
        meta.0.insert(WITHHELD_META_KEY.to_string(), value);
    }
}

/// Drain pending withheld into an existing result meta map (or create one).
pub fn drain_pending_into_meta(meta: &mut Option<MetaObject>) {
    if let Some(withheld) = take_pending_withheld() {
        attach_withheld_meta(meta, withheld);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_and_id_never_embed_secret_bytes() {
        let secret = b"S3cretValue9x!";
        let shape = crate::knowledge::describe_secret_shape(secret, "secret.context-assignment");
        assert!(!shape.contains("S3cretValue9x!"));
        assert!(shape.contains("14-char"));
        let id = mint_finding_id("cfg.json", "secret.context-assignment", 1, 1, &shape);
        assert!(id.starts_with("wf_"));
        assert!(!id.contains("S3cret"));
        let meta = WithheldMeta::from_content_findings(
            "cfg.json",
            &[SecretFindingDescriptor {
                rule_id: "secret.context-assignment",
                line_start: 1,
                line_end: 1,
                shape: shape.clone(),
            }],
        );
        let json = serde_json::to_string(&meta).unwrap();
        assert!(!json.contains("S3cretValue9x!"));
        assert!(json.contains("symforge") || json.contains("findings"));
    }

    #[test]
    fn path_rule_meta_does_not_invent_content_wf_ids() {
        let meta = WithheldMeta::path_rule_only(".env", "path.env");
        assert!(meta.findings.iter().all(|f| !f.id.starts_with("wf_")));
        assert!(
            meta.findings
                .iter()
                .all(|f| f.actions.iter().all(|a| !a.available))
        );
    }
}
