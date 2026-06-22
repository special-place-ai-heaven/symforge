//! STEL guarded apply pre-flight — symbol resolve, content verify, if_match (apply:true only).

use std::path::Path;
use std::sync::Arc;

use crate::live_index::IndexState;
use crate::live_index::query::{SymbolSelectorMatch, resolve_symbol_selector};
use crate::live_index::{IndexedFile, SharedIndex};

use super::edit_planner::EditValidationError;
use super::types::StelEditRequest;

/// Normalize bytes for a PRE-FLIGHT equality compare only (012 D6 /
/// contracts §3c): strip a leading UTF-8 BOM and fold CRLF/CR line endings to
/// LF, so an `if_match` or index-vs-disk compare that differs ONLY by line
/// endings or a BOM is not falsely rejected.
///
/// IMPORTANT: this is for the pre-flight gate ONLY. The write path's byte-exact
/// splice guard (`protocol::edit::guarded_atomic_write_file`) is intentionally
/// NOT normalized — it must compare the exact bytes being written so the
/// optimistic-concurrency guarantee (Principle IV idempotency) is preserved.
fn normalize_for_match(bytes: &[u8]) -> Vec<u8> {
    // Strip a single leading UTF-8 BOM (EF BB BF) if present.
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);

    // Fold CRLF and bare CR to LF. Iterating bytes is safe: CR (0x0D) and LF
    // (0x0A) are ASCII and never appear inside a multibyte UTF-8 sequence.
    let mut out = Vec::with_capacity(body.len());
    let mut i = 0;
    while i < body.len() {
        match body[i] {
            b'\r' => {
                out.push(b'\n');
                // Swallow a following LF so CRLF collapses to a single LF.
                if i + 1 < body.len() && body[i + 1] == b'\n' {
                    i += 1;
                }
            }
            other => out.push(other),
        }
        i += 1;
    }
    out
}

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
            // Surface-aware recovery (TR-02 / FR-012): never name `index_folder`
            // on the compact surface, where `symforge_edit` is reachable but
            // `index_folder` is dispatch-gated. The hint is computed from the
            // active surface, never a fixed string.
            let hint = crate::protocol::format::empty_index_recovery_hint(
                crate::protocol::surface_probe::surface_profile_from_env(),
            );
            return Err(EditValidationError::new(format!(
                "{hint} (symforge_edit apply requires a loaded index)"
            )));
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

    // The `if_match` value guard and the body-equality "already applied"
    // short-circuit are REPLACE-specific: they compare against the resolved
    // symbol's current body, which is the thing replace rewrites. For inserts the
    // resolved symbol is only the anchor (a new symbol is added beside it, not
    // rewritten), and for within-edits `body` is unused (the change is keyed on
    // `old_text`/`new_text`). The internal `insert_symbol` / `edit_within_symbol`
    // tools also expose no `if_match` field, so honoring it here would be a guard
    // the write path cannot enforce. Apply these checks for `Replace` only; the
    // splice-integrity guard (`base == disk` at write time) still protects every
    // op, and within-edits remain naturally idempotent (a missing `old_text` is a
    // no-op the internal tool reports).
    if super::edit_planner::effective_op(request) == super::types::StelEditOp::Replace {
        // Pre-flight compare is NORMALIZED (CRLF/LF + optional BOM) so an
        // `if_match` that matches the current body apart from line endings or a
        // BOM is not falsely rejected. The byte-exact write-time splice guard is
        // unaffected (it re-reads and compares raw bytes).
        if let Some(if_match) = request.if_match.as_deref()
            && normalize_for_match(if_match.as_bytes())
                != normalize_for_match(resolved.current_body.as_bytes())
        {
            return Err(EditValidationError::new(
                "if_match does not match current symbol body",
            ));
        }

        if request.idempotency_key.is_none()
            && request
                .body
                .as_deref()
                .is_some_and(|body| body == resolved.current_body)
        {
            return Ok(PreApplyOutcome::AlreadyApplied(resolved));
        }
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

/// PRE-FLIGHT index-vs-disk consistency check only (N-6, TR-06 boundary).
///
/// This confirms the index snapshot still matches disk at pre-flight time,
/// under the `index.read()` guard in [`run_pre_apply_gates`]. It is NOT the
/// write-time `if_match` guard: the read lock is released long before the
/// actual splice + write, so this check alone leaves a TOCTOU window. The
/// real optimistic-concurrency guarantee is the write-time re-read in
/// `protocol::edit::guarded_atomic_write_file`, which re-verifies the bytes
/// actually being written. Do not advertise this function as a write guard.
fn verify_index_matches_disk(
    file: &IndexedFile,
    abs_path: &Path,
) -> Result<(), EditValidationError> {
    let disk = std::fs::read(abs_path).map_err(|error| {
        EditValidationError::new(format!(
            "cannot read on-disk file for content verification: {error}"
        ))
    })?;
    // Normalized PRE-FLIGHT compare (CRLF/LF + optional BOM): a working tree
    // that differs from the index snapshot only by line endings or a BOM is not
    // a genuine drift and must not block apply. The write-time re-read in
    // `guarded_atomic_write_file` still compares the exact bytes being spliced.
    if normalize_for_match(file.content.as_slice()) != normalize_for_match(disk.as_slice()) {
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

    #[test]
    fn normalize_for_match_folds_crlf_and_strips_bom() {
        // CRLF == LF after normalization.
        assert_eq!(
            normalize_for_match(b"a\r\nb\r\n"),
            normalize_for_match(b"a\nb\n")
        );
        // Bare CR also folds to LF.
        assert_eq!(normalize_for_match(b"a\rb"), normalize_for_match(b"a\nb"));
        // Leading BOM is stripped before comparison.
        assert_eq!(
            normalize_for_match(b"\xEF\xBB\xBFhello"),
            normalize_for_match(b"hello")
        );
        // A genuine content difference still differs.
        assert_ne!(normalize_for_match(b"old"), normalize_for_match(b"new"));
    }

    #[test]
    fn pre_apply_accepts_if_match_differing_only_by_crlf_and_bom() {
        // On-disk + indexed body uses LF; the caller's `if_match` is byte-equal
        // EXCEPT for a leading BOM and CRLF line endings. The normalized
        // pre-flight compare must NOT falsely reject this valid edit.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        // Multi-line body so CRLF folding is actually exercised.
        let content = b"fn foo() {\n    ok();\n}\n";
        std::fs::write(src.join("lib.rs"), content).unwrap();
        let shared = LiveIndex::load(dir.path()).expect("load index");
        let abs = src.join("lib.rs");

        // Resolve the current body so the test's if_match mirrors it apart from
        // line endings + a BOM, independent of how the parser spans the symbol.
        let current_body = {
            let guard = shared.read();
            let file = guard.capture_shared_file("src/lib.rs").unwrap();
            resolve_symbol_in_file(&file, "src/lib.rs", "foo")
                .unwrap()
                .current_body
        };
        let crlf_bom_if_match = format!("\u{feff}{}", current_body.replace('\n', "\r\n"));
        assert_ne!(
            crlf_bom_if_match, current_body,
            "the test input must actually differ in raw bytes"
        );

        let outcome = run_pre_apply_gates(
            &shared,
            &StelEditRequest {
                path: "src/lib.rs".to_string(),
                symbol: Some("foo".to_string()),
                // A genuinely new body so we land on Ready, not AlreadyApplied.
                body: Some("fn foo() {\n    changed();\n}".to_string()),
                if_match: Some(crlf_bom_if_match),
                apply: Some(true),
                ..Default::default()
            },
            &abs,
        )
        .expect("CRLF/BOM-only if_match difference must not be rejected");
        assert!(
            matches!(outcome, PreApplyOutcome::Ready(_)),
            "expected a Ready pre-apply outcome, got {outcome:?}"
        );
    }
}
