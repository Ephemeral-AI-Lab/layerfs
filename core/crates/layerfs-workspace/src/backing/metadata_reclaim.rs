//! Explicit metadata reclamation with retained progress across known failures.
use super::{
    directory::identity,
    metadata::RootOwner,
    metadata_pages::{PageRef, PAGE},
    ownership::{page_name, CleanupFrame},
    payload::clock,
    segments::{self, Window},
};
use crate::*;
use std::{fs, io, sync::atomic::Ordering, time::Instant};
impl RootOwner {
    fn cleanup_step(
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
                let state = host.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
                let payload = state
                    .records
                    .iter()
                    .find(|p| p.id == owner.payload)
                    .ok_or(WorkspaceError::Io)?;
                let mut record = payload.state.lock().map_err(|_| WorkspaceError::Io)?;
                if record.custody != Some((arena.id, frame.page)) {
                    return Err(WorkspaceError::Io);
                }
                record.custody = None;
                drop(record);
                drop(state);
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
                let data = arena.load(frame.page, window, deadline)?;
                let edges = super::metadata_index::edges(&data)?;
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
                    .copied()
                    == Some(r)
            };
            if prospective {
                // No uncertain write is eligible for this path. The owned marker
                // is released only after explicit cleanup observes that condition.
                let host = self.arena.host()?;
                let records = host.payloads.state.lock().map_err(|_| WorkspaceError::Io)?;
                let record = records
                    .records
                    .iter()
                    .find(|record| {
                        record
                            .state
                            .lock()
                            .is_ok_and(|state| state.custody == Some((self.arena.id, r)))
                    })
                    .cloned()
                    .ok_or(WorkspaceError::Io)?;
                drop(records);
                record.state.lock().map_err(|_| WorkspaceError::Io)?.custody = None;
                self.state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .custodies
                    .pop();
            }
            let mut a = self.arena.state.lock().map_err(|_| WorkspaceError::Io)?;
            if r.epoch == 1 {
                if a.next != r.slot + 1 {
                    return Err(WorkspaceError::Io);
                }
                a.next -= 1;
                self.arena.host()?.slots.fetch_sub(1, Ordering::AcqRel);
            } else {
                a.free = PageRef {
                    slot: r.slot,
                    epoch: r.epoch - 1,
                };
                a.reusable += 1;
            }
            drop(a);
            self.state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .slot_pending = None;
        }
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
                    (*r, 1)
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
