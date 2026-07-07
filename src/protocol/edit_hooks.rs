//! Edit hooks — extension points for feature tentacles.
//!
//! The 7 edit handlers in [`crate::protocol::tools`] share two side-car steps that
//! feature tentacles want to customise without forking the handler bodies:
//!
//! 1. **Path resolution** — today, every handler resolves the caller's relative path
//!    against the bound repo root (`server.capture_repo_root()` + [`crate::protocol::edit::safe_repo_path`]).
//!    The `worktree-awareness` feature tentacle wants to redirect that resolution into
//!    the caller's working-directory worktree when one is active.
//! 2. **Post-write bookkeeping** — today, there is nothing. The `frecency-ranking`
//!    feature tentacle wants to record an access event whenever an edit commits so
//!    its scoring can boost recently-touched files.
//!
//! This module defines the [`EditHook`] trait and a process-wide registry. The
//! default hook ([`DefaultEditHook`]) is pre-registered so today's behaviour is
//! preserved byte-for-byte when no feature hook is installed.
//!
//! Hooks are object-safe (`Box<dyn EditHook>`) so feature tentacles can register
//! trait objects at startup.

use std::path::Path;
use std::sync::OnceLock;

use parking_lot::RwLock;

use crate::worktree::ResolvedTarget;

/// Contextual information passed to every hook call.
#[derive(Debug, Clone, Copy)]
pub struct EditContext<'a> {
    /// Relative path as supplied by the caller (e.g. `src/lib.rs`).
    pub relative_path: &'a str,
    /// Absolute path the current repo root resolves `relative_path` to.
    /// This is the edit target in the absence of any feature hook.
    pub indexed_absolute_path: &'a Path,
    /// Repository root currently bound to the server.
    pub repo_root: &'a Path,
    /// Optional caller-supplied working directory. Feature hooks (e.g.
    /// `worktree-awareness`) consume this to redirect writes into the matching
    /// git worktree. `None` means the caller did not supply one and the hook
    /// should fall back to its no-op default.
    pub working_directory: Option<&'a Path>,
}

/// Extension point for feature tentacles.
///
/// Implementors may customise where an edit writes ([`Self::resolve_target_path`])
/// and react to a successful commit ([`Self::after_edit_committed`]). Both methods
/// have safe defaults so implementors can override only the surface they care about.
///
/// Implementations must be `Send + Sync` and object-safe.
pub trait EditHook: Send + Sync {
    /// Resolve the absolute path the edit should target.
    ///
    /// Returns a [`ResolvedTarget`] so the handler can distinguish a
    /// pass-through (write to indexed path) from a reroute (write to a
    /// sibling worktree). The default implementation returns
    /// `ctx.indexed_absolute_path` with `rerouted: false`; feature
    /// tentacles may redirect (e.g. to a per-working-directory worktree).
    fn resolve_target_path(&self, ctx: &EditContext) -> Result<ResolvedTarget, String> {
        let abs = ctx.indexed_absolute_path.to_path_buf();
        Ok(ResolvedTarget {
            target_path: abs.clone(),
            indexed_path: abs,
            rerouted: false,
        })
    }

    /// Called after an atomic write has committed successfully.
    ///
    /// The default implementation is a no-op; feature tentacles may record the
    /// access (e.g. for frecency scoring).
    fn after_edit_committed(&self, _ctx: &EditContext, _resolved_path: &Path) {}
}

/// No-op default hook — preserves today's behaviour when no feature hook is
/// registered. Pre-registered at module-init time so the registry is never empty.
pub struct DefaultEditHook;

impl EditHook for DefaultEditHook {}

fn registry() -> &'static RwLock<Vec<Box<dyn EditHook>>> {
    static REGISTRY: OnceLock<RwLock<Vec<Box<dyn EditHook>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(vec![Box::new(DefaultEditHook) as Box<dyn EditHook>]))
}

/// Register a hook on the process-wide registry.
///
/// Registered hooks are appended; later registrations take precedence for path
/// resolution, and every registered hook is notified on commit. The pre-registered
/// [`DefaultEditHook`] sits at the bottom of the stack and acts as the fallback.
pub fn register(hook: Box<dyn EditHook>) {
    registry().write().push(hook);
}

/// Resolve the target path by walking registered hooks in reverse-registration
/// order and returning the first hook that makes an ACTIVE routing decision.
///
/// A hook makes an active decision when it returns an error (reject the edit)
/// or an actual reroute (`rerouted == true`). Observer-only hooks — e.g.
/// [`crate::live_index::frecency::FrecencyBumpHook`], which overrides only
/// `after_edit_committed` — inherit the DEFAULT [`EditHook::resolve_target_path`],
/// which returns a passthrough (`rerouted == false`, target == indexed). Such a
/// passthrough must NOT shadow an earlier hook's reroute/error: doing so made
/// worktree routing silently depend on registration order (F6 — when the
/// frecency hook registered after [`crate::worktree::WorktreeAwareEditHook`],
/// `next_back()` picked the frecency passthrough and every `working_directory`
/// edit contaminated the indexed root). So skip passthrough results and keep
/// looking; only when no hook makes an active decision do we return a
/// passthrough (byte-identical to pre-hook behavior — the most-recently-
/// registered hook's passthrough, exactly what `next_back()` used to yield).
///
/// Because [`DefaultEditHook`] is pre-registered and always returns `Ok`, this
/// function only returns `Err` when a feature hook explicitly fails resolution.
pub fn resolve(ctx: &EditContext) -> Result<ResolvedTarget, String> {
    let reg = registry().read();
    let mut passthrough: Option<ResolvedTarget> = None;
    for hook in reg.iter().rev() {
        let resolved = hook.resolve_target_path(ctx)?;
        if resolved.rerouted {
            return Ok(resolved);
        }
        if passthrough.is_none() {
            passthrough = Some(resolved);
        }
    }
    // The registry is seeded with DefaultEditHook, so a passthrough is always
    // produced unless a hook errored above; the fallback is defensive.
    match passthrough {
        Some(resolved) => Ok(resolved),
        None => DefaultEditHook.resolve_target_path(ctx),
    }
}

