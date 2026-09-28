//! Public read-only view leases over one pinned selected view.
//!
//! A lease pins the current selected view of the same attached Workspace and
//! resolves every lookup, listing, read and readlink through that exact
//! selection. The reads never mutate contents or namespace; the lease registry
//! is the charged pin/retirement bookkeeping that keeps the pinned revision
//! alive, and its release runs the checked retirement selector exactly once.
use super::namespace::child_path_active;
use crate::{backing::active::inode_key, runtime::view_leases::VIEW_LEASE_BYTES, *};
use layerfs_bridge::contract::{Operation, Response, Source, SYMLINK_TARGET_BYTES};
use std::{io::Cursor, time::Instant};

/// Pinned root, generation, revision and canonical base of one held lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewLeaseInfo {
    pub root: NodeAttributes,
    pub generation: u64,
    pub revision: u64,
    pub base: [u8; 32],
}

/// One entry resolved by one lease: selected identity and attributes only,
/// never a physical page identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewEntryData {
    pub serial: u64,
    pub kind: NodeKind,
    pub size: u64,
    pub references: u64,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}

/// One bounded page of a pinned directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewListPage {
    pub entries: Vec<(Vec<u8>, u64)>,
    pub continuation: Option<Vec<u8>>,
}

/// Bounded pinned bytes with end-of-file and the pinned logical size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewReadData {
    pub bytes: Vec<u8>,
    pub eof: bool,
    pub size: u64,
}

/// Read-only custody observation of one held lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewLeaseStatus {
    pub generation: u64,
    pub revision: u64,
    pub entries: usize,
    pub held_leases: usize,
}

/// The checked release outcome: Completed only after the retirement selector
/// finished; Retained means physical custody stayed and the Workspace stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewRelease {
    Completed,
    Retained(WorkspaceError),
}

impl Workspace {
    /// Pins the current selected view and registers one charged lease.
    /// The view cannot mutate or Commit through the lease; acquiring it only
    /// holds the pinned revision and its charge.
    pub fn pin_view(
        &self,
        token: [u8; VIEW_LEASE_BYTES],
        deadline: Instant,
    ) -> Result<ViewLeaseInfo, WorkspaceError> {
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let state = self.state()?;
        self.available(&state)?;
        if self.inner.active.is_none() {
            return Err(WorkspaceError::Unsupported);
        }
        let view = self.selected_view(&state)?;
        let (generation, revision) = match &view.active {
            Some(snapshot) => (snapshot.generation, snapshot.revision()?),
            None => (state.generation, state.revision),
        };
        let info = ViewLeaseInfo {
            root: self.inner.root,
            generation,
            revision,
            base: state.base,
        };
        self.inner
            .view_leases
            .register(token, view, generation, revision)?;
        // The pinned root is the lease's first registered entry, so the first
        // component lookup resolves from the root the selection pinned.
        let root_serial = self.inner.root.serial;
        self.inner.view_leases.bind_entry(
            &token,
            root_serial,
            crate::runtime::view_leases::HeldEntry {
                path: Vec::new(),
                kind: self.inner.root.kind,
                canonical: true,
                content: state.base,
                size: self.inner.root.size,
            },
        )?;
        Ok(info)
    }

