//! Consumer-wide admission and immutable root ownership for the local edit profile.
use super::{
    budget::{Budget, Charge},
    directory::Directory,
    metadata_pages::PageRef,
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
    time::Instant,
};
pub const ESCROW: u64 = 137 * 4096;
pub const WORKING: usize = 640 * 1024;
const RETAINED: usize = 128 * 1024;
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
    pub state: Mutex<ArenaState>,
    pub ledger_charge: Mutex<MetadataCharge>,
    pub _charge: MetadataCharge,
}
pub struct ArenaState {
    pub next: u32,
    pub free: PageRef,
    pub reusable: usize,
    pub ledgers: u32,
    pub ledger_identities: Vec<(u64, u64)>,
    pub pages: usize,
    pub allocated: u64,
    pub escrow: u64,
    pub blocked: bool,
    pub complete: bool,
    pub unrecoverable: bool,
}
pub struct RootOwner {
    pub arena: Arc<Arena>,
    pub state: Mutex<RootState>,
    pub _charge: MetadataCharge,
}
pub struct RootState {
    pub root: PageRef,
    pub temporary: Vec<PageRef>,
    pub slot_pending: Option<PageRef>,
    pub edge_progress: Option<(PageRef, usize)>,
    pub reserved: u64,
    pub pending: Option<Pending>,
    pub custodies: Vec<PageRef>,
    pub cleanup: Vec<super::ownership::CleanupFrame>,
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
            state: Mutex::new(ArenaState {
                next: 1,
                free: PageRef::NULL,
                reusable: 0,
                ledgers: 0,
                ledger_identities: Vec::new(),
                pages: 0,
                allocated: 0,
                escrow: 0,
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
    pub fn candidate(
        self: &Arc<Self>,
        arena: &Arc<Arena>,
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
        let reservation = ESCROW + if a.escrow == 0 { ESCROW } else { 0 };
        drop(a);
        self.reserve(reservation)?;
        let owner = Arc::new(RootOwner {
            arena: arena.clone(),
            state: Mutex::new(RootState {
                root: PageRef::NULL,
                temporary,
                slot_pending: None,
                edge_progress: None,
                reserved: reservation,
                pending: None,
                custodies,
                cleanup,
            }),
            _charge: charge,
        });
        roots.push(owner.clone());
        Ok(owner)
    }
    pub fn status(&self) -> Result<MetadataStatus, WorkspaceError> {
        let roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        let arenas = self.arenas.lock().map_err(|_| WorkspaceError::Io)?;
        let mut pages = 0;
        let mut reusable = 0;
        for a in arenas.iter() {
            let s = a.state.lock().map_err(|_| WorkspaceError::Io)?;
            pages += s.pages;
            reusable += s.reusable;
        }
        let s = self.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(MetadataStatus {
            allocated_pages: pages,
            reusable_pages: reusable,
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
    pub fn reclaim(
        self: &Arc<Self>,
        incarnation: [u8; 32],
        deadline: Instant,
    ) -> Result<MetadataCleanupReport, WorkspaceError> {
        let _writer = self.writer()?;
        let mut lease = self.payloads.window(3, 4)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let mut report = MetadataCleanupReport::default();
        loop {
            let root = {
                let roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
                roots
                    .iter()
                    .find(|r| {
                        r.arena.directory.incarnation == incarnation && Arc::strong_count(r) == 1
                    })
                    .cloned()
            };
            let Some(root) = root else { break };
            root.reclaim(window, deadline, &mut report)?;
            self.roots
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .retain(|r| !Arc::ptr_eq(r, &root));
            report.roots_released += 1;
            self.refresh()?;
        }
        report.remaining_roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?.len();
        Ok(report)
    }
    fn refresh(&self) -> Result<(), WorkspaceError> {
        let roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        let arenas = self.arenas.lock().map_err(|_| WorkspaceError::Io)?;
        let mut complete = true;
        let mut stopped = false;
        for arena in arenas.iter() {
            let mut pending = false;
            let mut known = true;
            let mut retained = false;
            for root in roots.iter().filter(|r| Arc::ptr_eq(&r.arena, arena)) {
                retained = true;
                let r = root.state.lock().map_err(|_| WorkspaceError::Io)?;
                if let Some(p) = &r.pending {
                    pending = true;
                    known &= p.identity.is_some() && p.allocated.is_some();
                }
            }
            let mut a = arena.state.lock().map_err(|_| WorkspaceError::Io)?;
            a.blocked = a.unrecoverable || pending;
            a.complete = known && (a.complete || !a.unrecoverable);
            if !retained && a.escrow > 0 {
                self.release(0, a.escrow)?;
                a.escrow = 0;
            }
            complete &= a.complete;
            stopped |= a.blocked;
        }
        let mut state = self.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        state.metadata_complete = complete;
        state.metadata_stopped = stopped;
        self.payloads.refresh(&mut state)
    }
    pub fn close(
        self: &Arc<Self>,
        arena: &Arc<Arena>,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        self.reclaim(arena.directory.incarnation, deadline)?;
        let _writer = self.writer()?;
        if self
            .roots
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .iter()
            .any(|r| Arc::ptr_eq(&r.arena, arena))
        {
            return Err(WorkspaceError::Busy);
        }
        let mut lease = self.payloads.window(3, 4)?;
        arena.close(lease.window.as_mut().ok_or(WorkspaceError::Io)?, deadline)?;
        self.arenas
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .retain(|a| !Arc::ptr_eq(a, arena));
        self.refresh()?;
        Ok(())
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
impl RootOwner {
    pub fn root(&self) -> Result<PageRef, WorkspaceError> {
        Ok(self.state.lock().map_err(|_| WorkspaceError::Io)?.root)
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
                self.arena.change_refs(page, -1, window, deadline)?;
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
                    .copied()
            };
            let Some(page) = page else { break };
            self.arena.change_refs(page, -1, window, deadline)?;
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .custodies
                .pop();
        }
        let host = self.arena.host.upgrade().ok_or(WorkspaceError::Closed)?;
        let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let mut a = self.arena.state.lock().map_err(|_| WorkspaceError::Io)?;
        if a.escrow == 0 {
            if s.reserved < ESCROW {
                return Err(WorkspaceError::Io);
            }
            a.escrow = ESCROW;
            s.reserved -= ESCROW;
        }
        let reserve = s.reserved;
        s.reserved = 0;
        drop(a);
        drop(s);
        host.release(0, reserve)
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
