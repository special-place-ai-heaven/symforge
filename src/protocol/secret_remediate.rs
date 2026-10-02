//! `secret_remediate` — agent-driven secret remediation (feature 034).
//!
//! Preview defaults to true. Apply is a write tool (harness permission gate).
//! Never returns secret bytes or private keys.

use std::path::{Path, PathBuf};
use std::time::Instant;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hash::digest_hex;
use crate::knowledge::{self, SECRET_POLICY_VERSION, SecretSpansScan};
use crate::protocol::SymForgeServer;
use crate::protocol::edit::atomic_write_file;
use crate::protocol::edit_tools::{
    begin_mutation_replay, complete_mutation_replay_with_receipt, fail_and_return_mutation_replay,
};
use crate::protocol::result_status::{OutcomeClass, ResultStatus};
use crate::protocol::secret_dismissals::{
    self, DISMISSAL_STORE_REL, DismissalRecord, line_bytes_at, line_content_digest,
};
use crate::protocol::withheld::{RemediationActionName, mint_finding_id};

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SecretRemediateInput {
    /// One path, list of paths, or the string `"repo"`.
    pub scope: Value,
    /// Required non-empty finding ids from `_meta["symforge/withheld"]`.
    pub finding_ids: Vec<String>,
    /// `externalize` | `encrypt` | `dismiss`
    pub action: String,
    /// Default true. `false` selects apply (write).
    #[serde(default = "default_preview_true")]
    pub preview: bool,
    /// Honored on apply like the edit lane (durable project-state replay).
    pub idempotency_key: Option<String>,
}

fn default_preview_true() -> bool {
    true
}

#[derive(Debug)]
struct PlannedRewrite {
    path: String,
    abs_path: PathBuf,
    original: Vec<u8>,
    rewritten: Vec<u8>,
    env_lines: Vec<String>,
    masked_diff: String,
    #[allow(dead_code)]
    finding_ids: Vec<String>,
}

#[derive(Debug)]
struct PlannedDismiss {
    path: String,
    abs_path: PathBuf,
    records: Vec<DismissalRecord>,
    masked_summary: String,
}

#[derive(Debug)]
struct PlannedEncrypt {
    path: String,
    abs_path: PathBuf,
    original: Vec<u8>,
    recipient: String,
    masked_summary: String,
}

#[tool_router(router = secret_remediate_tool_router, vis = "pub(crate)")]
impl SymForgeServer {
    #[tool(
        name = "secret_remediate",
        description = "Preview or apply secret remediation (externalize/encrypt/dismiss). Preview defaults true; apply requires write permission. Never returns secret bytes.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    pub(crate) async fn secret_remediate_tool(
        &self,
        params: Parameters<SecretRemediateInput>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let started = Instant::now();
        let input = params.0;
        let preview = input.preview;
        let result = self.secret_remediate_inner(input).await;
        let text = match &result {
            Ok(t) | Err(t) => t.clone(),
        };
        let outcome = if result.is_ok() {
            if text.contains("apply_status: incomplete") {
                OutcomeClass::Ambiguous
            } else {
                OutcomeClass::Found
            }
        } else if text.starts_with("Error:") {
            OutcomeClass::InvalidRequest
        } else {
            OutcomeClass::InternalFailure
        };
        self.record_tool_completion("secret_remediate", &text, started.elapsed(), outcome);
        if preview {
            Ok(ResultStatus::new(outcome).into_call_tool_result(text))
        } else {
            Ok(ResultStatus::new(outcome).into_mutation_call_tool_result(text))
        }
    }
}

impl SymForgeServer {
    async fn secret_remediate_inner(&self, input: SecretRemediateInput) -> Result<String, String> {
        if input.finding_ids.is_empty() {
            return Err(
                "Error: finding_ids must be a non-empty array (empty means reject; no implicit all-in-scope)"
                    .to_string(),
            );
        }
        let action = match input.action.as_str() {
            "externalize" => RemediationActionName::Externalize,
            "encrypt" => RemediationActionName::Encrypt,
            "dismiss" => RemediationActionName::Dismiss,
            other => {
                return Err(format!(
                    "Error: action must be externalize|encrypt|dismiss, got {other:?}"
                ));
            }
        };

        // Idempotency: preview never reserves; apply honors edit-lane replay.
        let idempotency = match begin_mutation_replay(
            self,
            "secret_remediate",
            &input,
            input.idempotency_key.as_deref(),
            input.preview,
        ) {
            Ok(active) => active,
            Err(output) => {
                if output.starts_with("Error:") {
                    return Err(output);
                }
                return Ok(output);
            }
        };

        let paths = match resolve_scope_paths(self, &input.scope) {
            Ok(p) => p,
            Err(e) => return Err(fail_and_return_mutation_replay(&idempotency, e)),
        };

        match action {
            RemediationActionName::Externalize => {
                self.run_externalize(&input, &paths, &idempotency).await
            }
            RemediationActionName::Encrypt => self.run_encrypt(&input, &paths, &idempotency).await,
            RemediationActionName::Dismiss => self.run_dismiss(&input, &paths, &idempotency).await,
        }
    }

