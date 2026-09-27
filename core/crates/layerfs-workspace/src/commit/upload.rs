//! Fixed-window upload of the captured final file sequence and its local bytes.
use super::source::ReplacementSource;
use crate::{
    backing::{
        metadata::{Arena, RootOwner},
        metadata_pieces::Cursor,
    },
    overlay::pieces::{Inode, PieceKind},
    runtime::host::operator_diagnostics_enabled,
    *,
};
use layerfs_bridge::contract::Source;
use std::{
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

pub(crate) struct FileUpload<'a> {
    workspace: Workspace,
    root: Arc<RootOwner>,
    inode: Inode,
    source: &'a mut ReplacementSource,
    declared_extents: u64,
    remaining: u64,
    position: u64,
    record: [u8; 24],
    record_at: usize,
    /// The one descriptor walk this upload owns, kept across its frames for the
    /// same reason the replacement walk is: a frame continues the frozen
    /// sequence instead of seeking its path again, and the immutable pinned
    /// pages are read without the metadata writer gate.
    cursor: Option<Cursor<Arc<Arena>>>,
    pub(crate) failure: Option<WorkspaceError>,
    diagnostic_enabled: bool,
    descriptor_source_calls: u64,
    descriptor_source_bytes: u64,
    descriptor_source_ns: u64,
    descriptor_cursor_next_calls: u64,
    descriptor_cursor_next_ns: u64,
    replacement_source_calls: u64,
    replacement_source_bytes: u64,
    replacement_source_ns: u64,
}

impl Drop for FileUpload<'_> {
    fn drop(&mut self) {
        if !self.diagnostic_enabled {
            return;
        }
        let source = &self.source.diagnostic;
        eprintln!(
            "LFS_COMMIT_SOURCE_CAUSE v=1 scope=save_file source_complete={} declared_extents={} final_length={} replacement_bytes={} descriptor_source_calls={} descriptor_source_bytes={} descriptor_source_ns={} descriptor_cursor_next_calls={} descriptor_cursor_next_ns={} replacement_source_calls={} replacement_source_bytes={} replacement_source_ns={} replacement_cursor_next_calls={} replacement_cursor_next_ns={} local_reader_create_calls={} local_reader_create_ns={} local_read_calls={} local_read_bytes={} local_read_ns={} local_segment_open_calls={} local_segment_open_ns={} local_aligned_read_calls={} local_aligned_read_bytes={} local_aligned_read_ns={}",
            self.complete(),
            self.declared_extents,
            self.inode.length,
            self.inode.replacement,
            self.descriptor_source_calls,
            self.descriptor_source_bytes,
            self.descriptor_source_ns,
            self.descriptor_cursor_next_calls,
            self.descriptor_cursor_next_ns,
            self.replacement_source_calls,
            self.replacement_source_bytes,
            self.replacement_source_ns,
            source.cursor_next_calls,
            source.cursor_next_ns,
            source.reader_create_calls,
            source.reader_create_ns,
            source.local_read_calls,
            source.local_read_bytes,
            source.local_read_ns,
            source.segment_open_calls,
            source.segment_open_ns,
            source.aligned_read_calls,
            source.aligned_read_bytes,
            source.aligned_read_ns,
        );
    }
}

impl<'a> FileUpload<'a> {
    pub(crate) fn new(
        workspace: Workspace,
        root: Arc<RootOwner>,
        inode: Inode,
        extents: u64,
        source: &'a mut ReplacementSource,
    ) -> Self {
        Self {
            workspace,
            root,
            inode,
            source,
            declared_extents: extents,
            remaining: extents,
            position: 0,
            record: [0; 24],
            record_at: 24,
            cursor: None,
            failure: None,
            diagnostic_enabled: operator_diagnostics_enabled(),
            descriptor_source_calls: 0,
            descriptor_source_bytes: 0,
            descriptor_source_ns: 0,
            descriptor_cursor_next_calls: 0,
            descriptor_cursor_next_ns: 0,
            replacement_source_calls: 0,
            replacement_source_bytes: 0,
            replacement_source_ns: 0,
        }
    }

