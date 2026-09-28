use super::page::{Kind, Page, PageRef, PAGE_BYTES};
use crate::{
    backing::{
        budget::Charge,
        directory::{identity, Directory},
        metadata::MetadataHost,
        segments,
    },
    BackingFailure, BackingPhase, WorkspaceError,
};
use std::{
    collections::BTreeMap,
    io,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

const ENTRY_BYTES: usize = 128;

#[derive(Clone, Copy)]
pub(super) enum Cause {
    Encode,
    CreateIdentity,
    Preallocate,
    DirectWrite,
    ReadbackIo,
    ReadbackAuth,
    Release,
    FitMerge,
    ActualMerge,
    NodeEncode,
    CacheDecode,
}

pub(super) struct Stamp<'a> {
    counter: &'a AtomicU64,
    began: Instant,
}

impl Drop for Stamp<'_> {
    fn drop(&mut self) {
        self.counter
            .fetch_add(self.began.elapsed().as_nanos() as u64, Ordering::Relaxed);
    }
}

struct Entry {
    identity: Option<(u64, u64)>,
    allocated: u64,
    reserved: u64,
    kind: Kind,
    ready: bool,
    pins: usize,
    unlinked: bool,
    birth: u64,
}

struct State {
    next: u64,
    entries: BTreeMap<PageRef, Entry>,
    entry_charge: Charge,
    allocated: u64,
    stopped: bool,
    complete: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct StoreStatus {
    pub pages: usize,
    pub pack_pages: usize,
    pub index_pages: usize,
    pub allocated_bytes: u64,
    pub reserved_bytes: u64,
    pub pinned_pages: usize,
    pub incomplete_pages: usize,
    pub admission_stopped: bool,
    pub accounting_complete: bool,
    pub pack_fetches: u64,
    pub index_fetches: u64,
    pub pack_page_writes: u64,
    pub index_page_writes: u64,
    pub page_encode_ns: u64,
    pub page_create_identity_ns: u64,
    pub page_preallocate_ns: u64,
    pub page_direct_write_ns: u64,
    pub page_readback_io_ns: u64,
    pub page_readback_auth_ns: u64,
    pub page_release_ns: u64,
    pub fit_merge_ns: u64,
    pub actual_merge_ns: u64,
    pub node_encode_ns: u64,
    pub cache_decode_ns: u64,
    pub directory_page_writes: u64,
    pub index_seeks: u64,
    pub index_node_visits: u64,
    pub index_cache_hits: u64,
    pub hot_writes: u64,
    pub hot_admissions: u64,
    pub hot_cursor_admissions: u64,
    pub hot_carries: u64,
    pub hot_normalizations: u64,
    /// Index page versions created in a subtree with no key update, solely
    /// to reconnect changed hot/cold targets. Includes staged attempts.
    pub representation_only_pages: u64,
    pub retirement_inspections: u64,
    pub minimum_leaf_split_bytes: Option<u64>,
    pub minimum_branch_split_bytes: Option<u64>,
}

#[derive(Clone, Copy)]
pub(super) enum Counter {
    DirectoryWrite,
    Seek,
    Visit,
    CacheHit,
    HotWrite,
    Admission,
    Carry,
    Normalization,
    RepresentationOnly,
    Retirement,
    CursorAdmission,
}

/// One page per verified private file permits exact block refunds. The host
/// quota is shared with legacy payload and metadata backing.
pub struct PageStore {
    directory: Arc<Directory>,
    host: Arc<MetadataHost>,
    state: Mutex<State>,
    pack_fetches: AtomicU64,
    index_fetches: AtomicU64,
    pack_page_writes: AtomicU64,
    index_page_writes: AtomicU64,
    counters: [AtomicU64; 11],
    cause_ns: [AtomicU64; 11],
    split_minimum: [AtomicU64; 2],
    _charge: Charge,
}

pub struct PagePin {
    store: Arc<PageStore>,
    reference: PageRef,
}

pub(super) struct PageReservation {
    store: Arc<PageStore>,
    remaining: usize,
}

impl Drop for PageReservation {
    fn drop(&mut self) {
        if self
            .store
            .host
            .release(0, (self.remaining * PAGE_BYTES) as u64)
            .is_err()
        {
            self.store.stop();
        }
    }
}

impl Drop for PagePin {
    fn drop(&mut self) {
        if let Ok(mut state) = self.store.state.lock() {
            if let Some(entry) = state.entries.get_mut(&self.reference) {
                entry.pins -= 1;
            }
        }
    }
}

fn name(kind: Kind, reference: PageRef) -> String {
    let prefix = match kind {
        Kind::Pack => "a-pack-v1",
        Kind::IndexLeaf | Kind::IndexBranch => "a-index-v2",
        Kind::HotDirectory => "a-hot-v2",
    };
    format!("{prefix}-{:016x}-{:016x}", reference.id, reference.epoch)
}

fn failure(
    phase: BackingPhase,
    reference: PageRef,
    allocated: u64,
    complete: bool,
    error: &io::Error,
) -> WorkspaceError {
    WorkspaceError::Backing(BackingFailure {
        phase,
        payload: reference.id,
        declared_bytes: PAGE_BYTES as u64,
        completed_bytes: 0,
        created_segments: u32::from(allocated > 0),
        allocated_bytes: allocated,
        reserved_bytes: 0,
        cleanup_failed: phase == BackingPhase::Cleanup,
        accounting_complete: complete,
        kind: error.kind(),
    })
}

impl PageStore {
    pub(super) fn reserve_pages(
        self: &Arc<Self>,
        pages: usize,
    ) -> Result<PageReservation, WorkspaceError> {
        let bytes = pages
            .checked_mul(PAGE_BYTES)
            .ok_or(WorkspaceError::Capacity)?;
        self.host.reserve(bytes as u64)?;
        Ok(PageReservation {
            store: self.clone(),
            remaining: pages,
        })
    }
    pub fn new(
        directory: Arc<Directory>,
        host: Arc<MetadataHost>,
    ) -> Result<Arc<Self>, WorkspaceError> {
        if directory.incarnation == [0; 32]
            || directory.path.parent() != Some(&*host.payloads.common.path)
        {
            return Err(WorkspaceError::InvalidInput);
        }
        let budget = &host.payloads.budget;
        Ok(Arc::new(Self {
            directory,
            host: host.clone(),
            state: Mutex::new(State {
                next: 1,
                entries: BTreeMap::new(),
                entry_charge: budget.reserve(0)?,
                allocated: 0,
                stopped: false,
                complete: true,
            }),
            pack_fetches: AtomicU64::new(0),
            index_fetches: AtomicU64::new(0),
            pack_page_writes: AtomicU64::new(0),
            index_page_writes: AtomicU64::new(0),
            counters: std::array::from_fn(|_| AtomicU64::new(0)),
            cause_ns: std::array::from_fn(|_| AtomicU64::new(0)),
            split_minimum: std::array::from_fn(|_| AtomicU64::new(u64::MAX)),
            _charge: budget.reserve(1024)?,
        }))
    }

