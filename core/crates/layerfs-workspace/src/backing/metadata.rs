//! Consumer-wide admission and immutable root ownership for the local edit profile.
use super::{
    budget::{Budget, Charge},
    directory::Directory,
    metadata_pages::PageRef,
    ownership::CleanupFrame,
    payload::{bare_failure, PayloadHost},
    segments::Window,
};
use crate::*;
use std::{
    mem::size_of,
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
    time::{Duration, Instant},
};
pub const CANDIDATE_BYTES: u64 = 137 * 4096;
pub const ESCROW: u64 = 208 * 4096;
pub const WORKING: usize = 640 * 1024;
const RETAINED: usize = 128 * 1024;
/// Not part of the public API.
pub type MetadataCharge = (Charge, Charge);
pub const MAX_ROOTS: usize = 32;
pub struct MetadataHost {
    pub payloads: Arc<PayloadHost>,
    pub gate: AtomicBool,
    pub slots: AtomicUsize,
    next: AtomicU64,
    pub roots: Mutex<Vec<Arc<RootOwner>>>,
    pub arenas: Mutex<Vec<Arc<Arena>>>,
    root_charge: Mutex<MetadataCharge>,
    arena_charge: Mutex<MetadataCharge>,
    persistent: Arc<Budget>,
    _charge: MetadataCharge,
}
pub struct Arena {
    pub id: u64,
    pub directory: Arc<Directory>,
    pub host: Weak<MetadataHost>,
    /// Ordinary product telemetry: how many metadata pages this arena has read
    /// since it was created. It shows whether a walk re-reads a path it already
    /// holds, and it never gates a read.
    pub reads: AtomicU64,
    pub state: Mutex<ArenaState>,
    pub ledger_charge: Mutex<MetadataCharge>,
    pub _charge: MetadataCharge,
}
pub struct ArenaState {
    pub next: u32,
    pub reserved_slots: usize,
    pub free: PageRef,
    pub reusable: usize,
    pub ledgers: u32,
    pub ledger_identities: Vec<(u64, u64)>,
    pub pages: usize,
    pub allocated: u64,
    pub blocked: bool,
    pub complete: bool,
    pub unrecoverable: bool,
}
pub struct RootOwner {
    pub arena: Arc<Arena>,
    pub parent: Option<Arc<RootOwner>>,
    pub fund: Option<Arc<ProgressFund>>,
    pub allowance: u64,
    pub state: Mutex<RootState>,
    pub _charge: MetadataCharge,
}
pub struct RootState {
    pub root: PageRef,
    pub temporary: Vec<PageRef>,
    pub slot_pending: Option<PageRef>,
    pub edge_progress: Option<(PageRef, usize)>,
    pub reserved: u64,
    pub slot_credits: usize,
    pub ledger_table: Option<LedgerTable>,
    pub completion_generation: Option<u64>,
    pub pending: Option<Pending>,
    /// The candidate's one custody page with the payload it names, so releasing
    /// it is an indexed lookup rather than a walk of the ownership registry.
    pub custodies: Vec<(PageRef, u64)>,
    pub cleanup: Vec<super::ownership::CleanupFrame>,
    pub cleanup_failed: bool,
}
pub struct LedgerTable {
    pub identities: Vec<(u64, u64)>,
    pub charge: MetadataCharge,
}
pub struct Pending {
    pub name: String,
    pub identity: Option<(u64, u64)>,
    pub allocated: Option<u64>,
    pub reservation: u64,
    pub collider: bool,
}
pub struct Writer {
    pub host: Arc<MetadataHost>,
    pub _memory: Charge,
}
impl Drop for Writer {
    fn drop(&mut self) {
        self._memory
            .resize(0)
            .expect("shrinking a working reservation cannot fail");
        self.host.gate.store(false, Ordering::Release);
    }
}
impl MetadataHost {
    pub fn new(payloads: Arc<PayloadHost>) -> Result<Arc<Self>, WorkspaceError> {
        let persistent = Budget::new(RETAINED);
        let charge = (
            payloads.budget.reserve(size_of::<Self>() + 64)?,
            persistent.reserve(size_of::<Self>() + 64)?,
        );
        Ok(Arc::new(Self {
            root_charge: Mutex::new((payloads.budget.reserve(0)?, persistent.reserve(0)?)),
            arena_charge: Mutex::new((payloads.budget.reserve(0)?, persistent.reserve(0)?)),
            persistent,
            _charge: charge,
            payloads,
            gate: AtomicBool::new(false),
            slots: AtomicUsize::new(0),
            next: AtomicU64::new(1),
            roots: Mutex::new(Vec::new()),
            arenas: Mutex::new(Vec::new()),
        }))
    }
    pub fn memory(&self, bytes: usize) -> Result<MetadataCharge, WorkspaceError> {
        Ok((
            self.payloads.budget.reserve(bytes)?,
            self.persistent.reserve(bytes)?,
        ))
    }
    pub fn writer(self: &Arc<Self>) -> Result<Writer, WorkspaceError> {
        self.gate
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| WorkspaceError::Busy)?;
        let memory = match self.payloads.budget.reserve(WORKING) {
            Ok(memory) => memory,
            Err(error) => {
                self.gate.store(false, Ordering::Release);
                return Err(error);
            }
        };
        Ok(Writer {
            host: self.clone(),
            _memory: memory,
        })
    }
    /// Acquires the writer gate, waiting deadline-bounded for a current holder.
    ///
    /// Every hold is short - one state read, one splice, one install - so a
    /// caller that meets contention waits for the holder instead of refusing
    /// with `Busy`: the Commit reconciliation's two ordering points and the
    /// mounted mutation paths must never turn a microsecond overlap into an
    /// `EBUSY` a shell command observes. The wait ends at the caller's own
    /// operation deadline, never past it.
    pub fn writer_until(self: &Arc<Self>, deadline: Instant) -> Result<Writer, WorkspaceError> {
        loop {
            match self.writer() {
                Ok(writer) => return Ok(writer),
                Err(WorkspaceError::Busy) => {
                    if Instant::now() >= deadline {
                        return Err(WorkspaceError::Deadline);
                    }
                    std::thread::sleep(Duration::from_micros(200));
                }
                Err(error) => return Err(error),
            }
        }
    }
    /// Metadata pages every arena of this host has read, as product telemetry.
    pub fn page_reads(&self) -> Result<u64, WorkspaceError> {
        let arenas = self.arenas.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(arenas
            .iter()
            .map(|arena| arena.reads.load(Ordering::Relaxed))
            .fold(0u64, u64::saturating_add))
    }
    pub fn arena(
        self: &Arc<Self>,
        directory: Arc<Directory>,
    ) -> Result<Arc<Arena>, WorkspaceError> {
        let mut arenas = self.arenas.lock().map_err(|_| WorkspaceError::Io)?;
        if arenas.len() == 11 {
            return Err(WorkspaceError::Capacity);
        }
        grow(&mut arenas, self, &self.arena_charge, 11)?;
        let id = self
            .next
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_add(1))
            .map_err(|_| WorkspaceError::Capacity)?;
        let arena = Arc::new(Arena {
            id,
            directory,
            host: Arc::downgrade(self),
            reads: AtomicU64::new(0),
            state: Mutex::new(ArenaState {
                next: 1,
                reserved_slots: 0,
                free: PageRef::NULL,
                reusable: 0,
                ledgers: 0,
                ledger_identities: Vec::new(),
                pages: 0,
                allocated: 0,
                blocked: false,
                complete: true,
                unrecoverable: false,
            }),
            ledger_charge: Mutex::new(self.memory(0)?),
            _charge: self.memory(size_of::<Arena>() + 256)?,
        });
        arenas.push(arena.clone());
        Ok(arena)
    }
    pub fn reserve(&self, bytes: u64) -> Result<(), WorkspaceError> {
        let mut s = self.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        if s.stopped {
            return Err(WorkspaceError::Busy);
        }
        if s.allocated
            .checked_add(s.reserved)
            .and_then(|n| n.checked_add(bytes))
            .is_none_or(|n| n > self.payloads.quota)
        {
            return Err(bare_failure(
                BackingPhase::Allocate,
                std::io::ErrorKind::StorageFull,
            ));
        }
        s.reserved += bytes;
        s.metadata_reserved += bytes;
        Ok(())
    }
    pub fn release(&self, allocated: u64, reserved: u64) -> Result<(), WorkspaceError> {
        let mut s = self.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        s.allocated = s
            .allocated
            .checked_sub(allocated)
            .ok_or(WorkspaceError::Io)?;
        s.metadata_allocated = s
            .metadata_allocated
            .checked_sub(allocated)
            .ok_or(WorkspaceError::Io)?;
        s.reserved = s.reserved.checked_sub(reserved).ok_or(WorkspaceError::Io)?;
        s.metadata_reserved = s
            .metadata_reserved
            .checked_sub(reserved)
            .ok_or(WorkspaceError::Io)?;
        Ok(())
    }
    pub fn transfer(&self, amount: u64, reserved: u64) -> Result<(), WorkspaceError> {
        let mut s = self.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        s.allocated = s.allocated.checked_add(amount).ok_or(WorkspaceError::Io)?;
        s.metadata_allocated = s
            .metadata_allocated
            .checked_add(amount)
            .ok_or(WorkspaceError::Io)?;
        s.reserved = s.reserved.checked_sub(reserved).ok_or(WorkspaceError::Io)?;
        s.metadata_reserved = s
            .metadata_reserved
            .checked_sub(reserved)
            .ok_or(WorkspaceError::Io)?;
        if amount > reserved {
            s.stopped = true;
            s.metadata_stopped = true;
        }
        Ok(())
    }

    /// The exact earlier root a maintained delta written into a candidate built
    /// on `owner` is a delta against. One generation may publish more than one
    /// root, so the operation's own base is the candidate's parent, never the
    /// root it is about to publish.
    pub fn anchor(owner: Option<&Arc<RootOwner>>) -> Option<Arc<RootOwner>> {
        owner.and_then(|owner| owner.parent.clone())
    }
    pub fn candidate(
        self: &Arc<Self>,
        arena: &Arc<Arena>,
        generation: u64,
        needs_completion: bool,
        parent: Option<Arc<RootOwner>>,
    ) -> Result<Arc<RootOwner>, WorkspaceError> {
        let mut roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        for root in roots.iter().filter(|r| Arc::ptr_eq(&r.arena, arena)) {
            if root
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .slot_pending
                .is_some()
            {
                return Err(WorkspaceError::Busy);
            }
        }
        if roots.len() == MAX_ROOTS {
            return Err(WorkspaceError::Capacity);
        }
        grow(&mut roots, self, &self.root_charge, MAX_ROOTS)?;
        let mut charge = self.memory(size_of::<RootOwner>() + 128 * size_of::<PageRef>() + 384)?;
        let mut temporary = Vec::new();
        temporary
            .try_reserve_exact(128)
            .map_err(|_| WorkspaceError::Capacity)?;
        resize_memory(
            &mut charge,
            size_of::<RootOwner>() + temporary.capacity() * size_of::<PageRef>() + 384,
        )?;
        let custodies = super::metadata_index::vector(1)?;
        let cleanup = super::metadata_index::vector(12)?;
        let a = arena.state.lock().map_err(|_| WorkspaceError::Io)?;
        if a.blocked {
            return Err(WorkspaceError::Busy);
        }
        let reservation = CANDIDATE_BYTES + if needs_completion { ESCROW } else { 0 };
        drop(a);
        self.reserve(reservation)?;
        let owner = Arc::new(RootOwner {
            arena: arena.clone(),
            parent,
            fund: None,
            allowance: CANDIDATE_BYTES,
            state: Mutex::new(RootState {
                root: PageRef::NULL,
                temporary,
                slot_pending: None,
                edge_progress: None,
                reserved: reservation,
                slot_credits: 0,
                ledger_table: None,
                completion_generation: needs_completion.then_some(generation),
                pending: None,
                custodies,
                cleanup,
                cleanup_failed: false,
            }),
            _charge: charge,
        });
        roots.push(owner.clone());
        Ok(owner)
    }
    fn empty_root(
        &self,
        arena: &Arc<Arena>,
        fund: Option<&Arc<ProgressFund>>,
        allowance: u64,
    ) -> Result<Arc<RootOwner>, WorkspaceError> {
        let charge = self.memory(size_of::<RootOwner>() + 128 * size_of::<PageRef>() + 384)?;
        Ok(Arc::new(RootOwner {
            arena: arena.clone(),
            parent: None,
            fund: fund.cloned(),
            allowance,
            state: Mutex::new(RootState {
                root: PageRef::NULL,
                temporary: super::metadata_index::vector(128)?,
                slot_pending: None,
                edge_progress: None,
                reserved: 0,
                slot_credits: 0,
                ledger_table: None,
                completion_generation: None,
                pending: None,
                custodies: super::metadata_index::vector(1)?,
                cleanup: super::metadata_index::vector(12)?,
                cleanup_failed: false,
            }),
            _charge: charge,
        }))
    }
    pub fn clean_capture_root(
        self: &Arc<Self>,
        arena: &Arc<Arena>,
        generation: u64,
    ) -> Result<Arc<RootOwner>, WorkspaceError> {
        let root = self.empty_root(arena, None, 0)?;
        let mut roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        if roots.len() == MAX_ROOTS {
            return Err(WorkspaceError::Capacity);
        }
        grow(&mut roots, self, &self.root_charge, MAX_ROOTS)?;
        let mut state = root.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.reserve(ESCROW)?;
        state.reserved = ESCROW;
        state.completion_generation = Some(generation);
        drop(state);
        roots.push(root.clone());
        Ok(root)
    }
    pub fn release_clean_capture_root(&self, root: &Arc<RootOwner>) -> Result<(), WorkspaceError> {
        let mut roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        let mut state = root.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.root != PageRef::NULL
            || state.pending.is_some()
            || !state.temporary.is_empty()
            || state.slot_credits != 0
            || state.completion_generation.is_none()
            || state.reserved != ESCROW
        {
            return Err(WorkspaceError::Io);
        }
        self.release(0, state.reserved)?;
        state.reserved = 0;
        state.completion_generation = None;
        drop(state);
        roots.retain(|existing| !Arc::ptr_eq(existing, root));
        Ok(())
    }
    pub fn reconciliation_root(
        self: &Arc<Self>,
        arena: &Arc<Arena>,
        fund: &Arc<ProgressFund>,
    ) -> Result<Arc<RootOwner>, WorkspaceError> {
        let ledger_charge = self.memory(1058 * 16)?;
        let ledger_table = LedgerTable {
            identities: super::metadata_index::vector(1058)?,
            charge: ledger_charge,
        };
        let owner = self.empty_root(arena, Some(fund), 64 * 4096)?;
        owner
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .ledger_table = Some(ledger_table);
        let mut roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        if roots.len() == MAX_ROOTS {
            return Err(WorkspaceError::Capacity);
        }
        grow(&mut roots, self, &self.root_charge, MAX_ROOTS)?;
        if arena.state.lock().map_err(|_| WorkspaceError::Io)?.blocked {
            return Err(WorkspaceError::Busy);
        }
        for root in roots.iter().filter(|root| Arc::ptr_eq(&root.arena, arena)) {
            if root
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .slot_pending
                .is_some()
            {
                return Err(WorkspaceError::Busy);
            }
        }
        let mut state = owner.state.lock().map_err(|_| WorkspaceError::Io)?;
        let mut arena_state = arena.state.lock().map_err(|_| WorkspaceError::Io)?;
        if arena_state.blocked
            || arena_state.next as usize - 1 + arena_state.reserved_slots + 26 > 65536
        {
            return Err(WorkspaceError::Capacity);
        }
        self.slots
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_add(26).filter(|total| *total <= 65536)
            })
            .map_err(|_| WorkspaceError::Capacity)?;
        if let Err(error) = fund.take(64 * 4096) {
            self.slots.fetch_sub(26, Ordering::AcqRel);
            return Err(error);
        }
        state.reserved = 64 * 4096;
        state.slot_credits = 26;
        arena_state.reserved_slots += 26;
        drop(arena_state);
        drop(state);
        roots.push(owner.clone());
        Ok(owner)
    }
    pub fn result_roots(
        self: &Arc<Self>,
        arena: &Arc<Arena>,
        fund: &Arc<ProgressFund>,
    ) -> Result<[Arc<RootOwner>; 2], WorkspaceError> {
        let first = self.empty_root(arena, Some(fund), 8 * 4096)?;
        let second = self.empty_root(arena, Some(fund), 8 * 4096)?;
        let mut roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        if roots.len() + 2 > MAX_ROOTS {
            return Err(WorkspaceError::Capacity);
        }
        if roots.capacity() < roots.len() + 2 {
            let count = roots
                .capacity()
                .saturating_mul(2)
                .max(roots.len() + 2)
                .min(MAX_ROOTS);
            let mut charge = self.memory(count * size_of::<Arc<RootOwner>>())?;
            let mut next = super::metadata_index::vector(count)?;
            resize_memory(&mut charge, next.capacity() * size_of::<Arc<RootOwner>>())?;
            next.append(&mut roots);
            drop(std::mem::replace(&mut *roots, next));
            *self.root_charge.lock().map_err(|_| WorkspaceError::Io)? = charge;
        }
        roots.push(first.clone());
        roots.push(second.clone());
        Ok([first, second])
    }
    pub fn remove_empty_result_roots(
        &self,
        roots: &[Arc<RootOwner>; 2],
    ) -> Result<(), WorkspaceError> {
        for root in roots {
            let state = root.state.lock().map_err(|_| WorkspaceError::Io)?;
            if state.root != PageRef::NULL
                || state.reserved != 0
                || state.pending.is_some()
                || !state.temporary.is_empty()
            {
                return Err(WorkspaceError::Busy);
            }
        }
        self.roots
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .retain(|root| !roots.iter().any(|r| Arc::ptr_eq(root, r)));
        Ok(())
    }
    pub fn status(&self) -> Result<MetadataStatus, WorkspaceError> {
        let roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        let arenas = self.arenas.lock().map_err(|_| WorkspaceError::Io)?;
        let mut pages = 0;
        let mut reusable = 0;
        let mut reserved_slots = 0;
        for a in arenas.iter() {
            let s = a.state.lock().map_err(|_| WorkspaceError::Io)?;
            pages += s.pages;
            reusable += s.reusable;
            reserved_slots += s.reserved_slots;
        }
        let s = self.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(MetadataStatus {
            allocated_pages: pages,
            reusable_pages: reusable,
            reserved_slots,
            roots: roots.len(),
            external_roots: roots.iter().filter(|r| Arc::strong_count(r) > 1).count(),
            allocated_bytes: s.metadata_allocated,
            reserved_bytes: s.metadata_reserved,
            working_bytes: self.persistent.used()
                + if self.gate.load(Ordering::Acquire) {
                    WORKING
                } else {
                    0
                },
            accounting_complete: s.metadata_complete,
            admission_stopped: s.metadata_stopped,
        })
    }
    pub fn quarantine(&self, arena: &Arena, accounting_complete: bool) {
        if let Ok(mut a) = arena.state.lock() {
            a.blocked = true;
            a.complete &= accounting_complete;
        }
        if let Ok(mut s) = self.payloads.state.lock() {
            s.metadata_stopped = true;
            s.stopped = true;
            s.metadata_complete &= accounting_complete;
            s.complete &= accounting_complete;
        }
    }
}
pub fn resize_memory(charge: &mut MetadataCharge, bytes: usize) -> Result<(), WorkspaceError> {
    charge.0.resize(bytes)?;
    charge.1.resize(bytes)?;
    Ok(())
}
fn grow<T>(
    values: &mut Vec<Arc<T>>,
    host: &MetadataHost,
    charge: &Mutex<MetadataCharge>,
    limit: usize,
) -> Result<(), WorkspaceError> {
    if values.len() < values.capacity() {
        return Ok(());
    }
    let count = values.capacity().saturating_mul(2).clamp(1, limit);
    let mut next_charge = host.memory(count * size_of::<Arc<T>>())?;
    let mut next = Vec::new();
    next.try_reserve_exact(count)
        .map_err(|_| WorkspaceError::Capacity)?;
    resize_memory(&mut next_charge, next.capacity() * size_of::<Arc<T>>())?;
    next.append(values);
    drop(std::mem::replace(values, next));
    *charge.lock().map_err(|_| WorkspaceError::Io)? = next_charge;
    Ok(())
}
pub struct ProgressFund {
    pub host: Weak<MetadataHost>,
    available: Mutex<FundState>,
    generation: Mutex<Option<u64>>,
    _charge: MetadataCharge,
}
struct FundState {
    bytes: u64,
    finished: bool,
}
impl ProgressFund {
    pub fn new(host: &Arc<MetadataHost>) -> Result<Arc<Self>, WorkspaceError> {
        Ok(Arc::new(Self {
            host: Arc::downgrade(host),
            available: Mutex::new(FundState {
                bytes: 0,
                finished: false,
            }),
            generation: Mutex::new(None),
            _charge: host.memory(256)?,
        }))
    }
    pub fn install(&self, reserve: CompletionReserve) -> Result<(), WorkspaceError> {
        let mut generation = self.generation.lock().map_err(|_| WorkspaceError::Io)?;
        if generation.is_some() {
            return Err(WorkspaceError::Io);
        }
        *generation = Some(reserve.generation);
        self.available.lock().map_err(|_| WorkspaceError::Io)?.bytes = reserve.bytes;
        Ok(())
    }
    pub fn take(&self, bytes: u64) -> Result<(), WorkspaceError> {
        let mut available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        if available.finished {
            return Err(WorkspaceError::Busy);
        }
        available.bytes = available
            .bytes
            .checked_sub(bytes)
            .ok_or(WorkspaceError::Capacity)?;
        Ok(())
    }
    pub fn give(&self, bytes: u64) -> Result<(), WorkspaceError> {
        let mut available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        if available.finished {
            return self
                .host
                .upgrade()
                .ok_or(WorkspaceError::Closed)?
                .release(0, bytes);
        }
        available.bytes = available
            .bytes
            .checked_add(bytes)
            .ok_or(WorkspaceError::Io)?;
        Ok(())
    }
    pub fn finish(&self) -> Result<(), WorkspaceError> {
        let mut available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        if available.finished {
            return Err(WorkspaceError::Busy);
        }
        self.host
            .upgrade()
            .ok_or(WorkspaceError::Closed)?
            .release(0, available.bytes)?;
        available.bytes = 0;
        available.finished = true;
        Ok(())
    }
    pub fn recycle(&self, bytes: u64) -> Result<(), WorkspaceError> {
        let host = self.host.upgrade().ok_or(WorkspaceError::Closed)?;
        let mut available = self.available.lock().map_err(|_| WorkspaceError::Io)?;
        if available.finished {
            return host.release(bytes, 0);
        }
        let mut state = host.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        state.allocated = state
            .allocated
            .checked_sub(bytes)
            .ok_or(WorkspaceError::Io)?;
        state.metadata_allocated = state
            .metadata_allocated
            .checked_sub(bytes)
            .ok_or(WorkspaceError::Io)?;
        state.reserved = state
            .reserved
            .checked_add(bytes)
            .ok_or(WorkspaceError::Io)?;
        state.metadata_reserved = state
            .metadata_reserved
            .checked_add(bytes)
            .ok_or(WorkspaceError::Io)?;
        available.bytes = available
            .bytes
            .checked_add(bytes)
            .ok_or(WorkspaceError::Io)?;
        Ok(())
    }
}
impl Drop for ProgressFund {
    fn drop(&mut self) {
        if let (Some(host), Ok(available)) = (self.host.upgrade(), self.available.get_mut()) {
            let _ = host.release(0, available.bytes);
        }
    }
}
pub struct CompletionReserve {
    pub generation: u64,
    pub bytes: u64,
}
impl RootOwner {
    pub fn root(&self) -> Result<PageRef, WorkspaceError> {
        Ok(self.state.lock().map_err(|_| WorkspaceError::Io)?.root)
    }
    pub fn release_slot_credits(&self) -> Result<(), WorkspaceError> {
        let host = self.arena.host.upgrade().ok_or(WorkspaceError::Closed)?;
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let mut arena = self.arena.state.lock().map_err(|_| WorkspaceError::Io)?;
        let remaining = arena
            .reserved_slots
            .checked_sub(state.slot_credits)
            .ok_or(WorkspaceError::Io)?;
        host.slots
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                count.checked_sub(state.slot_credits)
            })
            .map_err(|_| WorkspaceError::Io)?;
        arena.reserved_slots = remaining;
        state.slot_credits = 0;
        let unused = state.ledger_table.take();
        drop(arena);
        drop(state);
        drop(unused);
        Ok(())
    }
    pub fn reserve_result_update(&self) -> Result<(), WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.root != PageRef::NULL
            || state.reserved != 0
            || state.pending.is_some()
            || !state.temporary.is_empty()
            || !state.cleanup.is_empty()
        {
            return Err(WorkspaceError::Busy);
        }
        let bytes = 8 * 4096;
        self.fund.as_ref().ok_or(WorkspaceError::Io)?.take(bytes)?;
        state.reserved = bytes;
        Ok(())
    }
    pub fn release_bytes(&self, allocated: u64, reserved: u64) -> Result<(), WorkspaceError> {
        if let Some(fund) = &self.fund {
            if allocated > 0 {
                fund.recycle(allocated)?;
            }
            if reserved > 0 {
                fund.give(reserved)?;
            }
            Ok(())
        } else {
            self.arena
                .host
                .upgrade()
                .ok_or(WorkspaceError::Closed)?
                .release(allocated, reserved)
        }
    }
    pub fn take_completion(
        &self,
        generation: u64,
    ) -> Result<Option<CompletionReserve>, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let Some(owner) = state.completion_generation else {
            return Ok(None);
        };
        if owner != generation || state.reserved != ESCROW {
            return Err(WorkspaceError::Io);
        }
        state.completion_generation = None;
        state.reserved = 0;
        Ok(Some(CompletionReserve {
            generation,
            bytes: ESCROW,
        }))
    }
    pub fn seal(
        &self,
        root: PageRef,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        loop {
            let page = {
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .temporary
                    .last()
                    .copied()
            };
            let Some(page) = page else { break };
            if page != root {
                // A page whose only reference was this candidate's own
                // temporary slot — an intermediate root a later chained update
                // of the same publication replaced — reaches zero here. The
                // reclaim queues exactly this case for cleanup, and seal must
                // too: a page seal orphans would keep its slot and every edge
                // it holds referenced forever, and a workspace that published
                // this way could never close clean.
                if self.arena.change_refs(page, -1, window, deadline)? == 0 {
                    let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                    if state.cleanup.len() == 12 {
                        return Err(WorkspaceError::Capacity);
                    }
                    state.cleanup.push(CleanupFrame {
                        page,
                        next: 0,
                        phase: 0,
                    });
                }
            }
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.temporary.pop();
            if page == root {
                state.root = root;
            }
        }
        loop {
            let page = {
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .custodies
                    .last()
                    .map(|(page, _)| *page)
            };
            let Some(page) = page else { break };
            if self.arena.change_refs(page, -1, window, deadline)? == 0 {
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                if state.cleanup.len() == 12 {
                    return Err(WorkspaceError::Capacity);
                }
                state.cleanup.push(CleanupFrame {
                    page,
                    next: 0,
                    phase: 0,
                });
            }
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .custodies
                .pop();
        }
        // Complete every frame this seal queued before the candidate returns,
        // exactly as a reclaim would: a sealed root owns no pending cleanup, so
        // it stays eligible for the routine reclaim every later mutation runs.
        let mut report = MetadataCleanupReport::default();
        loop {
            let frame = {
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .cleanup
                    .last()
                    .copied()
            };
            let Some(frame) = frame else { break };
            self.cleanup_step(frame, window, deadline, &mut report)?;
        }
        self.release_slot_credits()?;
        let host = self.arena.host.upgrade().ok_or(WorkspaceError::Closed)?;
        let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let retained = if s.completion_generation.is_some() {
            ESCROW
        } else {
            0
        };
        let reserve = s.reserved.checked_sub(retained).ok_or(WorkspaceError::Io)?;
        s.reserved = retained;
        drop(s);
        if let Some(fund) = &self.fund {
            fund.give(reserve)
        } else {
            host.release(0, reserve)
        }
    }
}
impl Workspace {
    pub fn metadata_status(&self) -> Result<MetadataStatus, WorkspaceError> {
        self.host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?
            .status()
    }
    pub fn reclaim_metadata(
        &self,
        deadline: Instant,
    ) -> Result<MetadataCleanupReport, WorkspaceError> {
        let _operation = self.begin(false, deadline)?;
        self.host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?
            .reclaim(self.inner.incarnation, deadline)
    }
}