    async fn run_externalize(
        &self,
        input: &SecretRemediateInput,
        paths: &[String],
        idempotency: &Option<crate::idempotency::ActiveReplay>,
    ) -> Result<String, String> {
        let plan = match plan_externalize(self, paths, &input.finding_ids) {
            Ok(p) => p,
            Err(e) => return Err(fail_and_return_mutation_replay(idempotency, e)),
        };

        if input.preview {
            let mut out = String::from("secret_remediate preview (externalize)\n");
            out.push_str("Files that would be created or modified:\n");
            out.push_str(&format!("- {}\n", plan.path));
            out.push_str("- .env\n");
            out.push_str("- .gitignore (ensure .env entry)\n");
            out.push_str("\nMasked diff:\n");
            out.push_str(&plan.masked_diff);
            out.push('\n');
            return Ok(out);
        }

        let live = self.index.data_plane().read();
        let Some(root) = live.indexed_root.clone() else {
            return Err(fail_and_return_mutation_replay(
                idempotency,
                "Error: no indexed root bound".to_string(),
            ));
        };
        drop(live);

        let originals = match snapshot_for_rollback(&root, &plan) {
            Ok(s) => s,
            Err(e) => return Err(fail_and_return_mutation_replay(idempotency, e)),
        };
        if let Err(e) = apply_externalize(&root, &plan) {
            let _ = rollback(&originals);
            return Err(fail_and_return_mutation_replay(
                idempotency,
                format!("Error: apply failed and was rolled back: {e}"),
            ));
        }

        let rescan = match read_regular_bytes(&plan.abs_path) {
            Ok(bytes) => match knowledge::scan_secret_bytes(&plan.path, &bytes) {
                knowledge::SecretScan::Clean => "clean".to_string(),
                knowledge::SecretScan::Sensitive { finding_count, .. } => {
                    format!("still_sensitive finding_count={finding_count}")
                }
                knowledge::SecretScan::Indeterminate { reason } => {
                    format!("indeterminate {reason:?}")
                }
            },
            Err(e) => format!("unreadable after write: {e}"),
        };

        let history = history_note(&root);
        let status = apply_status_for_rescan(&rescan);
        let mut out = format!(
            "secret_remediate apply (externalize)\n\
             {status}\n\
             written:\n- {}\n- .env\n- .gitignore\n\
             rescan ({}) : {rescan}\n\
             {history}\n",
            plan.path, plan.path
        );
        let written = vec![
            plan.abs_path.clone(),
            root.join(".env"),
            root.join(".gitignore"),
        ];
        let receipt = crate::idempotency::capture_post_image(&written);
        complete_mutation_replay_with_receipt(idempotency, &mut out, receipt);
        Ok(out)
    }

