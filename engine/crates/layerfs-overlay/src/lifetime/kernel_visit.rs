//! One owner visit of a kernel request. The job is the request's whole
//! window in the owner, so it records no request source and leaves nothing
//! to release: install, revoke, close and reclamation are owner jobs too and
//! cannot run inside it. A visit that cannot decide changes nothing; its
//! request holds nothing until it visits again.
use crate::namespace::job_rows::{JobRows, LocalRow};
use crate::{
    db::integer, inode, BaseSource, Changes, InodeKind, KernelApplied, KernelDecision,
    KernelEffect, KernelMount, KernelObservation, LocalRead, OpenFile, Overlay, OverlayError,
    OverlayResult, SourceRows, WorkspaceState,
};

/// The kernel's own reference on the inode a kernel request names.
#[derive(Clone, Copy)]
pub(crate) enum Held {
    /// A lookup count on the inode.
    Lookup,
    /// An open regular-file descriptor of the inode.
    File(u64),
    /// An open file or directory descriptor of the inode.
    Handle(u64),
    /// An open directory descriptor of the inode.
    Directory(u64),
}
/// The descriptor a deciding visit records for the kernel request that
/// receives it.
#[derive(Clone, Copy)]
enum Opened {
    File { request: u64, writable: bool },
    Directory { request: u64 },
}
impl Overlay {
    /// The current base as this job's own source. It names no row and is
    /// accepted only while the Workspace still has this base and install
    /// frontier, which holds for the whole job that made it.
    pub(crate) fn visit_source(&self, state: WorkspaceState) -> OverlayResult<BaseSource> {
        Ok(BaseSource {
            engine: self.identity,
            owner: self.mint_owner()?,
            class: 3,
            root: state.base_root,
            installed: state.installed,
        })
    }
    /// The fence of one visit in one statement: the Workspace row, this
    /// mount attached and not revoked, and the kernel's own reference on the
    /// inode the request names. A missing or foreign Workspace, mount or
    /// reference is Stale; `live` refuses a closed Workspace before the mount
    /// is considered. The flag is the access mode of a held file descriptor.
    pub(crate) fn kernel_fence(
        &self,
        mount: KernelMount,
        serial: u64,
        held: Held,
        live: bool,
    ) -> OverlayResult<(WorkspaceState, bool)> {
        let (state, held, _) = self.fence_row(mount, serial, held, live, false)?;
        Ok((state, held != 0))
    }
    /// The same fence for a visit that decides over the inode it names. Its
    /// statement also read that inode's latest local row of the view, which
    /// starts the job's row memory: the decision and its publication do not
    /// read the row again. While the engine holds an orphan, an inode read
    /// asks the orphan domain first, so nothing is kept then and the row is
    /// read where it is asked for, as before.
    fn visit_fence(
        &self,
        mount: KernelMount,
        serial: u64,
        held: Held,
    ) -> OverlayResult<(WorkspaceState, bool, JobRows)> {
        let (state, held, row) = self.fence_row(mount, serial, held, true, true)?;
        let seen = JobRows::new();
        if !self.orphan_seen.get() {
            seen.record(integer(serial)?, &row);
        }
        Ok((state, held != 0, seen))
    }
    /// The same fence on an open directory descriptor, and its floor: the
    /// first cookie that is still an offset of the handle.
    pub(crate) fn directory_fence(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: u64,
        live: bool,
    ) -> OverlayResult<(WorkspaceState, u64)> {
        let (state, floor, _) =
            self.fence_row(mount, serial, Held::Directory(handle), live, false)?;
        let floor = u64::try_from(floor).map_err(|_| OverlayError::Invalid("directory floor"))?;
        Ok((state, floor))
    }
    /// `row` selects the statement that also returns the named inode's
    /// latest local row.
    fn fence_row(
        &self,
        mount: KernelMount,
        serial: u64,
        held: Held,
        live: bool,
        row: bool,
    ) -> OverlayResult<(WorkspaceState, i64, Option<LocalRow>)> {
        self.available()?;
        let key = integer(serial)?;
        let state = self.state()?;
        let local = if row {
            self.local_row(key, state.active, state.installed)?
        } else {
            None
        };
        if live && state.closed {
            return Err(OverlayError::Closed);
        }
        let open = self.open.borrow();
        if open.mount != Some((mount, false)) {
            return Err(OverlayError::Stale);
        }
        let file = |handle| {
            open.files
                .get(handle)
                .filter(|(file, (held, _))| *held == mount.owner && file.serial == serial)
        };
        let directory = |handle| {
            open.directories
                .get(handle)
                .map(|(held, _)| held)
                .filter(|held| held.directory.mount == mount && held.directory.serial == serial)
        };
        let value = match held {
            Held::Lookup => open.kernel.rows.get(&(mount.owner, serial)).map(|_| 1),
            Held::File(handle) => file(handle).map(|(file, _)| i64::from(file.writable)),
            Held::Handle(handle) => {
                if file(handle).is_some() || directory(handle).is_some() {
                    Some(1)
                } else {
                    None
                }
            }
            Held::Directory(handle) => directory(handle).map(|held| held.floor as i64),
        }
        .ok_or(OverlayError::Stale)?;
        Ok((state, value, local))
    }

