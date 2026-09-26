//! The prepared update as an ordered, replayable stream of rows.
//!
//! A Commit used to lower the captured frontier into resident vectors and send
//! them inside one metadata frame, so a generation wider than that frame was
//! refused before any command was sent. It now lowers the same frontier into a
//! stream: the request declares the *exact* rows that follow, and the rows
//! themselves are pulled one at a time - directory bindings first, in parent
//! order, then the identity rows, in serial order - from the same frozen cursors
//! the resident form read.
//!
//! **The declaration is measured, not estimated.** [`measure_prepared_totals`]
//! drains one [`PreparedFrontier`] without encoding anything; the emit pass opens
//! a second frontier and encodes exactly those rows. Both passes run the same
//! code over the same immutable pinned capture, so they agree row for row, and
//! the emit pass refuses to finish if its own counts differ from the declaration
//! it was sent with.
//!
//! **One row is resident at a time.** A directory row is as large as the
//! directory it describes and an identity row is fixed, so the memory a lowering
//! needs follows the widest directory in the generation rather than the number of
//! changed names in it. The body is handed to the transport in bounded chunks,
//! and a chunk is filled by pulling rows - never by holding them.
//!
//! The walk takes no metadata writer gate: the captured root is immutable and
//! pinned for this submission, so an ordinary mounted mutation may publish while
//! the frontier is lowered, exactly as it may during the frozen extent transfer.

use super::directories::DirectorySection;
use super::lower::IdentitySection;
use crate::overlay::snapshot::{Captured, Submission};
use crate::*;
use layerfs_bridge::contract::{
    put_directory_identity, put_directory_row, put_rooted_identity, put_stream_tag, PreparedTotals,
    Root, Source, ROLE_DIRECTORY_DECLARATION, ROLE_DIRECTORY_PATCH, ROLE_EXISTING_FILE,
    ROLE_EXISTING_SYMLINK, ROLE_FRESH_FILE, ROLE_FRESH_SYMLINK,
};
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

/// Bytes one chunk of the body carries before it is handed to the transport.
const STREAM_CHUNK_BYTES: usize = 16 * 1024;

/// One row of the prepared stream.
pub(crate) enum PreparedRow {
    /// One changed directory's final bindings, in name order.
    Directory {
        parent: u64,
        changes: Vec<(Vec<u8>, Option<u64>)>,
    },
    /// One identity row, in serial order.
    Identity(IdentityRow),
}

/// One identity row: what a serial became in this generation.
pub(crate) enum IdentityRow {
    /// A regular file or symlink, with the roots this Commit saved for it.
    Rooted {
        serial: u64,
        fresh: bool,
        symlink: bool,
        content: Root,
        metadata: Root,
    },
    /// An existing directory whose selected portable fields changed.
    DirectoryPatch {
        serial: u64,
        mode: u32,
        seconds: i64,
        nanos: u32,
    },
    /// A directory serial the canonical state has not accepted before.
    DirectoryDeclaration {
        serial: u64,
        mode: u32,
        seconds: i64,
        nanos: u32,
    },
}

/// Which section of the body the frontier is producing.
enum Section<'a> {
    Directories(DirectorySection<'a>),
    Identities(IdentitySection<'a>),
    Done,
}

/// The captured frontier, lowered one row at a time.
pub(crate) struct PreparedFrontier<'a> {
    workspace: Workspace,
    submission: &'a Submission,
    captured: &'a Captured,
    deadline: Instant,
    section: Section<'a>,
    totals: PreparedTotals,
}

impl<'a> PreparedFrontier<'a> {
    /// Opens one lowering of this submission's captured frontier.
    pub(crate) fn open(
        workspace: &Workspace,
        submission: &'a Submission,
        deadline: Instant,
    ) -> Result<Self, WorkspaceError> {
        let captured = submission.capture()?;
        let walk = workspace.dirty_walk(submission, deadline)?;
        Ok(Self {
            workspace: workspace.clone(),
            submission,
            captured,
            deadline,
            section: Section::Directories(DirectorySection::new(captured, walk, true)),
            totals: PreparedTotals::default(),
        })
    }