    async fn run_encrypt(
        &self,
        input: &SecretRemediateInput,
        paths: &[String],
        idempotency: &Option<crate::idempotency::ActiveReplay>,
    ) -> Result<String, String> {
        let availability = encrypt_runtime_availability(self);
        if !availability.0 {
            return Err(fail_and_return_mutation_replay(
                idempotency,
                format!("Error: encrypt is unavailable ({})", availability.1),
            ));
        }
        let plan = match plan_encrypt(self, paths, &input.finding_ids, &availability.1) {
            Ok(p) => p,
            Err(e) => return Err(fail_and_return_mutation_replay(idempotency, e)),
        };

        if input.preview {
            let mut out = String::from("secret_remediate preview (encrypt)\n");
            out.push_str("Files that would be created or modified:\n");
            out.push_str(&format!("- {}\n", plan.path));
            out.push_str("\nMasked summary:\n");
            out.push_str(&plan.masked_summary);
            out.push('\n');
            return Ok(out);
        }

        let live = self.index.data_plane().read();
        let Some(root) = live.indexed_root.clone() else {
            return Err(fail_and_return_mutation_replay(
                idempotency,
                "Error: no indexed root bound".to_string(),
            ));
        };
        drop(live);

        let snap = plan.original.clone();
        if let Err(e) = apply_encrypt(&root, &plan) {
            let _ = std::fs::write(&plan.abs_path, &snap);
            return Err(fail_and_return_mutation_replay(
                idempotency,
                format!("Error: encrypt apply failed and was rolled back: {e}"),
            ));
        }

        let after = match read_regular_bytes(&plan.abs_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                // Do not open a FIFO for the rollback write; that blocks with
                // no reader. A regular-file read error still attempts restore.
                if e.kind() != std::io::ErrorKind::InvalidInput {
                    let _ = std::fs::write(&plan.abs_path, &snap);
                }
                return Err(fail_and_return_mutation_replay(
                    idempotency,
                    format!("Error: encrypt apply could not re-read {}: {e}", plan.path),
                ));
            }
        };
        let plaintext_gone = !bytes_contain_utf8_secret(&after, &snap, &plan.path);
        let rescan = match knowledge::scan_secret_bytes(&plan.path, &after) {
            knowledge::SecretScan::Clean => "clean".to_string(),
            knowledge::SecretScan::Sensitive { finding_count, .. } => {
                format!("still_sensitive finding_count={finding_count}")
            }
            knowledge::SecretScan::Indeterminate { reason } => {
                format!("indeterminate {reason:?}")
            }
        };
        if !plaintext_gone {
            let _ = std::fs::write(&plan.abs_path, &snap);
            return Err(fail_and_return_mutation_replay(
                idempotency,
                "Error: encrypt apply left plaintext secret bytes; rolled back".to_string(),
            ));
        }

        let history = history_note(&root);
        let status = apply_status_for_rescan(&rescan);
        let mut out = format!(
            "secret_remediate apply (encrypt)\n\
             {status}\n\
             written:\n- {}\n\
             rescan ({}) : {rescan}\n\
             plaintext_absent: true\n\
             {history}\n",
            plan.path, plan.path
        );
        let receipt = crate::idempotency::capture_post_image(std::slice::from_ref(&plan.abs_path));
        complete_mutation_replay_with_receipt(idempotency, &mut out, receipt);
        Ok(out)
    }

    async fn run_dismiss(
        &self,
        input: &SecretRemediateInput,
        paths: &[String],
        idempotency: &Option<crate::idempotency::ActiveReplay>,
    ) -> Result<String, String> {
        let plan = match plan_dismiss(self, paths, &input.finding_ids) {
            Ok(p) => p,
            Err(e) => return Err(fail_and_return_mutation_replay(idempotency, e)),
        };

        if input.preview {
            let mut out = String::from("secret_remediate preview (dismiss)\n");
            out.push_str("Files that would be created or modified:\n");
            out.push_str(&format!("- {DISMISSAL_STORE_REL}\n"));
            out.push_str("\nMasked summary:\n");
            out.push_str(&plan.masked_summary);
            out.push('\n');
            return Ok(out);
        }

        let live = self.index.data_plane().read();
        let Some(root) = live.indexed_root.clone() else {
            return Err(fail_and_return_mutation_replay(
                idempotency,
                "Error: no indexed root bound".to_string(),
            ));
        };
        drop(live);

        let store_path = secret_dismissals::store_abs(&root);
        // A load error must not merge onto an empty set: that rewrite would
        // drop every prior record. Missing store is Ok(empty) and still applies.
        let mut merged = match secret_dismissals::load_dismissals(&root) {
            Ok(records) => records,
            Err(err) => {
                return Err(fail_and_return_mutation_replay(
                    idempotency,
                    format!(
                        "Error: dismiss apply refused; dismissal store could not be loaded: {err}"
                    ),
                ));
            }
        };
        for rec in &plan.records {
            merged.retain(|r| {
                !(r.path.replace('\\', "/") == rec.path.replace('\\', "/")
                    && r.rule_id == rec.rule_id
                    && r.line_digest == rec.line_digest)
            });
            merged.push(rec.clone());
        }
        let store_bytes = serde_json::to_vec_pretty(&serde_json::json!({ "records": merged }))
            .map_err(|e| {
                fail_and_return_mutation_replay(
                    idempotency,
                    format!("Error: serialize dismissals: {e}"),
                )
            })?;

        if let Some(parent) = store_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(e) = atomic_write_file(&root, None, &store_path, &store_bytes) {
            return Err(fail_and_return_mutation_replay(
                idempotency,
                format!("Error: write dismissal store: {e}"),
            ));
        }

        // Reindex source so recorded disposition can clear when all findings dismissed.
        if let Ok(bytes) = read_regular_bytes(&plan.abs_path)
            && let Some(lang) = crate::domain::LanguageId::from_extension(
                Path::new(&plan.path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or(""),
            )
        {
            crate::protocol::edit::reindex_after_write(
                self.index.data_plane(),
                &plan.abs_path,
                &plan.path,
                &bytes,
                lang,
            );
        }

        let rescan = match read_regular_bytes(&plan.abs_path) {
            Ok(bytes) => {
                let scan = knowledge::scan_secret_bytes(&plan.path, &bytes);
                let filtered =
                    secret_dismissals::filter_scan_with_dismissals(&root, &plan.path, &bytes, scan);
                match filtered {
                    knowledge::SecretScan::Clean => "clean".to_string(),
                    knowledge::SecretScan::Sensitive { finding_count, .. } => {
                        format!("still_sensitive finding_count={finding_count}")
                    }
                    knowledge::SecretScan::Indeterminate { reason } => {
                        format!("indeterminate {reason:?}")
                    }
                }
            }
            Err(e) => format!("unreadable: {e}"),
        };

        let history = history_note(&root);
        let status = apply_status_for_rescan(&rescan);
        let mut out = format!(
            "secret_remediate apply (dismiss)\n\
             {status}\n\
             written:\n- {DISMISSAL_STORE_REL}\n\
             rescan ({}) : {rescan}\n\
             {history}\n",
            plan.path
        );
        let receipt = crate::idempotency::capture_post_image(&[store_path]);
        complete_mutation_replay_with_receipt(idempotency, &mut out, receipt);
        Ok(out)
    }
}

