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
//!
//! SEMVER-PUBLIC: the flat re-exports below are an interface contract between
//! the SymForge engine and its embedders. A breaking change to any name or
//! signature here is a MAJOR bump. The `#[cfg(test)]` `contract` module at the
//! bottom of this file names every contracted item (types via `use`, functions
//! via full-signature fn-pointer bindings) so a rename, removal, or
//! signature-drift becomes a COMPILE FAILURE in SymForge's own embed test
//! suite rather than a downstream AAP surprise.

// ---------------------------------------------------------------------------
// Flat, semver-public facade. Re-exports ITEMS (types + free functions).
// ---------------------------------------------------------------------------
pub use crate::domain::{
    FileClassification, FileProcessingResult, LanguageId, ReferenceKind, SymbolKind, SymbolRecord,
};
pub use crate::git::GitRepo;
pub use crate::live_index::LiveIndex;
pub use crate::live_index::query::{SearchFilesTier, SearchFilesView};
pub use crate::live_index::search::{
    SymbolSearchResult, TextSearchError, TextSearchResult, search_symbols, search_text,
};
pub use crate::live_index::store::{
    IndexLoadSource, IndexedFile, ParseStatus, PublishedIndexState, PublishedIndexStatus,
    SharedIndex, SnapshotVerifyState,
};
pub use crate::parsing::process_file;

// ---------------------------------------------------------------------------
// Back-compat MODULE re-exports. AAP currently imports via deep module paths
// (e.g. `symforge::embed::parsing::process_file`). These re-export the MODULES
// `domain / git / live_index / parsing` — distinct names from every flat item
// above, so there is NO E0252 "defined multiple times" collision: the flat
// block exports `process_file` / `GitRepo` / `LiveIndex` (item names) while
// this line exports `parsing` / `git` / `live_index` / `domain` (module names).
// ---------------------------------------------------------------------------
pub use crate::{domain, git, live_index, parsing};

#[cfg(test)]
mod contract {
    //! Compile-time tripwire for the semver-public embedder facade.
    //!
    //! WHY THIS EXISTS: `symforge::embed` is a public, semver-stable interface
    //! contract between the SymForge engine and its embedders (AAP). This test
    //! module NAMES every contracted item so that:
    //!   * a removed or renamed item -> the `use` / path fails to compile,
    //!   * a changed function/method signature -> the `fn`-pointer binding
    //!     fails to compile (type mismatch).
    //! A breaking change to the contract therefore trips SymForge's own embed
    //! test suite at compile time, not a downstream consumer at integration
    //! time. The bindings are checked by the compiler; the body need not run.
    //!
    //! `#[allow(unused_imports)]` is intentional: these imports exist ONLY to
    //! name the contracted types, not to be used. Under `warnings = "deny"`
    //! an unused-import warning would otherwise fail the build.
    #[allow(unused_imports)]
    use crate::embed::{
        FileClassification, FileProcessingResult, GitRepo, IndexLoadSource, IndexedFile,
        LanguageId, LiveIndex, ParseStatus, PublishedIndexState, PublishedIndexStatus,
        ReferenceKind, SearchFilesTier, SearchFilesView, SharedIndex, SnapshotVerifyState,
        SymbolKind, SymbolRecord, SymbolSearchResult, TextSearchError, TextSearchResult,
    };

    // Also name the back-compat MODULE re-exports so their removal trips too.
    #[allow(unused_imports)]
    use crate::embed::{domain, git, live_index, parsing};

    use std::path::Path;

