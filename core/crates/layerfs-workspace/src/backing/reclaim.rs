use super::{
    directory::identity,
    payload::{clock, filename, header, segment_bytes, PayloadHost, Record},
    segments::{self, Window, ALIGN},
};
use crate::*;
use std::{io, sync::Arc, time::Instant};

impl PayloadHost {
    pub fn has_external_pins(&self, incarnation: [u8; 32]) -> Result<bool, WorkspaceError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .records
            .iter()
            .any(|record| {
                record.directory.incarnation == incarnation && Arc::strong_count(record) > 1
            }))
    }
    pub fn remaining(&self, incarnation: [u8; 32]) -> Result<usize, WorkspaceError> {
        Ok(self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .records
            .iter()
            .filter(|record| record.directory.incarnation == incarnation)
            .count())
    }
    pub fn maintain(self: &Arc<Self>, deadline: Instant) -> Result<(), WorkspaceError> {
        self.reclaim_registered(None, deadline).map(|_| ())
    }
    pub fn reclaim(
        self: &Arc<Self>,
        incarnation: [u8; 32],
        deadline: Instant,
    ) -> Result<CleanupReport, WorkspaceError> {
        self.reclaim_registered(Some(incarnation), deadline)
    }
    fn select_record(
        &self,
        incarnation: Option<[u8; 32]>,
    ) -> Result<Option<Arc<Record>>, WorkspaceError> {
        let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
        for record in &state.records {
            if Arc::strong_count(record) != 1
                || incarnation.is_some_and(|id| record.directory.incarnation != id)
            {
                continue;
            }
            let owner = record.state.lock().map_err(|_| WorkspaceError::Io)?;
            if owner.custody.is_some() {
                continue;
            }
            if incarnation.is_none()
                && (!owner.ready
                    || !owner.complete
                    || owner.admission_blocked
                    || owner.failure.is_some()
                    || owner.partial.is_some()
                    || owner.next_cleanup != 0
                    || owner.reserved != 0
                    || owner.completed != record.length
                    || owner.created != segments::segment_count(record.length))
            {
                continue;
            }
            return Ok(Some(record.clone()));
        }
        Ok(None)
    }
    // None selects healthy consumer-wide maintenance; Some is deliberate scoped cleanup.
    fn reclaim_registered(
        self: &Arc<Self>,
        incarnation: Option<[u8; 32]>,
        deadline: Instant,
    ) -> Result<CleanupReport, WorkspaceError> {
        clock(deadline)
            .map_err(|error| super::payload::bare_failure(BackingPhase::Cleanup, error.kind()))?;
        let routine = incarnation.is_none();
        let mut selected = if routine {
            self.select_record(None)?
        } else {
            None
        };
        if routine && selected.is_none() {
            return Ok(CleanupReport {
                remaining_payloads: self
                    .state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .records
                    .len(),
                ..CleanupReport::default()
            });
        }
        let mut lease = self.window(3, 4)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let mut report = CleanupReport::default();
        loop {
            let record = match selected.take() {
                Some(record) => Some(record),
                None => self.select_record(incarnation)?,
            };
            let Some(record) = record else {
                break;
            };
            if let Err(error) = self.reclaim_record(&record, window, deadline, &mut report) {
                return Err(self.failure(&record, BackingPhase::Cleanup, &error));
            }
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.records.retain(|current| current.id != record.id);
            // Excluded failed owners and metadata quarantine remain in this account.
            self.refresh(&mut state)?;
            report.payloads_released += 1;
        }
        report.remaining_payloads = self
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .records
            .len();
        Ok(report)
    }
    fn reclaim_record(
        &self,
        record: &Record,
        window: &mut Window,
        deadline: Instant,
        report: &mut CleanupReport,
    ) -> io::Result<()> {
        let directory = record.directory.file()?;
        loop {
            clock(deadline)?;
            let (index, end, partial) = {
                let state = record
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("payload ownership poisoned"))?;
                (
                    state.next_cleanup,
                    state
                        .created
                        .max(state.partial.map_or(0, |partial| partial.index + 1)),
                    state.partial,
                )
            };
            if index >= end {
                return Ok(());
            }
            let partial = partial.filter(|partial| partial.index == index);
            let name = filename(record.id, index);
            let opened = segments::open(&directory, &name);
            match opened {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(file) => {
                    if partial.is_some_and(|partial| partial.collider || partial.identity.is_none())
                    {
                        return Err(io::ErrorKind::PermissionDenied.into());
                    }
                    if let Some(expected) = partial.and_then(|partial| partial.identity) {
                        if identity(&file.metadata()?) != expected {
                            self.stop(record);
                            return Err(io::ErrorKind::PermissionDenied.into());
                        }
                    } else {
                        segments::read(&file, window, 0, ALIGN)?;
                        header(record, index).verify(window)?;
                    }
                    // No reader or acquisition can hold this record, and this is the last file FD.
                    drop(file);
                    segments::unlink(&directory, &name)?;
                }
            }
            let allocated = partial.map_or_else(
                || segment_bytes(record, index),
                |partial| partial.allocated.unwrap_or(0),
            );
            let reserved = partial.map_or(0, |partial| partial.reserved);
            let mut global = self
                .state
                .lock()
                .map_err(|_| io::Error::other("payload accounting poisoned"))?;
            let mut state = record
                .state
                .lock()
                .map_err(|_| io::Error::other("payload ownership poisoned"))?;
            state.allocated = state
                .allocated
                .checked_sub(allocated)
                .ok_or_else(|| io::Error::other("allocation ownership mismatch"))?;
            state.reserved = state
                .reserved
                .checked_sub(reserved)
                .ok_or_else(|| io::Error::other("reservation ownership mismatch"))?;
            global.allocated -= allocated;
            global.reserved -= reserved;
            state.next_cleanup += 1;
            if partial.is_some() {
                state.partial = None;
            }
            report.segments_released += 1;
            report.bytes_released += allocated + reserved;
        }
    }
}
impl Workspace {
    pub(crate) fn maintain_backing(&self, deadline: Instant) -> Result<(), WorkspaceError> {
        clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        if let Some(host) = &self.host.metadata {
            host.maintain(deadline)?;
        }
        if let Some(host) = &self.host.payloads {
            host.maintain(deadline)?;
        }
        Ok(())
    }
    /// Explicitly reclaims unreferenced inputs owned by this Workspace incarnation.
    pub fn reclaim_payloads(&self, deadline: Instant) -> Result<CleanupReport, WorkspaceError> {
        let _operation = self.begin(false, deadline)?;
        self.host
            .payloads
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?
            .reclaim(self.inner.incarnation, deadline)
    }
}