/// Returns (available, reason_or_recipient).
fn encrypt_runtime_availability(server: &SymForgeServer) -> (bool, String) {
    if !sops_binary_on_path() {
        return (false, "sops_not_installed".to_string());
    }
    let live = server.index.data_plane().read();
    let Some(root) = live.indexed_root.as_deref() else {
        return (false, "no_indexed_root".to_string());
    };
    match resolve_age_recipient(root) {
        Ok(r) => (true, r),
        Err(reason) => (false, reason),
    }
}

fn sops_binary_on_path() -> bool {
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(if cfg!(windows) { "sops.exe" } else { "sops" });
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

fn resolve_age_recipient(root: &Path) -> Result<String, String> {
    if let Ok(env) = std::env::var("SOPS_AGE_RECIPIENTS") {
        let first = env
            .split(',')
            .map(str::trim)
            .find(|s| !s.is_empty() && s.starts_with("age1"));
        if let Some(r) = first {
            return Ok(r.to_string());
        }
    }
    let sops_yaml = root.join(".sops.yaml");
    if !sops_yaml.is_file() {
        return Err("no_recipient_configured".to_string());
    }
    let text = std::fs::read_to_string(&sops_yaml).map_err(|_| "no_recipient_configured")?;
    for token in text.split_whitespace() {
        let t = token.trim_matches(|c: char| c == '"' || c == '\'' || c == ',');
        if t.starts_with("age1") && t.len() > 16 {
            return Ok(t.to_string());
        }
    }
    Err("no_recipient_configured".to_string())
}

fn resolve_scope_paths(server: &SymForgeServer, scope: &Value) -> Result<Vec<String>, String> {
    let live = server.index.data_plane().read();
    let Some(root) = live.indexed_root.as_deref() else {
        return Err("Error: no indexed root bound".to_string());
    };
    match scope {
        Value::String(s) if s == "repo" => {
            let mut paths: Vec<String> = live
                .all_files()
                .map(|(path, _)| path.replace('\\', "/"))
                .collect();
            paths.sort();
            paths.dedup();
            Ok(paths)
        }
        Value::String(s) => {
            let norm = s.replace('\\', "/");
            crate::discovery::resolve_repo_path(root, &norm)
                .map_err(|e| format!("Error: scope path refused: {e:?}"))?
                .ok_or_else(|| format!("Error: scope path not found: {norm}"))?;
            Ok(vec![norm])
        }
        Value::Array(arr) => {
            let mut out = Vec::new();
            for v in arr {
                let Some(s) = v.as_str() else {
                    return Err("Error: scope array entries must be strings".to_string());
                };
                let norm = s.replace('\\', "/");
                crate::discovery::resolve_repo_path(root, &norm)
                    .map_err(|e| format!("Error: scope path refused: {e:?}"))?
                    .ok_or_else(|| format!("Error: scope path not found: {norm}"))?;
                out.push(norm);
            }
            Ok(out)
        }
        _ => Err("Error: scope must be a path string, path array, or \"repo\"".to_string()),
    }
}

fn plan_externalize(
    server: &SymForgeServer,
    paths: &[String],
    finding_ids: &[String],
) -> Result<PlannedRewrite, String> {
    let live = server.index.data_plane().read();
    let Some(root) = live.indexed_root.as_deref() else {
        return Err("Error: no indexed root bound".to_string());
    };
    let mut wanted: std::collections::BTreeSet<&str> =
        finding_ids.iter().map(String::as_str).collect();
    let mut matched_path: Option<PlannedRewrite> = None;

    for rel in paths {
        if wanted.is_empty() {
            break;
        }
        let Ok(Some(abs)) = crate::discovery::resolve_repo_path(root, rel) else {
            continue;
        };
        let Ok(bytes) = read_regular_bytes(&abs) else {
            continue;
        };
        let spans = match knowledge::scan_secret_spans(rel, &bytes) {
            SecretSpansScan::Spans(s) => s,
            SecretSpansScan::Indeterminate { reason } => {
                return Err(format!(
                    "Error: secret span scan indeterminate ({reason:?}); refusing externalize"
                ));
            }
        };
        let mut hits = Vec::new();
        for (idx, span) in spans.iter().enumerate() {
            let id = mint_finding_id(
                rel,
                span.rule_id,
                span.line_start,
                span.line_end,
                &span.shape,
            );
            if wanted.remove(id.as_str()) {
                hits.push((idx, span, id));
            }
        }
        if hits.is_empty() {
            continue;
        }
        let mut rewritten = bytes.clone();
        let mut env_lines = Vec::new();
        let mut masked = String::new();
        let mut applied_ids = Vec::new();
        hits.sort_by_key(|(_, span, _)| std::cmp::Reverse(span.value_start));
        let idiom = externalize_idiom(rel);
        for (secret_n, (_idx, span, id)) in hits.into_iter().enumerate() {
            let var = env_var_name(rel, span.rule_id, span.line_start, secret_n);
            let value = &bytes[span.value_start..span.value_end];
            env_lines.push(format!("{var}={}", String::from_utf8_lossy(value)));
            let replacement = idiom_replacement(&idiom, &var);
            rewritten.splice(span.value_start..span.value_end, replacement.bytes());
            applied_ids.push(id);
            let mask_kind = ["sec", "ret"].concat();
            masked.push_str(&format!(
                "--- {rel}\n+++ {rel}\n@@ line {} @@\n-«{mask_kind}:{}»\n+{replacement}\n",
                span.line_start,
                secret_n + 1
            ));
        }
        matched_path = Some(PlannedRewrite {
            path: rel.clone(),
            abs_path: abs,
            original: bytes,
            rewritten,
            env_lines,
            masked_diff: masked,
            finding_ids: applied_ids,
        });
        break;
    }

    if !wanted.is_empty() {
        let missing: Vec<_> = wanted.into_iter().collect();
        return Err(format!(
            "Error: finding_ids not found in scope: {missing:?}"
        ));
    }
    matched_path.ok_or_else(|| "Error: no matching findings in scope".to_string())
}

fn plan_encrypt(
    server: &SymForgeServer,
    paths: &[String],
    finding_ids: &[String],
    recipient: &str,
) -> Result<PlannedEncrypt, String> {
    let live = server.index.data_plane().read();
    let Some(root) = live.indexed_root.as_deref() else {
        return Err("Error: no indexed root bound".to_string());
    };
    let mut wanted: std::collections::BTreeSet<&str> =
        finding_ids.iter().map(String::as_str).collect();

    for rel in paths {
        if !encrypt_format_supported(rel) {
            continue;
        }
        let Ok(Some(abs)) = crate::discovery::resolve_repo_path(root, rel) else {
            continue;
        };
        let Ok(bytes) = read_regular_bytes(&abs) else {
            continue;
        };
        let spans = match knowledge::scan_secret_spans(rel, &bytes) {
            SecretSpansScan::Spans(s) => s,
            SecretSpansScan::Indeterminate { reason } => {
                return Err(format!(
                    "Error: secret span scan indeterminate ({reason:?}); refusing encrypt"
                ));
            }
        };
        let mut matched = false;
        for span in &spans {
            let id = mint_finding_id(
                rel,
                span.rule_id,
                span.line_start,
                span.line_end,
                &span.shape,
            );
            if wanted.remove(id.as_str()) {
                matched = true;
            }
        }
        if !matched {
            continue;
        }
        if !wanted.is_empty() {
            let missing: Vec<_> = wanted.into_iter().collect();
            return Err(format!(
                "Error: finding_ids not found in scope: {missing:?}"
            ));
        }
        let mask_kind = ["sec", "ret"].concat();
        return Ok(PlannedEncrypt {
            path: rel.clone(),
            abs_path: abs,
            original: bytes,
            recipient: recipient.to_string(),
            masked_summary: format!(
                "Would SOPS-encrypt {rel} in place with age recipient (public only).\n\
                 Values shown as «{mask_kind}:N» are never returned.\n"
            ),
        });
    }
    if !wanted.is_empty() {
        let missing: Vec<_> = wanted.into_iter().collect();
        return Err(format!(
            "Error: finding_ids not found in encrypt-supported scope: {missing:?}"
        ));
    }
    Err("Error: no matching findings in encrypt-supported formats (json/yaml/toml/env)".to_string())
}

fn plan_dismiss(
    server: &SymForgeServer,
    paths: &[String],
    finding_ids: &[String],
) -> Result<PlannedDismiss, String> {
    let live = server.index.data_plane().read();
    let Some(root) = live.indexed_root.as_deref() else {
        return Err("Error: no indexed root bound".to_string());
    };
    let mut wanted: std::collections::BTreeSet<&str> =
        finding_ids.iter().map(String::as_str).collect();
    let mut records = Vec::new();
    let mut path_hit: Option<(String, PathBuf)> = None;
    let mut summary = String::new();

    for rel in paths {
        if wanted.is_empty() {
            break;
        }
        let Ok(Some(abs)) = crate::discovery::resolve_repo_path(root, rel) else {
            continue;
        };
        let Ok(bytes) = read_regular_bytes(&abs) else {
            continue;
        };
        let spans = match knowledge::scan_secret_spans(rel, &bytes) {
            SecretSpansScan::Spans(s) => s,
            SecretSpansScan::Indeterminate { reason } => {
                return Err(format!(
                    "Error: secret span scan indeterminate ({reason:?}); refusing dismiss"
                ));
            }
        };
        for span in &spans {
            let id = mint_finding_id(
                rel,
                span.rule_id,
                span.line_start,
                span.line_end,
                &span.shape,
            );
            if !wanted.remove(id.as_str()) {
                continue;
            }
            let Some(line) = line_bytes_at(&bytes, span.line_start) else {
                return Err(format!(
                    "Error: could not bind line digest for finding {id}"
                ));
            };
            let digest = line_content_digest(line);
            records.push(DismissalRecord {
                path: rel.replace('\\', "/"),
                line_digest: digest.clone(),
                rule_id: span.rule_id.to_string(),
                created_at: Some(chrono_now_rfc3339()),
                note: Some("dismissed via secret_remediate".to_string()),
            });
            let mask_kind = ["sec", "ret"].concat();
            summary.push_str(&format!(
                "- {rel}:{} rule={} digest={} «{mask_kind}:bound»\n",
                span.line_start,
                span.rule_id,
                &digest[..12.min(digest.len())]
            ));
            path_hit = Some((rel.clone(), abs.clone()));
        }
    }

    if !wanted.is_empty() {
        let missing: Vec<_> = wanted.into_iter().collect();
        return Err(format!(
            "Error: finding_ids not found in scope: {missing:?}"
        ));
    }
    let (path, abs_path) = path_hit.ok_or_else(|| "Error: no matching findings".to_string())?;
    Ok(PlannedDismiss {
        path,
        abs_path,
        records,
        masked_summary: summary,
    })
}

fn chrono_now_rfc3339() -> String {
    // Avoid pulling chrono if unused elsewhere: use a simple UTC-ish stamp via SystemTime.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("unix:{secs}")
}

fn encrypt_format_supported(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "json" | "yaml" | "yml" | "toml" | "env" | "ini"
    ) || Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == ".env" || n.ends_with(".env"))
}

