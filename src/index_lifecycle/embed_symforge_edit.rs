//! MCP `symforge_edit` for an embedded source. The shared STEL edit planner,
//! pre-apply gates and economics decide the edit; the planned replace, insert
//! or within-symbol step runs on this source's native edit lanes (routed
//! into an admitted worktree when `working_directory` asks for one), and the
//! shared ledger and envelope render the answer. This mirrors
//! `SymForgeServer::symforge_edit_stel_handler` on its daemon-less path.
//!
//! An apply carrying `idempotency_key` reserves one replay record for the
//! facade request itself, as MCP's legacy tool reserves one for its typed
//! input: an identical retry whose recorded post-image still verifies is
//! answered without writing, any other record under that key refuses with
//! MCP's reconciliation text and an error outcome, and the same key on a
//! different request conflicts.

use std::path::Path;

use super::embed_session::QuerySession;
use super::embedded::EmbeddedSourceHandle;
use crate::embed::parity::QueryPolicy;
use crate::embed::parity::edit::{
    AdmittedEditTarget, EditError, EditErrorKind, EditTarget, EditWithinRequest, InsertPosition,
    InsertRequest, ReplaceRequest, ResolvedEditTarget,
};
use crate::embed::parity::replay::{
    OutcomeKind, OwnerLease, ReplayKey, ReplayOutcome, ReplayState, ReplayStore,
    RequestFingerprint, ReserveOutcome,
};
use crate::embed::parity::stel::{
    OutcomeClass, StelEditRequest, SymforgeEditAnswer, SymforgeEditApplied, SymforgeEditAuthority,
};
use crate::stel::runtime::{self, FacadeLedgerInput, mutation_refusal_outcome};

/// MCP's refusal for a replay record that cannot be served.
const RECONCILIATION_REQUIRED: &str =
    "Error: Idempotency replay unavailable: stored operation requires reconciliation.";

fn answer(outcome: OutcomeClass, rendered: String) -> SymforgeEditAnswer {
    SymforgeEditAnswer {
        outcome,
        is_error: outcome != OutcomeClass::Found,
        rendered,
        replayed: false,
        applied: None,
    }
}

fn refusal(text: impl Into<String>) -> SymforgeEditAnswer {
    let text = text.into();
    answer(mutation_refusal_outcome(&text), text)
}

/// MCP's classification of a native edit lane refusal.
fn edit_refusal(tool: &str, error: &EditError) -> SymforgeEditAnswer {
    let outcome = match error {
        EditError::Edit(
            EditErrorKind::SymbolNotFound
            | EditErrorKind::FileNotAdmitted
            | EditErrorKind::TextNotFound
            | EditErrorKind::TargetFileMissing,
        ) => OutcomeClass::NotFound,
        EditError::Edit(EditErrorKind::AmbiguousSymbol { .. }) => OutcomeClass::Ambiguous,
        EditError::Edit(EditErrorKind::WriteUncertain) | EditError::Source(_) => {
            OutcomeClass::InternalFailure
        }
        EditError::Edit(_) => OutcomeClass::InvalidRequest,
    };
    let kind = match error {
        EditError::Edit(kind) => format!("{kind:?}"),
        EditError::Source(refusal) => format!("{:?}", refusal.kind()),
    };
    answer(outcome, format!("Error: {tool} refused: {kind}"))
}

/// A reserved facade replay record, released or settled exactly once.
struct Reservation {
    store: ReplayStore,
    lease: OwnerLease,
}

impl Reservation {
    fn release(self) {
        let _ = self.store.release_not_started(&self.lease);
    }
}

enum FacadeReplay {
    Reserved(Reservation),
    Answered(SymforgeEditAnswer),
}

/// What the native lane did for the planned step.
struct StepOutcome {
    body: String,
    committed: bool,
    applied: Option<SymforgeEditApplied>,
    /// The written bytes, read back through the target's own authority and
    /// matched to the lane's post-image hash.
    post_image: Option<Vec<u8>>,
}

/// The source an edit of `path` lands on: this one, or the admitted worktree
/// `working_directory` names, with MCP's reroute report.
struct EditTargetRoute<'a> {
    handle: &'a EmbeddedSourceHandle,
    authority: Option<&'a crate::embed::parity::edit::EditApplyAuthority>,
    resolved: Option<ResolvedEditTarget>,
}

