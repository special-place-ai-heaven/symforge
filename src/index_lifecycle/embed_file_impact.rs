//! MCP `analyze_file_impact` for an embedded source. The re-admission runs
//! before the query captures its publication, as the targeted freshen does,
//! so the claim binds the publication the answer was rendered from. Path
//! normalization, the engine (`guidance::file_impact`), the estimate and the
//! co-change section are MCP's own.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::embed::parity::file_impact::{FileImpactReport, FileImpactRequest};
use crate::embed::parity::{QueryOutput, QueryPolicy, QueryRefusalKind};
use crate::live_index::single_file::ReindexOutcome;
use crate::live_index::store::SharedIndex;

use super::activation::ProjectSourceAuthority;
use super::embed_query::{Budget, EmbeddedQuerySnapshot};
use super::guidance::file_impact::{self, ImpactFailure, ImpactSymbolCache, SymbolSnapshot};

/// The engine's answer, held until the query captures its publication.
pub(super) struct FileImpactAdmission {
    text: String,
    publication_generation: u64,
    /// MCP answers a missing file with one line and no co-change section.
    not_found: bool,
}

/// MCP's sidecar symbol cache, per binding: the last post-edit symbols per
/// path, cleared when the project generation moves.
#[derive(Default)]
pub(super) struct ImpactSymbols {
    generation: u64,
    symbols: HashMap<String, Vec<SymbolSnapshot>>,
}

pub(super) type ImpactSymbolStore = Mutex<ImpactSymbols>;

struct BindingCache<'a> {
    store: &'a ImpactSymbolStore,
    shared: &'a SharedIndex,
    expected_generation: u64,
}

impl BindingCache<'_> {
    fn lock(&self) -> Result<MutexGuard<'_, ImpactSymbols>, ImpactFailure> {
        if self.shared.current_project_generation() != self.expected_generation {
            return Err(ImpactFailure::Unavailable);
        }
        let mut store = self.store.lock().expect("embedded impact symbol cache");
        if store.generation != self.expected_generation {
            store.symbols.clear();
            store.generation = self.expected_generation;
        }
        Ok(store)
    }
}

impl ImpactSymbolCache for BindingCache<'_> {
    fn get(&mut self, path: &str) -> Result<Option<Vec<SymbolSnapshot>>, ImpactFailure> {
        Ok(self.lock()?.symbols.get(path).cloned())
    }

    fn store(&mut self, path: &str, symbols: Vec<SymbolSnapshot>) -> Result<(), ImpactFailure> {
        self.lock()?.symbols.insert(path.to_owned(), symbols);
        Ok(())
    }
}

pub(super) fn validate(request: &FileImpactRequest) -> Result<(), QueryRefusalKind> {
    if request.path.trim().is_empty() {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    Ok(())
}

/// MCP `impact_text`: normalize the caller's path, serialize with other
/// impact analyses on this data plane, and run the shared engine through the
/// canonical single-file admission seam.
pub(super) fn admit(
    shared: &SharedIndex,
    root: &Path,
    authority: &ProjectSourceAuthority,
    symbols: &ImpactSymbolStore,
    request: &FileImpactRequest,
    stop: &dyn Fn() -> Result<(), QueryRefusalKind>,
) -> Result<FileImpactAdmission, QueryRefusalKind> {
    let expected_generation = shared.current_project_generation();
    let requested = Path::new(&request.path);
    let absolute = if requested.is_absolute() {
        requested.to_path_buf()
    } else {
        root.join(requested)
    };
    let relative = crate::paths::normalize_event_path(&absolute, root)
        .ok_or(QueryRefusalKind::InvalidRequest)?;
    let path = crate::live_index::query::normalize_path_query(&relative);
    if path.is_empty() {
        return Err(QueryRefusalKind::InvalidRequest);
    }
    // The engine publishes this caller-supplied path into the index, so it
    // takes the same alias refusal as every other caller-path entry point.
    super::guidance::freshen::safe_repo_path_for_freshen(root, &path)
        .map_err(|_| QueryRefusalKind::InvalidRequest)?;

    // ponytail: 1 ms poll for MCP's async impact mutex; a blocking lock would
    // panic inside a host's async runtime.
    let _serial = loop {
        if let Some(guard) = shared.try_lock_impact_analysis() {
            break guard;
        }
        stop()?;
        std::thread::sleep(std::time::Duration::from_millis(1));
    };
    let observer = authority.active_observer();
    let mut cache = BindingCache {
        store: symbols,
        shared,
        expected_generation,
    };
    let rendered = file_impact::analyze_file_impact(
        shared,
        root,
        &path,
        request.new_file.unwrap_or(false),
        expected_generation,
        shared.published_generation(),
        &mut |relative, abs_path| {
            let receipt = crate::live_index::single_file::admit_and_index_single_path_with_receipt(
                relative,
                abs_path,
                shared,
                expected_generation,
            );
            // The V11 observation lane, as the targeted freshen observes its
            // re-admission: a refused observation only dirties the next cut.
            let _ = match receipt.outcome {
                ReindexOutcome::Reindexed => authority.observe_admission(observer, relative),
                ReindexOutcome::Removed => authority.observe_removal(observer, relative),
                _ => Ok(()),
            };
            receipt
        },
        &mut cache,
    );
    match rendered {
        Ok(render) => Ok(FileImpactAdmission {
            text: render.text,
            publication_generation: render.published.publication_generation,
            not_found: false,
        }),
        Err(ImpactFailure::NotFound) => Ok(FileImpactAdmission {
            text: format!("File not found on disk: {}", request.path),
            publication_generation: shared.published_generation().publication_generation,
            not_found: true,
        }),
        Err(ImpactFailure::Unavailable) => Err(QueryRefusalKind::StalePublication),
    }
}

/// Render the MCP answer over the captured publication, which must be the
/// one the re-admission produced.
pub(super) fn project(
    snapshot: &EmbeddedQuerySnapshot,
    request: &FileImpactRequest,
    policy: QueryPolicy,
    budget: &mut Budget,
) -> Result<QueryOutput, QueryRefusalKind> {
    let include_co_changes = request.include_co_changes.unwrap_or(false);
    let limit = request.co_changes_limit.unwrap_or(10) as usize;
    let rendered = if request.estimate == Some(true) {
        file_impact::estimate_text(include_co_changes, limit)
    } else {
        // A nested request (from `ask`) never ran the re-admission.
        let admission = budget
            .file_impact
            .take()
            .ok_or(QueryRefusalKind::UnsupportedOption)?;
        if admission.publication_generation != snapshot.generation.publication_generation {
            return Err(QueryRefusalKind::StalePublication);
        }
        let mut text = admission.text;
        if include_co_changes && !admission.not_found {
            let temporal = super::embed_temporal::temporal(snapshot, policy, budget)?;
            file_impact::append_co_changes(&mut text, &temporal, &request.path, limit);
        }
        text
    };
    budget.required(rendered.len())?;
    Ok(QueryOutput::FileImpact(FileImpactReport { rendered }))
}
