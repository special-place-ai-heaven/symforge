//! Hierarchical process capacity accounting (T035, T036).
//!
//! **Accounting only. This module never blocks.** Today's loader already owns a
//! per-load byte budget that blocks on a condvar inside the shared rayon pool
//! (`live_index::store`), and a process-wide *blocking* pool layered on top of
//! that is a deadlock rather than a refactor: a worker parked waiting for
//! process capacity holds a pool thread that the grant it is waiting for may
//! need. So this module hands out and reconciles charges; it does not make
//! anyone wait. The invariant "the leaf keeps its own per-load budget" is
//! binding until a loom or stress proof says otherwise.
//!
//! The property the whole module exists to keep is **conservation**: every byte
//! charged is either still held by a live allocation or has been refunded
//! exactly once. A charge that is refunded twice invents capacity the process
//! does not have; a charge that is never refunded leaks it. Both end in the same
//! place — a number that no longer describes the process.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::authority::PublicationIdentity;

/// Identity of one capacity owner in the hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OwnerIdentity(std::num::NonZeroU64);

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

impl OwnerIdentity {
    /// Mint a fresh never-reused owner identity.
    pub fn fresh() -> Self {
        let raw = NEXT_OWNER.fetch_add(1, Ordering::Relaxed);
        Self(std::num::NonZeroU64::new(raw).expect("owner counter starts at 1"))
    }
}

/// Identity of one capacity pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolIdentity(std::num::NonZeroU64);

static NEXT_POOL: AtomicU64 = AtomicU64::new(1);

impl PoolIdentity {
    fn fresh() -> Self {
        let raw = NEXT_POOL.fetch_add(1, Ordering::Relaxed);
        Self(std::num::NonZeroU64::new(raw).expect("pool counter starts at 1"))
    }
}

/// Why a capacity request was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapacityRefusal {
    /// The owner does not have enough headroom to satisfy the request.
    Exhausted {
        /// What was asked for.
        requested: u64,
        /// What was actually available.
        available: u64,
    },
    // NOTE: there is deliberately no `GrantAlreadyRedeemed` or `UnknownCharge`
    // variant. `redeem` consumes the grant by value so a second redemption is
    // unrepresentable, and an unknown refund is counted in `unknown_refunds`
    // rather than returned -- a `Drop` cannot propagate a `Result`. Advertising
    // a refusal this module never returns would imply a check that never runs.
    /// A child owner cannot be given more than its parent can back.
    ExceedsParent {
        /// What the child was to be given.
        requested: u64,
        /// What the parent could back.
        available: u64,
    },
    /// An owner still backs children, so its promise is not free to return.
    HasChildren {
        /// How many children still hold a limit backed by this owner.
        children: usize,
    },
    /// The owner named is not in this pool.
    UnknownOwner,
    /// The grant was issued by a different pool, so this one cannot honour it.
    ForeignGrant,
}

/// An immutable authorization to allocate a fixed number of bytes, exactly once.
///
/// Deliberately not `Clone`: a grant that could be duplicated could be redeemed
/// twice against one charge, which is how a process ends up believing it has
/// capacity it already spent.
///
/// **Charged from the moment it exists, and refunded when it dies.** `reserve`
/// charges the row, so a grant dropped on any path between reserve and redeem —
/// a `?`, an early return, a panic — used to leak those bytes permanently, with
/// no refund and no counter, and wedge `release_owner` for that owner forever.
/// Conservation is defined against physical drop for the permit; there is no
/// reason for the grant to be the one value in this module that escapes it.
#[derive(Debug)]
pub struct CapacityGrant {
    owner: OwnerIdentity,
    bytes: u64,
    charge: PublicationIdentity,
    /// Which pool issued it. A grant redeemed by a different pool would charge
    /// one account and refund another, leaving the issuer's charge outstanding
    /// forever while the redeemer counted an unknown refund — the loss recorded
    /// against the one account that did not lose anything.
    pool: PoolIdentity,
    /// The issuing pool, so an abandoned grant can refund itself.
    ledger: Arc<ProcessCapacityPool>,
    /// Set when `redeem` takes the charge over, so redemption is not a refund.
    redeemed: bool,
}

