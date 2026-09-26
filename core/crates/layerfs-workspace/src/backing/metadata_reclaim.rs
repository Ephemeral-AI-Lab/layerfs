//! Explicit metadata reclamation with retained progress across known failures.
use super::{
    directory::identity,
    metadata::{Arena, MetadataHost, RootOwner},
    metadata_pages::{PageRef, PAGE},
    ownership::{page_name, CleanupFrame},
    payload::clock,
    segments::{self, Window},
};
use crate::*;
use std::{
    fs, io,
    sync::{atomic::Ordering, Arc},
    time::{Duration, Instant},
};
impl RootOwner {
    pub(super) fn cleanup_step(
        &self,
        frame: CleanupFrame,
        window: &mut Window,
        deadline: Instant,
        report: &mut MetadataCleanupReport,
    ) -> Result<(), WorkspaceError> {
        clock(deadline).map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?;
        let arena = &self.arena;
        let owner = arena.read_owner(frame.page, window, deadline)?;
        if owner.refs != 0 || owner.role == 0 {
            return Err(WorkspaceError::Io);
        }
        if frame.phase == 0 {
            if owner.role == 2 {
                let host = arena.host()?;
                let payload = host
                    .payloads
                    .registered(owner.payload)?
                    .ok_or(WorkspaceError::Io)?;
                let mut record = payload.state.lock().map_err(|_| WorkspaceError::Io)?;
                if record.custody != Some((arena.id, frame.page)) {
                    return Err(WorkspaceError::Io);
                }
                record.custody = None;
                drop(record);
                // The custody page was this payload's last live reference. When
                // no operator still holds it, routine reclamation may now take
                // it: name it instead of leaving it for a later registry walk.
                if Arc::strong_count(&payload) == 2 {
                    host.payloads.note_released(payload.id);
                }
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .cleanup
                    .last_mut()
                    .ok_or(WorkspaceError::Io)?
                    .phase = 2;
                report.payload_custodies_released += 1;
                return Ok(());
            }
            let progress = {
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .edge_progress
            };
            let partial = progress
                .filter(|(page, _)| *page == frame.page)
                .map(|(_, count)| count);
            if owner.edges || partial.is_some() {
                // A page under cleanup may be keyed cells or one file's extent
                // sequence: both declare their own edges, so the raw body and
                // the kind-aware extraction decide them. The cell decoder would
                // refuse an extent page and quarantine a healthy arena.
                let bytes = arena.load_raw(frame.page, window, deadline)?;
                let edges = super::metadata_index::edges_raw(
                    arena.directory.incarnation,
                    frame.page,
                    &bytes,
                )?;
                let count = partial.unwrap_or(edges.len());
                if count > edges.len() {
                    return Err(WorkspaceError::Io);
                }
                if frame.next < count {
                    let r = edges[frame.next];
                    let count = arena.change_refs(r, -1, window, deadline)?;
                    let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                    s.cleanup.last_mut().ok_or(WorkspaceError::Io)?.next += 1;
                    if count == 0 {
                        if s.cleanup.len() == 12 {
                            return Err(WorkspaceError::Capacity);
                        }
                        s.cleanup.push(CleanupFrame {
                            page: r,
                            next: 0,
                            phase: 0,
                        });
                    }
                    return Ok(());
                }
            }
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .cleanup
                .last_mut()
                .ok_or(WorkspaceError::Io)?
                .phase = 1;
            return Ok(());
        }
        if frame.phase == 1 {
            let name = page_name(frame.page);
            match segments::open(
                arena
                    .directory
                    .file()
                    .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?
                    .as_ref(),
                &name,
            ) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(self.failure(BackingPhase::Cleanup, error.kind())),
                Ok(file) => {
                    if identity(
                        &file
                            .metadata()
                            .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?,
                    ) != (owner.device, owner.inode)
                    {
                        return Err(WorkspaceError::Io);
                    }
                    drop(file);
                    segments::unlink(
                        arena
                            .directory
                            .file()
                            .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?
                            .as_ref(),
                        &name,
                    )
                    .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?;
                }
            }
            self.release_bytes(PAGE as u64, 0)?;
            {
                let mut a = arena.state.lock().map_err(|_| WorkspaceError::Io)?;
                a.pages -= 1;
                a.allocated -= PAGE as u64;
            }
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .cleanup
                .last_mut()
                .ok_or(WorkspaceError::Io)?
                .phase = 2;
            report.pages_reclaimed += 1;
            return Ok(());
        }
        arena.free_slot(frame.page, window, deadline)?;
        let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        state.cleanup.pop();
        if state
            .edge_progress
            .is_some_and(|(page, _)| page == frame.page)
        {
            state.edge_progress = None;
        }
        Ok(())
    }
    pub fn reclaim(
        &self,
        window: &mut Window,
        deadline: Instant,
        report: &mut MetadataCleanupReport,
    ) -> Result<(), WorkspaceError> {
        let result = self.reclaim_owned(window, deadline, report);
        match self.state.lock() {
            Ok(mut state) => state.cleanup_failed = result.is_err(),
            Err(_) if result.is_ok() => return Err(WorkspaceError::Io),
            Err(_) => {}
        }
        result
    }
    /// Reclaims only this attempt's known unfinished page before a caller
    /// retries local reconciliation. Its completed temporary pages and quota
    /// reservation remain owned by the attempt for the eventual seal.
    pub(crate) fn repair_pending_for_retry(&self, deadline: Instant) -> Result<(), WorkspaceError> {
        let host = self.arena.host()?;
        let _writer = host.writer_until(deadline)?;
        if !self
            .arena
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .complete
        {
            return Err(WorkspaceError::Busy);
        }
        self.cleanup_pending(deadline, true)?;
        host.refresh()
    }
    /// Cleans a known pending page without releasing the candidate root.
    /// A retry keeps its reserved slot; full reclaim gives the slot back.
    fn cleanup_pending(&self, deadline: Instant, resume: bool) -> Result<(), WorkspaceError> {
        if self
            .arena
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .unrecoverable
        {
            return Err(WorkspaceError::Busy);
        }
        let pending = self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .pending
            .take();
        if let Some(pending) = pending {
            let result = (|| -> Result<(), WorkspaceError> {
                clock(deadline)
                    .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?;
                let name = &pending.name;
                match fs::symlink_metadata(self.arena.directory.path.join(name)) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(self.failure(BackingPhase::Cleanup, e.kind())),
                    Ok(_) => {
                        let file = segments::open(
                            self.arena
                                .directory
                                .file()
                                .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?
                                .as_ref(),
                            name,
                        )
                        .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?;
                        if pending.collider
                            || pending.identity
                                != Some(identity(&file.metadata().map_err(|error| {
                                    self.failure(BackingPhase::Cleanup, error.kind())
                                })?))
                        {
                            return Err(WorkspaceError::Denied);
                        }
                        drop(file);
                        segments::unlink(
                            self.arena
                                .directory
                                .file()
                                .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?
                                .as_ref(),
                            name,
                        )
                        .map_err(|error| self.failure(BackingPhase::Cleanup, error.kind()))?;
                    }
                }
                self.release_bytes(pending.allocated.unwrap_or(0), pending.reservation)?;
                let mut a = self.arena.state.lock().map_err(|_| WorkspaceError::Io)?;
                a.allocated -= pending.allocated.unwrap_or(0);
                drop(a);
                self.state.lock().map_err(|_| WorkspaceError::Io)?.reserved -= pending.reservation;
                Ok(())
            })();
            if let Err(error) = result {
                self.state.lock().map_err(|_| WorkspaceError::Io)?.pending = Some(pending);
                return Err(error);
            }
        }
        let slot = {
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .slot_pending
        };
        if let Some(r) = slot {
            let prospective = {
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .custodies
                    .last()
                    .map(|(page, _)| *page)
                    == Some(r)
            };
            if prospective {
                // No uncertain write is eligible for this path. The owned marker
                // is released only after explicit cleanup observes that condition.
                let host = self.arena.host()?;
                let payload = self
                    .state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .custodies
                    .last()
                    .map(|(_, payload)| *payload)
                    .ok_or(WorkspaceError::Io)?;
                let record = host
                    .payloads
                    .registered(payload)?
                    .ok_or(WorkspaceError::Io)?;
                if record.state.lock().map_err(|_| WorkspaceError::Io)?.custody
                    != Some((self.arena.id, r))
                {
                    return Err(WorkspaceError::Io);
                }
                record.state.lock().map_err(|_| WorkspaceError::Io)?.custody = None;
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .custodies
                    .pop();
                if Arc::strong_count(&record) == 2 {
                    host.payloads.note_released(record.id);
                }
            }
            let mut owner = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            let mut a = self.arena.state.lock().map_err(|_| WorkspaceError::Io)?;
            if r.epoch == 1 {
                if a.next != r.slot + 1 {
                    return Err(WorkspaceError::Io);
                }
                a.next -= 1;
                if resume {
                    // The slot claim stays reserved for the same attempt.
                    a.reserved_slots = a.reserved_slots.checked_add(1).ok_or(WorkspaceError::Io)?;
                    owner.slot_credits = owner
                        .slot_credits
                        .checked_add(1)
                        .ok_or(WorkspaceError::Io)?;
                } else {
                    self.arena.host()?.slots.fetch_sub(1, Ordering::AcqRel);
                }
            } else {
                a.free = PageRef {
                    slot: r.slot,
                    epoch: r.epoch - 1,
                };
                a.reusable += 1;
            }
            owner.slot_pending = None;
        }
        Ok(())
    }
    fn reclaim_owned(
        &self,
        window: &mut Window,
        deadline: Instant,
        report: &mut MetadataCleanupReport,
    ) -> Result<(), WorkspaceError> {
        self.cleanup_pending(deadline, false)?;
        loop {
            let frame = {
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .cleanup
                    .last()
                    .copied()
            };
            if let Some(frame) = frame {
                self.cleanup_step(frame, window, deadline, report)?;
                continue;
            }
            let (r, kind) = {
                let s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                if let Some(r) = s.temporary.last() {
                    (*r, 0)
                } else if let Some(r) = s.custodies.last() {
                    (r.0, 1)
                } else {
                    (s.root, 2)
                }
            };
            if r == PageRef::NULL {
                break;
            }
            let count = self.arena.change_refs(r, -1, window, deadline)?;
            let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            if count == 0 {
                s.cleanup.push(CleanupFrame {
                    page: r,
                    next: 0,
                    phase: 0,
                });
            }
            match kind {
                0 => {
                    s.temporary.pop();
                }
                1 => {
                    s.custodies.pop();
                }
                _ => {
                    s.root = PageRef::NULL;
                }
            }
        }
        self.release_slot_credits()?;
        let reserve = {
            let mut s = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            let reserve = s.reserved;
            s.reserved = 0;
            reserve
        };
        self.release_bytes(0, reserve)?;
        Ok(())
    }
}