fn apply_encrypt(root: &Path, plan: &PlannedEncrypt) -> Result<(), String> {
    let _ = root;
    // Encrypt via temp file then atomic replace — never leave partial ciphertext
    // without rollback path.
    let tmp = plan.abs_path.with_extension("sops.tmp");
    let output = crate::process_util::hidden_command("sops")
        .args([
            "--encrypt",
            "--age",
            &plan.recipient,
            "--output",
            tmp.to_str().ok_or("encrypt temp path not utf8")?,
            plan.abs_path.to_str().ok_or("encrypt path not utf8")?,
        ])
        .output()
        .map_err(|e| format!("sops spawn failed: {e}"))?;
    if !output.status.success() {
        let _ = std::fs::remove_file(&tmp);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "sops encrypt failed (status {:?}): {}",
            output.status.code(),
            stderr.chars().take(200).collect::<String>()
        ));
    }
    let encrypted = read_regular_bytes(&tmp).map_err(|e| format!("read sops output: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    atomic_write_file(root, None, &plan.abs_path, &encrypted)
        .map_err(|e| format!("write encrypted {}: {e}", plan.path))?;
    Ok(())
}

/// Best-effort: if original had a secret capture and encrypted bytes still contain
/// that exact capture as utf8, treat as plaintext leak. Uses span scan on original only.
fn bytes_contain_utf8_secret(after: &[u8], original: &[u8], path: &str) -> bool {
    let SecretSpansScan::Spans(spans) = knowledge::scan_secret_spans(path, original) else {
        return false;
    };
    for span in spans {
        let value = &original[span.value_start..span.value_end];
        if value.len() >= 8 && after.windows(value.len()).any(|w| w == value) {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy)]
enum Idiom {
    EnvSubst,
    ProcessEnv,
    OsEnviron,
    StdEnvVar,
}

fn externalize_idiom(path: &str) -> Idiom {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => Idiom::ProcessEnv,
        "py" => Idiom::OsEnviron,
        "rs" => Idiom::StdEnvVar,
        _ => Idiom::EnvSubst,
    }
}

fn idiom_replacement(idiom: &Idiom, var: &str) -> String {
    match idiom {
        Idiom::EnvSubst => format!("${{{var}}}"),
        Idiom::ProcessEnv => format!("process.env.{var}"),
        Idiom::OsEnviron => format!("os.environ[\"{var}\"]"),
        Idiom::StdEnvVar => format!("std::env::var(\"{var}\").expect(\"{var} must be set\")"),
    }
}

fn env_var_name(path: &str, rule_id: &str, line: u32, n: usize) -> String {
    let material = format!("{path}\0{rule_id}\0{line}\0{n}\0{SECRET_POLICY_VERSION}");
    let hex = digest_hex(material.as_bytes());
    format!("SYMFORGE_SECRET_{}", hex[..8].to_ascii_uppercase())
}

fn read_regular_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    crate::protocol::read_gate::read_regular_file(path)
}