impl CapacityGrant {
    /// The owner that issued this grant.
    pub fn owner(&self) -> OwnerIdentity {
        self.owner
    }

    /// How many bytes it authorizes.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The charge identity, which the refund must name.
    pub fn charge(&self) -> PublicationIdentity {
        self.charge
    }
}

impl Drop for CapacityGrant {
    fn drop(&mut self) {
        // Redemption hands the charge to the permit, which owns the refund from
        // then on. Only an abandoned grant refunds here.
        if !self.redeemed {
            self.ledger.refund(self.owner, self.charge, self.bytes);
        }
    }
}

/// A live permit. Holding one is what keeps capacity charged.
///
/// Conservation is defined against the **physical** drop of this value, not
/// against a logical "release" call: a caller that forgets to release still
/// refunds when the permit is dropped, and a caller that releases early
/// cannot refund twice because the release consumes the permit.
#[derive(Debug)]
pub struct CapacityPermit {
    owner: OwnerIdentity,
    bytes: u64,
    charge: PublicationIdentity,
    ledger: Arc<ProcessCapacityPool>,
    refunded: bool,
}

impl CapacityPermit {
    /// How many bytes this allocation holds charged.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// The charge identity this allocation will refund.
    pub fn charge(&self) -> PublicationIdentity {
        self.charge
    }

    /// Release the charge explicitly, consuming the allocation.
    ///
    /// Returns the bytes refunded. Taking `self` by value is what makes a double
    /// refund unrepresentable rather than merely discouraged.
    pub fn release(mut self) -> u64 {
        let refunded = self.ledger.refund(self.owner, self.charge, self.bytes);
        self.refunded = true;
        refunded
    }
}

impl Drop for CapacityPermit {
    fn drop(&mut self) {
        if !self.refunded {
            // The holder never released. Refund anyway: an un-refunded charge is
            // a permanent leak of process capacity, and "the caller should have
            // released" is not an accounting policy.
            self.ledger.refund(self.owner, self.charge, self.bytes);
        }
    }
}

/// One owner's accounting row.
#[derive(Debug, Default)]
struct OwnerRow {
    limit: u64,
    charged: u64,
    /// Charges issued and not yet refunded, so a refund can be validated rather
    /// than trusted.
    /// Keyed by charge identity. A `HashMap` rather than a `BTreeMap`
    /// because identities are deliberately not `Ord`: they are names, not a
    /// ranking, and nothing here may derive authority from one sorting before
    /// another.
    outstanding: HashMap<PublicationIdentity, u64>,
    parent: Option<OwnerIdentity>,
}

/// The process-wide capacity pool, kept as a ledger of charges.
///
/// Hierarchical: a child owner's limit is backed by its parent's headroom, so
/// the sum of everything charged beneath a root can never exceed that root.
#[derive(Debug)]
pub struct ProcessCapacityPool {
    identity: PoolIdentity,
    rows: std::sync::Mutex<BTreeMap<OwnerIdentity, OwnerRow>>,
    /// Refunds that named a charge the ledger did not have. A non-zero value
    /// means somebody is refunding capacity that was never charged.
    ///
    /// No public sequence reaches it any more: `redeem` refuses a foreign grant,
    /// `release_owner` refuses while charges or children are outstanding, and a
    /// permit refunds exactly once by construction. It stays as a fail-closed
    /// backstop for a future caller, and its unreachability is a property the
    /// oracles pin by proving each of those refusals — not a claim that the
    /// counter was exercised.
    unknown_refunds: AtomicU64,
}

impl Default for ProcessCapacityPool {
    fn default() -> Self {
        Self {
            identity: PoolIdentity::fresh(),
            rows: std::sync::Mutex::new(BTreeMap::new()),
            unknown_refunds: AtomicU64::new(0),
        }
    }
}

