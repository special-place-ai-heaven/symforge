use std::path::PathBuf;
use std::time::{Duration, Instant};

use symforge::embed::parity::host::{
    HostLimits, HostPhase, HostRequest, HostResponse, HostRights, HostRoom, HostRoomConfig,
    HostRoomGrant, HostRuntimeOwner,
};
use symforge::embed::parity::{QueryLimits, QueryRequest};
use symforge::embed::{ProcessIndexRuntime, engine_info};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let info = engine_info();
    assert!(!info.version.is_empty());
    let request = serde_json::to_vec(&HostRequest::Catalog)?;
    assert!(!request.is_empty());

    // A real host supplies its authenticated room's authoritative local source.
    // Passing a path opts into an actual live room open/status/close smoke test.
    let Some(root) = std::env::args_os().nth(1).map(PathBuf::from) else {
        return Ok(());
    };
    let runtime = ProcessIndexRuntime::acquire().map_err(|error| format!("{error:?}"))?;
    let owner = HostRuntimeOwner::new(runtime);
    let grant = HostRoomGrant::new(
        "fixture-room".into(),
        root,
        "fixture-guest-source".into(),
        HostRights::read_only(),
    )
    .map_err(|error| format!("{error:?}"))?;
    let room = HostRoom::open_in_runtime(
        &owner,
        grant,
        HostRoomConfig {
            room_id: "fixture-room".into(),
            limits: HostLimits::default(),
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    let control = room.control().map_err(|error| format!("{error:?}"))?;
    let status = room
        .dispatch_wire(&serde_json::to_vec(&HostRequest::Status)?, &control)
        .map_err(|error| format!("{error:?}"))?;
    assert!(!status.is_empty());
    if let Some(path) = std::env::args().nth(2) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let HostResponse::Status(status) = room
                .dispatch(&HostRequest::Status, &control)
                .map_err(|error| format!("{error:?}"))?
            else {
                unreachable!()
            };
            if status.phase == HostPhase::Current {
                break;
            }
            if Instant::now() >= deadline {
                return Err("source did not publish before the example deadline".into());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let request = HostRequest::Query {
            request: QueryRequest::File {
                path,
                start_line: None,
                end_line: None,
            },
            limits: QueryLimits::default(),
        };
        let bytes = room
            .dispatch_wire(&serde_json::to_vec(&request)?, &control)
            .map_err(|error| format!("{error:?}"))?;
        let HostResponse::Query(reply) = serde_json::from_slice(&bytes)? else {
            return Err("expected query receipt".into());
        };
        assert!(!reply.publication_identity.is_empty());
        assert!(!reply.canonical_argument_hash.is_empty());
    }
    room.close(&control).map_err(|error| format!("{error:?}"))?;
    owner.shutdown(&control)
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}
