//! Frozen fixture for the tool-correctness harness (scripts/verify-tools.cjs).
//! Ground truth here is hand-known and stable. Do NOT edit casually — the
//! snapshots and grep oracles are pinned to this exact content.

/// A minted API key with a hashed secret.
pub struct ApiKey {
    pub id: u64,
    pub hash: String,
}

impl ApiKey {
    /// Check a presented secret against this key's stored hash.
    pub fn verify(&self, secret: &str) -> bool {
        hash_secret(secret) == self.hash
    }
}

/// Bootstrap auth state: one boot token plus a set of minted keys.
pub struct AuthState {
    pub boot_token: String,
    pub keys: Vec<ApiKey>,
}

impl AuthState {
    /// Verify a presented Bearer token: boot token first, then minted keys.
    pub fn verify_token(&self, token: &str) -> bool {
        if token == self.boot_token {
            return true;
        }
        self.keys.iter().any(|k| k.verify(token))
    }

    /// Middleware entry point: only calls verify_token, nothing else.
    pub fn require_bearer(&self, presented: Option<&str>) -> bool {
        match presented {
            Some(token) => self.verify_token(token),
            None => false,
        }
    }
}

/// Hash a secret. Toy implementation — fixture only.
pub fn hash_secret(secret: &str) -> String {
    format!("h:{secret}")
}