    /// Naming every contracted item forces removal/rename/signature drift to be
    /// a compile error. Functions use FULL-signature `fn`-pointer bindings, so a
    /// changed parameter or return type ALSO breaks compilation (not just a
    /// rename). No runtime logic — the bindings are the assertion.
    #[test]
    fn facade_contract_is_stable() {
        // --- Type contract: name each contracted type in type position. ---
        // (The `use` block above already fails to compile if any is removed or
        // renamed; these turbofish references additionally pin the names here.)
        fn _assert_named<T>() {}
        _assert_named::<FileClassification>();
        _assert_named::<FileProcessingResult>();
        _assert_named::<LanguageId>();
        _assert_named::<ReferenceKind>();
        _assert_named::<SymbolKind>();
        _assert_named::<SymbolRecord>();
        _assert_named::<SearchFilesTier>();
        _assert_named::<SearchFilesView>();
        _assert_named::<SymbolSearchResult>();
        _assert_named::<TextSearchResult>();
        _assert_named::<TextSearchError>();
        _assert_named::<IndexLoadSource>();
        _assert_named::<IndexedFile>();
        _assert_named::<ParseStatus>();
        _assert_named::<PublishedIndexState>();
        _assert_named::<PublishedIndexStatus>();
        _assert_named::<SharedIndex>();
        _assert_named::<SnapshotVerifyState>();
        _assert_named::<LiveIndex>();
        _assert_named::<GitRepo>();

        // --- Function / method contract: FULL-signature fn-pointer bindings. ---
        // A signature change (param type, arity, or return type) makes these
        // bindings fail to type-check.

        // Free functions. Referenced via the fully-qualified `crate::embed::`
        // re-export path so the binding pins the FACADE name (not the engine's
        // internal path) — removal/rename of the re-export breaks compilation.
        let _search_symbols: fn(&LiveIndex, &str, Option<&str>, usize) -> SymbolSearchResult =
            crate::embed::search_symbols;
        let _search_text: fn(
            &LiveIndex,
            Option<&str>,
            Option<&[String]>,
            bool,
        ) -> Result<TextSearchResult, TextSearchError> = crate::embed::search_text;
        let _process_file: fn(&str, &[u8], LanguageId) -> FileProcessingResult =
            crate::embed::process_file;

        // Associated functions (no `&self`).
        let _load: fn(&Path) -> anyhow::Result<SharedIndex> = LiveIndex::load;
        let _from_parse_result: fn(FileProcessingResult, Vec<u8>) -> IndexedFile =
            IndexedFile::from_parse_result;
        let _from_extension: fn(&str) -> Option<LanguageId> = LanguageId::from_extension;
        let _for_code_path: fn(&str) -> FileClassification = FileClassification::for_code_path;

        // `GitRepo` — the exactly-three methods AAP calls. `open` is an assoc
        // fn; `file_at_ref` / `changed_paths_between_refs` are `&self` methods
        // bound as fn-item paths with `&GitRepo` as the explicit receiver type.
        let _git_open: fn(&Path) -> Result<GitRepo, String> = GitRepo::open;
        let _git_file_at_ref: fn(&GitRepo, &str, &str) -> Result<Option<String>, String> =
            GitRepo::file_at_ref;
        let _git_changed_paths: fn(&GitRepo, &str, &str) -> Result<Vec<String>, String> =
            GitRepo::changed_paths_between_refs;

        // Silence unused-binding lints without weakening the contract: each
        // binding's TYPE is the assertion; touching them keeps them live.
        let _ = (
            _search_symbols,
            _search_text,
            _process_file,
            _load,
            _from_parse_result,
            _from_extension,
            _for_code_path,
            _git_open,
            _git_file_at_ref,
            _git_changed_paths,
        );

        // Back-compat module paths still resolve (deep-path imports AAP uses).
        let _deep_process_file: fn(&str, &[u8], LanguageId) -> FileProcessingResult =
            parsing::process_file;
        let _deep_search_symbols: fn(
            &live_index::LiveIndex,
            &str,
            Option<&str>,
            usize,
        ) -> live_index::search::SymbolSearchResult = live_index::search::search_symbols;
        let _deep_git_open: fn(&Path) -> Result<git::GitRepo, String> = git::GitRepo::open;
        let _deep_for_code_path: fn(&str) -> domain::FileClassification =
            domain::FileClassification::for_code_path;
        let _ = (
            _deep_process_file,
            _deep_search_symbols,
            _deep_git_open,
            _deep_for_code_path,
        );
    }
}
