//! Frozen fixture. Second home of a `verify` fn — used to prove the tools
//! surface BOTH definitions of a same-named symbol, not just one.

use crate::auth::AuthState;

/// A live session guarded by an auth state.
pub struct Session {
    pub token: String,
    pub auth: AuthState,
}

impl Session {
    /// Distinct `verify` (not the ApiKey one) — same name, different type.
    pub fn verify(&self) -> bool {
        self.auth.require_bearer(Some(&self.token))
    }
}