    /// One component of the pinned namespace under a directory this lease
    /// issued, resolved through the pinned selection only.
    pub fn view_lookup(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
        parent: u64,
        name: &[u8],
        deadline: Instant,
    ) -> Result<ViewEntryData, WorkspaceError> {
        if name.is_empty() || name.len() > 255 || name.contains(&0) || name.contains(&b'/') {
            return Err(WorkspaceError::InvalidInput);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        {
            let state = self.state()?;
            self.available(&state)?;
        }
        let directory = self
            .inner
            .view_leases
            .entry(token, parent, Some(NodeKind::Directory))?;
        let resolved = self.inner.view_leases.with_view(token, |view| {
            self.resolve_child_active(
                &mut operation,
                view,
                parent,
                &directory.path,
                name,
                deadline,
            )
        })?;
        let path = child_path_active(&directory.path, name)?;
        let entry = crate::runtime::view_leases::HeldEntry {
            path,
            kind: resolved.original.kind,
            canonical: resolved.canonical,
            content: resolved.content,
            // The effective pinned size: the selected HotInode length for an
            // active entry, the base record's length for a canonical one.
            size: resolved.attr.size,
        };
        self.inner
            .view_leases
            .bind_entry(token, resolved.original.serial, entry)?;
        Ok(ViewEntryData {
            serial: resolved.original.serial,
            kind: resolved.original.kind,
            size: resolved.attr.size,
            references: resolved.attr.references,
            mode: resolved.attr.mode,
            mtime_seconds: resolved.attr.mtime_seconds,
            mtime_nanoseconds: resolved.attr.mtime_nanoseconds,
        })
    }

    /// One bounded page of a pinned directory listing, merged from the pinned
    /// active bindings and the pinned canonical base in one revision.
    pub fn view_list(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
        directory: u64,
        after: Option<&[u8]>,
        limit: usize,
        deadline: Instant,
    ) -> Result<ViewListPage, WorkspaceError> {
        if limit == 0 || limit > 128 {
            return Err(WorkspaceError::InvalidInput);
        }
        if let Some(after) = after {
            if after.len() > 255 || after.contains(&0) {
                return Err(WorkspaceError::InvalidInput);
            }
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        {
            let state = self.state()?;
            self.available(&state)?;
        }
        let held = self
            .inner
            .view_leases
            .entry(token, directory, Some(NodeKind::Directory))?;
        let entries = self.inner.view_leases.with_view(token, |view| {
            self.list_view_active(
                &mut operation,
                view,
                (directory, held.path.as_slice()),
                after.unwrap_or(&[]),
                limit,
                deadline,
            )
        })?;
        let continuation = (entries.len() == limit)
            .then(|| entries.last().map(|(name, _)| name.clone()))
            .flatten();
        Ok(ViewListPage {
            entries,
            continuation,
        })
    }

    /// Bounded pinned bytes of one file this lease issued, resolved through
    /// the pinned selection (Base/Zero/generic overlap/Payload through the
    /// pinned snapshot, or the immutable pinned canonical content root).
    pub fn view_read(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
        file: u64,
        offset: u64,
        size: usize,
        deadline: Instant,
    ) -> Result<ViewReadData, WorkspaceError> {
        if size > MAX_READ_BYTES {
            return Err(WorkspaceError::Capacity);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        {
            let state = self.state()?;
            self.available(&state)?;
        }
        let entry = self
            .inner
            .view_leases
            .entry(token, file, Some(NodeKind::File))?;
        let pinned_size = entry.size;
        let length = if offset >= pinned_size {
            0
        } else {
            (pinned_size - offset).min(size as u64) as usize
        };
        let charge = self.host.budget.reserve(length)?;
        let mut bytes = Vec::new();
        if length > 0 {
            bytes
                .try_reserve_exact(length)
                .map_err(|_| WorkspaceError::Capacity)?;
            bytes.resize(length, 0);
        }
        let active_inode = self.inner.view_leases.with_view(token, |view| {
            Ok(view
                .active
                .as_ref()
                .map(|snapshot| snapshot.get(&inode_key(file)))
                .transpose()?
                .flatten()
                .is_some())
        })?;
        if active_inode {
            let backing = self
                .inner
                .active
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let read = self.inner.view_leases.with_view(token, |view| {
                let snapshot = view.active.as_ref().ok_or(WorkspaceError::Io)?;
                backing.read_file(
                    file,
                    offset,
                    &mut bytes,
                    Some(snapshot),
                    |root, start, output| {
                        operation.remote()?;
                        let mut out = Cursor::new(output);
                        let end = start + out.get_ref().len() as u64;
                        let response = self.call(
                            Operation::ReadFile { root, start, end },
                            end - start,
                            &mut out,
                            deadline,
                        )?;
                        if response
                            != (Response::Read {
                                length: end - start,
                            })
                            || out.position() != end - start
                        {
                            return Err(WorkspaceError::InvalidInput);
                        }
                        Ok(())
                    },
                    |payload, start, output| {
                        let mut reader = payload.reader(start..start + output.len() as u64)?;
                        let mut done = 0;
                        while done < output.len() {
                            let count = Source::read(
                                &mut reader,
                                &mut output[done..],
                                deadline,
                                &self.inner.stopping,
                            )
                            .map_err(|_| WorkspaceError::Io)?;
                            if count == 0 {
                                return Err(WorkspaceError::Io);
                            }
                            done += count;
                        }
                        Ok(())
                    },
                )
            })?;
            if read != length {
                return Err(WorkspaceError::Io);
            }
        } else if entry.canonical {
            operation.remote()?;
            let mut output = Cursor::new(bytes.as_mut_slice());
            let response = self.call(
                Operation::ReadFile {
                    root: entry.content,
                    start: offset,
                    end: offset + length as u64,
                },
                length as u64,
                &mut output,
                deadline,
            )?;
            if response
                != (Response::Read {
                    length: length as u64,
                })
                || output.position() != length as u64
            {
                return Err(WorkspaceError::InvalidInput);
            }
        } else {
            return Err(WorkspaceError::NotFound);
        }
        drop(charge);
        Ok(ViewReadData {
            eof: offset + length as u64 >= pinned_size,
            size: pinned_size,
            bytes,
        })
    }

    /// The exact pinned symlink target bytes.
    pub fn view_readlink(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
        link: u64,
        deadline: Instant,
    ) -> Result<Vec<u8>, WorkspaceError> {
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        {
            let state = self.state()?;
            self.available(&state)?;
        }
        let entry = self
            .inner
            .view_leases
            .entry(token, link, Some(NodeKind::Symlink))?;
        let size = entry.size;
        if size > SYMLINK_TARGET_BYTES as u64 {
            return Err(WorkspaceError::Io);
        }
        let (active_inode, pinned_base) = self.inner.view_leases.with_view(token, |view| {
            Ok((
                view.active
                    .as_ref()
                    .map(|snapshot| snapshot.get(&inode_key(link)))
                    .transpose()?
                    .flatten()
                    .is_some(),
                view.base,
            ))
        })?;
        if active_inode {
            let backing = self
                .inner
                .active
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let bytes = self.inner.view_leases.with_view(token, |view| {
                let snapshot = view.active.as_ref().ok_or(WorkspaceError::Io)?;
                let mut bytes = vec![0; size as usize];
                let count = backing.read_file(
                    link,
                    0,
                    &mut bytes,
                    Some(snapshot),
                    |_, _, _| Err(WorkspaceError::Io),
                    |payload, start, output| {
                        let mut reader = payload.reader(start..start + output.len() as u64)?;
                        let mut done = 0;
                        while done < output.len() {
                            let count = Source::read(
                                &mut reader,
                                &mut output[done..],
                                deadline,
                                &self.inner.stopping,
                            )
                            .map_err(|_| WorkspaceError::Io)?;
                            if count == 0 {
                                return Err(WorkspaceError::Io);
                            }
                            done += count;
                        }
                        Ok(())
                    },
                )?;
                if count != bytes.len() {
                    return Err(WorkspaceError::Io);
                }
                Ok(bytes)
            })?;
            if bytes.len() > SYMLINK_TARGET_BYTES || bytes.contains(&0) {
                return Err(WorkspaceError::InvalidInput);
            }
            Ok(bytes)
        } else if entry.canonical {
            self.readlink_inode_canonical(&mut operation, pinned_base, link, deadline)
        } else {
            Err(WorkspaceError::NotFound)
        }
    }

    /// Read-only custody observation of one held lease.
    pub fn view_status(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
    ) -> Result<ViewLeaseStatus, WorkspaceError> {
        let state = self.state()?;
        self.available(&state)?;
        let (generation, revision, entries) = self.inner.view_leases.observe(token)?;
        Ok(ViewLeaseStatus {
            generation,
            revision,
            entries,
            held_leases: self.inner.view_leases.held()?,
        })
    }

    /// The checked release: the pinned snapshot releases through the
    /// retirement selector exactly once; a failed unlink retains physical
    /// custody, stops the Workspace and is reported, never retried.
    pub fn release_view(
        &self,
        token: &[u8; VIEW_LEASE_BYTES],
    ) -> Result<ViewRelease, WorkspaceError> {
        let state = self.state()?;
        self.available(&state)?;
        match self.inner.view_leases.release(token) {
            Ok(()) => Ok(ViewRelease::Completed),
            // The workspace stopped with the failed unlink cohort in charged
            // custody; the outcome is the report, not a retry.
            Err(WorkspaceError::Busy) => Err(WorkspaceError::Busy),
            Err(error) => Ok(ViewRelease::Retained(error)),
        }
    }

    /// Number of held view leases; zero is close-clean's requirement.
    pub fn held_view_leases(&self) -> Result<usize, WorkspaceError> {
        self.inner.view_leases.held()
    }
}
