from pathlib import Path


def must_replace(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"missing: {label}")
    return text.replace(old, new, 1)


def patch_parity():
    p = Path("src/embed/parity.rs")
    text = p.read_text(encoding="utf-8")
    text = must_replace(
        text,
        """    pub source: Option<Vec<u8>>,
    pub source_byte_start: u64,
    pub source_byte_end: u64,
}""",
        """    pub source: Option<Vec<u8>>,
    pub source_byte_start: u64,
    pub source_byte_end: u64,
    /// Leading doc-comment byte range when the parser attached one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_byte_start: Option<u64>,
    /// Exclusive end of the attached doc-comment range.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_byte_end: Option<u64>,
}""",
        "QuerySymbol fields",
    )
    text = must_replace(
        text,
        """    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub before_line: Option<u32>,
    pub after_line: Option<u32>,
}""",
        """    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub before_line: Option<u32>,
    pub after_line: Option<u32>,
    /// First-line signature on the old side when the symbol existed before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_signature: Option<String>,
    /// First-line signature on the new side when the symbol exists after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_signature: Option<String>,
}""",
        "QuerySymbolChange fields",
    )
    p.write_text(text, encoding="utf-8")
    print("parity.rs ok")


def patch_host():
    p = Path("src/embed/parity/host.rs")
    text = p.read_text(encoding="utf-8")
    text = must_replace(
        text,
        """pub struct HostProgress {
    pub files_discovered: u64,
    pub files_parsed: u64,
    pub symbols_found: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostStatusReport {""",
        """pub struct HostProgress {
    pub files_discovered: u64,
    pub files_parsed: u64,
    pub symbols_found: u64,
}

/// Closed cause for a failed or blocked indexing observation on host status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HostIndexFailureCause {
    ParseFailed,
    PartialParse,
    PublicationBlocked,
    StateUnavailable,
}

/// Typed index-failure evidence on [`HostStatus`]. Prefer this over free-text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct HostIndexFailure {
    pub cause: HostIndexFailureCause,
    /// Repository-relative path when the failure is file-scoped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostStatusReport {""",
        "HostIndexFailure types",
    )
    text = must_replace(
        text,
        """pub struct HostStatus {
    pub room_id: String,
    pub engine: HostEngineIdentity,
    pub phase: HostPhase,
    pub binding_identity: String,
    pub source_capture_publication_identity: Option<String>,
    pub source_version: u64,
    pub observer_epoch: u64,
    pub progress: HostProgress,
}""",
        """pub struct HostStatus {
    pub room_id: String,
    pub engine: HostEngineIdentity,
    pub phase: HostPhase,
    pub binding_identity: String,
    pub source_capture_publication_identity: Option<String>,
    pub source_version: u64,
    pub observer_epoch: u64,
    pub progress: HostProgress,
    /// Present when status observed a blocked publication or a failed census file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_failure: Option<HostIndexFailure>,
}""",
        "HostStatus.index_failure",
    )
    p.write_text(text, encoding="utf-8")
    print("host.rs ok")