    pub(crate) fn complete(&self) -> bool {
        self.remaining == 0
            && self.record_at == 24
            && self.position == self.inode.length
            && self.source.complete()
    }

    pub(crate) fn source_failure(&mut self) -> Option<WorkspaceError> {
        self.failure.take().or_else(|| self.source.failure.take())
    }

    fn descriptor_bytes(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
    ) -> Result<usize, WorkspaceError> {
        let mut filled = 0;
        if self.record_at < 24 {
            let take = (24 - self.record_at).min(out.len());
            out[..take].copy_from_slice(&self.record[self.record_at..self.record_at + take]);
            self.record_at += take;
            filled += take;
        }
        if filled == out.len() || self.remaining == 0 {
            return Ok(filled);
        }
        let host = self
            .workspace
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut lease = host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        if self.cursor.is_none() {
            self.cursor = Some(self.root.arena.cursor(
                self.inode.pieces,
                self.position,
                self.inode.length,
                window,
                deadline,
            )?);
        }
        while filled < out.len() && self.remaining > 0 {
            let cursor = self.cursor.as_mut().ok_or(WorkspaceError::Io)?;
            let started = self.diagnostic_enabled.then(Instant::now);
            let next = cursor.next(window);
            if let Some(started) = started {
                self.descriptor_cursor_next_calls += 1;
                self.descriptor_cursor_next_ns = self
                    .descriptor_cursor_next_ns
                    .saturating_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
            }
            let (start, piece) = next?.ok_or(WorkspaceError::Io)?;
            if start != self.position {
                return Err(WorkspaceError::Io);
            }
            self.position = self
                .position
                .checked_add(piece.length)
                .filter(|end| *end <= self.inode.length)
                .ok_or(WorkspaceError::Io)?;
            let kind = match piece.kind {
                PieceKind::Base => 0u64,
                PieceKind::Local => 1,
                PieceKind::Zero => 2,
            };
            self.record[..8].copy_from_slice(&kind.to_be_bytes());
            self.record[8..16]
                .copy_from_slice(&if kind == 0 { piece.offset } else { 0 }.to_be_bytes());
            self.record[16..].copy_from_slice(&piece.length.to_be_bytes());
            self.remaining -= 1;
            let take = (out.len() - filled).min(24);
            out[filled..filled + take].copy_from_slice(&self.record[..take]);
            self.record_at = take;
            filled += take;
        }
        Ok(filled)
    }
}

impl Source for FileUpload<'_> {
    fn read(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if out.is_empty() {
            return Ok(0);
        }
        if self.remaining > 0 || self.record_at < 24 {
            let started = self.diagnostic_enabled.then(Instant::now);
            let result = self.descriptor_bytes(out, deadline);
            if let Some(started) = started {
                self.descriptor_source_calls += 1;
                self.descriptor_source_ns = self
                    .descriptor_source_ns
                    .saturating_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
                if let Ok(count) = result.as_ref() {
                    self.descriptor_source_bytes =
                        self.descriptor_source_bytes.saturating_add(*count as u64);
                }
            }
            return result.map_err(|error| {
                self.failure = Some(error.clone());
                io::Error::other(error)
            });
        }
        if self.position != self.inode.length {
            self.failure = Some(WorkspaceError::Io);
            return Err(io::ErrorKind::InvalidData.into());
        }
        let started = self.diagnostic_enabled.then(Instant::now);
        let result = self.source.read(out, deadline, cancel);
        if let Some(started) = started {
            self.replacement_source_calls += 1;
            self.replacement_source_ns = self
                .replacement_source_ns
                .saturating_add(started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
            if let Ok(count) = result.as_ref() {
                self.replacement_source_bytes =
                    self.replacement_source_bytes.saturating_add(*count as u64);
            }
        }
        result
    }
}
