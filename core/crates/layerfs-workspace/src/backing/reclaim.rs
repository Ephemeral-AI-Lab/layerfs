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
    pub fn reclaim(
        self: &Arc<Self>,
        incarnation: [u8; 32],
        deadline: Instant,
    ) -> Result<CleanupReport, WorkspaceError> {
        clock(deadline)
            .map_err(|error| super::payload::bare_failure(BackingPhase::Cleanup, error.kind()))?;
        let mut lease = self.window(3, 4)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let mut report = CleanupReport::default();
        loop {
            let record = {
                let state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
                state
                    .records
                    .iter()
                    .find(|record| {
                        record.directory.incarnation == incarnation
                            && Arc::strong_count(record) == 1
                            && record
                                .state
                                .lock()
                                .is_ok_and(|state| state.custody.is_none())
                    })
                    .cloned()
            };
            let Some(record) = record else {
                break;
            };
            if let Err(error) = self.reclaim_record(&record, window, deadline, &mut report) {
                return Err(self.failure(&record, BackingPhase::Cleanup, &error));
            }
            let mut state = self.state.lock().map_err(|_| WorkspaceError::Io)?;
            state.records.retain(|current| current.id != record.id);
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
