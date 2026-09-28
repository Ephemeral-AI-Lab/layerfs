//! Charged registry of held public read-only view leases.
//!
//! One lease is one pinned selected view plus every entry that view issued.
//! The registry is the server-side authority for entry binding: a lookup never
//! trusts a client-supplied path, and an entry resolved by one lease is refused
//! by every other. Holding a lease keeps its pinned revision in the active
//! index's frozen set, so the charged pin/retirement bookkeeping is real.
use crate::{
    backing::budget::{Budget, Charge},
    filesystem::namespace_view::View,
    NodeKind, WorkspaceError,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

/// Held leases per Workspace. The bound is part of the public contract.
pub const MAXIMUM_VIEW_LEASES: usize = 32;
/// Lease-token bytes: one tag byte plus 32 unguessable bytes.
pub const VIEW_LEASE_BYTES: usize = 33;
/// Fixed charge per held lease, covering the token, root record and guard.
const LEASE_BYTES: usize = 256;
/// Fixed charge per issued entry, covering its pinned path and record.
const ENTRY_BYTES: usize = 512;

/// One entry a lease issued, as resolved in the pinned view.
pub(crate) struct HeldEntry {
    pub(crate) path: Vec<u8>,
    pub(crate) kind: NodeKind,
    pub(crate) canonical: bool,
    pub(crate) content: [u8; 32],
    pub(crate) size: u64,
}

struct HeldLease {
    view: View,
    generation: u64,
    revision: u64,
    entries: BTreeMap<u64, HeldEntry>,
    charge: Charge,
}

/// Server-side registry of held view leases for one Workspace.
pub struct ViewLeases {
    inner: Mutex<BTreeMap<[u8; VIEW_LEASE_BYTES], HeldLease>>,
    budget: Arc<Budget>,
}

impl ViewLeases {
    pub(crate) fn new(budget: &Arc<Budget>) -> Result<Self, WorkspaceError> {
        Ok(Self {
            inner: Mutex::new(BTreeMap::new()),
            budget: budget.clone(),
        })
    }

    /// Number of held leases; zero is close-clean's requirement.
    pub fn held(&self) -> Result<usize, WorkspaceError> {
        Ok(self.inner.lock().map_err(|_| WorkspaceError::Io)?.len())
    }

    pub(crate) fn register(
        &self,
        token: [u8; VIEW_LEASE_BYTES],
        view: View,
        generation: u64,
        revision: u64,
    ) -> Result<(), WorkspaceError> {
        let mut inner = self.inner.lock().map_err(|_| WorkspaceError::Io)?;
        if inner.contains_key(&token) {
            return Err(WorkspaceError::InvalidInput);
        }
        if inner.len() == MAXIMUM_VIEW_LEASES {
            return Err(WorkspaceError::Capacity);
        }
        let charge = self.budget.reserve(LEASE_BYTES)?;
        inner.insert(
            token,
            HeldLease {
                view,
                generation,
                revision,
                entries: BTreeMap::new(),
                charge,
            },
        );
        Ok(())
    }

    /// Pinned generation, pinned revision and issued-entry count.
    pub(crate) fn observe(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
    ) -> Result<(u64, u64, usize), WorkspaceError> {
        let inner = self.inner.lock().map_err(|_| WorkspaceError::Io)?;
        let lease = inner.get(token).ok_or(WorkspaceError::Denied)?;
        Ok((lease.generation, lease.revision, lease.entries.len()))
    }

    /// Runs one read against the held pinned view. The registry lock is held
    /// for the read's duration, which serializes lease reads exactly like the
    /// daemon control slot serializes every other control operation.
    pub(crate) fn with_view<R>(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
        read: impl FnOnce(&View) -> Result<R, WorkspaceError>,
    ) -> Result<R, WorkspaceError> {
        let inner = self.inner.lock().map_err(|_| WorkspaceError::Io)?;
        let lease = inner.get(token).ok_or(WorkspaceError::Denied)?;
        read(&lease.view)
    }

    /// Charges and records one entry this lease resolved, bound to its serial.
    pub(crate) fn bind_entry(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
        serial: u64,
        entry: HeldEntry,
    ) -> Result<(), WorkspaceError> {
        let mut inner = self.inner.lock().map_err(|_| WorkspaceError::Io)?;
        let lease = inner.get_mut(token).ok_or(WorkspaceError::Denied)?;
        if lease.entries.contains_key(&serial) {
            return Ok(());
        }
        lease
            .charge
            .resize(LEASE_BYTES + (lease.entries.len() + 1) * ENTRY_BYTES)?;
        lease.entries.insert(serial, entry);
        Ok(())
    }

    /// The pinned path and record facts of one entry this lease issued.
    pub(crate) fn entry(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
        serial: u64,
        kind: Option<NodeKind>,
    ) -> Result<HeldEntry, WorkspaceError> {
        let inner = self.inner.lock().map_err(|_| WorkspaceError::Io)?;
        let lease = inner.get(token).ok_or(WorkspaceError::Denied)?;
        let entry = lease.entries.get(&serial).ok_or(WorkspaceError::Denied)?;
        if entry.kind != kind.unwrap_or(entry.kind) {
            return Err(WorkspaceError::WrongKind);
        }
        Ok(HeldEntry {
            path: entry.path.clone(),
            kind: entry.kind,
            canonical: entry.canonical,
            content: entry.content,
            size: entry.size,
        })
    }

    /// Releases one lease through the checked retirement path. On success the
    /// pinned revision's retirement selector ran and the charge is refunded.
    /// On failure the snapshot is already released with the failed unlink
    /// cohort in charged custody; the Workspace stops and the lease is spent,
    /// so the outcome is reported and never retried.
    pub(crate) fn release(&self, token: &[u8; VIEW_LEASE_BYTES]) -> Result<(), WorkspaceError> {
        let mut inner = self.inner.lock().map_err(|_| WorkspaceError::Io)?;
        let lease = inner.get_mut(token).ok_or(WorkspaceError::Denied)?;
        let released = lease.view.release_active();
        if released.is_ok() {
            let mut lease = inner.remove(token).ok_or(WorkspaceError::Denied)?;
            lease.charge.resize(0)?;
        }
        released
    }
}
