//! Bearer authentication for the operator server (`symforge serve`).
//!
//! Encodes the secure-by-default rule (FR-002..004, G-033):
//! - a configured key is enforced on **every** request (constant-time compare);
//! - with **no** key, requests are accepted **only** on a loopback bind;
//! - a non-loopback bind with no key **refuses to start** ([`AuthConfig::refuse_to_start`]).
//!
//! The axum middleware that consumes this config is [`require_bearer`] (US1/T014):
//! it extracts `Authorization: Bearer <key>`, applies the policy here (constant-time
//! verify), returns `401 Unauthorized` on a missing/invalid key when auth is
//! required, and otherwise calls the next handler. [`super::serve::run`] layers it
//! in front of `/mcp` via [`bearer_auth_layer`].

/// Authentication configuration for one server instance.
///
/// Holds at most a single static Bearer key. `None` means "no key configured"
/// — permitted only when the bind is loopback (see [`AuthConfig::requires_auth`]
/// and [`AuthConfig::refuse_to_start`]).
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    /// The single static Bearer key, if one was resolved from
    /// `--api-key` / `--api-key-env`. `None` = unauthenticated.
    pub api_key: Option<String>,
}

impl AuthConfig {
    /// Construct from an optional resolved key.
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }

    /// Whether requests must present a valid Bearer token.
    ///
    /// `true` when a key is configured OR the bind is non-loopback. A key is
    /// always enforced; a non-loopback bind always requires auth (and, with no
    /// key, the server refuses to start — see [`Self::refuse_to_start`]).
    pub fn requires_auth(&self, is_loopback: bool) -> bool {
        self.api_key.is_some() || !is_loopback
    }

    /// Constant-time check of a presented Bearer token against the configured key.
    ///
    /// Returns `false` when no key is configured (callers gate on
    /// [`Self::requires_auth`] first). The comparison is constant-time over the
    /// configured key length to avoid leaking it via a timing side channel; a
    /// length mismatch still folds every byte before returning `false`.
    pub fn verify(&self, presented: &str) -> bool {
        match self.api_key.as_deref() {
            Some(expected) => constant_time_eq(expected.as_bytes(), presented.as_bytes()),
            None => false,
        }
    }

    /// Enforce the secure-default startup rule (FR-003, G-033).
    ///
    /// Errors when the bind is non-loopback and no key is configured — the
    /// server must refuse to start rather than expose an unauthenticated
    /// surface on a routable address.
    pub fn refuse_to_start(&self, is_loopback: bool) -> Result<(), AuthStartupError> {
        if !is_loopback && self.api_key.is_none() {
            return Err(AuthStartupError::NonLoopbackWithoutKey);
        }
        Ok(())
    }
}

/// Shared state for the [`require_bearer`] middleware.
///
/// Carries the resolved [`AuthConfig`] and whether the bind is loopback, so the
/// middleware can apply the exact secure-default rule
/// ([`AuthConfig::requires_auth`]) without re-deriving it per request.
#[derive(Debug, Clone)]
pub struct AuthLayerState {
    auth: AuthConfig,
    is_loopback: bool,
}

impl AuthLayerState {
    /// Build the layer state from a resolved auth config and the bind's loopback flag.
    pub fn new(auth: AuthConfig, is_loopback: bool) -> Self {
        Self { auth, is_loopback }
    }
}

/// Extract a `Bearer` token from an `Authorization` header value.
///
/// Returns the token (trimmed) when the header is `Bearer <token>`
/// (scheme match is ASCII-case-insensitive per RFC 7235); `None` otherwise.
fn parse_bearer(header: &str) -> Option<&str> {
    let rest = header.strip_prefix("Bearer ").or_else(|| {
        // Case-insensitive scheme match without allocating.
        let (scheme, rest) = header.split_once(' ')?;
        scheme.eq_ignore_ascii_case("bearer").then_some(rest)
    })?;
    let token = rest.trim();
    (!token.is_empty()).then_some(token)
}