impl RootOwner {
    fn routine_eligible(&self) -> Result<bool, WorkspaceError> {
        {
            let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            if state.cleanup_failed
                || state.pending.is_some()
                || state.slot_pending.is_some()
                || state.edge_progress.is_some()
                || !state.temporary.is_empty()
                || !state.custodies.is_empty()
                || !state.cleanup.is_empty()
            {
                return Ok(false);
            }
        }
        let arena = self.arena.state.lock().map_err(|_| WorkspaceError::Io)?;
        Ok(arena.complete && !arena.blocked && !arena.unrecoverable)
    }
}
impl MetadataHost {
    pub fn maintain(self: &Arc<Self>, deadline: Instant) -> Result<(), WorkspaceError> {
        self.reclaim_registered(None, deadline).map(|_| ())
    }
    pub fn reclaim(
        self: &Arc<Self>,
        incarnation: [u8; 32],
        deadline: Instant,
    ) -> Result<MetadataCleanupReport, WorkspaceError> {
        self.reclaim_registered(Some(incarnation), deadline)
    }
    fn select_root(
        &self,
        incarnation: Option<[u8; 32]>,
    ) -> Result<Option<Arc<RootOwner>>, WorkspaceError> {
        let roots = self.roots.lock().map_err(|_| WorkspaceError::Io)?;
        for root in roots.iter() {
            if Arc::strong_count(root) != 1
                || incarnation.is_some_and(|id| root.arena.directory.incarnation != id)
            {
                continue;
            }
            if incarnation.is_none() && !root.routine_eligible()? {
                continue;
            }
            return Ok(Some(root.clone()));
        }
        Ok(None)
    }
    // None selects healthy consumer-wide maintenance; Some is deliberate scoped cleanup.
    fn reclaim_registered(
        self: &Arc<Self>,
        incarnation: Option<[u8; 32]>,
        deadline: Instant,
    ) -> Result<MetadataCleanupReport, WorkspaceError> {
        clock(deadline)
            .map_err(|error| super::payload::bare_failure(BackingPhase::Cleanup, error.kind()))?;
        let routine = incarnation.is_none();
        let mut selected = if routine {
            self.select_root(None)?
        } else {
            None
        };
        if routine && selected.is_none() {
            return Ok(MetadataCleanupReport {
                remaining_roots: self.roots.lock().map_err(|_| WorkspaceError::Io)?.len(),
                ..MetadataCleanupReport::default()
            });
        }
        // Routine cleanup can stay charged for a later pass while the
        // successor builder owns the shared I/O window. Explicit reclaim
        // still waits to its deadline instead of pretending cleanup completed.
        let mut lease = loop {
            match self.payloads.window(3, 4) {
                Ok(lease) => break lease,
                Err(WorkspaceError::Busy) if routine => {
                    return Ok(MetadataCleanupReport {
                        remaining_roots: self.roots.lock().map_err(|_| WorkspaceError::Io)?.len(),
                        ..MetadataCleanupReport::default()
                    });
                }
                Err(WorkspaceError::Busy) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_micros(200));
                }
                Err(WorkspaceError::Busy) => return Err(WorkspaceError::Deadline),
                Err(error) => return Err(error),
            }
        };
        let _writer = self.writer_until(deadline)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let mut report = MetadataCleanupReport::default();
        loop {
            let root = match selected.take() {
                Some(root) => Some(root),
                None => self.select_root(incarnation)?,
            };
            let Some(root) = root else {
                break;
            };
            if routine && !root.routine_eligible()? {
                continue;
            }
            root.reclaim(window, deadline, &mut report)?;
            self.roots
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .retain(|r| !Arc::ptr_eq(r, &root));
            report.roots_released += 1;
            // Routine work cannot clear quarantine or reinterpret incomplete ownership.
            if !routine {
                self.refresh()?;
            }
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
            for root in roots.iter().filter(|r| Arc::ptr_eq(&r.arena, arena)) {
                let r = root.state.lock().map_err(|_| WorkspaceError::Io)?;
                if let Some(p) = &r.pending {
                    pending = true;
                    known &= p.identity.is_some() && p.allocated.is_some();
                }
            }
            let mut a = arena.state.lock().map_err(|_| WorkspaceError::Io)?;
            a.blocked = a.unrecoverable || pending;
            a.complete = known && (a.complete || !a.unrecoverable);
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
