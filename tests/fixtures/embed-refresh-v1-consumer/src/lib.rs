#![allow(dead_code)]

use std::time::Duration;

use symforge::embed::parity::host::OperationControl;
use symforge::embed::parity::{QueryClaim, QueryLimits, QueryRequest};
use symforge::embed::{
    EmbeddedSourceHandle, GraphPublicationLease, SourceChangeVersion, SourceRuntimePhase,
};

pub fn name_refresh_v1_exports() {
    let _ = std::any::type_name::<GraphPublicationLease>();
    let _ = std::any::type_name::<SourceChangeVersion>();
}

pub fn use_refresh_v1_api(
    handle: &EmbeddedSourceHandle,
    request: &QueryRequest,
    limits: QueryLimits,
    since: SourceChangeVersion,
    timeout: Duration,
    control: &OperationControl,
) {
    let _: SourceChangeVersion = handle.change_version();
    let _ = handle.wait_for_change(since, timeout);

    let lease = handle.begin_graph_publication_lease(timeout);
    let _ = handle.query_allowing_refresh(request, limits);
    let _ = handle.query_allowing_refresh_with_control(request, limits, control);

    if let Ok(lease) = lease {
        let _: &GraphPublicationLease = &lease;
        let _ = handle.query_with_graph_lease(request, limits, &lease);
        let _ = handle.query_with_graph_lease_and_control(request, limits, &lease, control);
    }
}

pub fn inspect_refresh_v1_claim(claim: &QueryClaim) -> (SourceRuntimePhase, bool) {
    (claim.source_phase(), claim.is_refreshing())
}