/// axum middleware enforcing the Bearer auth policy in front of `/mcp`.
///
/// * When auth is **not** required (no key configured **and** loopback bind),
///   the request passes through untouched.
/// * When auth **is** required, the `Authorization: Bearer <key>` header must be
///   present and constant-time-equal to the configured key; otherwise the
///   request is rejected with `401 Unauthorized` and **no tool executes**.
///
/// Layered via [`bearer_auth_layer`]. Kept as a thin policy adapter so the
/// constant-time verify and the secure-default rule live only on [`AuthConfig`].
#[cfg(feature = "server")]
pub async fn require_bearer(
    axum::extract::State(state): axum::extract::State<AuthLayerState>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    if !state.auth.requires_auth(state.is_loopback) {
        return next.run(request).await;
    }

    let presented = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_bearer);

    match presented {
        Some(token) if state.auth.verify(token) => next.run(request).await,
        _ => (
            StatusCode::UNAUTHORIZED,
            "Unauthorized: missing or invalid Bearer token",
        )
            .into_response(),
    }
}

/// Apply the [`require_bearer`] middleware to a router with the given state.
///
/// `serve::run` calls this on the `/mcp` router so the secure-default rule is
/// enforced in exactly one place, in front of the transport. Returning the
/// wrapped [`axum::Router`] avoids naming the (unnameable) middleware layer type.
#[cfg(feature = "server")]
pub fn apply_bearer_auth(router: axum::Router, state: AuthLayerState) -> axum::Router {
    router.layer(axum::middleware::from_fn_with_state(state, require_bearer))
}

/// Startup-time auth policy violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AuthStartupError {
    /// A non-loopback bind was requested without an API key.
    #[error(
        "refusing to bind a non-loopback address without an API key: pass --api-key or --api-key-env (a routable bind must be authenticated)"
    )]
    NonLoopbackWithoutKey,
}

