//! A Store session's acquisition epoch, shared by both engines (#776): the
//! identity of the session's own first acquired operation, zero until then.
//! Read without a lock; once set it never changes.
use layerfs_storage::port::acquisition::{AcquisitionResult, Owner};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

#[derive(Default)]
pub(crate) struct Epoch {
    value: AtomicU64,
    /// Held by a `begin` only while the epoch is still zero, so two first
    /// operations cannot both claim one. A live owner's check never takes it,
    /// so a `begin` that waits for the lane blocks no read.
    first: Mutex<()>,
}

impl Epoch {
    /// The epoch, zero before the session's first operation.
    pub(crate) fn get(&self) -> u64 {
        self.value.load(Ordering::Acquire)
    }

    /// The epoch once no first `begin` is in flight: while it is zero, waits
    /// for one that may be claiming it (readiness, not a retry), so the
    /// operation it is creating is never taken for an abandoned one. Only
    /// the abandoned-operation calls need this; a live owner's epoch is set.
    pub(crate) fn settled(&self) -> u64 {
        let epoch = self.get();
        if epoch != 0 {
            return epoch;
        }
        // The guard protects no data, so a poisoned one is still a wait.
        let _first = self.first.lock().unwrap_or_else(|error| error.into_inner());
        self.get()
    }

    /// One `begin`, given the known epoch (zero: the operation claims one).
    /// A first `begin` holds `first` across its transaction, so a concurrent
    /// first `begin` waits for it and then reuses its epoch; the epoch is set
    /// only after the commit is known. Later `begin`s take no lock.
    pub(crate) fn begin(
        &self,
        begin: impl FnOnce(u64) -> AcquisitionResult<Owner>,
    ) -> AcquisitionResult<Owner> {
        let known = self.get();
        if known != 0 {
            return begin(known);
        }
        let _first = self.first.lock().unwrap_or_else(|error| error.into_inner());
        let known = self.get();
        let owner = begin(known)?;
        if known == 0 {
            self.value.store(owner.epoch, Ordering::Release);
        }
        Ok(owner)
    }
}