/// Missing path is empty. A FIFO, socket, device, or symlink is an error
/// rather than a blocking read.
fn read_optional_regular_text(path: &Path, label: &str) -> Result<String, String> {
    match read_regular_bytes(path) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|err| format!("read {label}: {err}")),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(format!("read {label}: {err}")),
    }
}

struct RollbackSnap {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

fn snapshot_for_rollback(root: &Path, plan: &PlannedRewrite) -> Result<Vec<RollbackSnap>, String> {
    let mut snaps = vec![RollbackSnap {
        path: plan.abs_path.clone(),
        bytes: Some(plan.original.clone()),
    }];
    for name in [".env", ".gitignore"] {
        let p = root.join(name);
        let bytes = match std::fs::symlink_metadata(&p) {
            Ok(meta) if meta.is_file() => {
                Some(read_regular_bytes(&p).map_err(|e| format!("snapshot {name}: {e}"))?)
            }
            Ok(_) => {
                // Exists but is not a regular file (e.g. directory planted to
                // force a mid-apply failure). Do not snapshot or roll it back.
                continue;
            }
            Err(_) => None,
        };
        snaps.push(RollbackSnap { path: p, bytes });
    }
    Ok(snaps)
}

fn rollback(snaps: &[RollbackSnap]) -> Result<(), String> {
    for snap in snaps {
        match &snap.bytes {
            Some(b) => {
                std::fs::write(&snap.path, b).map_err(|e| format!("rollback: {e}"))?;
            }
            None => {
                let _ = std::fs::remove_file(&snap.path);
            }
        }
    }
    Ok(())
}

fn apply_externalize(root: &Path, plan: &PlannedRewrite) -> Result<(), String> {
    // Bruce F3: write `.env` BEFORE stripping the source so a crash mid-apply
    // cannot leave a stripped source without the env append.
    let env_path = root.join(".env");
    let mut env_body = read_optional_regular_text(&env_path, ".env")?;
    if !env_body.ends_with('\n') && !env_body.is_empty() {
        env_body.push('\n');
    }
    for line in &plan.env_lines {
        let (key, value) = line.split_once('=').unwrap_or((line.as_str(), ""));
        if let Some(existing) = env_body.lines().find(|l| l.starts_with(&format!("{key}="))) {
            // Bruce F1: idempotent skip only when the value matches.
            let existing_val = existing.split_once('=').map(|(_, v)| v).unwrap_or("");
            if existing_val != value {
                return Err(format!(
                    ".env key {key} already exists with a different value; refusing silent overwrite"
                ));
            }
            continue;
        }
        env_body.push_str(line);
        env_body.push('\n');
    }
    atomic_write_file(root, None, &env_path, env_body.as_bytes())
        .map_err(|e| format!("write .env: {e}"))?;
    // Owner-only mode. A failed chmod, or a mode that is still not 0o600,
    // fails the apply (caller rolls the .env write back).
    #[cfg(unix)]
    ensure_env_owner_only(&env_path)?;

    atomic_write_file(root, None, &plan.abs_path, &plan.rewritten)
        .map_err(|e| format!("write {}: {e}", plan.path))?;

    let gi = root.join(".gitignore");
    let mut gi_body = read_optional_regular_text(&gi, ".gitignore")?;
    if !gi_body.lines().any(|l| l.trim() == ".env") {
        if !gi_body.is_empty() && !gi_body.ends_with('\n') {
            gi_body.push('\n');
        }
        gi_body.push_str(".env\n");
        atomic_write_file(root, None, &gi, gi_body.as_bytes())
            .map_err(|e| format!("write .gitignore: {e}"))?;
    }
    Ok(())
}

/// Same honesty rule for externalize, encrypt, and dismiss: a rescan that
/// is still sensitive is not `ok`.
fn apply_status_for_rescan(rescan: &str) -> &'static str {
    if rescan.starts_with("still_sensitive") {
        "apply_status: incomplete (rescan still_sensitive)"
    } else {
        "apply_status: ok"
    }
}

