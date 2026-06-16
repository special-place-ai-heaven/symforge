//! Operator admin UI (`/admin`) + versioned JSON API (`/api/v1`) on the serve
//! router (G-037).
//!
//! The UI is **embedded vanilla static assets** (`index.html` + `app.js` +
//! `style.css` via `include_str!`; no npm build, no `rust-embed` — see
//! `research.md` D2). It fetches `/api/v1/*` and renders the dashboard, keys,
//! and diagnostics views with real values and clean empty / unavailable states
//! (FR-001 / FR-003 / FR-006).
//!
//! [`build_admin_router`] returns the combined `/admin` + `/api/v1` router with
//! the [`ServerRuntime`] as axum state. [`crate::server::serve::run`] mounts it
//! alongside `/mcp` and layers the shared Bearer-auth + Origin-gate in front
//! (one enforcement point — FR-002 / FR-007 / GATE-1).

#[cfg(feature = "server")]
pub mod api_v1;

#[cfg(feature = "server")]
use axum::Router;
#[cfg(feature = "server")]
use axum::http::{StatusCode, header};
#[cfg(feature = "server")]
use axum::response::IntoResponse;
#[cfg(feature = "server")]
use axum::routing::{delete, get, post};

#[cfg(feature = "server")]
use super::ServerRuntime;

/// Mount path for the admin UI.
pub const ADMIN_PATH: &str = "/admin";

// Embedded assets (compiled into the binary; no filesystem dependency at runtime).
#[cfg(feature = "server")]
const INDEX_HTML: &str = include_str!("assets/index.html");
#[cfg(feature = "server")]
const APP_JS: &str = include_str!("assets/app.js");
#[cfg(feature = "server")]
const STYLE_CSS: &str = include_str!("assets/style.css");

#[cfg(feature = "server")]
async fn serve_index() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        INDEX_HTML,
    )
}

#[cfg(feature = "server")]
async fn serve_app_js() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        APP_JS,
    )
}

#[cfg(feature = "server")]
async fn serve_style_css() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
        STYLE_CSS,
    )
}

/// Build the combined `/admin` (embedded UI) + `/api/v1/*` (JSON) router with
/// the [`ServerRuntime`] as state.
///
/// The returned router carries **no** auth or Origin layer — those are applied
/// once, in front of the merged serve router, by [`crate::server::serve::run`]
/// (the same single-enforcement-point discipline `/mcp` uses).
#[cfg(feature = "server")]
pub fn build_admin_router(runtime: &ServerRuntime) -> Router {
    Router::new()
        // Admin UI assets.
        .route(ADMIN_PATH, get(serve_index))
        .route("/admin/", get(serve_index))
        .route("/admin/app.js", get(serve_app_js))
        .route("/admin/style.css", get(serve_style_css))
        // Versioned JSON API.
        .route("/api/v1/summary", get(api_v1::get_summary))
        .route("/api/v1/surface", get(api_v1::get_surface))
        .route("/api/v1/harness", get(api_v1::get_harness))
        .route("/api/v1/system", get(api_v1::get_system))
        .route(
            "/api/v1/keys",
            get(api_v1::list_keys).post(api_v1::mint_key),
        )
        .route("/api/v1/keys/{id}/rotate", post(api_v1::rotate_key))
        .route("/api/v1/keys/{id}", delete(api_v1::revoke_key))
        .with_state(runtime.clone())
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn embedded_assets_are_non_empty_and_reference_endpoints() {
        // The HTML must load the JS + CSS and the JS must reference every
        // /api/v1 endpoint the dashboard depends on (render-fallback evidence).
        assert!(INDEX_HTML.contains("/admin/app.js"));
        assert!(INDEX_HTML.contains("/admin/style.css"));
        assert!(!STYLE_CSS.is_empty());
        for endpoint in ["/summary", "/surface", "/harness", "/system", "/keys"] {
            assert!(
                APP_JS.contains(endpoint),
                "app.js must reference {endpoint}"
            );
        }
    }
}