    pub fn incarnation(&self) -> [u8; 32] {
        self.directory.incarnation
    }

    pub fn budget(&self) -> Arc<crate::backing::budget::Budget> {
        self.host.payloads.budget.clone()
    }

    pub(super) fn count(&self, counter: Counter, count: u64) {
        self.counters[counter as usize].fetch_add(count, Ordering::Relaxed);
    }

    pub(super) fn split(&self, level: u8, minimum: usize) {
        self.split_minimum[usize::from(level > 0)].fetch_min(minimum as u64, Ordering::Relaxed);
    }

    /// Physical birth is allocation custody, unchanged by Hot/Cold conversion.
    pub(super) fn birth(&self, reference: PageRef) -> Result<u64, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let entry = state
            .entries
            .get(&reference)
            .ok_or(WorkspaceError::NotFound)?;
        if !entry.ready {
            return Err(WorkspaceError::Io);
        }
        Ok(entry.birth)
    }

    pub(super) fn remaining_quota(&self) -> Result<u64, WorkspaceError> {
        let status = self.host.payloads.status()?;
        status
            .quota_bytes
            .checked_sub(status.allocated_bytes)
            .and_then(|left| left.checked_sub(status.reserved_bytes))
            .ok_or(WorkspaceError::Io)
    }

    pub fn kind(&self, reference: PageRef) -> Result<Kind, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let entry = state
            .entries
            .get(&reference)
            .ok_or(WorkspaceError::NotFound)?;
        if !entry.ready {
            return Err(WorkspaceError::Io);
        }
        Ok(entry.kind)
    }

    pub(super) fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.stopped = true;
        }
    }

    pub fn create(
        &self,
        kind: Kind,
        generation: u64,
        revision: u64,
        records: u16,
        body: &[u8],
    ) -> Result<PageRef, WorkspaceError> {
        Ok(self
            .create_verified(kind, generation, revision, records, body)?
            .0)
    }

    /// Creation returns the authenticated page it wrote and verified in place,
    /// so a selector can decode exactly the bytes a later reader would see.
    pub(super) fn create_verified(
        &self,
        kind: Kind,
        generation: u64,
        revision: u64,
        records: u16,
        body: &[u8],
    ) -> Result<(PageRef, Page), WorkspaceError> {
        self.create_from(kind, generation, revision, records, body, None)
    }

    pub(super) fn stamp(&self, phase: Cause) -> Stamp<'_> {
        Stamp {
            counter: &self.cause_ns[phase as usize],
            began: Instant::now(),
        }
    }

    pub(super) fn create_from(
        &self,
        kind: Kind,
        generation: u64,
        revision: u64,
        records: u16,
        body: &[u8],
        reservation: Option<&mut PageReservation>,
    ) -> Result<(PageRef, Page), WorkspaceError> {
        let _scratch = self
            .host
            .payloads
            .budget
            .reserve(PAGE_BYTES + ENTRY_BYTES)?;
        let reference = {
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            if state.stopped {
                return Err(WorkspaceError::Busy);
            }
            let id = state.next;
            state.next = state.next.checked_add(1).ok_or(WorkspaceError::Capacity)?;
            PageRef { id, epoch: 1 }
        };
        let encode = self.stamp(Cause::Encode);
        let page = Page::new(
            kind,
            self.directory.incarnation,
            reference,
            generation,
            revision,
            records,
            body,
        )?;
        drop(encode);
        let directory = self.directory.file()?;
        if let Some(reservation) = reservation {
            if !std::ptr::eq(self, Arc::as_ptr(&reservation.store)) || reservation.remaining == 0 {
                return Err(WorkspaceError::Capacity);
            }
            reservation.remaining -= 1;
        } else {
            self.host.reserve(PAGE_BYTES as u64)?;
        }
        {
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            let Some(bytes) = state
                .entries
                .len()
                .checked_add(1)
                .and_then(|entries| entries.checked_mul(ENTRY_BYTES))
            else {
                self.host.release(0, PAGE_BYTES as u64)?;
                return Err(WorkspaceError::Capacity);
            };
            if let Err(error) = state.entry_charge.resize(bytes) {
                self.host.release(0, PAGE_BYTES as u64)?;
                return Err(error);
            }
            state.entries.insert(
                reference,
                Entry {
                    identity: None,
                    allocated: 0,
                    reserved: PAGE_BYTES as u64,
                    kind,
                    ready: false,
                    pins: 0,
                    unlinked: false,
                    birth: revision,
                },
            );
        }
        let filename = name(kind, reference);
        let identity_time = self.stamp(Cause::CreateIdentity);
        let file = match segments::create(&directory, &filename) {
            Ok(file) => file,
            Err(error) => {
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                state.entries.remove(&reference);
                if error.kind() == io::ErrorKind::AlreadyExists {
                    state.stopped = true;
                }
                let bytes = state.entries.len() * ENTRY_BYTES;
                state.entry_charge.resize(bytes)?;
                drop(state);
                self.host.release(0, PAGE_BYTES as u64)?;
                return Err(failure(BackingPhase::Create, reference, 0, true, &error));
            }
        };
        let file_identity = match file.metadata() {
            Ok(metadata) => identity(&metadata),
            Err(error) => {
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                state.stopped = true;
                state.complete = false;
                return Err(failure(BackingPhase::Create, reference, 0, false, &error));
            }
        };
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .entries
            .get_mut(&reference)
            .ok_or(WorkspaceError::Io)?
            .identity = Some(file_identity);
        drop(identity_time);
        let preallocation_time = self.stamp(Cause::Preallocate);
        let allocation = segments::allocate(&file, PAGE_BYTES as u64);
        let observed = segments::allocated(&file);
        let allocated = match observed {
            Ok(bytes) => bytes,
            Err(error) => {
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                state.stopped = true;
                state.complete = false;
                return Err(failure(BackingPhase::Allocate, reference, 0, false, &error));
            }
        };
        if let Err(error) = self.host.transfer(allocated, PAGE_BYTES as u64) {
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.stopped = true;
            state.complete = false;
            return Err(error);
        }
        {
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.allocated = state
                .allocated
                .checked_add(allocated)
                .ok_or(WorkspaceError::Io)?;
            let entry = state
                .entries
                .get_mut(&reference)
                .ok_or(WorkspaceError::Io)?;
            entry.allocated = allocated;
            entry.reserved = 0;
            if allocated > PAGE_BYTES as u64 {
                state.stopped = true;
            }
        }
        if let Err(error) = allocation {
            self.stop();
            return Err(failure(
                BackingPhase::Allocate,
                reference,
                allocated,
                true,
                &error,
            ));
        }
        if allocated != PAGE_BYTES as u64 {
            self.stop();
            return Err(failure(
                BackingPhase::Allocate,
                reference,
                allocated,
                true,
                &io::Error::other("active page allocation size mismatch"),
            ));
        }
        drop(preallocation_time);
        let mut lease = self.host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        window.0[..PAGE_BYTES].copy_from_slice(&page.bytes);
        let write_time = self.stamp(Cause::DirectWrite);
        if let Err(error) = segments::write(&file, window, 0, PAGE_BYTES) {
            self.stop();
            return Err(failure(
                BackingPhase::Write,
                reference,
                allocated,
                true,
                &error,
            ));
        }
        drop(write_time);
        window.0[..PAGE_BYTES].fill(0);
        let readback_time = self.stamp(Cause::ReadbackIo);
        if let Err(error) = segments::read(&file, window, 0, PAGE_BYTES) {
            self.stop();
            return Err(failure(
                BackingPhase::Verify,
                reference,
                allocated,
                true,
                &error,
            ));
        }
        drop(readback_time);
        let authenticate_time = self.stamp(Cause::ReadbackAuth);
        let mut verified = Page {
            bytes: [0; PAGE_BYTES],
        };
        verified.bytes.copy_from_slice(&window.0[..PAGE_BYTES]);
        if verified
            .verify(kind, self.directory.incarnation, reference)
            .is_err()
        {
            self.stop();
            return Err(failure(
                BackingPhase::Verify,
                reference,
                allocated,
                true,
                &io::Error::new(io::ErrorKind::InvalidData, "active page readback mismatch"),
            ));
        }
        if verified.bytes != page.bytes {
            self.stop();
            return Err(failure(
                BackingPhase::Verify,
                reference,
                allocated,
                true,
                &io::Error::new(io::ErrorKind::InvalidData, "active page readback changed"),
            ));
        }
        drop(authenticate_time);
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .entries
            .get_mut(&reference)
            .ok_or(WorkspaceError::Io)?
            .ready = true;
        match kind {
            Kind::Pack => &self.pack_page_writes,
            Kind::IndexLeaf | Kind::IndexBranch | Kind::HotDirectory => &self.index_page_writes,
        }
        .fetch_add(1, Ordering::Relaxed);
        if kind == Kind::HotDirectory {
            self.count(Counter::DirectoryWrite, 1);
        }
        Ok((reference, page))
    }

    pub fn read(&self, reference: PageRef, kind: Kind) -> Result<Page, WorkspaceError> {
        let (expected, charged) = {
            let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            let entry = state
                .entries
                .get(&reference)
                .ok_or(WorkspaceError::NotFound)?;
            if !entry.ready || entry.kind != kind {
                return Err(WorkspaceError::Io);
            }
            (entry.identity.ok_or(WorkspaceError::Io)?, entry.allocated)
        };
        let directory = self.directory.file()?;
        let file = segments::open(&directory, &name(kind, reference))?;
        if identity(&file.metadata()?) != expected || segments::allocated(&file)? != charged {
            return Err(WorkspaceError::Io);
        }
        let mut lease = self.host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        segments::read(&file, window, 0, PAGE_BYTES)?;
        let mut page = Page {
            bytes: [0; PAGE_BYTES],
        };
        page.bytes.copy_from_slice(&window.0[..PAGE_BYTES]);
        page.verify(kind, self.directory.incarnation, reference)?;
        match kind {
            Kind::Pack => &self.pack_fetches,
            Kind::IndexLeaf | Kind::IndexBranch | Kind::HotDirectory => &self.index_fetches,
        }
        .fetch_add(1, Ordering::Relaxed);
        Ok(page)
    }

    pub fn pin(self: &Arc<Self>, reference: PageRef) -> Result<PagePin, WorkspaceError> {
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let entry = state
            .entries
            .get_mut(&reference)
            .ok_or(WorkspaceError::NotFound)?;
        if !entry.ready {
            return Err(WorkspaceError::Io);
        }
        entry.pins = entry.pins.checked_add(1).ok_or(WorkspaceError::Capacity)?;
        Ok(PagePin {
            store: self.clone(),
            reference,
        })
    }

    pub fn release(&self, reference: PageRef) -> Result<u64, WorkspaceError> {
        let _release_time = self.stamp(Cause::Release);
        let (kind, expected, charged, reserved, unlinked) = {
            let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            let entry = state
                .entries
                .get(&reference)
                .ok_or(WorkspaceError::NotFound)?;
            if entry.pins > 0 {
                return Err(WorkspaceError::Busy);
            }
            (
                entry.kind,
                entry.identity,
                entry.allocated,
                entry.reserved,
                entry.unlinked,
            )
        };
        let mut charged = charged;
        if !unlinked {
            let expected = expected.ok_or(WorkspaceError::Busy)?;
            let directory = self.directory.file()?;
            let filename = name(kind, reference);
            let file = segments::open(&directory, &filename)?;
            let metadata = file.metadata()?;
            let actual = segments::allocated(&file)?;
            if identity(&metadata) != expected || (reserved == 0 && actual != charged) {
                return Err(WorkspaceError::Io);
            }
            if reserved > 0 {
                self.host.transfer(actual, reserved)?;
                let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                let entry = state
                    .entries
                    .get_mut(&reference)
                    .ok_or(WorkspaceError::Io)?;
                entry.allocated = actual;
                entry.reserved = 0;
                state.allocated = state
                    .allocated
                    .checked_add(actual)
                    .ok_or(WorkspaceError::Io)?;
                charged = actual;
            }
            drop(file);
            segments::unlink(&directory, &filename).map_err(|error| {
                failure(BackingPhase::Cleanup, reference, charged, true, &error)
            })?;
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .entries
                .get_mut(&reference)
                .ok_or(WorkspaceError::Io)?
                .unlinked = true;
        }
        self.host.release(charged, 0)?;
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        state.allocated = state
            .allocated
            .checked_sub(charged)
            .ok_or(WorkspaceError::Io)?;
        state.entries.remove(&reference).ok_or(WorkspaceError::Io)?;
        let bytes = state.entries.len() * ENTRY_BYTES;
        state.entry_charge.resize(bytes)?;
        if state.entries.is_empty() {
            state.complete = true;
        }
        Ok(charged)
    }

    pub(crate) fn source_counts(&self) -> (u64, u64) {
        (
            self.index_fetches.load(Ordering::Relaxed),
            self.counters[Counter::Seek as usize].load(Ordering::Relaxed),
        )
    }

    pub fn status(&self) -> Result<StoreStatus, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let minimum = |at: usize| {
            let value = self.split_minimum[at].load(Ordering::Relaxed);
            (value != u64::MAX).then_some(value)
        };
        Ok(StoreStatus {
            pages: state.entries.len(),
            pack_pages: state
                .entries
                .values()
                .filter(|entry| entry.kind == Kind::Pack)
                .count(),
            index_pages: state
                .entries
                .values()
                .filter(|entry| entry.kind != Kind::Pack)
                .count(),
            allocated_bytes: state.allocated,
            reserved_bytes: state.entries.values().map(|entry| entry.reserved).sum(),
            pinned_pages: state
                .entries
                .values()
                .filter(|entry| entry.pins > 0)
                .count(),
            incomplete_pages: state.entries.values().filter(|entry| !entry.ready).count(),
            admission_stopped: state.stopped,
            accounting_complete: state.complete,
            pack_fetches: self.pack_fetches.load(Ordering::Relaxed),
            index_fetches: self.index_fetches.load(Ordering::Relaxed),
            pack_page_writes: self.pack_page_writes.load(Ordering::Relaxed),
            index_page_writes: self.index_page_writes.load(Ordering::Relaxed),
            page_encode_ns: self.cause_ns[Cause::Encode as usize].load(Ordering::Relaxed),
            page_create_identity_ns: self.cause_ns[Cause::CreateIdentity as usize]
                .load(Ordering::Relaxed),
            page_preallocate_ns: self.cause_ns[Cause::Preallocate as usize].load(Ordering::Relaxed),
            page_direct_write_ns: self.cause_ns[Cause::DirectWrite as usize]
                .load(Ordering::Relaxed),
            page_readback_io_ns: self.cause_ns[Cause::ReadbackIo as usize].load(Ordering::Relaxed),
            page_readback_auth_ns: self.cause_ns[Cause::ReadbackAuth as usize]
                .load(Ordering::Relaxed),
            page_release_ns: self.cause_ns[Cause::Release as usize].load(Ordering::Relaxed),
            fit_merge_ns: self.cause_ns[Cause::FitMerge as usize].load(Ordering::Relaxed),
            actual_merge_ns: self.cause_ns[Cause::ActualMerge as usize].load(Ordering::Relaxed),
            node_encode_ns: self.cause_ns[Cause::NodeEncode as usize].load(Ordering::Relaxed),
            cache_decode_ns: self.cause_ns[Cause::CacheDecode as usize].load(Ordering::Relaxed),
            directory_page_writes: self.counters[Counter::DirectoryWrite as usize]
                .load(Ordering::Relaxed),
            index_seeks: self.counters[Counter::Seek as usize].load(Ordering::Relaxed),
            index_node_visits: self.counters[Counter::Visit as usize].load(Ordering::Relaxed),
            index_cache_hits: self.counters[Counter::CacheHit as usize].load(Ordering::Relaxed),
            hot_writes: self.counters[Counter::HotWrite as usize].load(Ordering::Relaxed),
            hot_admissions: self.counters[Counter::Admission as usize].load(Ordering::Relaxed),
            hot_cursor_admissions: self.counters[Counter::CursorAdmission as usize]
                .load(Ordering::Relaxed),
            hot_carries: self.counters[Counter::Carry as usize].load(Ordering::Relaxed),
            hot_normalizations: self.counters[Counter::Normalization as usize]
                .load(Ordering::Relaxed),
            representation_only_pages: self.counters[Counter::RepresentationOnly as usize]
                .load(Ordering::Relaxed),
            retirement_inspections: self.counters[Counter::Retirement as usize]
                .load(Ordering::Relaxed),
            minimum_leaf_split_bytes: minimum(0),
            minimum_branch_split_bytes: minimum(1),
        })
    }

    pub fn close(&self) -> Result<(), WorkspaceError> {
        loop {
            let (next, complete) = {
                let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                (state.entries.keys().next().copied(), state.complete)
            };
            let Some(reference) = next else {
                return if complete {
                    Ok(())
                } else {
                    Err(WorkspaceError::Io)
                };
            };
            self.release(reference)?;
        }
    }
}
