//! Dark surface adapters (T038).
//!
//! **Dark means: computes the answer, changes nothing.** Each adapter works out
//! what the lifecycle registry WOULD do for an admission — which capacity owner
//! charges it, where its state may live, whether a protected root is permitted —
//! and returns that decision as a value. It does not touch any V10 production
//! admission path, and it is not called from one. Slice 4 is what moves
//! production onto these answers; the spec schedules a test proving this slice
//! has not.
//!
//! The reason to build the decision now, unwired, is that it is the same
//! decision Slice 4 must make under real traffic. Getting it wrong here is cheap;
//! getting it wrong there is a regression.
//!
//! **SC-019 is enforced, not assumed.** A protected root must reach admission
//! without any state write or durability probe beneath the source root. That is
//! not a comment: `plan_admission` returns a placement that cannot be
//! project-local for a protected root, and refuses rather than silently
//! relocating, so a caller that asked for the wrong thing learns it did.

use std::sync::Arc;

use super::authority::BindingAuthority;
use super::capacity::OwnerIdentity;
use super::process_runtime::{ProcessRuntime, SurfaceKind};
use super::registry::{
    ProjectKey, ProjectRegistry, RegistryRefusal, RootProtection, SlotIdentity, StatePlacement,
};

/// What an adapter decided for one admission, without performing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionPlan {
    surface: SurfaceKind,
    key: ProjectKey,
    placement: StatePlacement,
    owner: OwnerIdentity,
    /// Whether this plan touches the source root at all.
    ///
    /// Recorded rather than asserted: SC-019 forbids state and durability probes
    /// beneath a protected root, and a plan that claims compliance without a
    /// value to check is a claim nobody can test.
    touches_source_root: bool,
}

impl AdmissionPlan {
    /// The surface this admission came through.
    pub fn surface(&self) -> SurfaceKind {
        self.surface
    }

    /// The project being admitted.
    pub fn key(&self) -> &ProjectKey {
        &self.key
    }

    /// Where derived state would live.
    pub fn placement(&self) -> StatePlacement {
        self.placement
    }

    /// The capacity owner that would be charged.
    pub fn owner(&self) -> OwnerIdentity {
        self.owner
    }

    /// Whether executing this plan would write beneath the source root.
    pub fn touches_source_root(&self) -> bool {
        self.touches_source_root
    }
}

/// Why an adapter refused to plan an admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterRefusal {
    /// The surface is not attached to the process runtime.
    SurfaceNotAttached {
        /// The surface that was asked for.
        surface: SurfaceKind,
    },
    /// The registry refused the admission itself.
    Registry(RegistryRefusal),
}

/// Plan an admission for `surface` without performing it.
///
/// Returns the decision the lifecycle would make. Nothing is written, no
/// registry entry is created, and no capacity is charged: this is the
/// behaviour-neutral half, and Slice 4 is what makes it act.
pub fn plan_admission(
    runtime: &Arc<ProcessRuntime>,
    surface: SurfaceKind,
    key: ProjectKey,
    protection: RootProtection,
    authorized: bool,
    requested: StatePlacement,
) -> Result<AdmissionPlan, AdapterRefusal> {
    let owner = runtime
        .owner_for(surface)
        .ok_or(AdapterRefusal::SurfaceNotAttached { surface })?;

    let placement = match (protection, authorized, requested) {
        // A protected root with no authorization is refused outright.
        (RootProtection::Protected, false, _) => {
            return Err(AdapterRefusal::Registry(
                RegistryRefusal::ProtectedWithoutAuthorization,
            ));
        }
        // Authorization permits indexing, not writing beneath the root. Refuse
        // rather than quietly relocating: a caller that asked for project-local
        // state must learn its request was not honoured, or it will believe
        // state is somewhere it is not.
        (RootProtection::Protected, true, StatePlacement::ProjectLocal) => {
            return Err(AdapterRefusal::Registry(
                RegistryRefusal::ProtectedWithoutAuthorization,
            ));
        }
        (RootProtection::Protected, true, chosen) => chosen,
        (RootProtection::Normal, _, chosen) => chosen,
    };

    Ok(AdmissionPlan {
        surface,
        key,
        placement,
        owner,
        touches_source_root: placement == StatePlacement::ProjectLocal,
    })
}

/// Execute a plan against the lifecycle registry.
///
/// Separated from planning so the decision can be inspected without acting on
/// it, which is what lets a dark adapter be checked at all. Slice 4 calls this;
/// nothing in production does today.
pub fn execute_plan(
    registry: &Arc<ProjectRegistry>,
    plan: &AdmissionPlan,
    binding: BindingAuthority,
    protection: RootProtection,
    authorized: bool,
) -> Result<SlotIdentity, AdapterRefusal> {
    registry
        .admit(
            plan.key().clone(),
            binding,
            protection,
            authorized,
            plan.placement(),
        )
        .map_err(AdapterRefusal::Registry)
}