impl ProcessCapacityPool {
    /// A ledger with no owners.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// This pool's identity.
    pub fn identity(&self) -> PoolIdentity {
        self.identity
    }

    /// Create a root owner with a fixed limit.
    pub fn root(self: &Arc<Self>, limit: u64) -> OwnerIdentity {
        let identity = OwnerIdentity::fresh();
        self.rows.lock().expect("capacity ledger mutex").insert(
            identity,
            OwnerRow {
                limit,
                parent: None,
                ..OwnerRow::default()
            },
        );
        identity
    }

    /// Create a child owner backed by `parent`'s remaining headroom.
    pub fn child(
        self: &Arc<Self>,
        parent: OwnerIdentity,
        limit: u64,
    ) -> Result<OwnerIdentity, CapacityRefusal> {
        let mut rows = self.rows.lock().expect("capacity ledger mutex");
        let available = rows
            .get(&parent)
            .map(|row| row.limit.saturating_sub(row.charged))
            .unwrap_or(0);
        if limit > available {
            return Err(CapacityRefusal::ExceedsParent {
                requested: limit,
                available,
            });
        }
        // A child's limit is charged against the parent immediately: capacity
        // promised to a child is capacity the parent can no longer promise
        // elsewhere, whether or not the child has spent it yet.
        if let Some(row) = rows.get_mut(&parent) {
            row.charged += limit;
        }
        let identity = OwnerIdentity::fresh();
        rows.insert(
            identity,
            OwnerRow {
                limit,
                parent: Some(parent),
                ..OwnerRow::default()
            },
        );
        Ok(identity)
    }

    /// Reserve `bytes` from `owner`, producing a single-use grant.
    pub fn reserve(
        self: &Arc<Self>,
        owner: OwnerIdentity,
        bytes: u64,
    ) -> Result<CapacityGrant, CapacityRefusal> {
        let mut rows = self.rows.lock().expect("capacity ledger mutex");
        // An owner this pool does not have is not a full owner. Reporting
        // `Exhausted { available: 0 }` for it gave one answer to two different
        // questions, and the caller cannot tell "your account is spent" from
        // "you have no account".
        let row = rows.get_mut(&owner).ok_or(CapacityRefusal::UnknownOwner)?;
        let available = row.limit.saturating_sub(row.charged);
        if bytes > available {
            return Err(CapacityRefusal::Exhausted {
                requested: bytes,
                available,
            });
        }
        let charge = PublicationIdentity::fresh();
        row.charged += bytes;
        row.outstanding.insert(charge, bytes);
        drop(rows);
        Ok(CapacityGrant {
            owner,
            bytes,
            charge,
            pool: self.identity,
            ledger: Arc::clone(self),
            redeemed: false,
        })
    }

    /// Redeem a grant into a live permit.
    ///
    /// Consumes the grant by value, so one grant can never back two permits, and
    /// refuses a grant this pool did not issue. Honouring a foreign grant would
    /// charge the issuing pool forever while the refund landed here, so the pool
    /// that actually lost capacity would be the one reporting a clean account.
    pub fn redeem(
        self: &Arc<Self>,
        mut grant: CapacityGrant,
    ) -> Result<CapacityPermit, CapacityRefusal> {
        if grant.pool != self.identity {
            // Left un-redeemed on purpose: the refusal returns the grant's bytes
            // to the pool that actually charged them, through the grant's own
            // `Drop`, rather than stranding them because the wrong pool asked.
            return Err(CapacityRefusal::ForeignGrant);
        }
        // The permit takes the charge over from here, so the grant's `Drop` must
        // not also refund it.
        grant.redeemed = true;
        Ok(CapacityPermit {
            owner: grant.owner,
            bytes: grant.bytes,
            charge: grant.charge,
            ledger: Arc::clone(self),
            refunded: false,
        })
    }

    /// How many bytes `owner` currently has charged.
    pub fn charged(&self, owner: OwnerIdentity) -> u64 {
        self.rows
            .lock()
            .expect("capacity ledger mutex")
            .get(&owner)
            .map(|row| row.charged)
            .unwrap_or(0)
    }

