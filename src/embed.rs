//! `symforge::embed` — the engine-only facade for library embedders (e.g. AAP).
//!
//! This namespace re-exports the parsing + live-index + query + git engine
//! WITHOUT the daemon / sidecar / protocol-server / CLI surface. Depend on it
//! with:
//!
//! ```toml
//! symforge = { version = "*", default-features = false, features = ["embed"] }
//! ```
//!
//! Because the `server` feature is off, none of the server modules (and none of
//! their heavy/unsafe deps: axum, rmcp, clap, reqwest, process signaling) are
//! compiled, so server-side breakage cannot reach an embedding consumer.

pub use crate::domain;
pub use crate::git;
pub use crate::live_index::{self, LiveIndex};
pub use crate::parsing;