    /// Opens one *measuring* lowering: the same rows, no bookkeeping recorded.
    pub(crate) fn measure(
        workspace: &Workspace,
        submission: &'a Submission,
        deadline: Instant,
    ) -> Result<Self, WorkspaceError> {
        let captured = submission.capture()?;
        let walk = workspace.dirty_walk(submission, deadline)?;
        Ok(Self {
            workspace: workspace.clone(),
            submission,
            captured,
            deadline,
            section: Section::Directories(DirectorySection::new(captured, walk, false)),
            totals: PreparedTotals::default(),
        })
    }

    /// The exact rows this lowering has produced so far.
    pub(crate) fn totals(&self) -> PreparedTotals {
        self.totals
    }

    /// The next row, or `None` once the whole stream has been produced.
    pub(crate) fn next(&mut self) -> Result<Option<PreparedRow>, WorkspaceError> {
        loop {
            let row = match &mut self.section {
                Section::Directories(section) => {
                    match section.next(&self.workspace, self.deadline)? {
                        Some(row) => row,
                        None => {
                            let identities = IdentitySection::open(
                                &self.workspace,
                                self.submission,
                                self.captured,
                                self.deadline,
                                section.record(),
                            )?;
                            self.section = Section::Identities(identities);
                            continue;
                        }
                    }
                }
                Section::Identities(section) => {
                    match section.next(&self.workspace, self.submission, self.deadline)? {
                        Some(row) => row,
                        None => {
                            self.section = Section::Done;
                            return Ok(None);
                        }
                    }
                }
                Section::Done => return Ok(None),
            };
            match &row {
                PreparedRow::Directory { changes, .. } => {
                    self.totals.directories = self
                        .totals
                        .directories
                        .checked_add(1)
                        .ok_or(WorkspaceError::Capacity)?;
                    self.totals.names = self
                        .totals
                        .names
                        .checked_add(changes.len() as u64)
                        .ok_or(WorkspaceError::Capacity)?;
                    self.totals.name_bytes = self
                        .totals
                        .name_bytes
                        .checked_add(
                            changes
                                .iter()
                                .map(|(name, _)| 10 + name.len() as u64)
                                .sum::<u64>(),
                        )
                        .ok_or(WorkspaceError::Capacity)?;
                }
                PreparedRow::Identity(row) => {
                    self.totals.identities = self
                        .totals
                        .identities
                        .checked_add(1)
                        .ok_or(WorkspaceError::Capacity)?;
                    match row {
                        IdentityRow::Rooted { fresh, .. } => {
                            if *fresh {
                                self.totals.fresh = self
                                    .totals
                                    .fresh
                                    .checked_add(1)
                                    .ok_or(WorkspaceError::Capacity)?;
                            }
                        }
                        IdentityRow::DirectoryPatch { .. } => {
                            self.totals.patches = self
                                .totals
                                .patches
                                .checked_add(1)
                                .ok_or(WorkspaceError::Capacity)?;
                        }
                        IdentityRow::DirectoryDeclaration { .. } => {
                            self.totals.declarations = self
                                .totals
                                .declarations
                                .checked_add(1)
                                .ok_or(WorkspaceError::Capacity)?;
                            self.totals.fresh = self
                                .totals
                                .fresh
                                .checked_add(1)
                                .ok_or(WorkspaceError::Capacity)?;
                        }
                    }
                }
            }
            return Ok(Some(row));
        }
    }
}

/// Measures the exact rows one Commit would send, without sending them.
pub(crate) fn measure_prepared_totals(
    workspace: &Workspace,
    submission: &Submission,
    deadline: Instant,
) -> Result<PreparedTotals, WorkspaceError> {
    let mut frontier = PreparedFrontier::measure(workspace, submission, deadline)?;
    while frontier.next()?.is_some() {}
    let totals = frontier.totals();
    totals.check().map_err(|_| WorkspaceError::Capacity)?;
    Ok(totals)
}