/// Constant-time byte-slice equality.
///
/// Compares the two slices without short-circuiting on the first differing
/// byte, so the running time does not depend on *where* they differ. Length is
/// folded into the accumulator (an unequal length yields a non-zero result),
/// but the loop always runs over `max(a.len(), b.len())` so a length mismatch
/// is not itself a fast path. Self-contained (no new dependency); unit-tested
/// against the obvious vectors below.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    // Fold the length difference in first so unequal-length inputs can never
    // compare equal regardless of content. Fold *every* byte of the `usize`
    // XOR into the accumulator (P2-A): a plain `as u8` cast truncates, so length
    // pairs whose difference is a multiple of 256 (e.g. 256 vs 0) would zero the
    // length term and only the content loop would guard them. Folding all bytes
    // means any non-zero length difference sets `diff`.
    let len_xor = a.len() ^ b.len();
    let mut diff: u8 = 0;
    for shift in (0..usize::BITS).step_by(8) {
        diff |= (len_xor >> shift) as u8;
    }
    let n = a.len().max(b.len());
    for i in 0..n {
        // Out-of-range reads are replaced by 0; because `diff` already carries
        // the length mismatch, the result stays `false` for unequal lengths.
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_for_equal_slices() {
        assert!(constant_time_eq(b"sf_demo_key", b"sf_demo_key"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn constant_time_eq_rejects_differing_slices() {
        assert!(!constant_time_eq(b"sf_demo_key", b"sf_wrong_key"));
        assert!(!constant_time_eq(b"abc", b"abd"));
    }

    #[test]
    fn constant_time_eq_rejects_length_mismatch() {
        assert!(!constant_time_eq(b"short", b"shorter"));
        assert!(!constant_time_eq(b"key", b""));
        assert!(!constant_time_eq(b"", b"key"));
        // A prefix must not pass for a longer configured key.
        assert!(!constant_time_eq(b"sf_demo_key_long", b"sf_demo_key"));
    }

    #[test]
    fn constant_time_eq_rejects_length_diff_multiple_of_256() {
        // P2-A regression: a truncating `as u8` length fold would zero the
        // length term for length pairs differing by a multiple of 256. The
        // bytes that overlap are identical here, so only the length fold can
        // reject. Lengths 0 vs 256 and 256 vs 512 both differ by 256.
        let empty: &[u8] = b"";
        let block_256 = vec![b'a'; 256];
        let block_512 = vec![b'a'; 512];
        assert!(
            !constant_time_eq(empty, &block_256),
            "0 vs 256 (diff 256) must reject despite identical overlapping bytes"
        );
        assert!(
            !constant_time_eq(&block_256, &block_512),
            "256 vs 512 (diff 256) must reject"
        );
        // Sanity: equal 256-byte blocks still pass.
        assert!(constant_time_eq(&block_256, &vec![b'a'; 256]));
    }

    // T007/T008: key-set behavior.

    #[test]
    fn key_set_correct_passes_wrong_and_empty_fail() {
        let auth = AuthConfig::new(Some("sf_demo_key".to_string()));
        assert!(auth.verify("sf_demo_key"), "correct key must pass");
        assert!(!auth.verify("sf_wrong_key"), "wrong key must fail");
        assert!(!auth.verify(""), "empty presented key must fail");
    }

    #[test]
    fn key_set_requires_auth_on_any_bind() {
        let auth = AuthConfig::new(Some("k".to_string()));
        assert!(
            auth.requires_auth(true),
            "key set => auth required on loopback"
        );
        assert!(
            auth.requires_auth(false),
            "key set => auth required on non-loopback"
        );
    }

    #[test]
    fn key_set_refuse_to_start_is_ok_on_any_bind() {
        let auth = AuthConfig::new(Some("k".to_string()));
        assert!(auth.refuse_to_start(true).is_ok());
        assert!(auth.refuse_to_start(false).is_ok());
    }

    // T007/T008: no-key behavior.

    #[test]
    fn no_key_loopback_requires_no_auth() {
        let auth = AuthConfig::new(None);
        assert!(
            !auth.requires_auth(true),
            "no key + loopback => no auth required"
        );
        assert!(
            auth.refuse_to_start(true).is_ok(),
            "no key + loopback must start"
        );
        // verify() always false with no key (callers gate on requires_auth first).
        assert!(!auth.verify("anything"));
    }

    // T014: Bearer header parsing.

    #[test]
    fn parse_bearer_extracts_token() {
        assert_eq!(parse_bearer("Bearer sf_demo_key"), Some("sf_demo_key"));
        // Scheme match is case-insensitive.
        assert_eq!(parse_bearer("bearer sf_demo_key"), Some("sf_demo_key"));
        assert_eq!(parse_bearer("BEARER sf_demo_key"), Some("sf_demo_key"));
        // Surrounding whitespace on the token is trimmed.
        assert_eq!(parse_bearer("Bearer   sf_demo_key  "), Some("sf_demo_key"));
    }

    #[test]
    fn parse_bearer_rejects_non_bearer_and_empty() {
        assert_eq!(parse_bearer("Basic abc123"), None);
        assert_eq!(parse_bearer("sf_demo_key"), None);
        assert_eq!(parse_bearer("Bearer "), None);
        assert_eq!(parse_bearer("Bearer    "), None);
        assert_eq!(parse_bearer(""), None);
    }

    #[test]
    fn no_key_non_loopback_requires_auth_and_refuses_to_start() {
        let auth = AuthConfig::new(None);
        assert!(
            auth.requires_auth(false),
            "no key + non-loopback => auth required"
        );
        assert_eq!(
            auth.refuse_to_start(false),
            Err(AuthStartupError::NonLoopbackWithoutKey),
            "no key + non-loopback must refuse to start"
        );
    }
}
