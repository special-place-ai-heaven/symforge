//! Run with: cargo run --no-default-features --features embed --example embed_query -- PATH
//! The host owns one runtime and one admitted source; no daemon or transport is needed.

#[cfg(feature = "embed")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::{Duration, Instant};
    use symforge::embed::parity::{QueryLimits, QueryRequest};
    use symforge::embed::{EmbeddedSourceSpec, ProcessIndexRuntime, SourceRuntimePhase};

    let root = std::env::args_os().nth(1).ok_or("pass a repository path")?;
    let runtime = ProcessIndexRuntime::acquire()?;
    let source = runtime.open_embedded_source(EmbeddedSourceSpec::current_worktree(root.into()))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while source.runtime_view().phase != SourceRuntimePhase::Current {
        if Instant::now() >= deadline {
            return Err("source did not become current before the host deadline".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }

    // Request/response DTOs cross a host's serialization boundary; the source
    // handle and its authority remain owned Rust values in the host process.
    let request = QueryRequest::SearchSymbols {
        query: None,
        path_prefix: None,
        kind: Some("function".into()),
        include_tests: false,
    };
    let serialized = serde_json::to_vec(&request)?;
    let request = serde_json::from_slice(&serialized)?;
    let claim = source.query(&request, QueryLimits::default())?;
    println!(
        "publication={} truncated={}",
        claim.publication_identity(),
        claim.truncated()
    );
    println!("{}", serde_json::to_string_pretty(claim.value())?);
    source.begin_close().wait(deadline)?;
    Ok(())
}

#[cfg(not(feature = "embed"))]
fn main() {
    eprintln!("Enable the embed feature to run this example.");
}