def patch_embed_query():
    p = Path("src/index_lifecycle/embed_query.rs")
    text = p.read_text(encoding="utf-8")
    text = must_replace(
        text,
        """        source: None,
        source_byte_start: 0,
        source_byte_end: 0,
    })
}""",
        """        source: None,
        source_byte_start: 0,
        source_byte_end: 0,
        doc_byte_start: symbol.doc_byte_range.map(|(start, _)| start as u64),
        doc_byte_end: symbol.doc_byte_range.map(|(_, end)| end as u64),
    })
}""",
        "symbol_row doc range",
    )
    text = must_replace(
        text,
        "type DiffSymbols = BTreeMap<(String, String, u32), (String, u32)>;",
        "type DiffSymbols = BTreeMap<(String, String, u32), (String, u32, String)>;",
        "DiffSymbols type",
    )
    text = must_replace(
        text,
        """        symbols.insert(
            (kind, symbol.name, *occurrence),
            (crate::hash::digest_hex(body), symbol.line_range.0 + 1),
        );""",
        """        let signature = super::guidance::edit_body::extract_signature(
            bytes,
            symbol.byte_range,
        );
        symbols.insert(
            (kind, symbol.name, *occurrence),
            (crate::hash::digest_hex(body), symbol.line_range.0 + 1, signature),
        );""",
        "diff_symbols insert",
    )
    text = must_replace(
        text,
        """        let before_hash = before.map(|entry| entry.0.clone());
        let after_hash = after.map(|entry| entry.0.clone());
        let size = kind.len()
            + name.len()
            + change.len()
            + before_hash.as_ref().map_or(0, String::len)
            + after_hash.as_ref().map_or(0, String::len);
        if !budget.row(size) {
            break;
        }
        changes.push(QuerySymbolChange {
            name,
            kind,
            occurrence,
            change: change.into(),
            before_hash,
            after_hash,
            before_line: before.map(|entry| entry.1),
            after_line: after.map(|entry| entry.1),
        });""",
        """        let before_hash = before.map(|entry| entry.0.clone());
        let after_hash = after.map(|entry| entry.0.clone());
        let before_signature = before.map(|entry| entry.2.clone());
        let after_signature = after.map(|entry| entry.2.clone());
        let size = kind.len()
            + name.len()
            + change.len()
            + before_hash.as_ref().map_or(0, String::len)
            + after_hash.as_ref().map_or(0, String::len)
            + before_signature.as_ref().map_or(0, String::len)
            + after_signature.as_ref().map_or(0, String::len);
        if !budget.row(size) {
            break;
        }
        changes.push(QuerySymbolChange {
            name,
            kind,
            occurrence,
            change: change.into(),
            before_hash,
            after_hash,
            before_line: before.map(|entry| entry.1),
            after_line: after.map(|entry| entry.1),
            before_signature,
            after_signature,
        });""",
        "diff push signatures",
    )
    p.write_text(text, encoding="utf-8")
    print("embed_query.rs ok")


