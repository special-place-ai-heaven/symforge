//! Process-global `SYMFORGE_SURFACE` guard for STEL integration tests.
#![allow(unsafe_code)]

use std::ffi::OsString;
use std::sync::LazyLock;

use tokio::sync::Mutex;

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

pub fn set_symforge_surface(value: &str) -> EnvVarGuard {
    EnvVarGuard::set("SYMFORGE_SURFACE", value)
}

#[allow(dead_code)]
pub fn clear_symforge_surface() -> EnvVarGuard {
    EnvVarGuard::unset("SYMFORGE_SURFACE")
}
