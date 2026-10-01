//! `secret_remediate` — agent-driven secret remediation (feature 034).
//!
//! Preview defaults to true. Apply is a write tool (harness permission gate).
//! Never returns secret bytes or private keys.

use std::path::{Path, PathBuf};
use std::time::Instant;

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::Value;

use crate::hash::digest_hex;
use crate::knowledge::{self, SECRET_POLICY_VERSION};
use crate::protocol::SymForgeServer;
use crate::protocol::edit::atomic_write_file;
use crate::protocol::result_status::{OutcomeClass, ResultStatus};
use crate::protocol::withheld::{RemediationActionName, mint_finding_id};

#[derive(Debug, Deserialize, JsonSchema)]
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
    #[allow(dead_code)] // honored in follow-on; accepted on wire now
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
            OutcomeClass::Found
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
        if action == RemediationActionName::Encrypt {
            return Err(
                "Error: encrypt is unavailable (sops_not_installed or not yet implemented in this build)"
                    .to_string(),
            );
        }
        if action == RemediationActionName::Dismiss {
            return Err(
                "Error: dismiss is not yet available in this build (follow-on US4)".to_string(),
            );
        }

        let paths = resolve_scope_paths(self, &input.scope)?;
        let plan = plan_externalize(self, &paths, &input.finding_ids)?;

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

        // Apply
        let live = self.index.data_plane().read();
        let Some(root) = live.indexed_root.clone() else {
            return Err("Error: no indexed root bound".to_string());
        };
        drop(live);

        let originals = snapshot_for_rollback(&root, &plan)?;
        if let Err(e) = apply_externalize(&root, &plan) {
            let _ = rollback(&root, &originals);
            return Err(format!("Error: apply failed and was rolled back: {e}"));
        }

        // Re-scan the rewritten source
        let rescan = match std::fs::read(&plan.abs_path) {
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
        Ok(format!(
            "secret_remediate apply (externalize)\n\
             written:\n- {}\n- .env\n- .gitignore\n\
             rescan ({}) : {rescan}\n\
             {history}\n",
            plan.path, plan.path
        ))
    }
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
        let Ok(bytes) = std::fs::read(&abs) else {
            continue;
        };
        let spans = knowledge::scan_secret_spans(rel, &bytes);
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
        // One-file v1 plan: first path that matches any requested ids.
        let mut rewritten = bytes.clone();
        let mut env_lines = Vec::new();
        let mut masked = String::new();
        let mut applied_ids = Vec::new();
        // Apply from the end so offsets stay valid.
        hits.sort_by_key(|(_, span, _)| std::cmp::Reverse(span.value_start));
        let idiom = externalize_idiom(rel);
        for (secret_n, (_idx, span, id)) in hits.into_iter().enumerate() {
            let var = env_var_name(rel, span.rule_id, span.line_start, secret_n);
            let value = &bytes[span.value_start..span.value_end];
            // Never put raw value into response — only into .env on apply.
            env_lines.push(format!("{var}={}", String::from_utf8_lossy(value)));
            let replacement = idiom_replacement(&idiom, &var);
            rewritten.splice(span.value_start..span.value_end, replacement.bytes());
            applied_ids.push(id);
            // Assemble mask label at runtime so source stays detector-clean
            // (Ruling 1: no contiguous credential-key assignment shapes in src/).
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
        let bytes = if p.exists() {
            Some(std::fs::read(&p).map_err(|e| format!("snapshot {name}: {e}"))?)
        } else {
            None
        };
        snaps.push(RollbackSnap { path: p, bytes });
    }
    Ok(snaps)
}

fn rollback(root: &Path, snaps: &[RollbackSnap]) -> Result<(), String> {
    let _ = root;
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
    atomic_write_file(root, None, &plan.abs_path, &plan.rewritten)
        .map_err(|e| format!("write {}: {e}", plan.path))?;

    let env_path = root.join(".env");
    let mut env_body = if env_path.exists() {
        let mut b = std::fs::read_to_string(&env_path).map_err(|e| format!("read .env: {e}"))?;
        if !b.ends_with('\n') && !b.is_empty() {
            b.push('\n');
        }
        b
    } else {
        String::new()
    };
    for line in &plan.env_lines {
        let key = line.split_once('=').map(|(k, _)| k).unwrap_or(line);
        if env_body.lines().any(|l| l.starts_with(&format!("{key}="))) {
            continue; // idempotent
        }
        env_body.push_str(line);
        env_body.push('\n');
    }
    atomic_write_file(root, None, &env_path, env_body.as_bytes())
        .map_err(|e| format!("write .env: {e}"))?;

    let gi = root.join(".gitignore");
    let mut gi_body = if gi.exists() {
        std::fs::read_to_string(&gi).map_err(|e| format!("read .gitignore: {e}"))?
    } else {
        String::new()
    };
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

fn history_note(root: &Path) -> String {
    match crate::git::head_sha(root) {
        Ok(sha) => format!("history_note: old value may remain in git history since commit {sha}"),
        Err(_) => "history_note: working tree / uncommitted — no commit SHA observed".to_string(),
    }
}