def patch_embed_host():
    p = Path("src/index_lifecycle/embed_host.rs")
    text = p.read_text(encoding="utf-8")
    text = must_replace(
        text,
        """use crate::embed::parity::host::{
    HostArtifactOutcome, HostCheckpointOutcome, HostCheckpointReceipt, HostEngineIdentity,
    HostHealth, HostHealthNotApplicable, HostHealthProjection, HostHealthReport, HostHealthRequest,
    HostLimits, HostPhase, HostProgress, HostPublishedHealth, HostRefusal, HostRefusalKind,
    HostSourceProof, HostStateLocation, HostStatus, HostVerifyPhase, OperationControl, map_stop,
};""",
        """use crate::embed::parity::host::{
    HostArtifactOutcome, HostCheckpointOutcome, HostCheckpointReceipt, HostEngineIdentity,
    HostHealth, HostHealthNotApplicable, HostHealthProjection, HostHealthReport, HostHealthRequest,
    HostIndexFailure, HostIndexFailureCause, HostLimits, HostPhase, HostProgress,
    HostPublishedHealth, HostRefusal, HostRefusalKind, HostSourceProof, HostStateLocation,
    HostStatus, HostVerifyPhase, OperationControl, map_stop,
};""",
        "embed_host imports",
    )
    text = must_replace(
        text,
        """pub(crate) fn status(
    source: &EmbeddedSourceHandle,
    room_id: &str,
    engine: &HostEngineIdentity,
) -> HostStatus {
    let view = source.runtime_view();
    let progress = source.index_progress();
    HostStatus {
        room_id: room_id.to_owned(),
        engine: engine.clone(),
        phase: match view.phase {
            SourceRuntimePhase::Blocked => HostPhase::Blocked,
            SourceRuntimePhase::Current => HostPhase::Current,
            SourceRuntimePhase::Loading => HostPhase::Loading,
            SourceRuntimePhase::Refreshing => HostPhase::Refreshing,
            SourceRuntimePhase::Stopped => HostPhase::Stopped,
            SourceRuntimePhase::Stopping => HostPhase::Stopping,
        },
        binding_identity: view.binding_identity,
        source_capture_publication_identity: view.current_publication_identity,
        source_version: view.source_version,
        observer_epoch: view.observer_epoch,
        progress: HostProgress {
            files_discovered: progress.files_discovered,
            files_parsed: progress.files_parsed,
            symbols_found: progress.symbols_found,
        },
    }
}""",
        """pub(crate) fn status(
    source: &EmbeddedSourceHandle,
    room_id: &str,
    engine: &HostEngineIdentity,
) -> HostStatus {
    let view = source.runtime_view();
    let progress = source.index_progress();
    let phase = match view.phase {
        SourceRuntimePhase::Blocked => HostPhase::Blocked,
        SourceRuntimePhase::Current => HostPhase::Current,
        SourceRuntimePhase::Loading => HostPhase::Loading,
        SourceRuntimePhase::Refreshing => HostPhase::Refreshing,
        SourceRuntimePhase::Stopped => HostPhase::Stopped,
        SourceRuntimePhase::Stopping => HostPhase::Stopping,
    };
    let index_failure = observed_index_failure(source, phase);
    HostStatus {
        room_id: room_id.to_owned(),
        engine: engine.clone(),
        phase,
        binding_identity: view.binding_identity,
        source_capture_publication_identity: view.current_publication_identity,
        source_version: view.source_version,
        observer_epoch: view.observer_epoch,
        progress: HostProgress {
            files_discovered: progress.files_discovered,
            files_parsed: progress.files_parsed,
            symbols_found: progress.symbols_found,
        },
        index_failure,
    }
}

fn observed_index_failure(
    source: &EmbeddedSourceHandle,
    phase: HostPhase,
) -> Option<HostIndexFailure> {
    if phase == HostPhase::Blocked {
        return Some(HostIndexFailure {
            cause: HostIndexFailureCause::PublicationBlocked,
            path: None,
        });
    }
    let Ok(census) = source.index_census() else {
        return None;
    };
    census.value().files.iter().find_map(|file| {
        let cause = match file.parse_status {
            super::public_api::CensusParseStatus::Failed => HostIndexFailureCause::ParseFailed,
            super::public_api::CensusParseStatus::PartialParse => {
                HostIndexFailureCause::PartialParse
            }
            super::public_api::CensusParseStatus::Parsed => return None,
        };
        Some(HostIndexFailure {
            cause,
            path: Some(file.path.clone()),
        })
    })
}""",
        "status + observed_index_failure",
    )
    p.write_text(text, encoding="utf-8")
    print("embed_host.rs ok")


def patch_changelog():
    p = Path("CHANGELOG.md")
    text = p.read_text(encoding="utf-8")
    if "doc_byte_start" in text:
        print("CHANGELOG already updated")
        return
    needle = "`CensusFile` gains `reference_count`"
    addition = (
        "`QuerySymbol` exposes attached doc-comment byte ranges (`doc_byte_start` / "
        "`doc_byte_end`). `QuerySymbolChange` exposes `before_signature` / "
        "`after_signature` for old-side and new-side first-line signatures. "
        "`HostStatus` gains optional typed `index_failure` (`HostIndexFailureCause` + path) "
        "when status observes a blocked publication or a failed/partial census file.\n\n"
    )
    text = must_replace(text, needle, addition + needle, "CHANGELOG needle")
    p.write_text(text, encoding="utf-8")
    print("CHANGELOG ok")


if __name__ == "__main__":
    patch_parity()
    patch_host()
    patch_embed_query()
    patch_embed_host()
    patch_changelog()
