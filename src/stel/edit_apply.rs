//! STEL guarded apply pre-flight — symbol resolve, content verify, if_match (apply:true only).

use std::path::Path;
use std::sync::Arc;

use crate::live_index::IndexState;
use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};
use crate::live_index::{IndexedFile, SharedIndex};

use super::edit_planner::EditValidationError;
use super::types::StelEditRequest;

/// Resolved symbol span used for apply metadata and pre-flight gates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedEditSymbol {
    pub path: String,
    pub name: String,
    pub byte_start: u32,
    pub byte_end: u32,
    pub line_start: u32,
    pub line_end: u32,
    pub current_body: String,
}

/// Outcome of guarded pre-apply validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreApplyOutcome {
    Ready(ResolvedEditSymbol),
    AlreadyApplied(ResolvedEditSymbol),
}

/// Whether the request opts into committed apply.
pub fn apply_requested(request: &StelEditRequest) -> bool {
    request.apply == Some(true)
}

/// Run apply-only gates after L1 validation and path freshening.
pub fn run_pre_apply_gates(
    index: &SharedIndex,
    request: &StelEditRequest,
    abs_path: &Path,
) -> Result<PreApplyOutcome, EditValidationError> {
    let guard = index.read();
    match guard.index_state() {
        IndexState::Ready => {}
        IndexState::Empty => {
            return Err(EditValidationError::new(
                "Index not loaded; call index_folder before symforge_edit apply",
            ));
        }
        IndexState::Loading => {
            return Err(EditValidationError::new(
                "Index is still loading; retry symforge_edit apply when ready",
            ));
        }
        IndexState::CircuitBreakerTripped { summary } => {
            return Err(EditValidationError::new(format!(
                "Index degraded: {summary}"
            )));
        }
    }

    let path = request.path.trim();
    let name = request.symbol.as_deref().unwrap_or("").trim();
    let file = guard
        .capture_shared_file(path)
        .ok_or_else(|| EditValidationError::new(format!("file not found in index: {path}")))?;

    verify_index_matches_disk(&file, abs_path)?;

    let resolved = resolve_symbol_in_file(&file, path, name)?;

    if let Some(if_match) = request.if_match.as_deref() {
        if if_match != resolved.current_body {
            return Err(EditValidationError::new(
                "if_match does not match current symbol body",
            ));
        }
    }

    if request.idempotency_key.is_none()
        && request
            .body
            .as_deref()
            .is_some_and(|body| body == resolved.current_body)
    {
        return Ok(PreApplyOutcome::AlreadyApplied(resolved));
    }

    Ok(PreApplyOutcome::Ready(resolved))
}

pub fn format_apply_metadata(resolved: &ResolvedEditSymbol, write_mode: &str) -> String {
    format!(
        "Write mode: {write_mode}\n\
         Changed file: {}\n\
         Symbol: {}\n\
         Byte range: {}-{}\n\
         Line range: {}-{}",
        resolved.path,
        resolved.name,
        resolved.byte_start,
        resolved.byte_end,
        resolved.line_start,
        resolved.line_end,
    )
}

pub fn format_already_applied_body(resolved: &ResolvedEditSymbol) -> String {
    format!(
        "Decision: already applied\n\
         {}\n\
         SymForge did not rewrite the symbol; on-disk body already matches the requested body.",
        format_apply_metadata(resolved, "already_applied")
    )
}

fn verify_index_matches_disk(
    file: &IndexedFile,
    abs_path: &Path,
) -> Result<(), EditValidationError> {
    let disk = std::fs::read(abs_path).map_err(|error| {
        EditValidationError::new(format!(
            "cannot read on-disk file for content verification: {error}"
        ))
    })?;
    if file.content.as_slice() != disk.as_slice() {
        return Err(EditValidationError::new(
            "on-disk content does not match index snapshot for apply",
        ));
    }
    Ok(())
}

fn resolve_symbol_in_file(
    file: &Arc<IndexedFile>,
    path: &str,
    name: &str,
) -> Result<ResolvedEditSymbol, EditValidationError> {
    match resolve_symbol_selector(file, name, None, None) {
        SymbolSelectorMatch::NotFound => Err(EditValidationError::new(format!(
            "symbol not found: {name} in {path}"
        ))),
        SymbolSelectorMatch::Ambiguous(lines) => Err(EditValidationError::new(format!(
            "ambiguous symbol `{name}` in {path}; candidates at lines {lines:?}"
        ))),
        SymbolSelectorMatch::Selected(_, sym) => {
            let start = sym.byte_range.0 as usize;
            let end = sym.byte_range.1 as usize;
            let body_bytes = file
                .content
                .get(start..end)
                .ok_or_else(|| EditValidationError::new("symbol byte range out of bounds"))?;
            let current_body = std::str::from_utf8(body_bytes)
                .map_err(|_| EditValidationError::new("symbol body is not valid UTF-8"))?
                .to_string();
            Ok(ResolvedEditSymbol {
                path: path.to_string(),
                name: name.to_string(),
                byte_start: sym.byte_range.0,
                byte_end: sym.byte_range.1,
                line_start: sym.line_range.0 + 1,
                line_end: sym.line_range.1 + 1,
                current_body,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_index::LiveIndex;
    #[test]
    fn apply_requested_requires_explicit_true() {
        assert!(!apply_requested(&StelEditRequest::default()));
        assert!(!apply_requested(&StelEditRequest {
            apply: Some(false),
            ..Default::default()
        }));
        assert!(apply_requested(&StelEditRequest {
            apply: Some(true),
            ..Default::default()
        }));
    }

    #[test]
    fn pre_apply_rejects_if_match_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        let content = b"fn foo() { old }\n";
        std::fs::write(src.join("lib.rs"), content).unwrap();
        let shared = LiveIndex::load(dir.path()).expect("load index");
        let abs = src.join("lib.rs");
        let err = run_pre_apply_gates(
            &shared,
            &StelEditRequest {
                path: "src/lib.rs".to_string(),
                symbol: Some("foo".to_string()),
                body: Some("fn foo() { new }".to_string()),
                if_match: Some("fn foo() { wrong }".to_string()),
                ..Default::default()
            },
            &abs,
        )
        .unwrap_err();
        assert!(err.message.contains("if_match"));
    }
}
