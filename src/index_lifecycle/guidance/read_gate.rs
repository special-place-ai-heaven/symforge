//! Transport-independent admission policy over exact byte buffers.
use super::read_admission_format as format;
use super::withheld::WithheldMeta;
use crate::domain::{FileDisposition, IndexTargets, LanguageId, MetadataOnlyReason};
use crate::live_index::LiveIndex;
use std::path::Path;

pub(crate) fn hard_scope_refusal(relative_path: &str) -> Option<String> {
    crate::discovery::path_is_hard_scope_excluded(Path::new(relative_path)).then(|| {
        format!(
            "{relative_path} [error: VCS and runtime-state internals are outside \
             source scope; a disk observation never reads them]"
        )
    })
}

pub(crate) fn normalize_requested_path(raw: &str) -> String {
    let mut normalized = raw.trim().replace('\\', "/");
    while normalized.starts_with("./") {
        normalized = normalized[2..].to_string();
    }
    normalized.trim_matches('/').to_string()
}

pub(crate) fn unverified_notice(live: &LiveIndex, requested: &str) -> Option<String> {
    let path = normalize_requested_path(requested);
    if crate::knowledge::sensitive_path_rule(&path).is_some() {
        return None;
    }
    live.unverified_since_restore(&path)
        .map(|reason| format::unverified_since_restore(&path, reason))
}

pub(crate) fn refuse_by_policy_with(
    live: &LiveIndex,
    relative_path: &str,
    record: &mut dyn FnMut(WithheldMeta),
) -> Option<String> {
    // Current path rule — no read needed.
    if let Some(rule_id) = crate::knowledge::sensitive_path_rule(relative_path) {
        record(WithheldMeta::path_rule_only(relative_path, rule_id));
        return Some(format::content_withheld_by_path_rule(
            relative_path,
            rule_id,
        ));
    }

    // Recorded disposition on the publication that produced the miss — no read
    // needed. A missing entry is not authorization: it means the manifest has
    // nothing to say, and the current-bytes classification still applies.
    if let Some(FileDisposition::MetadataOnly { reason }) =
        live.capture_file_disposition(relative_path)
    {
        match reason {
            // A recorded content demotion carrying the reserved indeterminate id
            // is a detector FAILURE, not a match: reindexing cannot change it.
            MetadataOnlyReason::SensitiveContent { rule_ids, .. }
                if rule_ids
                    .iter()
                    .any(|id| id == crate::knowledge::INDETERMINATE_RULE_ID) =>
            {
                record(WithheldMeta::unscanned(relative_path));
                return Some(format::content_withheld_unscanned(relative_path));
            }
            MetadataOnlyReason::SensitivePath { rule_id } => {
                record(WithheldMeta::path_rule_only(relative_path, rule_id));
                return Some(format::content_withheld_by_path_rule(
                    relative_path,
                    rule_id,
                ));
            }
            MetadataOnlyReason::SensitiveContent {
                rule_ids,
                finding_count,
            } => {
                return Some(format::content_withheld_by_admission(
                    relative_path,
                    rule_ids,
                    *finding_count,
                    &[],
                ));
            }
            _ => {}
        }
    }
    // Last, so a sensitive path keeps its policy refusal: a restored row the
    // snapshot verify could not reconcile. Its index row is withheld, and
    // disk bytes are not a substitute the verify vouched for.
    unverified_notice(live, relative_path)
}

