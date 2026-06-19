//! Process-global `SYMFORGE_SURFACE` guard for STEL integration tests.
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::sync::LazyLock;

use tokio::sync::Mutex;

// Serializes the process-global env mutation so the FULL-envelope opt-in is
// deterministic even without `--test-threads=1`. `surface_honesty` holds it
// around `force_full_stel_envelope` on its live-render path; the compact/replay
// harnesses hold it around the surface-select guard. Some including binaries do
// not reference it, so the dead-code allow stays.
#[allow(dead_code)]
pub static COMPACT_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

pub struct EnvVarGuard {
    key: &'static str,
    previous: Option<OsString>,
}

impl EnvVarGuard {
    pub fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, previous }
    }

    // Only the surface-default conformance binary clears the var; other test
    // binaries include this shared module but use `set` exclusively.
    #[allow(dead_code)]
    pub fn unset(key: &'static str) -> Self {
        let previous = std::env::var_os(key);
        unsafe {
            std::env::remove_var(key);
        }
        Self { key, previous }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(previous) => unsafe {
                std::env::set_var(self.key, previous);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}

// Unused in the `surface_honesty` binary (which only forces the full envelope);
// used by the surface/replay harnesses that select the compact surface.
#[allow(dead_code)]
pub fn set_symforge_surface(value: &str) -> EnvVarGuard {
    EnvVarGuard::set("SYMFORGE_SURFACE", value)
}

#[allow(dead_code)]
pub fn clear_symforge_surface() -> EnvVarGuard {
    EnvVarGuard::unset("SYMFORGE_SURFACE")
}

/// Force the FULL multi-line trust envelope for the duration of a test.
///
/// The live trust envelope is COMPACT by default (the one-line form). Integration
/// surfaces that verify the full contract — `── stel ──` header, `decision:`,
/// `ledger:`, the per-call economics block — must opt back into FULL via
/// `SYMFORGE_STEL_FULL`. Like the surface guard this is process-global, so callers
/// hold `COMPACT_ENV_LOCK` and rely on the RAII restore (the suite runs
/// `--test-threads=1`). Returns a guard; keep it bound for the test's lifetime.
#[allow(dead_code)]
pub fn force_full_stel_envelope() -> EnvVarGuard {
    EnvVarGuard::set("SYMFORGE_STEL_FULL", "1")
}
