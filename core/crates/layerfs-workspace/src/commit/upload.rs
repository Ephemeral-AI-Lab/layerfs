//! Fixed-window upload of the captured final file sequence and its local bytes.
use super::source::ReplacementSource;
use crate::{
    backing::{
        metadata::{Arena, RootOwner},
        metadata_pieces::Cursor,
    },
    overlay::pieces::{Inode, PieceKind},
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
            remaining: extents,
            position: 0,
            record: [0; 24],
            record_at: 24,
            cursor: None,
            failure: None,
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
            let (start, piece) = cursor.next(window)?.ok_or(WorkspaceError::Io)?;
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
            return self.descriptor_bytes(out, deadline).map_err(|error| {
                self.failure = Some(error.clone());
                io::Error::other(error)
            });
        }
        if self.position != self.inode.length {
            self.failure = Some(WorkspaceError::Io);
            return Err(io::ErrorKind::InvalidData.into());
        }
        self.source.read(out, deadline, cancel)
    }
}