    /// How many bytes `owner` can still reserve.
    pub fn available(&self, owner: OwnerIdentity) -> u64 {
        self.rows
            .lock()
            .expect("capacity ledger mutex")
            .get(&owner)
            .map(|row| row.limit.saturating_sub(row.charged))
            .unwrap_or(0)
    }

    /// How many charges `owner` has outstanding.
    pub fn outstanding_charges(&self, owner: OwnerIdentity) -> usize {
        self.rows
            .lock()
            .expect("capacity ledger mutex")
            .get(&owner)
            .map(|row| row.outstanding.len())
            .unwrap_or(0)
    }

    /// Refunds that named a charge this ledger never issued.
    ///
    /// Reported rather than silently ignored: a non-zero count means some caller
    /// believes it holds capacity the ledger never gave it.
    pub fn unknown_refunds(&self) -> u64 {
        self.unknown_refunds.load(Ordering::Relaxed)
    }

    /// Release a child owner, refunding its whole promise to its parent.
    ///
    /// Refuses while the child still holds outstanding charges OR still backs
    /// children of its own. Both would let the process promise the same bytes
    /// twice: a child's limit is charged to its parent at `child()` time and
    /// recorded in `charged`, never in `outstanding`, so an owner with live
    /// children looked perfectly drained to an outstanding-only check. Releasing
    /// it returned its whole limit to the grandparent while its own children kept
    /// spending against a limit nothing backed any more.
    pub fn release_owner(self: &Arc<Self>, owner: OwnerIdentity) -> Result<u64, CapacityRefusal> {
        let mut rows = self.rows.lock().expect("capacity ledger mutex");
        let Some(row) = rows.get(&owner) else {
            // Releasing an owner this pool does not have refunded nothing, and
            // reporting `Ok(0)` for it is a success this call never observed.
            return Err(CapacityRefusal::UnknownOwner);
        };
        if !row.outstanding.is_empty() {
            return Err(CapacityRefusal::Exhausted {
                requested: 0,
                available: row.limit.saturating_sub(row.charged),
            });
        }
        // Scanning beats a maintained counter here: the rows are few, and a
        // counter is a second representation of the same fact that can drift
        // from it.
        let children = rows
            .values()
            .filter(|candidate| candidate.parent == Some(owner))
            .count();
        if children > 0 {
            return Err(CapacityRefusal::HasChildren { children });
        }
        let limit = row.limit;
        let parent = row.parent;
        // A recorded parent that is not in the pool means the refund has nowhere
        // to land. Returning the limit anyway would report capacity restored to
        // an account that does not exist.
        if let Some(parent) = parent
            && !rows.contains_key(&parent)
        {
            return Err(CapacityRefusal::UnknownOwner);
        }
        rows.remove(&owner);
        if let Some(parent) = parent
            && let Some(parent_row) = rows.get_mut(&parent)
        {
            parent_row.charged = parent_row.charged.saturating_sub(limit);
        }
        Ok(limit)
    }

    /// Refund one charge. Returns the bytes actually refunded.
    ///
    /// A charge the ledger does not hold refunds ZERO and increments
    /// `unknown_refunds`. Refunding it anyway would invent capacity.
    fn refund(&self, owner: OwnerIdentity, charge: PublicationIdentity, bytes: u64) -> u64 {
        let mut rows = self.rows.lock().expect("capacity ledger mutex");
        let Some(row) = rows.get_mut(&owner) else {
            self.unknown_refunds.fetch_add(1, Ordering::Relaxed);
            return 0;
        };
        match row.outstanding.remove(&charge) {
            Some(recorded) => {
                debug_assert_eq!(
                    recorded, bytes,
                    "charge size changed between reserve and refund"
                );
                row.charged = row.charged.saturating_sub(recorded);
                recorded
            }
            None => {
                self.unknown_refunds.fetch_add(1, Ordering::Relaxed);
                0
            }
        }
    }
}
