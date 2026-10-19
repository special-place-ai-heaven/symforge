//! Axum router wiring all sidecar endpoints.
//!
//! Bearer is layered on the whole router only when the listener is
//! `NonLoopbackAuthenticated`. `/health` is a documented probe exemption on
//! that listener; every other route requires `Authorization: Bearer <token>`.
//! Loopback listeners get no Bearer layer.

use axum::{
    Router,
    extract::Request,
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::get,
};

use super::{SidecarState, handlers};

/// Build the axum `Router` with all routes, injecting `SidecarState` as state.
///
/// Routes:
/// - `GET /health`          → `health_handler` (ungated even when Bearer is on)
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
///
/// `bearer_token` is `Some` only for a non-loopback authenticated listener.
/// The Bearer layer is attached to this entire router, not a single workflow
/// route. `/health` is the only documented exemption.
pub fn build_router(state: SidecarState, bearer_token: Option<String>) -> Router {
    let router = Router::new()
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
        ));

    // `with_state` type-erases so the optional Bearer layer does not split
    // the function's return type. No layer on loopback (hook-compat).
    match bearer_token {
        Some(token) => router
            .layer(axum::middleware::from_fn_with_state(
                token,
                require_sidecar_bearer,
            ))
            .with_state(state),
        None => router.with_state(state),
    }
}

/// Constant-time byte-slice equality (daemon shape).
///
/// Returns `false` on length mismatch and otherwise XOR-folds every byte pair
/// into an accumulator, returning `acc == 0`. There is no early return on the
/// first differing byte, so the running time depends only on the input length,
/// not on the position of the first mismatch.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc: u8 = 0;
    for (lhs, rhs) in a.iter().zip(b.iter()) {
        acc |= lhs ^ rhs;
    }
    acc == 0
}

fn bearer_token_matches(expected: &str, provided: &str) -> bool {
    // Hash both sides to a fixed 32-byte SHA-256 digest first so the
    // comparison width is constant regardless of the presented token's length.
    let expected_digest = crate::hash::digest(expected.as_bytes());
    let provided_digest = crate::hash::digest(provided.as_bytes());
    constant_time_eq(&expected_digest, &provided_digest)
}

fn provided_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let header_value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, provided_token) = header_value.split_once(' ')?;
    scheme
        .eq_ignore_ascii_case("bearer")
        .then_some(provided_token)
}

fn sidecar_bearer_verdict(
    path: &str,
    headers: &HeaderMap,
    expected_token: &str,
) -> Result<(), StatusCode> {
    // Documented probe exemption. Every other route on this layer is gated.
    if path == "/health" {
        return Ok(());
    }
    let Some(provided) = provided_bearer_token(headers) else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    if bearer_token_matches(expected_token, provided) {
        Ok(())
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn require_sidecar_bearer(
    axum::extract::State(expected_token): axum::extract::State<String>,
    request: Request,
    next: Next,
) -> Response {
    match sidecar_bearer_verdict(request.uri().path(), request.headers(), &expected_token) {
        Ok(()) => next.run(request).await,
        Err(status) => (status, "sidecar authentication required").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn auth_headers(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn sidecar_bearer_gates_whole_router_except_health() {
        let expected = "sidecar-test-fixture";
        let empty = HeaderMap::new();
        let authed = auth_headers("Bearer sidecar-test-fixture");
        let wrong = auth_headers("Bearer wrong-fixture");

        assert_eq!(sidecar_bearer_verdict("/health", &empty, expected), Ok(()));
        assert_eq!(sidecar_bearer_verdict("/health", &wrong, expected), Ok(()));

        for path in [
            "/outline",
            "/impact",
            "/symbol-context",
            "/repo-map",
            "/prompt-context",
            "/workflows/source-read",
            "/workflows/search-hit-expansion",
            "/workflows/post-edit-impact",
            "/workflows/repo-start",
            "/workflows/prompt-context",
            "/stats",
        ] {
            assert_eq!(
                sidecar_bearer_verdict(path, &empty, expected),
                Err(StatusCode::UNAUTHORIZED),
                "{path} without Bearer must 401"
            );
            assert_eq!(
                sidecar_bearer_verdict(path, &wrong, expected),
                Err(StatusCode::UNAUTHORIZED),
                "{path} with a wrong fixture must 401"
            );
            assert_eq!(
                sidecar_bearer_verdict(path, &authed, expected),
                Ok(()),
                "{path} with the matching Bearer must pass"
            );
        }
    }

    #[test]
    fn sidecar_bearer_hashes_then_compares_in_constant_time() {
        assert!(bearer_token_matches("abc", "abc"));
        assert!(!bearer_token_matches("abc", "abd"));
        assert!(!bearer_token_matches("short", "longer-fixture"));
        assert!(constant_time_eq(b"abcdefgh", b"abcdefgh"));
        assert!(!constant_time_eq(b"abcdefgh", b"abcdefgX"));
        assert!(!constant_time_eq(b"short", b"longer-fixture"));
    }
}
