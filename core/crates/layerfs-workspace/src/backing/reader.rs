use super::{
    payload::{clock, filename, header, OwnedPayload, WindowLease},
    segments::{self, ALIGN, DATA_BYTES, INLINE_START, WINDOW_BYTES},
};
use crate::{runtime::host::operator_diagnostics_enabled, BackingPhase, WorkspaceError};
use layerfs_bridge::contract::Source;
use std::{
    fs::File,
    io,
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

#[derive(Clone, Copy, Default)]
pub(crate) struct PayloadIoCounts {
    pub(crate) open_calls: u64,
    pub(crate) open_ns: u64,
    pub(crate) aligned_read_calls: u64,
    pub(crate) aligned_read_bytes: u64,
    pub(crate) aligned_read_ns: u64,
}

fn open_segment(
    directory: &File,
    name: &str,
    counts: &mut PayloadIoCounts,
    enabled: bool,
) -> io::Result<File> {
    let started = enabled.then(Instant::now);
    let result = segments::open(directory, name);
    if let Some(started) = started {
        counts.open_calls = counts.open_calls.saturating_add(1);
        counts.open_ns = counts
            .open_ns
            .saturating_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
    }
    result
}

fn read_segment(
    file: &File,
    window: &mut segments::Window,
    offset: u64,
    length: usize,
    counts: &mut PayloadIoCounts,
    enabled: bool,
) -> io::Result<()> {
    let started = enabled.then(Instant::now);
    let result = segments::read(file, window, offset, length);
    if let Some(started) = started {
        counts.aligned_read_calls = counts.aligned_read_calls.saturating_add(1);
        counts.aligned_read_ns = counts
            .aligned_read_ns
            .saturating_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
        if result.is_ok() {
            counts.aligned_read_bytes = counts.aligned_read_bytes.saturating_add(length as u64);
        }
    }
    result
}

pub struct PayloadReader {
    file: Option<(u32, File)>,
    owner: OwnedPayload,
    lease: WindowLease,
    position: u64,
    end: u64,
    diagnostic_enabled: bool,
    diagnostic: PayloadIoCounts,
}
impl PayloadReader {
    pub(crate) fn io_counts(&self) -> PayloadIoCounts {
        self.diagnostic
    }

    pub(crate) fn new(owner: OwnedPayload, range: Range<u64>) -> Result<Self, WorkspaceError> {
        if range.start > range.end || range.end > owner.len() {
            return Err(WorkspaceError::InvalidInput);
        }
        if !owner
            .record
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .ready
        {
            return Err(WorkspaceError::Busy);
        }
        let lease = owner.host.window(1, 3)?;
        Ok(Self {
            file: None,
            owner,
            lease,
            position: range.start,
            end: range.end,
            diagnostic_enabled: operator_diagnostics_enabled(),
            diagnostic: PayloadIoCounts::default(),
        })
    }
    fn read_inner(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        clock(deadline)?;
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if out.is_empty() || self.position == self.end {
            return Ok(0);
        }
        let index = (self.position / DATA_BYTES as u64) as u32;
        let window = self
            .lease
            .window
            .as_mut()
            .ok_or_else(|| io::Error::other("missing payload read window"))?;
        if self
            .file
            .as_ref()
            .is_none_or(|(current, _)| *current != index)
        {
            drop(self.file.take());
            let directory = self.owner.record.directory.file()?;
            let file = open_segment(
                &directory,
                &filename(self.owner.record.id, index),
                &mut self.diagnostic,
                self.diagnostic_enabled,
            )?;
            read_segment(
                &file,
                window,
                0,
                ALIGN,
                &mut self.diagnostic,
                self.diagnostic_enabled,
            )?;
            header(&self.owner.record, index).verify(window)?;
            self.file = Some((index, file));
        }
        if segments::inline(self.owner.record.length) {
            clock(deadline)?;
            if cancel.load(Ordering::Acquire) {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let count = out.len().min((self.end - self.position) as usize);
            let start = INLINE_START + self.position as usize;
            out[..count].copy_from_slice(&window.0[start..start + count]);
            self.position += count as u64;
            return Ok(count);
        }
        let local = self.position % DATA_BYTES as u64;
        let offset = ALIGN as u64 + local;
        let prefix = (offset % ALIGN as u64) as usize;
        let count = (out.len() as u64)
            .min(self.end - self.position)
            .min(DATA_BYTES as u64 - local)
            .min((WINDOW_BYTES - prefix) as u64) as usize;
        let physical = (prefix + count).div_ceil(ALIGN) * ALIGN;
        let file = &self
            .file
            .as_ref()
            .ok_or_else(|| io::Error::other("missing payload descriptor"))?
            .1;
        read_segment(
            file,
            window,
            offset - prefix as u64,
            physical,
            &mut self.diagnostic,
            self.diagnostic_enabled,
        )?;
        clock(deadline)?;
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        out[..count].copy_from_slice(&window.0[prefix..prefix + count]);
        self.position += count as u64;
        Ok(count)
    }
}
impl Source for PayloadReader {
    fn read(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        match self.read_inner(out, deadline, cancel) {
            Ok(count) => Ok(count),
            Err(error) => {
                if matches!(
                    error.kind(),
                    io::ErrorKind::InvalidData
                        | io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::NotFound
                ) {
                    self.owner.host.stop(&self.owner.record);
                }
                match self
                    .owner
                    .host
                    .failure(&self.owner.record, BackingPhase::Read, &error)
                {
                    WorkspaceError::Backing(failure) => Err(io::Error::new(error.kind(), failure)),
                    failure => Err(io::Error::other(failure)),
                }
            }
        }
    }
}
impl Drop for PayloadReader {
    fn drop(&mut self) {
        // The file must close before the record pin can become cleanup-eligible.
        drop(self.file.take());
    }
}