    /// LOOKUP or GETATTR in one visit. The callback is one bounded semantic
    /// decision over current rows and base facts it holds or reads from
    /// memory; it performs no provider I/O. A decision that still needs a
    /// base fact changes nothing here. A positive LOOKUP takes its kernel
    /// lookup reference in the same transaction as the answer.
    pub fn observe_kernel_visit<T>(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: Option<u64>,
        lookup: bool,
        decide: impl FnOnce(SourceRows<'_>, BaseSource) -> OverlayResult<KernelDecision<T>>,
    ) -> KernelObservation<T> {
        self.observe_visit(mount, serial, handle, lookup, None, decide)
    }
    /// OPEN of a regular file in one visit, under the kernel's lookup
    /// reference on the inode. The same bounded decision; when it finishes
    /// with the file, its descriptor, which names the request that
    /// receives it, is written in the deciding transaction. Nothing
    /// else is recorded: no request source and no processing read, so the
    /// reply is followed by no release. A file whose last name is gone stays
    /// openable under the kernel's reference.
    pub fn open_kernel_visit<T>(
        &self,
        mount: KernelMount,
        request: u64,
        serial: u64,
        writable: bool,
        decide: impl FnOnce(SourceRows<'_>, BaseSource) -> OverlayResult<KernelDecision<T>>,
    ) -> KernelObservation<T> {
        let opened = Opened::File { request, writable };
        self.observe_visit(mount, serial, None, false, Some(opened), decide)
    }
    /// OPENDIR in one visit, under the kernel's lookup reference on the
    /// inode. When the same bounded decision finishes with the directory,
    /// its descriptor (the handle row, its lease, the open count) is written
    /// in the deciding transaction for the request that receives it. No
    /// request source and no processing read are recorded, so the reply is
    /// followed by no release.
    pub fn opendir_kernel_visit<T>(
        &self,
        mount: KernelMount,
        request: u64,
        serial: u64,
        decide: impl FnOnce(SourceRows<'_>, BaseSource) -> OverlayResult<KernelDecision<T>>,
    ) -> KernelObservation<T> {
        let opened = Opened::Directory { request };
        self.observe_visit(mount, serial, None, false, Some(opened), decide)
    }
    fn observe_visit<T>(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: Option<u64>,
        lookup: bool,
        open: Option<Opened>,
        decide: impl FnOnce(SourceRows<'_>, BaseSource) -> OverlayResult<KernelDecision<T>>,
    ) -> KernelObservation<T> {
        let mut decision = None;
        let mut open_candidate = None;
        let mut directory_candidate = None;
        let result = self
            .atomic(|| {
                let held = handle.map_or(Held::Lookup, Held::Handle);
                // The job's rounds read each inode row once, and the row
                // of the inode it names with its fence.
                let (state, _, seen) = self.visit_fence(mount, serial, held)?;
                let source = self.visit_source(state)?;
                let rows = self.source_rows_at(source, state, Some(&seen));
                let inode = match decide(rows, source)? {
                    KernelDecision::Needs(value) => {
                        decision = Some(value);
                        return Ok(());
                    }
                    KernelDecision::Finished { inode, value } => {
                        decision = Some(value);
                        inode
                    }
                };
                let Some(inode) = inode else { return Ok(()) };
                inode::check(&inode)?;
                if lookup {
                    if inode.nlink == 0 && inode.serial != mount.root {
                        return Err(OverlayError::Missing);
                    }
                    self.add_kernel_lookup(mount, inode.serial)?;
                    if inode.kind == InodeKind::Directory {
                        self.set_kernel_parent(mount, inode.serial, serial)?;
                    }
                } else if inode.serial != serial {
                    return Err(OverlayError::Invalid("kernel observation serial"));
                }
                let (request, writable) = match open {
                    None => return Ok(()),
                    Some(Opened::File { request, writable }) => (request, writable),
                    Some(Opened::Directory { request }) => {
                        if inode.kind != InodeKind::Directory
                            || (inode.nlink == 0 && inode.serial != mount.root)
                        {
                            return Err(OverlayError::Missing);
                        }
                        let request = request.to_be_bytes();
                        directory_candidate =
                            Some(self.retain_kernel_directory(mount, &request, serial)?);
                        return Ok(());
                    }
                };
                if inode.kind != InodeKind::File {
                    return Err(OverlayError::Missing);
                }
                // The descriptor's row names the mount and the kernel
                // request that receives it.
                let key = super::kernel_file::kernel_request(request);
                open_candidate =
                    Some(self.retain_file(Some(mount.owner), key, serial, writable)?);
                Ok(())
            })
            .map(|()| None);
        KernelObservation {
            decision,
            result,
            open_candidate,
            directory_candidate,
        }
    }
    /// The local part of one READ window, through its descriptor, or of one
    /// READLINK window, under the kernel's lookup reference, and the base
    /// root its inherited bytes belong to. The job only reads: it records no
    /// request, source or reader, so its request has nothing to release and
    /// holds back no install, revoke or close. An unlinked file is read
    /// through its descriptor from the layers and root its orphan retains.
    pub fn read_kernel_visit(
        &self,
        mount: KernelMount,
        serial: u64,
        handle: Option<u64>,
        offset: u64,
        length: u32,
    ) -> OverlayResult<([u8; 32], Option<LocalRead>)> {
        let held = handle.map_or(Held::Lookup, Held::Handle);
        let (state, _) = self.kernel_fence(mount, serial, held, true)?;
        let key = integer(serial)?;
        // No orphan row exists while this engine holds none.
        if self.orphan_seen.get() {
            if let Some(orphan) = self.orphan(key)? {
                // Without a descriptor a removed regular file is gone.
                if handle.is_none() && self.orphan_layer(key)?.kind == InodeKind::File {
                    return Err(OverlayError::Missing);
                }
                let local = self.orphan_read(key, orphan, offset, length)?;
                return Ok((orphan.root, local));
            }
        }
        let local = self.read_layers(serial, state.active.0, state.installed, offset, length)?;
        Ok((state.base_root, local))
    }
    /// One kernel mutation in one visit. The callback decides over current
    /// rows of the Workspace row the fence read; when it returns changes
    /// they are published with the kernel custody their reply hands over, in
    /// one transaction, against that same row. When it returns none, nothing
    /// is written. A handle-addressed mutation is given its exact open
    /// descriptor, which the fence read: it is not read again.
    pub fn mutate_kernel_visit(
        &self,
        mount: KernelMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
        decide: impl FnOnce(
            SourceRows<'_>,
            Option<OpenFile>,
        ) -> OverlayResult<Option<(Changes, KernelEffect)>>,
    ) -> OverlayResult<Option<KernelApplied>> {
        self.atomic(|| {
            let held = handle.map_or(Held::Lookup, Held::File);
            // The rows the decision reads are the rows it publishes over;
            // the fence read the row of the inode the request names.
            let (state, writable, seen) = self.visit_fence(mount, serial, held)?;
            let file = handle.map(|owner| OpenFile {
                engine: self.identity,
                owner,
                serial,
                writable,
            });
            let source = self.visit_source(state)?;
            let rows = self.source_rows_at(source, state, Some(&seen));
            let Some((changes, effect)) = decide(rows, file)? else {
                return Ok(None);
            };
            let checked = self.check_changes(&changes)?;
            let publication =
                self.apply_checked(source, state, file, Some(&seen), &changes, &checked)?;
            self.kernel_effect(mount, request.to_be_bytes(), &changes, effect, publication)
                .map(Some)
        })
    }
}