/// Invoke [`EditHook::after_edit_committed`] on every registered hook.
pub fn after_commit(ctx: &EditContext, resolved_path: &Path) {
    let reg = registry().read();
    for hook in reg.iter() {
        hook.after_edit_committed(ctx, resolved_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn default_hook_returns_indexed_path_unchanged() {
        let repo_root = PathBuf::from("/tmp/repo");
        let abs = repo_root.join("src/lib.rs");
        let ctx = EditContext {
            relative_path: "src/lib.rs",
            indexed_absolute_path: &abs,
            repo_root: &repo_root,
            working_directory: None,
        };
        let resolved = DefaultEditHook.resolve_target_path(&ctx).expect("resolves");
        assert_eq!(resolved.target_path, abs);
        assert_eq!(resolved.indexed_path, abs);
        assert!(!resolved.rerouted);
    }

    #[test]
    fn default_hook_after_commit_is_noop() {
        let repo_root = PathBuf::from("/tmp/repo");
        let abs = repo_root.join("src/lib.rs");
        let ctx = EditContext {
            relative_path: "src/lib.rs",
            indexed_absolute_path: &abs,
            repo_root: &repo_root,
            working_directory: None,
        };
        // Should not panic or mutate anything observable.
        DefaultEditHook.after_edit_committed(&ctx, &abs);
    }

    #[test]
    fn registry_resolves_via_default_when_only_default_registered() {
        // This test relies on the registry state being the default seeding only,
        // which is true at process start. Other tests in this file do not register
        // feature hooks, so the invariant holds with `--test-threads=1`.
        let repo_root = PathBuf::from("/tmp/repo-registry");
        let abs = repo_root.join("src/lib.rs");
        let ctx = EditContext {
            relative_path: "src/lib.rs",
            indexed_absolute_path: &abs,
            repo_root: &repo_root,
            working_directory: None,
        };
        let resolved = resolve(&ctx).expect("resolves");
        assert_eq!(resolved.target_path, abs);
        assert!(!resolved.rerouted);
    }

    #[test]
    fn default_hook_ignores_working_directory() {
        // Default hook is feature-agnostic: a supplied `working_directory` does
        // not change resolution. Feature hooks (e.g. `worktree-awareness`) are
        // the only consumers; the default impl always returns
        // `indexed_absolute_path` unchanged so backward compat holds even when
        // callers supply the new field.
        let repo_root = PathBuf::from("/tmp/repo");
        let abs = repo_root.join("src/lib.rs");
        let cwd = PathBuf::from("/tmp/some/worktree");
        let ctx = EditContext {
            relative_path: "src/lib.rs",
            indexed_absolute_path: &abs,
            repo_root: &repo_root,
            working_directory: Some(&cwd),
        };
        let resolved = DefaultEditHook.resolve_target_path(&ctx).expect("resolves");
        assert_eq!(resolved.target_path, abs);
        assert!(!resolved.rerouted);
    }

    #[test]
    fn resolve_prefers_active_reroute_over_later_passthrough_hook() {
        // Root-cause regression (F6): `resolve` must honor an ACTIVE reroute from an
        // earlier-registered hook even when a LATER-registered observer-only hook
        // (mirroring `FrecencyBumpHook`, which overrides only `after_edit_committed`)
        // inherits the default passthrough. The old `next_back()` returned only the
        // last hook, so a passthrough observer registered after the worktree hook
        // silently killed routing and every `working_directory` edit hit the indexed
        // root. Using a unique sentinel `relative_path` keeps these hooks inert for
        // every other test that shares this process-global registry.
        const SENTINEL: &str = "F6_SENTINEL_REROUTE";

        struct ReroutingHook;
        impl EditHook for ReroutingHook {
            fn resolve_target_path(&self, ctx: &EditContext) -> Result<ResolvedTarget, String> {
                if ctx.relative_path == SENTINEL {
                    Ok(ResolvedTarget {
                        target_path: PathBuf::from("/worktree").join(ctx.relative_path),
                        indexed_path: ctx.indexed_absolute_path.to_path_buf(),
                        rerouted: true,
                    })
                } else {
                    let abs = ctx.indexed_absolute_path.to_path_buf();
                    Ok(ResolvedTarget {
                        target_path: abs.clone(),
                        indexed_path: abs,
                        rerouted: false,
                    })
                }
            }
        }

        // Observer-only: overrides ONLY after_edit_committed, exactly like
        // FrecencyBumpHook, so its resolve_target_path is the default passthrough.
        struct ObserverOnlyHook;
        impl EditHook for ObserverOnlyHook {
            fn after_edit_committed(&self, _ctx: &EditContext, _p: &Path) {}
        }

        // Register the router FIRST, the observer LAST — the order that broke prod.
        register(Box::new(ReroutingHook));
        register(Box::new(ObserverOnlyHook));

        let repo_root = PathBuf::from("/repo");
        let abs = repo_root.join(SENTINEL);
        let ctx = EditContext {
            relative_path: SENTINEL,
            indexed_absolute_path: &abs,
            repo_root: &repo_root,
            working_directory: Some(Path::new("/worktree")),
        };
        let resolved = resolve(&ctx).expect("resolves");
        assert!(
            resolved.rerouted,
            "an active reroute must win over a later observer-only passthrough hook"
        );
        assert_eq!(
            resolved.target_path,
            PathBuf::from("/worktree").join(SENTINEL)
        );
    }
}
