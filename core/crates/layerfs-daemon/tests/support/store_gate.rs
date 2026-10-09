//! A gate at the public pack persistence port of a Store's read sessions, for
//! proofs that need a request INSIDE a Store read. Test code at a port the
//! product takes by injection (`Storage::new` over `Arc<dyn PackPersistence>`,
//! as `mounted_commit_failures.rs` wraps the writer); not a product hook.
//!
//! While armed, every read call of a gated session stops before it is
//! forwarded to the real provider, so the product's reader lease, its
//! canonical client and the request that made the demand are all inside
//! that one provider call. `release` lets every held call go on to the real
//! provider, whose answer is returned unchanged.
//!
//! A held call cannot wait forever: at `HOLD` it gives up with the port's
//! own definite refusal and is counted in `expired`, which every test
//! asserts is zero. The file is standalone: the including test declares it
//! as `#[path = "support/store_gate.rs"] mod store_gate;`.
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{LocatedObject, SignatureRow},
    port::{
        AcquiredPackRead, PackPersistence, PackReadPlan, PersistedPack, PersistedPackRead,
        PersistenceError, Publication, Published, PublishedPack, Reserve, Reserved,
        ValueGroupQuery, ValueGroups,
    },
    StoragePolicy,
};
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::{Duration, Instant},
};

/// The longest one provider call is held. Far above every bounded wait of a
/// test and far below the two-minute ceiling of a test command.
pub const HOLD: Duration = Duration::from_secs(20);

/// Read calls of the gated sessions, by count.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Calls {
    /// Read calls that reached the port.
    pub entered: u64,
    /// Read calls that returned to the product, with any answer.
    pub returned: u64,
    /// Read calls stopped at the gate right now.
    pub holding: u64,
    /// Held calls that gave up at `HOLD`. A staging failure, never expected.
    pub expired: u64,
}
#[derive(Default)]
struct State {
    armed: bool,
    calls: Calls,
}
#[derive(Default)]
pub struct Gate {
    state: Mutex<State>,
    changed: Condvar,
}
impl Gate {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panicking test thread must not turn every later call into a
        // second panic on a product worker.
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
    /// Every read call made from now on is held until `release`.
    pub fn arm(&self) {
        self.lock().armed = true;
    }
    /// Lets every held call go on and holds none afterwards.
    pub fn release(&self) {
        self.lock().armed = false;
        self.changed.notify_all();
    }
    pub fn observe(&self) -> Calls {
        self.lock().calls
    }
    fn enter(&self) -> Result<Pass<'_>, PersistenceError> {
        let mut state = self.lock();
        state.calls.entered += 1;
        if state.armed {
            state.calls.holding += 1;
            let deadline = Instant::now() + HOLD;
            while state.armed {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    state.calls.holding -= 1;
                    state.calls.expired += 1;
                    state.calls.returned += 1;
                    return Err(PersistenceError::Refused {
                        status: "test gate held past its bound".into(),
                    });
                }
                state = self
                    .changed
                    .wait_timeout(state, left)
                    .unwrap_or_else(|poison| poison.into_inner())
                    .0;
            }
            state.calls.holding -= 1;
        }
        Ok(Pass(self))
    }
}
/// One read call between the gate and its return to the product.
struct Pass<'a>(&'a Gate);
impl Drop for Pass<'_> {
    fn drop(&mut self) {
        self.0.lock().calls.returned += 1;
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        // Nothing can still be held: a held call borrows the gate.
        self.changed.notify_all();
    }
}

/// One real read session behind the gate. Its policy is read ungated (the
/// Store reads it once when it is composed) and it never writes.
pub struct Gated {
    pub inner: Arc<dyn PackPersistence>,
    pub gate: Arc<Gate>,
}
type Answer<T> = Result<T, PersistenceError>;
impl PackPersistence for Gated {
    fn policy(&self) -> Answer<StoragePolicy> {
        self.inner.policy()
    }
    fn locate(&self, ids: &[ObjectId], out: &mut Vec<LocatedObject>) -> Answer<()> {
        let _pass = self.gate.enter()?;
        self.inner.locate(ids, out)
    }
    fn read_packs(&self, ids: &[i64], out: &mut Vec<PersistedPack>) -> Answer<()> {
        let _pass = self.gate.enter()?;
        self.inner.read_packs(ids, out)
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn PackReadPlan,
    ) -> Answer<PersistedPackRead> {
        let _pass = self.gate.enter()?;
        self.inner.read_pack_selection(id, plan)
    }
    fn read_scoped_pack(&self, id: i64, plan: &mut dyn PackReadPlan) -> Answer<AcquiredPackRead> {
        let _pass = self.gate.enter()?;
        self.inner.read_scoped_pack(id, plan)
    }
    fn value_groups(&self, query: ValueGroupQuery<'_>) -> Answer<ValueGroups> {
        let _pass = self.gate.enter()?;
        self.inner.value_groups(query)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Answer<()> {
        let _pass = self.gate.enter()?;
        self.inner.signatures(out)
    }
    fn reserve(&self, request: Reserve) -> Answer<Reserved> {
        self.inner.reserve(request)
    }
    fn publication_pack_cost(&self, pack: &PublishedPack) -> Answer<(usize, u64)> {
        self.inner.publication_pack_cost(pack)
    }
    fn publish(&self, batch: &Publication) -> Answer<Published> {
        self.inner.publish(batch)
    }
}
