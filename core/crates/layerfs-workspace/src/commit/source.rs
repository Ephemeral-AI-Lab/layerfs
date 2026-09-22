//! The existing bridge Source contract over captured replacement spans.
use crate::{
    backing::{metadata::RootOwner, reader::PayloadReader},
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
pub struct ReplacementSource {
    workspace: Workspace,
    root: Arc<RootOwner>,
    inode: Inode,
    position: u64,
    emitted: u64,
    reader: Option<PayloadReader>,
    left: u64,
    zero: bool,
    pub failure: Option<WorkspaceError>,
}
impl ReplacementSource {
    pub fn new(workspace: Workspace, root: Arc<RootOwner>, inode: Inode) -> Self {
        Self {
            workspace,
            root,
            inode,
            position: 0,
            emitted: 0,
            reader: None,
            left: 0,
            zero: false,
            failure: None,
        }
    }
    pub fn complete(&self) -> bool {
        self.emitted == self.inode.replacement
    }
    fn pull(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> Result<usize, WorkspaceError> {
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        if cancel.load(Ordering::Acquire) {
            return Err(WorkspaceError::Busy);
        }
        if out.is_empty() {
            return Ok(0);
        }
        loop {
            if self.zero {
                let count = self.left.min(out.len().min(MAX_READ_BYTES) as u64) as usize;
                if count == 0 {
                    return Err(WorkspaceError::Io);
                }
                out[..count].fill(0);
                self.left -= count as u64;
                self.emitted += count as u64;
                self.zero = self.left != 0;
                return Ok(count);
            }
            if let Some(reader) = &mut self.reader {
                let limit = self.left.min(out.len().min(MAX_READ_BYTES) as u64) as usize;
                let count =
                    Source::read(reader, &mut out[..limit], deadline, cancel).map_err(|error| {
                        error
                            .get_ref()
                            .and_then(|inner| inner.downcast_ref::<BackingFailure>())
                            .map_or(WorkspaceError::Io, |failure| {
                                WorkspaceError::Backing(failure.clone())
                            })
                    })?;
                if count == 0 {
                    return Err(WorkspaceError::Io);
                }
                self.left -= count as u64;
                self.emitted += count as u64;
                if self.left == 0 {
                    self.reader = None;
                }
                return Ok(count);
            }
            if self.position == self.inode.length {
                if !self.complete() {
                    return Err(WorkspaceError::Io);
                }
                return Ok(0);
            }
            let piece = {
                let host = self
                    .workspace
                    .host
                    .metadata
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?;
                let _view = host.writer()?;
                let mut window = host.payloads.window(1, 3)?;
                self.root.arena.piece_at(
                    self.inode.pieces,
                    self.position,
                    window.window.as_mut().ok_or(WorkspaceError::Io)?,
                    deadline,
                )?
            };
            if piece.start != self.position {
                return Err(WorkspaceError::Io);
            }
            self.position = self
                .position
                .checked_add(piece.length)
                .filter(|position| *position <= self.inode.length)
                .ok_or(WorkspaceError::Io)?;
            match piece.kind {
                PieceKind::Base => continue,
                PieceKind::Zero => {
                    self.left = piece.length;
                    self.zero = true;
                }
                PieceKind::Local => {
                    let payload = self.root.arena.payload(piece.payload, piece.custody)?;
                    self.left = piece.length;
                    self.reader = Some(payload.reader(piece.offset..piece.offset + piece.length)?);
                }
            }
        }
    }
}
impl Source for ReplacementSource {
    fn read(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        match self.pull(out, deadline, cancel) {
            Ok(count) => Ok(count),
            Err(error) => {
                self.failure = Some(error.clone());
                let kind = match &error {
                    WorkspaceError::Backing(failure) => failure.kind,
                    WorkspaceError::Deadline => io::ErrorKind::TimedOut,
                    WorkspaceError::Busy => io::ErrorKind::WouldBlock,
                    _ => io::ErrorKind::Other,
                };
                Err(io::Error::new(kind, error))
            }
        }
    }
}