/// The prepared body: rows encoded one at a time as the transport pulls bytes.
pub(crate) struct PreparedStream<'a> {
    frontier: PreparedFrontier<'a>,
    buffer: Vec<u8>,
    at: usize,
    started: bool,
    declared: PreparedTotals,
    failure: Option<WorkspaceError>,
}

impl<'a> PreparedStream<'a> {
    /// Opens the body one request declared, over a fresh lowering.
    pub(crate) fn open(
        workspace: &Workspace,
        submission: &'a Submission,
        deadline: Instant,
        declared: PreparedTotals,
    ) -> Result<Self, WorkspaceError> {
        Ok(Self {
            frontier: PreparedFrontier::open(workspace, submission, deadline)?,
            buffer: Vec::new(),
            at: 0,
            started: false,
            declared,
            failure: None,
        })
    }

    /// Fills the buffer from the frontier until it holds one chunk or the stream
    /// ends.
    fn fill(&mut self) -> Result<bool, WorkspaceError> {
        if self.at < self.buffer.len() {
            return Ok(true);
        }
        self.buffer.clear();
        self.at = 0;
        if !self.started {
            self.started = true;
            put_stream_tag(&mut self.buffer).map_err(|_| WorkspaceError::Capacity)?;
        }
        while self.buffer.len() < STREAM_CHUNK_BYTES {
            let Some(row) = self.frontier.next()? else {
                break;
            };
            match row {
                PreparedRow::Directory { parent, changes } => {
                    put_directory_row(&mut self.buffer, parent, &changes)
                        .map_err(|_| WorkspaceError::Capacity)?;
                }
                PreparedRow::Identity(row) => match row {
                    IdentityRow::Rooted {
                        serial,
                        fresh,
                        symlink,
                        content,
                        metadata,
                    } => {
                        let role = match (fresh, symlink) {
                            (false, false) => ROLE_EXISTING_FILE,
                            (false, true) => ROLE_EXISTING_SYMLINK,
                            (true, false) => ROLE_FRESH_FILE,
                            (true, true) => ROLE_FRESH_SYMLINK,
                        };
                        put_rooted_identity(&mut self.buffer, role, serial, &content, &metadata)
                            .map_err(|_| WorkspaceError::Capacity)?;
                    }
                    IdentityRow::DirectoryPatch {
                        serial,
                        mode,
                        seconds,
                        nanos,
                    } => {
                        put_directory_identity(
                            &mut self.buffer,
                            ROLE_DIRECTORY_PATCH,
                            serial,
                            mode,
                            seconds,
                            nanos,
                        )
                        .map_err(|_| WorkspaceError::Capacity)?;
                    }
                    IdentityRow::DirectoryDeclaration {
                        serial,
                        mode,
                        seconds,
                        nanos,
                    } => {
                        put_directory_identity(
                            &mut self.buffer,
                            ROLE_DIRECTORY_DECLARATION,
                            serial,
                            mode,
                            seconds,
                            nanos,
                        )
                        .map_err(|_| WorkspaceError::Capacity)?;
                    }
                },
            }
        }
        Ok(!self.buffer.is_empty())
    }
}

impl Source for PreparedStream<'_> {
    fn read(
        &mut self,
        out: &mut [u8],
        _deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if out.is_empty() {
            return Ok(0);
        }
        match self.fill() {
            Ok(false) => {
                // The stream is over: the rows it produced must be exactly the
                // rows the request declared, or the update the service receives
                // would not be the update this Commit meant to make.
                if self.frontier.totals() != self.declared {
                    self.failure = Some(WorkspaceError::Io);
                    return Err(io::Error::other("prepared stream rows"));
                }
                Ok(0)
            }
            Ok(true) => {
                let take = out.len().min(self.buffer.len() - self.at);
                out[..take].copy_from_slice(&self.buffer[self.at..self.at + take]);
                self.at += take;
                Ok(take)
            }
            Err(error) => {
                self.failure = Some(error.clone());
                Err(io::Error::other(error))
            }
        }
    }
}