fn arg(args: &serde_json::Value, key: &str) -> String {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// The current bytes of `path` beneath `handle`'s admitted root, read through
/// its original anchor in any phase: a just-applied edit is still being
/// republished by the worker when its post-image is read back.
fn current_bytes(handle: &EmbeddedSourceHandle, path: &str) -> Option<Vec<u8>> {
    handle
        .source_authority()?
        .read_regular_beneath_anchor(Path::new(path), crate::knowledge::SECRET_SCAN_MAX_BYTES)
        .ok()?
}

impl EmbeddedSourceHandle {
    /// MCP `symforge_edit` parity. A preview needs no authority. An apply
    /// (`apply: true`) needs the host's write authority, and
    /// `working_directory` routes it into one of `authority.admitted` as the
    /// native edit route does. `session` receives MCP's ledger event and
    /// served-token summary; `policy` governs the durable economics ledger as
    /// it does for a query. Refuses only when the source itself is
    /// unavailable.
    pub fn symforge_edit(
        &self,
        request: &StelEditRequest,
        session: Option<&QuerySession>,
        policy: QueryPolicy,
        authority: Option<SymforgeEditAuthority<'_>>,
    ) -> Result<SymforgeEditAnswer, super::public_api::EmbedSourceRefusal> {
        use crate::stel::controller::evaluate_edit_plan;
        use crate::stel::edit_apply::{
            PreApplyOutcome, apply_requested, format_already_applied_body, format_apply_metadata,
            run_pre_apply_gates_on_ready,
        };
        use crate::stel::edit_planner::{build_edit_plan, edit_plan_summary_line};
        use crate::stel::planner::confidence_label;

        if let Err(error) = crate::stel::edit_planner::validate_edit_request(request) {
            return Ok(refusal(error.message));
        }
        let mut snapshot = self.capture_query_snapshot(b"symforge-edit")?;
        if let Some(project) = request
            .project
            .as_deref()
            .filter(|project| !project.trim().is_empty())
            && super::embed_changes::check_project(&snapshot, Some(project)).is_err()
        {
            return Ok(refusal(format!(
                "Error: project_routing: project '{project}' is not available on this \
                 connection: this source is bound to the single project at {}.",
                crate::paths::normalized_path_string(&snapshot.root)
            )));
        }
        let apply = apply_requested(request);
        let admitted: &[AdmittedEditTarget<'_>] = authority
            .as_ref()
            .map_or(&[], |authority| authority.admitted);
        let mut resolved_symbol = None;
        let mut reservation: Option<Reservation> = None;
        if apply {
            let Some(authority) = authority.as_ref() else {
                return Ok(refusal(
                    "Error: symforge_edit apply requires the host's write authority.",
                ));
            };
            // MCP freshens the exact edit path before its gates read it.
            if self.freshen_exact_path(&request.path).is_err() {
                return Ok(refusal(format!(
                    "Error: stale publication for {}; retry symforge_edit apply",
                    request.path
                )));
            }
            snapshot = self.capture_query_snapshot(b"symforge-edit-apply")?;
            let abs_path = match crate::discovery::resolve_repo_path(&snapshot.root, &request.path)
            {
                Ok(Some(path)) => path,
                _ => {
                    return Ok(refusal(format!(
                        "Error: file not found at {}",
                        request.path
                    )));
                }
            };
            if let Some(key) = request.idempotency_key.as_deref() {
                match self.reserve_facade_replay(&snapshot, authority, admitted, key, request) {
                    FacadeReplay::Reserved(reserved) => reservation = Some(reserved),
                    FacadeReplay::Answered(answer) => {
                        if let Some(session) = session {
                            session.with_context(|context| {
                                context.record_summary_output(
                                    "symforge_edit",
                                    crate::stel::handler::estimate_tokens(&answer.rendered),
                                )
                            });
                        }
                        return Ok(answer);
                    }
                }
            }
            match run_pre_apply_gates_on_ready(&snapshot.generation.live, request, &abs_path) {
                Ok(PreApplyOutcome::Ready(symbol)) => resolved_symbol = Some(symbol),
                // F6: a `working_directory` apply targets a worktree whose copy
                // may still lack the change; proceed to the routed write.
                Ok(PreApplyOutcome::AlreadyApplied(symbol))
                    if request.working_directory.is_some() =>
                {
                    resolved_symbol = Some(symbol);
                }
                Ok(PreApplyOutcome::AlreadyApplied(symbol)) => {
                    if let Some(reserved) = reservation {
                        reserved.release();
                    }
                    let plan = build_edit_plan(request).expect("validated edit request");
                    let decision = evaluate_edit_plan(&plan);
                    let routing_meta = format!(
                        "Mode: guarded apply (already applied)\n\
                         Route confidence: {}\n\
                         Chosen tool: replace_symbol_body\n\
                         Economics: {} ({})",
                        confidence_label(plan.confidence),
                        decision.decision.as_str(),
                        decision.decision_reason,
                    );
                    let body =
                        format!("{routing_meta}\n\n{}", format_already_applied_body(&symbol));
                    let output = self.record_edit_answer(
                        session,
                        policy,
                        FacadeLedgerInput {
                            surface: "symforge_edit",
                            plan: &plan,
                            decision: &decision,
                            plan_summary: edit_plan_summary_line(&plan),
                            session_tokens_served: session.map_or(0, QuerySession::served_tokens)
                                as i64,
                            body: &body,
                            legacy_executed: false,
                            selected_tool: "replace_symbol_body",
                            tools_called: None,
                            tuned: None,
                        },
                    );
                    return Ok(answer(OutcomeClass::Found, output));
                }
                Err(error) => {
                    if let Some(reserved) = reservation {
                        reserved.release();
                    }
                    return Ok(refusal(error.message));
                }
            }
        }

        let plan = match build_edit_plan(request) {
            Ok(plan) => plan,
            Err(error) => {
                if let Some(reserved) = reservation {
                    reserved.release();
                }
                return Ok(refusal(error.message));
            }
        };
        let decision = evaluate_edit_plan(&plan);
        let step = plan
            .steps
            .first()
            .expect("edit planner always emits at least one step");
        if let Some(reserved) = reservation.as_ref()
            && reserved.store.mark_started(&reserved.lease).is_err()
        {
            if let Some(reserved) = reservation {
                reserved.release();
            }
            return Ok(refusal(RECONCILIATION_REQUIRED));
        }
        // The native lane's own replay key. A keyed facade apply derives it
        // from the caller's key; a keyless one, which MCP never replays, gets
        // a key no retry can name.
        let inner_key = apply.then(|| {
            let seed = match request.idempotency_key.as_deref() {
                Some(key) => key.as_bytes().to_vec(),
                None => serde_json::to_vec(&(
                    request,
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |elapsed| elapsed.as_nanos()),
                ))
                .unwrap_or_default(),
            };
            format!("symforge-edit/{}", crate::hash::digest_hex(&seed))
        });
        let executed = self.run_edit_step(
            request,
            &step.tool,
            &step.args,
            admitted,
            authority.as_ref().map(|authority| authority.authority),
            inner_key.as_deref(),
        );
        let invocation = serde_json::to_string(&step.args).unwrap_or_else(|_| "{}".to_string());
        let mode_line = if apply {
            "Mode: guarded apply (committed when validation succeeds)"
        } else {
            "Mode: preview-only (dry_run); default when apply is omitted or false"
        };
        let routing_meta = format!(
            "{mode_line}\n\
             Route confidence: {}\n\
             Chosen tool: {}\n\
             Invocation: {}\n\
             Rationale: {}\n\
             Economics: {} ({})",
            confidence_label(plan.confidence),
            step.tool,
            invocation,
            plan.confidence_rationale,
            decision.decision.as_str(),
            decision.decision_reason,
        );
        // The apply-metadata block MCP inserts before the tool body.
        let metadata = |write_mode: &str| {
            resolved_symbol
                .as_ref()
                .filter(|_| apply)
                .map(|symbol| format_apply_metadata(symbol, write_mode))
        };
        let executed = match executed {
            Ok(executed) => executed,
            Err(error) => {
                let reservation = reservation.take();
                if let Some(reserved) = reservation {
                    // A refused write left the bytes as they were; an
                    // uncertain one needs reconciliation before any retry.
                    let settled = !matches!(error, EditError::Edit(EditErrorKind::WriteUncertain))
                        && ReplayOutcome::from_response_and_post_image(
                            OutcomeKind::Rejected,
                            b"symforge_edit_refused",
                            b"",
                        )
                        .is_ok_and(|rejected| {
                            reserved.store.fail(&reserved.lease, &rejected).is_ok()
                        });
                    if !settled {
                        let _ = reserved.store.mark_uncertain(&reserved.lease);
                    }
                }
                let mut refused = edit_refusal(&step.tool, &error);
                if let Some(text) = self.mcp_refusal_text(request, &step.args, &error) {
                    refused.rendered = text;
                }
                let body = match metadata("failed") {
                    Some(metadata) => {
                        format!("{routing_meta}\n\n{metadata}\n\n{}", refused.rendered)
                    }
                    None => format!("{routing_meta}\n\n{}", refused.rendered),
                };
                let output = self.record_edit_answer(
                    session,
                    policy,
                    FacadeLedgerInput {
                        surface: "symforge_edit",
                        plan: &plan,
                        decision: &decision,
                        plan_summary: edit_plan_summary_line(&plan),
                        session_tokens_served: session.map_or(0, QuerySession::served_tokens)
                            as i64,
                        body: &body,
                        legacy_executed: false,
                        selected_tool: &step.tool,
                        tools_called: None,
                        tuned: None,
                    },
                );
                return Ok(answer(refused.outcome, output));
            }
        };
        let body = match metadata(if executed.committed {
            "committed"
        } else {
            "failed"
        }) {
            Some(metadata) => format!("{routing_meta}\n\n{metadata}\n\n{}", executed.body),
            None => format!("{routing_meta}\n\n{}", executed.body),
        };
        let mut output = self.record_edit_answer(
            session,
            policy,
            FacadeLedgerInput {
                surface: "symforge_edit",
                plan: &plan,
                decision: &decision,
                plan_summary: edit_plan_summary_line(&plan),
                session_tokens_served: session.map_or(0, QuerySession::served_tokens) as i64,
                body: &body,
                legacy_executed: apply && executed.committed,
                selected_tool: &step.tool,
                tools_called: None,
                // Edits are byte-grounded, not floor-tuned.
                tuned: None,
            },
        );
        if let Some(reserved) = reservation {
            let completed = executed.post_image.as_deref().is_some_and(|bytes| {
                ReplayOutcome::from_response_and_post_image(
                    OutcomeKind::Applied,
                    output.as_bytes(),
                    bytes,
                )
                .is_ok_and(|outcome| reserved.store.complete(&reserved.lease, &outcome).is_ok())
            });
            if !completed {
                let _ = reserved.store.mark_uncertain(&reserved.lease);
                output.push_str("\nIdempotency warning: failed to store replay result");
            }
        }
        let mut result = answer(OutcomeClass::Found, output);
        result.applied = executed.applied;
        Ok(result)
    }

    /// Reserve the facade's replay record, or answer an existing one: a
    /// completed apply whose post-image still verifies replays without
    /// writing; anything else refuses with MCP's reconciliation text.
    fn reserve_facade_replay(
        &self,
        snapshot: &super::embed_query::EmbeddedQuerySnapshot,
        authority: &SymforgeEditAuthority<'_>,
        admitted: &[AdmittedEditTarget<'_>],
        key: &str,
        request: &StelEditRequest,
    ) -> FacadeReplay {
        let mut fingerprinted = request.clone();
        fingerprinted.idempotency_key = None;
        let reserved = (|| {
            let key = ReplayKey::new(key).ok()?;
            let fingerprint = RequestFingerprint::for_json(
                "embed_symforge_edit_v1",
                &serde_json::json!({"scope": authority.authority.scope, "request": fingerprinted}),
            )
            .ok()?;
            let state_dir = snapshot.state_dir.as_deref()?;
            let anchor = snapshot.authority.physical_root_stable_key()?;
            let store = ReplayStore::open_bound(
                &snapshot.root,
                state_dir,
                &authority.authority.scope,
                anchor,
            )
            .ok()?;
            Some(
                store
                    .reserve(&key, &fingerprint)
                    .map(|outcome| (store, outcome)),
            )
        })();
        match reserved {
            None => FacadeReplay::Answered(refusal(
                "Error: durable project-state replay is unavailable for this binding.",
            )),
            Some(Err(crate::embed::parity::replay::ReplayError::Conflict)) => {
                FacadeReplay::Answered(refusal(
                    "Idempotency conflict: the key already names a different symforge_edit request",
                ))
            }
            Some(Err(_)) => FacadeReplay::Answered(refusal(RECONCILIATION_REQUIRED)),
            Some(Ok((store, ReserveOutcome::Acquired(lease)))) => {
                FacadeReplay::Reserved(Reservation { store, lease })
            }
            Some(Ok((_, ReserveOutcome::Existing(record)))) => {
                let applied = record.state == ReplayState::Completed
                    && record
                        .outcome
                        .as_ref()
                        .is_some_and(|outcome| outcome.kind == OutcomeKind::Applied);
                let target = self.edit_target(request, admitted, None).ok();
                let current = target.and_then(|target| current_bytes(target.handle, &request.path));
                match current {
                    Some(bytes) if applied && record.matches_post_image(&bytes) => {
                        let mut replayed = answer(
                            OutcomeClass::Found,
                            format!(
                                "Idempotency replay: symforge_edit already applied to {}; the \
                                 recorded post-image ({}) still holds, so nothing was written.",
                                request.path,
                                crate::hash::digest_hex(&bytes)
                            ),
                        );
                        replayed.replayed = true;
                        FacadeReplay::Answered(replayed)
                    }
                    _ => FacadeReplay::Answered(refusal(RECONCILIATION_REQUIRED)),
                }
            }
        }
    }

    /// The source `request.path` lands on: this one, or the admitted worktree
    /// its `working_directory` names.
    fn edit_target<'a>(
        &'a self,
        request: &StelEditRequest,
        admitted: &[AdmittedEditTarget<'a>],
        authority: Option<&'a crate::embed::parity::edit::EditApplyAuthority>,
    ) -> Result<EditTargetRoute<'a>, EditError> {
        let Some(working_directory) = request.working_directory.as_deref() else {
            return Ok(EditTargetRoute {
                handle: self,
                authority,
                resolved: None,
            });
        };
        let route = self.route_edit(&request.path, Path::new(working_directory), admitted)?;
        Ok(match route.target {
            Some(target) => EditTargetRoute {
                handle: target.handle,
                authority: Some(target.authority),
                resolved: Some(route.resolved),
            },
            None => EditTargetRoute {
                handle: self,
                authority,
                resolved: Some(route.resolved),
            },
        })
    }

    /// Run the planned step on the native edit lanes: plan the symbol on this
    /// source, rebase onto the routed target, then preview, or apply with
    /// `inner_key` under the target's authority.
    fn run_edit_step(
        &self,
        request: &StelEditRequest,
        tool: &str,
        args: &serde_json::Value,
        admitted: &[AdmittedEditTarget<'_>],
        authority: Option<&crate::embed::parity::edit::EditApplyAuthority>,
        inner_key: Option<&str>,
    ) -> Result<StepOutcome, EditError> {
        let name = arg(args, "name");
        let planned = self.edit_plan(&EditTarget {
            path: request.path.clone(),
            name: name.clone(),
            kind: None,
            symbol_line: None,
        })?;
        let target = self.edit_target(request, admitted, authority)?;
        let route = match request.working_directory.as_deref() {
            Some(working_directory) => {
                Some(self.route_edit(&request.path, Path::new(working_directory), admitted)?)
            }
            None => None,
        };
        let guard = match route.as_ref() {
            Some(route) => self.rebase_guard(route, &planned.guard)?,
            None => planned.guard,
        };
        // MCP's tool text for the step, through the route when one was given.
        let render = |body: &crate::embed::parity::edit::EditBody| match route.as_ref() {
            Some(route) => route.render_body(body),
            None => body.render(),
        };
        let handle = target.handle;
        let Some(key) = inner_key else {
            let body = match tool {
                "replace_symbol_body" => {
                    handle
                        .preview_replace(&ReplaceRequest {
                            guard,
                            new_body: arg(args, "new_body"),
                        })?
                        .body
                }
                "insert_symbol" => {
                    handle
                        .preview_insert(&InsertRequest {
                            guard,
                            position: position(args),
                            content: arg(args, "content"),
                        })?
                        .body
                }
                _ => {
                    handle
                        .preview_edit_within(&within(guard, args))?
                        .change
                        .body
                }
            };
            return Ok(StepOutcome {
                body: render(&body),
                committed: false,
                applied: None,
                post_image: None,
            });
        };
        let authority = target
            .authority
            .ok_or(EditError::Edit(EditErrorKind::WriteAuthorityRefused))?;
        let (path, post_image_hash, refresh, body) = match tool {
            "replace_symbol_body" => {
                let applied = handle.apply_replace(
                    &ReplaceRequest {
                        guard,
                        new_body: arg(args, "new_body"),
                    },
                    authority,
                    key,
                )?;
                (
                    applied.path,
                    applied.post_image_hash,
                    applied.refresh_ticket_identity,
                    applied.body,
                )
            }
            "insert_symbol" => {
                let applied = handle.apply_insert(
                    &InsertRequest {
                        guard,
                        position: position(args),
                        content: arg(args, "content"),
                    },
                    authority,
                    key,
                )?;
                (
                    applied.path,
                    applied.post_image_hash,
                    applied.refresh_ticket_identity,
                    applied.body,
                )
            }
            _ => {
                let applied = handle.apply_edit_within(&within(guard, args), authority, key)?;
                (
                    applied.path,
                    applied.post_image_hash,
                    applied.refresh_ticket_identity,
                    applied.body,
                )
            }
        };
        let post_image = current_bytes(handle, &path)
            .filter(|bytes| crate::hash::digest_hex(bytes) == post_image_hash);
        let wrote_to = target.resolved.as_ref().map_or_else(
            || {
                dunce::simplified(&authority.root)
                    .join(&path)
                    .display()
                    .to_string()
            },
            |resolved| resolved.target_path.display().to_string(),
        );
        Ok(StepOutcome {
            body: render(&body),
            committed: true,
            applied: Some(SymforgeEditApplied {
                path,
                wrote_to,
                post_image_hash,
                refresh_ticket_identity: refresh,
            }),
            post_image,
        })
    }

    /// MCP's own text for a step the native lane refused, where the MCP tool
    /// renders one from the indexed file: the symbol resolver's not-found or
    /// ambiguity message, and `edit_within_symbol`'s selection refusals.
    fn mcp_refusal_text(
        &self,
        request: &StelEditRequest,
        args: &serde_json::Value,
        error: &EditError,
    ) -> Option<String> {
        use super::guidance::edit_body::{edit_within_refusal, resolve_or_error};
        use crate::edit_safety::structural::WithinSelectionError;
        let EditError::Edit(kind) = error else {
            return None;
        };
        let snapshot = self.capture_query_snapshot(b"symforge-edit-refusal").ok()?;
        let file = snapshot.generation.live.get_file(&request.path)?;
        let name = arg(args, "name");
        let selection = match kind {
            EditErrorKind::SymbolNotFound | EditErrorKind::AmbiguousSymbol { .. } => {
                return resolve_or_error(file, &name, None, None).err();
            }
            EditErrorKind::TextNotFound => WithinSelectionError::NotFound,
            EditErrorKind::OccurrenceOutOfRange { requested, total } => {
                WithinSelectionError::OccurrenceOutOfRange {
                    requested: *requested,
                    total: *total,
                }
            }
            EditErrorKind::ConflictingTargeting => WithinSelectionError::ConflictingTargeting,
            _ => return None,
        };
        let (_, symbol) = resolve_or_error(file, &name, None, None).ok()?;
        let body = file
            .content
            .get(symbol.effective_start() as usize..symbol.byte_range.1 as usize)
            .and_then(|body| std::str::from_utf8(body).ok())
            .unwrap_or("");
        Some(edit_within_refusal(
            selection,
            body,
            &arg(args, "old_text"),
            &name,
        ))
    }

    /// Record the answer's economics in the session ledger and, under
    /// `policy`, the source's durable store; return the enveloped text.
    fn record_edit_answer(
        &self,
        session: Option<&QuerySession>,
        policy: QueryPolicy,
        input: FacadeLedgerInput<'_>,
    ) -> String {
        let (output, event) = runtime::render_facade_answer(input);
        if let Some(session) = session {
            session.stel_ledger().push(event.clone());
            session.with_context(|context| {
                context.record_summary_output(
                    "symforge_edit",
                    crate::stel::handler::estimate_tokens(&output),
                )
            });
        }
        if let Some(store) = self.stel_store(policy) {
            runtime::record_durably_inline(&store, &event);
        }
        output
    }
}

fn position(args: &serde_json::Value) -> InsertPosition {
    if args.get("position").and_then(serde_json::Value::as_str) == Some("before") {
        InsertPosition::Before
    } else {
        InsertPosition::After
    }
}

fn within(
    guard: crate::embed::parity::edit::EditGuard,
    args: &serde_json::Value,
) -> EditWithinRequest {
    EditWithinRequest {
        guard,
        old_text: arg(args, "old_text"),
        new_text: arg(args, "new_text"),
        replace_all: args
            .get("replace_all")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        occurrence: None,
        near_line: None,
    }
}
