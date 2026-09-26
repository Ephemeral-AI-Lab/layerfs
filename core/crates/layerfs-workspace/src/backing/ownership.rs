//! Mutable, bounded disk ownership ledger; immutable data pages are never adopted.
use super::{
    directory::identity,
    metadata::*,
    metadata_pages::{self, PageData, PageRef, HEADER, PAGE},
    payload::{clock, OwnedPayload},
    segments::{self, Window},
};
use crate::*;
use std::{
    fs::{File, Metadata},
    io,
    sync::{atomic::Ordering, Arc},
    time::Instant,
};
const RECORDS: u32 = 62;
#[derive(Clone, Copy)]
pub struct CleanupFrame {
    pub page: PageRef,
    pub next: usize,
    pub phase: u8,
}
#[derive(Clone, Copy, Default)]
pub struct Owner {
    pub epoch: u32,
    pub role: u8,
    pub edges: bool,
    pub refs: u64,
    pub payload: u64,
    pub length: u64,
    pub device: u64,
    pub inode: u64,
    pub next: PageRef,
}
impl Owner {
    fn bytes(self) -> [u8; 64] {
        let mut b = [0; 64];
        b[..4].copy_from_slice(&self.epoch.to_be_bytes());
        b[4] = self.role;
        b[5] = u8::from(self.edges);
        for (at, n) in [
            (8, self.refs),
            (16, self.payload),
            (24, self.length),
            (32, self.device),
            (40, self.inode),
        ] {
            b[at..at + 8].copy_from_slice(&n.to_be_bytes());
        }
        b[48..56].copy_from_slice(&self.next.bytes());
        b
    }
    fn parse(b: &[u8]) -> Result<Self, WorkspaceError> {
        if b.len() != 64 || b[4] > 2 || b[5] > 1 || b[6..8].iter().chain(&b[56..]).any(|x| *x != 0)
        {
            return Err(WorkspaceError::Io);
        }
        Ok(Self {
            epoch: u32::from_be_bytes(b[..4].try_into().map_err(|_| WorkspaceError::Io)?),
            role: b[4],
            edges: b[5] == 1,
            refs: super::super::overlay::pieces::get(b, 8)?,
            payload: super::super::overlay::pieces::get(b, 16)?,
            length: super::super::overlay::pieces::get(b, 24)?,
            device: super::super::overlay::pieces::get(b, 32)?,
            inode: super::super::overlay::pieces::get(b, 40)?,
            next: PageRef::parse(&b[48..56])?,
        })
    }
}
fn ledger_name(index: u32) -> String {
    format!("m-ledger-{index:08x}")
}
pub(super) fn page_name(r: PageRef) -> String {
    format!("m-page-{:08x}-{:08x}", r.slot, r.epoch)
}
#[cfg(unix)]
fn physical(metadata: &Metadata) -> io::Result<u64> {
    use std::os::unix::fs::MetadataExt;
    metadata
        .blocks()
        .checked_mul(512)
        .ok_or_else(|| io::ErrorKind::InvalidData.into())
}
#[cfg(not(unix))]
fn physical(_: &Metadata) -> io::Result<u64> {
    Err(io::ErrorKind::Unsupported.into())
}
fn ledger_check(arena: &Arena, index: u32, b: &[u8]) -> Result<(), WorkspaceError> {
    metadata_pages::verify(b)?;
    if &b[..8] != b"LFSWOWN1"
        || b[8..40] != arena.directory.incarnation
        || b[40..44] != index.to_be_bytes()
        || b[44..96].iter().any(|x| *x != 0)
    {
        return Err(WorkspaceError::Io);
    }
    Ok(())
}
impl Arena {
    pub(super) fn host(&self) -> Result<Arc<MetadataHost>, WorkspaceError> {
        self.host.upgrade().ok_or(WorkspaceError::Closed)
    }
    pub(super) fn failure(&self, phase: BackingPhase, kind: io::ErrorKind) -> WorkspaceError {
        let Ok(host) = self.host() else {
            return WorkspaceError::Closed;
        };
        let (pages, files) = {
            let Ok(a) = self.state.lock() else {
                return WorkspaceError::Io;
            };
            (a.pages, a.pages + a.ledgers as usize)
        };
        let Ok(s) = host.payloads.state.lock() else {
            return WorkspaceError::Io;
        };
        WorkspaceError::Backing(BackingFailure {
            phase,
            payload: 0,
            declared_bytes: 0,
            completed_bytes: pages as u64 * PAGE as u64,
            created_segments: files as u32,
            allocated_bytes: s.metadata_allocated,
            reserved_bytes: s.metadata_reserved,
            cleanup_failed: phase == BackingPhase::Cleanup,
            accounting_complete: s.metadata_complete,
            kind,
        })
    }
    fn unknown_allocation(&self, extra: u64) -> Result<(), WorkspaceError> {
        let host = self.host()?;
        let mut arena = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if arena.unrecoverable && !arena.complete {
            return Ok(());
        }
        arena.blocked = true;
        arena.unrecoverable = true;
        arena.complete = false;
        let mut state = host.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        state.metadata_stopped = true;
        state.stopped = true;
        state.metadata_complete = false;
        state.complete = false;
        arena.allocated = arena
            .allocated
            .checked_add(extra)
            .ok_or(WorkspaceError::Io)?;
        state.metadata_allocated = state
            .metadata_allocated
            .checked_add(extra)
            .ok_or(WorkspaceError::Io)?;
        state.allocated = state
            .allocated
            .checked_add(extra)
            .ok_or(WorkspaceError::Io)?;
        Ok(())
    }
    fn read_error(&self, phase: BackingPhase, error: io::Error) -> WorkspaceError {
        if matches!(
            error.kind(),
            io::ErrorKind::NotFound | io::ErrorKind::UnexpectedEof
        ) {
            if let Err(failure) = self.unknown_allocation(0) {
                return failure;
            }
        }
        self.failure(phase, error.kind())
    }
    fn validate_file(
        &self,
        file: &File,
        expected: (u64, u64),
        phase: BackingPhase,
    ) -> Result<(), WorkspaceError> {
        let metadata = match file.metadata() {
            Ok(metadata) => metadata,
            Err(error) => {
                self.unknown_allocation(0)?;
                return Err(self.failure(phase, error.kind()));
            }
        };
        if identity(&metadata) != expected {
            return Err(WorkspaceError::Denied);
        }
        let allocated = match physical(&metadata) {
            Ok(allocated) => allocated,
            Err(error) => {
                self.unknown_allocation(0)?;
                return Err(self.failure(phase, error.kind()));
            }
        };
        if metadata.len() != PAGE as u64 || allocated != PAGE as u64 {
            // Do not refund a smaller observation or double-charge repeated failures.
            // A larger observed allocation must be retained even beyond the quota.
            self.unknown_allocation(allocated.saturating_sub(PAGE as u64))?;
            return Err(self.failure(phase, io::ErrorKind::InvalidData));
        }
        Ok(())
    }
    fn ledger_file(
        &self,
        index: u32,
        update: bool,
        phase: BackingPhase,
    ) -> Result<std::fs::File, WorkspaceError> {
        let expected = {
            *self
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .ledger_identities
                .get(index as usize)
                .ok_or(WorkspaceError::Io)?
        };
        let directory = self
            .directory
            .file()
            .map_err(|error| self.read_error(phase, error))?;
        let file = if update {
            segments::open_update(&directory, &ledger_name(index))
                .map_err(|error| self.read_error(phase, error))?
        } else {
            segments::open(&directory, &ledger_name(index))
                .map_err(|error| self.read_error(phase, error))?
        };
        self.validate_file(&file, expected, phase)?;
        Ok(file)
    }
    fn reserve_ledger_identity(&self, candidate: &RootOwner) -> Result<(), WorkspaceError> {
        let host = self.host()?;
        let capacity = {
            let s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            if s.ledger_identities.len() < s.ledger_identities.capacity() {
                return Ok(());
            }
            let capacity = s
                .ledger_identities
                .capacity()
                .saturating_mul(2)
                .clamp(1, 1058);
            if capacity <= s.ledger_identities.len() {
                return Err(WorkspaceError::Capacity);
            }
            capacity
        };
        let prepared = candidate
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .ledger_table
            .take();
        let mut table = match prepared {
            Some(mut table) => {
                table.identities.shrink_to(capacity);
                if table.identities.capacity() != capacity {
                    candidate
                        .state
                        .lock()
                        .map_err(|_| WorkspaceError::Io)?
                        .ledger_table = Some(table);
                    return Err(WorkspaceError::Capacity);
                }
                resize_memory(&mut table.charge, capacity * 16)?;
                table
            }
            None if candidate.allowance == 64 * 4096 => return Err(WorkspaceError::Capacity),
            None => LedgerTable {
                charge: host.memory(capacity * 16)?,
                identities: super::metadata_index::vector(capacity)?,
            },
        };
        let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        let mut charge = self.ledger_charge.lock().map_err(|_| WorkspaceError::Io)?;
        if table.identities.capacity() < s.ledger_identities.len() + 1 {
            return Err(WorkspaceError::Io);
        }
        table.identities.append(&mut s.ledger_identities);
        let old = std::mem::replace(&mut s.ledger_identities, table.identities);
        let old_charge = std::mem::replace(&mut *charge, table.charge);
        drop(charge);
        drop(s);
        drop(old);
        drop(old_charge);
        Ok(())
    }
    pub fn read_owner(
        &self,
        r: PageRef,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<Owner, WorkspaceError> {
        if self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .unrecoverable
        {
            return Err(WorkspaceError::Busy);
        }
        clock(deadline).map_err(|error| self.read_error(BackingPhase::Read, error))?;
        if r == PageRef::NULL {
            return Err(WorkspaceError::Io);
        }
        let index = (r.slot - 1) / RECORDS;
        let file = self.ledger_file(index, false, BackingPhase::Read)?;
        segments::read(&file, window, 0, PAGE)
            .map_err(|error| self.read_error(BackingPhase::Read, error))?;
        drop(file);
        ledger_check(self, index, &window.0[..PAGE])?;
        let at = HEADER + ((r.slot - 1) % RECORDS) as usize * 64;
        let record = Owner::parse(&window.0[at..at + 64])?;
        if record.epoch != r.epoch {
            return Err(WorkspaceError::Io);
        }
        Ok(record)
    }
    fn set_owner(
        &self,
        r: PageRef,
        record: Owner,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        clock(deadline).map_err(|error| self.failure(BackingPhase::Write, error.kind()))?;
        let index = (r.slot - 1) / RECORDS;
        let file = self.ledger_file(index, true, BackingPhase::Write)?;
        segments::read(&file, window, 0, PAGE)
            .map_err(|error| self.read_error(BackingPhase::Write, error))?;
        ledger_check(self, index, &window.0[..PAGE])?;
        let at = HEADER + ((r.slot - 1) % RECORDS) as usize * 64;
        window.0[at..at + 64].copy_from_slice(&record.bytes());
        metadata_pages::seal(&mut window.0[..PAGE]);
        let result = segments::write(&file, window, 0, PAGE);
        drop(file);
        if let Err(error) = result {
            self.host()?.quarantine(self, true);
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .unrecoverable = true;
            return Err(self.failure(BackingPhase::Write, error.kind()));
        }
        Ok(())
    }
    pub fn change_refs(
        &self,
        r: PageRef,
        delta: i64,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<u64, WorkspaceError> {
        let mut owner = self.read_owner(r, window, deadline)?;
        if owner.role == 0 {
            return Err(WorkspaceError::Io);
        }
        owner.refs = if delta >= 0 {
            owner.refs.checked_add(delta as u64)
        } else {
            owner.refs.checked_sub(delta.unsigned_abs())
        }
        .ok_or(WorkspaceError::Io)?;
        self.set_owner(r, owner, window, deadline)?;
        Ok(owner.refs)
    }
    fn allocate_slot(
        &self,
        candidate: &RootOwner,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageRef, WorkspaceError> {
        let host = self.host()?;
        let free = self.state.lock().map_err(|_| WorkspaceError::Io)?.free;
        if free != PageRef::NULL {
            let old = self.read_owner(free, window, deadline)?;
            if old.role != 0 || old.refs != 0 {
                return Err(WorkspaceError::Io);
            }
            let epoch = old.epoch.checked_add(1).ok_or(WorkspaceError::Capacity)?;
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.free = old.next;
            state.reusable -= 1;
            let r = PageRef {
                slot: free.slot,
                epoch,
            };
            candidate
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .slot_pending = Some(r);
            return Ok(r);
        }
        let slot = {
            let mut reserved = candidate.state.lock().map_err(|_| WorkspaceError::Io)?;
            let mut arena = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            let slot = arena.next;
            if slot > 65536 {
                return Err(WorkspaceError::Capacity);
            }
            if reserved.slot_credits > 0 {
                arena.reserved_slots = arena
                    .reserved_slots
                    .checked_sub(1)
                    .ok_or(WorkspaceError::Io)?;
                reserved.slot_credits -= 1;
            } else {
                if slot as usize + arena.reserved_slots > 65536 {
                    return Err(WorkspaceError::Capacity);
                }
                host.slots
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                        (count < 65536).then_some(count + 1)
                    })
                    .map_err(|_| WorkspaceError::Capacity)?;
            }
            arena.next = slot + 1;
            reserved.slot_pending = Some(PageRef { slot, epoch: 1 });
            slot
        };
        let index = (slot - 1) / RECORDS;
        let ledgers = self.state.lock().map_err(|_| WorkspaceError::Io)?.ledgers;
        if index == ledgers {
            self.reserve_ledger_identity(candidate)?;
            window.0[..PAGE].fill(0);
            window.0[..8].copy_from_slice(b"LFSWOWN1");
            window.0[8..40].copy_from_slice(&self.directory.incarnation);
            window.0[40..44].copy_from_slice(&index.to_be_bytes());
            metadata_pages::seal(&mut window.0[..PAGE]);
            let identity = candidate.create_file(&ledger_name(index), window, deadline)?;
            {
                let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                s.ledger_identities.push(identity);
                s.ledgers += 1;
            }
            candidate
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .pending = None;
        }
        Ok(PageRef { slot, epoch: 1 })
    }
    pub fn load(
        &self,
        r: PageRef,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageData, WorkspaceError> {
        clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let owner = self.read_owner(r, window, deadline)?;
        if owner.role != 1 {
            return Err(WorkspaceError::Io);
        }
        let file = segments::open(
            self.directory
                .file()
                .map_err(|error| self.read_error(BackingPhase::Read, error))?
                .as_ref(),
            &page_name(r),
        )
        .map_err(|error| self.read_error(BackingPhase::Read, error))?;
        self.validate_file(&file, (owner.device, owner.inode), BackingPhase::Read)?;
        segments::read(&file, window, 0, PAGE)
            .map_err(|error| self.read_error(BackingPhase::Read, error))?;
        drop(file);
        let result = PageData::decode(self.directory.incarnation, r, &window.0[..PAGE]);
        if result.is_err() {
            self.host()?.quarantine(self, true);
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .unrecoverable = true;
        }
        result
    }
    /// Reads one page's encoded body without decoding it, for the page kinds
    /// whose body is not a keyed cell list. Identity and address are checked
    /// exactly as `load` checks them.
    pub fn load_raw(
        &self,
        r: PageRef,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<[u8; metadata_pages::PAGE], WorkspaceError> {
        clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let owner = self.read_owner(r, window, deadline)?;
        if owner.role != 1 {
            return Err(WorkspaceError::Io);
        }
        let file = segments::open(
            self.directory
                .file()
                .map_err(|error| self.read_error(BackingPhase::Read, error))?
                .as_ref(),
            &page_name(r),
        )
        .map_err(|error| self.read_error(BackingPhase::Read, error))?;
        self.validate_file(&file, (owner.device, owner.inode), BackingPhase::Read)?;
        segments::read(&file, window, 0, metadata_pages::PAGE)
            .map_err(|error| self.read_error(BackingPhase::Read, error))?;
        drop(file);
        let mut bytes = [0; metadata_pages::PAGE];
        bytes.copy_from_slice(&window.0[..metadata_pages::PAGE]);
        // A body this reader cannot address is a corrupt page, exactly as a
        // failed decode is.
        if metadata_pages::verify(&bytes).is_err() {
            self.host()?.quarantine(self, true);
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .unrecoverable = true;
        }
        Ok(bytes)
    }
    pub fn custody(
        &self,
        candidate: &RootOwner,
        payload: &OwnedPayload,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageRef, WorkspaceError> {
        if payload.record.directory.incarnation != self.directory.incarnation {
            return Err(WorkspaceError::InvalidInput);
        }
        let custody = {
            payload
                .record
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .custody
        };
        if let Some((arena, r)) = custody {
            if arena != self.id {
                return Err(WorkspaceError::InvalidInput);
            }
            let owner = self.read_owner(r, window, deadline)?;
            if owner.role != 2
                || owner.payload != payload.record.id
                || owner.length != payload.len()
            {
                return Err(WorkspaceError::Io);
            }
            return Ok(r);
        }
        if !candidate
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .custodies
            .is_empty()
        {
            return Err(WorkspaceError::Capacity);
        }
        let r = self.allocate_slot(candidate, window, deadline)?;
        // A mutable ledger write may report failure after installing this record.
        // Keep payload ownership and the prospective slot before attempting it.
        payload
            .record
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .custody = Some((self.id, r));
        {
            let mut s = candidate.state.lock().map_err(|_| WorkspaceError::Io)?;
            s.custodies.push(r);
        }
        self.set_owner(
            r,
            Owner {
                epoch: r.epoch,
                role: 2,
                refs: 1,
                payload: payload.record.id,
                length: payload.len(),
                ..Owner::default()
            },
            window,
            deadline,
        )?;
        candidate
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .slot_pending = None;
        Ok(r)
    }
    pub fn payload(&self, id: u64, custody: PageRef) -> Result<OwnedPayload, WorkspaceError> {
        let host = self.host()?;
        let state = host.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
        let record = state
            .records
            .iter()
            .find(|r| r.id == id && r.directory.incarnation == self.directory.incarnation)
            .ok_or(WorkspaceError::Io)?;
        if record.state.lock().map_err(|_| WorkspaceError::Io)?.custody != Some((self.id, custody))
        {
            return Err(WorkspaceError::Io);
        }
        Ok(OwnedPayload {
            host: host.payloads.clone(),
            record: record.clone(),
        })
    }
    pub(super) fn free_slot(
        &self,
        r: PageRef,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<(), WorkspaceError> {
        let next = self.state.lock().map_err(|_| WorkspaceError::Io)?.free;
        self.set_owner(
            r,
            Owner {
                epoch: r.epoch,
                next,
                ..Owner::default()
            },
            window,
            deadline,
        )?;
        let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        s.free = r;
        s.reusable += 1;
        Ok(())
    }
    pub fn close(&self, window: &mut Window, deadline: Instant) -> Result<(), WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        if state.pages != 0 || state.reserved_slots != 0 {
            return Err(WorkspaceError::Busy);
        }
        drop(state);
        loop {
            clock(deadline).map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?;
            let count = self.state.lock().map_err(|_| WorkspaceError::Io)?.ledgers;
            if count == 0 {
                break;
            }
            let index = count - 1;
            let name = ledger_name(index);
            let file = self.ledger_file(index, false, BackingPhase::Cleanup)?;
            segments::read(&file, window, 0, PAGE)
                .map_err(|error| self.read_error(BackingPhase::Cleanup, error))?;
            ledger_check(self, index, &window.0[..PAGE])?;
            drop(file);
            segments::unlink(
                self.directory
                    .file()
                    .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?
                    .as_ref(),
                &name,
            )
            .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?;
            self.host()?.release(PAGE as u64, 0)?;
            let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            s.ledgers -= 1;
            s.ledger_identities.pop();
            s.allocated -= PAGE as u64;
        }
        let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        self.host()?
            .slots
            .fetch_sub((s.next - 1) as usize, Ordering::AcqRel);
        s.next = 1;
        s.free = PageRef::NULL;
        s.reusable = 0;
        Ok(())
    }
}
impl RootOwner {
    pub(super) fn failure(&self, phase: BackingPhase, kind: io::ErrorKind) -> WorkspaceError {
        let Ok(s) = self.state.lock() else {
            return WorkspaceError::Io;
        };
        let Ok(a) = self.arena.state.lock() else {
            return WorkspaceError::Io;
        };
        WorkspaceError::Backing(BackingFailure {
            phase,
            payload: 0,
            declared_bytes: self.allowance,
            completed_bytes: s.temporary.len() as u64 * PAGE as u64,
            created_segments: s.temporary.len() as u32
                + u32::from(s.pending.as_ref().is_some_and(|p| p.identity.is_some())),
            allocated_bytes: a.allocated,
            reserved_bytes: s.reserved,
            cleanup_failed: phase == BackingPhase::Cleanup,
            accounting_complete: a.complete,
            kind,
        })
    }
    fn create_file(
        &self,
        name: &str,
        window: &Window,
        deadline: Instant,
    ) -> Result<(u64, u64), WorkspaceError> {
        clock(deadline).map_err(|error| self.failure(BackingPhase::Create, error.kind()))?;
        let host = self.arena.host()?;
        {
            let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            if s.pending.is_some() || s.reserved < PAGE as u64 {
                return Err(WorkspaceError::Capacity);
            }
            s.pending = Some(Pending {
                name: name.into(),
                identity: None,
                allocated: None,
                reservation: PAGE as u64,
                collider: false,
            });
        }
        let mut result = (|| -> Result<(u64, u64), WorkspaceError> {
            let directory = self
                .arena
                .directory
                .file()
                .map_err(|error| self.failure(BackingPhase::Create, error.kind()))?;
            let file = match segments::create(&directory, name) {
                Ok(file) => file,
                Err(error) => {
                    if error.kind() == io::ErrorKind::AlreadyExists {
                        self.state
                            .lock()
                            .map_err(|_| WorkspaceError::Io)?
                            .pending
                            .as_mut()
                            .ok_or(WorkspaceError::Io)?
                            .collider = true;
                    }
                    return Err(self.failure(BackingPhase::Create, error.kind()));
                }
            };
            let identity = identity(
                &file
                    .metadata()
                    .map_err(|error| self.failure(BackingPhase::Create, error.kind()))?,
            );
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .pending
                .as_mut()
                .ok_or(WorkspaceError::Io)?
                .identity = Some(identity);
            let allocation = segments::allocate(&file, PAGE as u64);
            let observed = allocation
                .as_ref()
                .map(|n| *n)
                .or_else(|_| segments::allocated(&file));
            let amount = match observed {
                Ok(n) => n,
                Err(error) => {
                    host.quarantine(&self.arena, false);
                    let kind = allocation
                        .as_ref()
                        .err()
                        .map_or(error.kind(), io::Error::kind);
                    return Err(self.failure(BackingPhase::Allocate, kind));
                }
            };
            host.transfer(amount, PAGE as u64)?;
            {
                let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                s.reserved -= PAGE as u64;
                let pending = s.pending.as_mut().ok_or(WorkspaceError::Io)?;
                pending.reservation = 0;
                pending.allocated = Some(amount);
            }
            self.arena
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .allocated += amount;
            if let Err(error) = allocation {
                return Err(self.failure(BackingPhase::Allocate, error.kind()));
            }
            if amount != PAGE as u64 {
                return Err(self.failure(BackingPhase::Allocate, io::ErrorKind::Unsupported));
            }
            clock(deadline).map_err(|error| self.failure(BackingPhase::Create, error.kind()))?;
            segments::write(&file, window, 0, PAGE)
                .map_err(|error| self.failure(BackingPhase::Write, error.kind()))?;
            drop(file);
            clock(deadline).map_err(|error| self.failure(BackingPhase::Create, error.kind()))?;
            Ok(identity)
        })();
        if result.is_err() {
            let complete = self
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .pending
                .as_ref()
                .is_some_and(|p| p.identity.is_some() && p.allocated.is_some());
            host.quarantine(&self.arena, complete);
            if let Err(WorkspaceError::Backing(failure)) = &mut result {
                failure.accounting_complete &= complete;
            }
        }
        result
    }
    /// Writes one page whose body an encoder fills for the allocated identity.
    /// Ownership, references and accounting are the same as `write_page`; the
    /// raw form exists because a piece page is not a keyed-cell page.
    pub(crate) fn write_raw_page<S>(
        &self,
        window: &mut Window,
        deadline: Instant,
        encode: impl FnOnce(PageRef, &mut [u8]) -> Result<S, WorkspaceError>,
    ) -> Result<(PageRef, S), WorkspaceError> {
        if self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .temporary
            .len()
            == 128
        {
            return Err(WorkspaceError::Capacity);
        }
        let r = self.arena.allocate_slot(self, window, deadline)?;
        let state = encode(r, &mut window.0[..PAGE])?;
        // A page's edges are declared by the page itself, and the window is the
        // only place the encoded bytes exist: the ledger write below reuses the
        // same window page, so the edges must be taken before it.
        let edges = super::metadata_index::edges_raw(
            self.arena.directory.incarnation,
            r,
            &window.0[..PAGE],
        )?;
        let identity = self.create_file(&page_name(r), window, deadline)?;
        self.arena.set_owner(
            r,
            Owner {
                epoch: r.epoch,
                role: 1,
                refs: 1,
                device: identity.0,
                inode: identity.1,
                ..Owner::default()
            },
            window,
            deadline,
        )?;
        {
            let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            s.temporary.push(r);
            s.pending = None;
            s.slot_pending = None;
        }
        self.arena
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .pages += 1;
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .edge_progress = Some((r, 0));
        for (index, edge) in edges.into_iter().enumerate() {
            self.arena.change_refs(edge, 1, window, deadline)?;
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .edge_progress = Some((r, index + 1));
        }
        let mut owner = self.arena.read_owner(r, window, deadline)?;
        owner.edges = true;
        self.arena.set_owner(r, owner, window, deadline)?;
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .edge_progress = None;
        Ok((r, state))
    }
    pub fn write_page(
        &self,
        data: PageData,
        window: &mut Window,
        deadline: Instant,
    ) -> Result<PageRef, WorkspaceError> {
        if self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .temporary
            .len()
            == 128
        {
            return Err(WorkspaceError::Capacity);
        }
        let r = self.arena.allocate_slot(self, window, deadline)?;
        data.encode(self.arena.directory.incarnation, r, &mut window.0[..PAGE])?;
        let identity = self.create_file(&page_name(r), window, deadline)?;
        self.arena.set_owner(
            r,
            Owner {
                epoch: r.epoch,
                role: 1,
                refs: 1,
                device: identity.0,
                inode: identity.1,
                ..Owner::default()
            },
            window,
            deadline,
        )?;
        {
            let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            s.temporary.push(r);
            s.pending = None;
            s.slot_pending = None;
        }
        self.arena
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .pages += 1;
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .edge_progress = Some((r, 0));
        for (index, edge) in super::metadata_index::edges(&data)?.into_iter().enumerate() {
            self.arena.change_refs(edge, 1, window, deadline)?;
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .edge_progress = Some((r, index + 1));
        }
        let mut owner = self.arena.read_owner(r, window, deadline)?;
        owner.edges = true;
        self.arena.set_owner(r, owner, window, deadline)?;
        self.state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .edge_progress = None;
        Ok(r)
    }
}
