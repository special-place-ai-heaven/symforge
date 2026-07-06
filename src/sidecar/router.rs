//! Axum router wiring all sidecar endpoints.

use axum::{Router, routing::get};

use super::{SidecarState, handlers};

/// Build the axum `Router` with all routes, injecting `SidecarState` as state.
///
/// Routes:
/// - `GET /health`          → `health_handler`
/// - `GET /outline`         → `outline_handler`
/// - `GET /impact`          → `impact_handler`
/// - `GET /symbol-context`  → `symbol_context_handler`
/// - `GET /repo-map`        → `repo_map_handler`
/// - `GET /prompt-context`  → `prompt_context_handler`
/// - `GET /workflows/source-read`           → `workflow_source_read_handler`
/// - `GET /workflows/search-hit-expansion`  → `workflow_search_hit_expansion_handler`
/// - `GET /workflows/post-edit-impact`      → `workflow_post_edit_impact_handler`
/// - `GET /workflows/repo-start`            → `workflow_repo_start_handler`
/// - `GET /workflows/prompt-context`        → `workflow_prompt_narrowing_handler`
/// - `GET /stats`           → `stats_handler`
pub fn build_router(state: SidecarState) -> Router {
    Router::new()
        .route("/health", get(handlers::health_handler))
        .route("/outline", get(handlers::outline_handler))
        .route("/impact", get(handlers::impact_handler))
        .route("/symbol-context", get(handlers::symbol_context_handler))
        .route("/repo-map", get(handlers::repo_map_handler))
        .route("/prompt-context", get(handlers::prompt_context_handler))
        .route(
            "/workflows/source-read",
            get(handlers::workflow_source_read_handler),
        )
        .route(
            "/workflows/search-hit-expansion",
            get(handlers::workflow_search_hit_expansion_handler),
        )
        .route(
            "/workflows/post-edit-impact",
            get(handlers::workflow_post_edit_impact_handler),
        )
        .route(
            "/workflows/repo-start",
            get(handlers::workflow_repo_start_handler),
        )
        .route(
            "/workflows/prompt-context",
            get(handlers::workflow_prompt_narrowing_handler),
        )
        .route("/stats", get(handlers::stats_handler))
        // Dogfood #6 / spec 012 FR-006b (hook half): 409 when a request's
        // `caller_root` does not match the current index root, so hooks fall
        // back to the daemon instead of getting wrong-project answers.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            handlers::caller_root_guard,
        ))
        .with_state(state)
}
