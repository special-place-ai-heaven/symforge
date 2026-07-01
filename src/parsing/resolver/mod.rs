//! SP-0C spike — minimal Rust call resolver (Program 015).
//!
//! ponytail: throwaway-grade falsifier code. Resolves a `Call` reference to its
//! target **only** via same-file definitions and in-file `use` imports/aliases
//! (resolver-port-notes.md §3-4 same-file, §5-6 imports). No cross-file
//! registry, no trait dispatch, no stdlib prelude, no type inference, no FFI to
//! CBM. The real hybrid resolver lands at C-S3-*. This exists only to measure
//! same-file+import resolution accuracy against the benchmark fixtures.

mod rust;

pub use rust::resolve_rust_source;

/// How a call site was resolved (subset of `data-model.md` `ResolverStrategy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolverStrategy {
    /// Target is a definition in the same file.
    SameFile,
    /// Target reached through an in-file `use` import or alias.
    Import,
    /// v1 cannot resolve (method on an inferred receiver, stdlib, cross-file).
    Unresolved,
}

/// A single resolved (or unresolved) call site.
#[derive(Clone, Debug)]
pub struct ResolvedCall {
    /// Simple name at the call site (e.g. `helper`, `new`, `len`).
    pub name: String,
    /// 1-based line of the call site.
    pub line: u32,
    /// Best-effort fully-qualified callee, `None` when unresolved.
    pub callee_qname: Option<String>,
    pub strategy: ResolverStrategy,
}
