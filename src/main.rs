//! The symforge binary: a shim over the crate's own dispatcher (Feature 020
//! Slice 4, C5 prep). The exposure flip retires the raw module surface this
//! file used to consume; all dispatch logic lives in `cli::entry`.

fn main() -> anyhow::Result<()> {
    symforge::cli::entry::run_main(std::env::args_os().collect())
}