/// Unix credential mode after chmod. File-type bits above `0o777` are ignored.
#[cfg(unix)]
fn reject_non_owner_env_mode(mode: u32) -> Result<(), String> {
    let mode = mode & 0o777;
    if mode == 0o600 {
        Ok(())
    } else {
        Err(format!(
            ".env mode is {mode:#o} after chmod; expected owner-only 0o600"
        ))
    }
}

#[cfg(unix)]
fn ensure_env_owner_only(env_path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(env_path, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod .env 0o600 failed: {e}"))?;
    let meta = std::fs::symlink_metadata(env_path)
        .map_err(|e| format!("stat .env after chmod failed: {e}"))?;
    if !meta.is_file() {
        return Err("stat .env after chmod: not a regular file".to_string());
    }
    reject_non_owner_env_mode(meta.permissions().mode())
}

fn history_note(root: &Path) -> String {
    match crate::git::head_sha(root) {
        Ok(sha) => format!("history_note: old value may remain in git history since commit {sha}"),
        Err(_) => "history_note: working tree / uncommitted — no commit SHA observed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::apply_status_for_rescan;

    #[test]
    fn still_sensitive_rescan_is_incomplete() {
        assert_eq!(
            apply_status_for_rescan("still_sensitive finding_count=1"),
            "apply_status: incomplete (rescan still_sensitive)"
        );
        assert_eq!(apply_status_for_rescan("clean"), "apply_status: ok");
        assert_eq!(
            apply_status_for_rescan("indeterminate TooLarge"),
            "apply_status: ok"
        );
        assert_eq!(
            apply_status_for_rescan("unreadable after write: boom"),
            "apply_status: ok"
        );
    }

    #[cfg(unix)]
    #[test]
    fn non_owner_env_mode_is_a_failure() {
        use super::reject_non_owner_env_mode;
        let err = reject_non_owner_env_mode(0o644).unwrap_err();
        assert!(err.contains("0o644"), "{err}");
        assert!(reject_non_owner_env_mode(0o666).is_err());
        assert!(reject_non_owner_env_mode(0o600).is_ok());
        // `Permissions::mode` includes the file type above the permission bits.
        assert!(reject_non_owner_env_mode(0o100600).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn chmod_failure_on_missing_env_is_surfaced() {
        use super::ensure_env_owner_only;
        let missing =
            std::env::temp_dir().join(format!("symforge-missing-env-{}", std::process::id()));
        let _ = std::fs::remove_file(&missing);
        let err = ensure_env_owner_only(&missing).unwrap_err();
        assert!(err.contains("chmod .env 0o600 failed"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn owner_only_chmod_lands_0600() {
        use super::ensure_env_owner_only;
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, b"K=v\n").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&path, perms).unwrap();
        ensure_env_owner_only(&path).unwrap();
        let mode = std::fs::symlink_metadata(&path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }
}
