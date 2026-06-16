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

/// Inline SVG favicon (006 cosmetic): a small "SF" mark on the accent color.
/// Embedded so `GET /favicon.ico` (and the explicit `/admin/favicon.svg` link in
/// `index.html`) return `200` instead of the lone `404` console error. SVG is a
/// valid favicon format in modern browsers and needs no binary asset / build
/// step (matches the no-npm, `include_str!`-only asset policy).
#[cfg(feature = "server")]
const FAVICON_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="6" fill="#4ea1ff"/><text x="16" y="22" font-family="Segoe UI, Arial, sans-serif" font-size="16" font-weight="700" text-anchor="middle" fill="#06101e">SF</text></svg>"##;

#[cfg(feature = "server")]
async fn serve_favicon() -> impl IntoResponse {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        FAVICON_SVG,
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
        // Favicon (006 cosmetic): both the explicit link target and the bare
        // `/favicon.ico` the browser requests by default — kills the lone 404.
        .route("/admin/favicon.svg", get(serve_favicon))
        .route("/favicon.ico", get(serve_favicon))
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

    #[test]
    fn favicon_asset_present_and_linked() {
        // 006 cosmetic: a real favicon kills the lone /favicon.ico 404. The SVG
        // must be non-empty valid markup and the HTML must link it.
        assert!(FAVICON_SVG.contains("<svg"));
        assert!(FAVICON_SVG.contains("</svg>"));
        assert!(
            INDEX_HTML.contains("/admin/favicon.svg"),
            "index.html must link the favicon"
        );
    }

    #[test]
    fn mobile_overflow_guards_present_in_assets() {
        // 006 cosmetic: the mobile overflow fix relies on a horizontal scroll
        // container + a <=480px wrap rule + the path cell class. Guard them so a
        // future asset edit cannot silently regress the narrow-viewport layout.
        assert!(
            STYLE_CSS.contains(".table-scroll"),
            "style.css must define the horizontal scroll container"
        );
        assert!(
            STYLE_CSS.contains("max-width: 480px"),
            "style.css must carry the <=480px mobile media query"
        );
        assert!(
            APP_JS.contains("table-scroll"),
            "app.js must wrap the harness table in a scroll container"
        );
        assert!(
            APP_JS.contains("path-cell"),
            "app.js must tag the config path cell for wrapping"
        );
    }
}
