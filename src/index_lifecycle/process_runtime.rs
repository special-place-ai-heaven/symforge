//! Shared process runtime and factory-incarnation registry (T034).
//!
//! One process, one capacity domain. The daemon, stdio, `serve` and embed
//! surfaces are four doors into the same process, and today each one builds its
//! own index machinery with no shared accounting between them — so two surfaces
//! in one process can each believe they own the whole budget.
//!
//! **Spawns nothing.** `live_index` is not feature-gated, so this module
//! compiles and runs under `--features embed`, where the public contract
//! promises no implicit background machinery. Everything here is a value: no
//! threads, no timers, no tasks. A runtime that started a reaper the moment an
//! embedder constructed it would break that promise silently.
//!
//! A **factory incarnation** is one construction of the process's index
//! machinery. It persists across reconnects — a client dropping and re-attaching
//! rejoins the same incarnation — and gets a fresh never-reused identity when
//! the machinery is genuinely rebuilt. That distinction is what lets a later
//! slice tell "the same runtime you were talking to" from "a new one wearing the
//! same address".

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::capacity::{CapacityRefusal, OwnerIdentity, ProcessCapacityPool};

/// Identity of one construction of the process index machinery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IncarnationIdentity(std::num::NonZeroU64);

static NEXT_INCARNATION: AtomicU64 = AtomicU64::new(1);

impl IncarnationIdentity {
    /// Mint a fresh never-reused incarnation identity.
    pub fn fresh() -> Self {
        let raw = NEXT_INCARNATION.fetch_add(1, Ordering::Relaxed);
        Self(std::num::NonZeroU64::new(raw).expect("incarnation counter starts at 1"))
    }
}

/// Which door into the process a surface came through.
///
/// Named rather than numbered so accounting is legible: "stdio is holding 400
/// MiB" is actionable, "owner 7 is holding 400 MiB" is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurfaceKind {
    /// The shared local daemon.
    Daemon,
    /// A stdio MCP connection.
    Stdio,
    /// The operator HTTP server.
    Serve,
    /// An in-process embedder.
    Embed,
}

impl SurfaceKind {
    /// Every surface, for exhaustive accounting.
    pub const ALL: [SurfaceKind; 4] = [Self::Daemon, Self::Stdio, Self::Serve, Self::Embed];

    /// A stable human-readable name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Stdio => "stdio",
            Self::Serve => "serve",
            Self::Embed => "embed",
        }
    }
}

/// Why a runtime request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeRefusal {
    /// The process capacity domain cannot back this surface.
    Capacity(CapacityRefusal),
    /// This surface is already registered in this incarnation.
    SurfaceAlreadyRegistered {
        /// The surface that was already present.
        surface: SurfaceKind,
    },
    /// The named surface is not registered in this incarnation.
    SurfaceNotRegistered {
        /// The surface that was asked for.
        surface: SurfaceKind,
    },
}

/// One process's shared runtime.
///
/// Holds the single capacity root every surface is charged against, and the
/// registry of surfaces attached to the current incarnation.
#[derive(Debug)]
pub struct ProcessIndexRuntime {
    incarnation: IncarnationIdentity,
    ledger: Arc<ProcessCapacityPool>,
    root: OwnerIdentity,
    surfaces: std::sync::Mutex<HashMap<SurfaceKind, OwnerIdentity>>,
}

impl ProcessIndexRuntime {
    /// Construct a fresh incarnation owning `process_bytes` of capacity.
    pub fn incarnate(process_bytes: u64) -> Arc<Self> {
        let ledger = ProcessCapacityPool::new();
        let root = ledger.root(process_bytes);
        Arc::new(Self {
            incarnation: IncarnationIdentity::fresh(),
            ledger,
            root,
            surfaces: std::sync::Mutex::new(HashMap::new()),
        })
    }

    /// This incarnation's identity.
    ///
    /// Stable across reconnects; a client that drops and re-attaches sees the
    /// same value, and sees a different one only if the machinery was genuinely
    /// rebuilt.
    pub fn incarnation(&self) -> IncarnationIdentity {
        self.incarnation
    }

    /// The process-wide capacity domain every surface shares.
    pub fn ledger(&self) -> &Arc<ProcessCapacityPool> {
        &self.ledger
    }

    /// The root owner backing the whole process.
    pub fn root_owner(&self) -> OwnerIdentity {
        self.root
    }

    /// Attach a surface, giving it a capacity owner beneath the process root.
    ///
    /// Refuses a second attach of the same surface rather than silently
    /// replacing it: replacing would orphan the previous owner's charges, and
    /// the process would then believe capacity is free that something still
    /// holds.
    pub fn attach(
        self: &Arc<Self>,
        surface: SurfaceKind,
        bytes: u64,
    ) -> Result<OwnerIdentity, RuntimeRefusal> {
        let mut surfaces = self.surfaces.lock().expect("process runtime mutex");
        if surfaces.contains_key(&surface) {
            return Err(RuntimeRefusal::SurfaceAlreadyRegistered { surface });
        }
        let owner = self
            .ledger
            .child(self.root, bytes)
            .map_err(RuntimeRefusal::Capacity)?;
        surfaces.insert(surface, owner);
        Ok(owner)
    }

    /// Detach a surface, returning its promise to the process root.
    ///
    /// Refuses while the surface still holds charges, because returning the
    /// promise then would let the process hand the same bytes to another
    /// surface while the first is still using them.
    pub fn detach(self: &Arc<Self>, surface: SurfaceKind) -> Result<u64, RuntimeRefusal> {
        let mut surfaces = self.surfaces.lock().expect("process runtime mutex");
        let owner = *surfaces
            .get(&surface)
            .ok_or(RuntimeRefusal::SurfaceNotRegistered { surface })?;
        let returned = self
            .ledger
            .release_owner(owner)
            .map_err(RuntimeRefusal::Capacity)?;
        surfaces.remove(&surface);
        Ok(returned)
    }

    /// The capacity owner for an attached surface.
    pub fn owner_for(&self, surface: SurfaceKind) -> Option<OwnerIdentity> {
        self.surfaces
            .lock()
            .expect("process runtime mutex")
            .get(&surface)
            .copied()
    }

    /// Which surfaces are attached to this incarnation.
    pub fn attached(&self) -> Vec<SurfaceKind> {
        let surfaces = self.surfaces.lock().expect("process runtime mutex");
        let mut names: Vec<SurfaceKind> = surfaces.keys().copied().collect();
        names.sort();
        names
    }

    /// Bytes still promisable to a new surface.
    pub fn available(&self) -> u64 {
        self.ledger.available(self.root)
    }
}
