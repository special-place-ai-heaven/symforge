//! The symforge binary: a shim over the crate's public server door
//! (Feature 020 Slice 4, C5). The exposure flip retired the raw module
//! surface this file used to consume; all dispatch logic lives behind
//! `symforge::server_api::run`.

// Issue #876: on Linux glibc, jemalloc with background purge returns freed
// index generations to the OS; glibc kept them resident. Measured, see
// docs/solutions/allocator-comparison-876.md. Other targets keep the system
// allocator. Only the binary chooses: the library never installs one.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn main() -> std::process::ExitCode {
    match symforge::server_api::run(std::env::args_os().collect()) {
        Ok(symforge::server_api::ServerExit::Success) => std::process::ExitCode::SUCCESS,
        // cli-serve contract: a refused start exits 2, distinct from a
        // generic failure, so operators/CI can detect a refused bind.
        Ok(symforge::server_api::ServerExit::RefusedToStart) => std::process::ExitCode::from(2),
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
