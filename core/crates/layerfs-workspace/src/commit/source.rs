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
        let mut filled = 0usize;
        // One extent may span several pulls, so the piece already in progress
        // drains first and the walk then continues from its end.
        if self.zero {
            let count =
                self.left
                    .min((out.len() - filled).min(MAX_READ_BYTES) as u64) as usize;
            if count == 0 {
                return Err(WorkspaceError::Io);
            }
            out[filled..filled + count].fill(0);
            self.left -= count as u64;
            self.emitted += count as u64;
            self.zero = self.left != 0;
            filled += count;
        } else if let Some(reader) = &mut self.reader {
            let limit =
                self.left
                    .min((out.len() - filled).min(MAX_READ_BYTES) as u64) as usize;
            let count = Source::read(reader, &mut out[filled..filled + limit], deadline, cancel)
                .map_err(|error| {
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
            filled += count;
            if self.left == 0 {
                self.reader = None;
            }
        }
        if filled == out.len() {
            return Ok(filled);
        }
        if self.position == self.inode.length {
            if filled > 0 {
                return Ok(filled);
            }
            if !self.complete() {
                return Err(WorkspaceError::Io);
            }
            return Ok(0);
        }
        // The frozen sequence is walked by one bounded cursor per pull: it
        // steps forward through the leaves in order and the buffer fills from
        // every non-base extent it passes, so the walk visits each page of its
        // path once — exactly like the lowering walk — instead of re-seeking
        // per extent.
        let root = self.root.clone();
        let host = self
            .workspace
            .host
            .metadata
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let _view = host.writer()?;
        let mut lease = host.payloads.window(1, 3)?;
        let window = lease.window.as_mut().ok_or(WorkspaceError::Io)?;
        let mut cursor = root.arena.cursor(
            self.inode.pieces,
            self.position,
            self.inode.length,
            window,
            deadline,
        )?;
        while filled < out.len() {
            let Some((start, piece)) = cursor.next(window)? else {
                break;
            };
            if start != self.position {
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
                    let mut left = piece.length;
                    while left > 0 && filled < out.len() {
                        let count =
                            left.min((out.len() - filled).min(MAX_READ_BYTES) as u64) as usize;
                        out[filled..filled + count].fill(0);
                        left -= count as u64;
                        self.emitted += count as u64;
                        filled += count;
                    }
                    if left > 0 {
                        self.left = left;
                        self.zero = true;
                        break;
                    }
                }
                PieceKind::Local => {
                    let payload = root.arena.payload(piece.payload, piece.custody)?;
                    let mut reader = payload.reader(piece.offset..piece.offset + piece.length)?;
                    let mut left = piece.length;
                    while left > 0 && filled < out.len() {
                        let limit =
                            left.min((out.len() - filled).min(MAX_READ_BYTES) as u64) as usize;
                        let count = Source::read(
                            &mut reader,
                            &mut out[filled..filled + limit],
                            deadline,
                            cancel,
                        )
                        .map_err(|error| {
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
                        left -= count as u64;
                        self.emitted += count as u64;
                        filled += count;
                    }
                    if left > 0 {
                        self.left = left;
                        self.reader = Some(reader);
                        break;
                    }
                }
            }
        }
        Ok(filled)
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