pub(crate) fn classify_admitted_bytes_with(
    live: &LiveIndex,
    relative_path: &str,
    bytes: &[u8],
    record: &mut dyn FnMut(WithheldMeta),
) -> Option<String> {
    // Fail closed on bytes the detector cannot have inspected, and do it HERE so
    // the refusal MESSAGE can be honest. `classify_stable_content` demotes both
    // populations correctly on its own — it collapses the scan-budget refusal
    // into `SensitiveContent`, and since Ruling 4 it encoding-validates the whole
    // buffer on every path — but neither cause is legible in that verdict, so its
    // refusal would name a detector match that never happened. Placed before
    // `classify_stable_content` so a binary buffer is not pointlessly scanned;
    // `detect_lfs_pointer` requires valid UTF-8 under 1 KiB, so no pointer is
    // swallowed here.
    if crate::knowledge::exceeds_scan_limit(bytes.len())
        || crate::knowledge::decode_searchable_text(bytes).is_err()
    {
        record(WithheldMeta::unscanned(relative_path));
        return Some(format::content_withheld_unscanned(relative_path));
    }
    let language = Path::new(relative_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .and_then(LanguageId::from_extension);
    let targets = IndexTargets::for_path(relative_path, language.as_ref());
    // Only the two security variants deny. Every other `MetadataOnlyReason`
    // (binary, encoding, LFS, path collision, oversized, …) keeps today's
    // behavior — this gate takes no position on them. The resource-limit and
    // encoding cases are already decided above, so the remaining `Indeterminate`
    // failures here are the global ones (policy compilation, internal), which
    // `classify_stable_content` maps to `SensitiveContent` carrying the reserved
    // indeterminate id — a detector failure, so the honest message, not the one
    // naming a match.
    // The scan's finding lines are kept beside the verdict for the refusal
    // text; they are never part of the recorded disposition.
    let mut finding_lines = Vec::new();
    let mut finding_descriptors: Vec<crate::knowledge::SecretFindingDescriptor> = Vec::new();
    if let crate::knowledge::StableContentAdmission::MetadataOnly(
        MetadataOnlyReason::SensitiveContent {
            rule_ids,
            finding_count,
        },
    ) = crate::knowledge::classify_stable_content_with(
        relative_path,
        targets,
        bytes,
        |path, bytes| {
            let scan = match live.indexed_root.as_deref() {
                Some(root) => {
                    crate::knowledge::secret_dismissals::scan_with_dismissals(root, path, bytes)
                }
                None => crate::knowledge::scan_secret_bytes(path, bytes),
            };
            if let crate::knowledge::SecretScan::Sensitive {
                line_ranges,
                findings,
                ..
            } = &scan
            {
                finding_lines.clone_from(line_ranges);
                finding_descriptors.clone_from(findings);
            }
            scan
        },
    ) {
        return Some(
            if rule_ids
                .iter()
                .any(|id| id == crate::knowledge::INDETERMINATE_RULE_ID)
            {
                record(WithheldMeta::unscanned(relative_path));
                format::content_withheld_unscanned(relative_path)
            } else {
                if !finding_descriptors.is_empty() {
                    record(WithheldMeta::from_content_findings(
                        relative_path,
                        &finding_descriptors,
                    ));
                }
                format::content_withheld_by_admission(
                    relative_path,
                    &rule_ids,
                    finding_count,
                    &finding_lines,
                )
            },
        );
    }

    // Permit. These are the only bytes any gated lane may render or parse.
    None
}

/// Match bounded current bytes to the recorded rule set before adding actionable
/// positions to a refusal. A changed verdict never weakens the recorded refusal.
pub(crate) fn recorded_finding_evidence_from_bytes(
    relative_path: &str,
    bytes: &[u8],
    recorded: &[String],
) -> (
    Vec<(u32, u32)>,
    Vec<crate::knowledge::SecretFindingDescriptor>,
) {
    if crate::knowledge::exceeds_scan_limit(bytes.len()) {
        return (Vec::new(), Vec::new());
    }
    match crate::knowledge::scan_secret_bytes(relative_path, bytes) {
        crate::knowledge::SecretScan::Sensitive {
            rule_ids,
            line_ranges,
            findings,
            ..
        } if rule_ids.len() == recorded.len()
            && rule_ids
                .iter()
                .all(|rule| recorded.iter().any(|seen| seen == rule)) =>
        {
            (line_ranges, findings)
        }
        _ => (Vec::new(), Vec::new()),
    }
}
