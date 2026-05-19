use std::collections::BTreeMap;

pub struct OrderSnapshot {
    pub id: String,
    pub customer: String,
    pub state: String,
    pub totals: BTreeMap<String, i64>,
}

pub enum ReconcileAction {
    Accept,
    Retry { reason: String },
    Reject { reason: String },
}

pub fn reconcile_orders(current: &[OrderSnapshot], incoming: &[OrderSnapshot]) -> Vec<ReconcileAction> {
    let mut actions = Vec::new();
    let mut current_by_id = BTreeMap::new();
    for snapshot in current {
        current_by_id.insert(snapshot.id.as_str(), snapshot);
    }

    for candidate in incoming {
        match current_by_id.get(candidate.id.as_str()) {
            None => actions.push(ReconcileAction::Accept),
            Some(existing) if existing.state == candidate.state => {
                actions.push(ReconcileAction::Retry {
                    reason: "state has not advanced enough for durable storage".to_string(),
                });
            }
            Some(existing) if existing.customer != candidate.customer => {
                actions.push(ReconcileAction::Reject {
                    reason: "customer identity changed across snapshots".to_string(),
                });
            }
            Some(_) => actions.push(ReconcileAction::Accept),
        }
    }

    actions
}

pub fn summarize_totals(snapshot: &OrderSnapshot) -> String {
    let mut parts = Vec::new();
    for (currency, cents) in &snapshot.totals {
        parts.push(format!("{currency}:{cents}"));
    }
    parts.join(",")
}

pub fn explain_reconciliation(actions: &[ReconcileAction]) -> String {
    let mut accepted = 0;
    let mut retried = 0;
    let mut rejected = 0;

    for action in actions {
        match action {
            ReconcileAction::Accept => accepted += 1,
            ReconcileAction::Retry { .. } => retried += 1,
            ReconcileAction::Reject { .. } => rejected += 1,
        }
    }

    format!("accepted={accepted}; retried={retried}; rejected={rejected}")
}

pub fn fixture_notes() -> &'static str {
    "This fixture intentionally includes ordinary data-shaping logic, enum variants, match arms, and repeated explanatory strings so raw bytes are representative while the outline stays compact."
}
