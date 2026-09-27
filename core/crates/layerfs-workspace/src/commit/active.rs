//! Lower one pinned active index revision into the existing SaveFile and C5 stream.
use crate::{
    backing::{
        active::{
            dirty_key, inode_key, namespace_key, ActivePackReader, ActiveSnapshot, Extent,
            ExtentKind, HotInode, NamespaceRecord,
        },
        budget::Charge,
    },
    commit::completion::CommitAttempt,
    overlay::snapshot::{Captured, SavedInode, Submission},
    runtime::state::OperationGuard,
    *,
};
use layerfs_bridge::contract::{
    put_directory_identity, put_directory_row, put_rooted_identity, put_stream_tag,
    CommitOutcomeWire, Inspect, Operation, PreparedChanges, PreparedTotals, Response, Root, Source,
    ROLE_DIRECTORY_DECLARATION, ROLE_DIRECTORY_PATCH, ROLE_EXISTING_FILE, ROLE_EXISTING_SYMLINK,
    ROLE_FRESH_FILE, ROLE_FRESH_SYMLINK,
};
use std::{
    collections::BTreeMap,
    io,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

pub(crate) struct ActiveStream {
    bytes: Vec<u8>,
    at: usize,
    _charge: Charge,
}

impl Source for ActiveStream {
    fn read(&mut self, out: &mut [u8], _: Instant, cancel: &AtomicBool) -> io::Result<usize> {
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let take = out.len().min(self.bytes.len() - self.at);
        out[..take].copy_from_slice(&self.bytes[self.at..self.at + take]);
        self.at += take;
        Ok(take)
    }
}

struct ActiveUpload {
    workspace: Workspace,
    view: Arc<ActiveSnapshot>,
    pack_reader: ActivePackReader,
    serial: u64,
    descriptors: Vec<u8>,
    extents: Vec<Extent>,
    descriptor_at: usize,
    extent_at: usize,
    offset: u64,
    replacement: u64,
    emitted: u64,
    failure: Option<WorkspaceError>,
    _charge: Charge,
}

impl ActiveUpload {
    fn new(
        workspace: Workspace,
        view: Arc<ActiveSnapshot>,
        serial: u64,
        extents: Vec<Extent>,
        replacement: u64,
        mut charge: Charge,
    ) -> Result<Self, WorkspaceError> {
        charge.resize(extents.capacity() * std::mem::size_of::<Extent>() + extents.len() * 24)?;
        let mut descriptors = Vec::with_capacity(extents.len() * 24);
        for extent in &extents {
            let kind = match extent.kind {
                ExtentKind::Base => 0u64,
                ExtentKind::Zero => 2,
                ExtentKind::Packed | ExtentKind::Payload => 1,
            };
            descriptors.extend_from_slice(&kind.to_be_bytes());
            descriptors
                .extend_from_slice(&if kind == 0 { extent.source_offset } else { 0 }.to_be_bytes());
            descriptors.extend_from_slice(&(extent.end - extent.start).to_be_bytes());
        }
        charge.resize(
            descriptors.capacity()
                + extents.capacity() * std::mem::size_of::<Extent>()
                + std::mem::size_of::<ActivePackReader>(),
        )?;
        let active = workspace
            .inner
            .active
            .as_ref()
            .cloned()
            .ok_or(WorkspaceError::Unsupported)?;
        let pack_reader = ActivePackReader::new(active, view.clone());
        Ok(Self {
            workspace,
            view,
            pack_reader,
            serial,
            descriptors,
            extents,
            descriptor_at: 0,
            extent_at: 0,
            offset: 0,
            replacement,
            emitted: 0,
            failure: None,
            _charge: charge,
        })
    }

    fn complete(&self) -> bool {
        self.descriptor_at == self.descriptors.len() && self.emitted == self.replacement
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
        if self.descriptor_at < self.descriptors.len() {
            let take = out.len().min(self.descriptors.len() - self.descriptor_at);
            out[..take]
                .copy_from_slice(&self.descriptors[self.descriptor_at..self.descriptor_at + take]);
            self.descriptor_at += take;
            return Ok(take);
        }
        if out.is_empty() || self.emitted == self.replacement {
            return Ok(0);
        }
        while self.extent_at < self.extents.len()
            && self.extents[self.extent_at].kind == ExtentKind::Base
        {
            self.extent_at += 1;
            self.offset = 0;
        }
        let extent = *self.extents.get(self.extent_at).ok_or(WorkspaceError::Io)?;
        let length = extent.end - extent.start;
        let take = (length - self.offset).min(out.len().min(MAX_READ_BYTES) as u64) as usize;
        if take == 0 {
            return Err(WorkspaceError::Io);
        }
        let read = match extent.kind {
            ExtentKind::Zero => {
                out[..take].fill(0);
                take
            }
            ExtentKind::Packed => {
                self.pack_reader.read(
                    self.serial,
                    extent,
                    extent.start + self.offset,
                    &mut out[..take],
                )?;
                take
            }
            ExtentKind::Payload => {
                let backing = self
                    .workspace
                    .inner
                    .active
                    .as_ref()
                    .ok_or(WorkspaceError::Unsupported)?;
                backing.read_file(
                    self.serial,
                    extent.start + self.offset,
                    &mut out[..take],
                    Some(&self.view),
                    |_, _, _| Err(WorkspaceError::Io),
                    |payload, start, output| {
                        let mut reader = payload.reader(start..start + output.len() as u64)?;
                        let mut done = 0;
                        while done < output.len() {
                            let count =
                                Source::read(&mut reader, &mut output[done..], deadline, cancel)
                                    .map_err(|_| WorkspaceError::Io)?;
                            if count == 0 {
                                return Err(WorkspaceError::Io);
                            }
                            done += count;
                        }
                        Ok(())
                    },
                )?
            }
            ExtentKind::Base => return Err(WorkspaceError::Io),
        };
        if read != take {
            return Err(WorkspaceError::Io);
        }
        self.offset += read as u64;
        self.emitted += read as u64;
        if self.offset == length {
            self.extent_at += 1;
            self.offset = 0;
        }
        Ok(read)
    }
}

impl Source for ActiveUpload {
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
                Err(io::Error::other(error))
            }
        }
    }
}

type DirtyRows = Vec<(u64, HotInode)>;

fn scan_dirty(
    workspace: &Workspace,
    captured: &Captured,
    view: &ActiveSnapshot,
) -> Result<(DirtyRows, Charge), WorkspaceError> {
    let mut lower = dirty_key(captured.generation, 0).to_vec();
    let upper = dirty_key(captured.generation + 1, 0);
    let charge = workspace
        .host
        .budget
        .reserve(captured.count * std::mem::size_of::<(u64, HotInode)>())?;
    let mut rows = Vec::with_capacity(captured.count);
    loop {
        let page = view.scan(&lower, &upper, 128)?;
        for (key, value) in page.entries() {
            if key.len() != 17
                || key[..9] != lower[..9]
                || value != &[1]
                || rows.len() == captured.count
            {
                return Err(WorkspaceError::Io);
            }
            let serial = u64::from_be_bytes(key[9..17].try_into().map_err(|_| WorkspaceError::Io)?);
            let inode = HotInode::parse(&view.get(&inode_key(serial))?.ok_or(WorkspaceError::Io)?)?;
            if inode.generation != captured.generation || inode.revision > captured.revision {
                return Err(WorkspaceError::Io);
            }
            rows.push((serial, inode));
        }
        let Some((last, _)) = page.entries().last() else {
            break;
        };
        lower = last.clone();
        lower.push(0);
    }
    if rows.len() != captured.count {
        return Err(WorkspaceError::Io);
    }
    Ok((rows, charge))
}

fn scan_extents(
    workspace: &Workspace,
    view: &ActiveSnapshot,
    serial: u64,
    inode: HotInode,
) -> Result<(Vec<Extent>, Charge), WorkspaceError> {
    let mut charge = workspace.host.budget.reserve(0)?;
    if inode.length == 0 {
        return Ok((Vec::new(), charge));
    }
    let mut extents = Vec::new();
    if inode.storage == 1 {
        charge.resize(4 * std::mem::size_of::<Extent>())?;
        extents.extend(inode.inline.into_iter().flatten());
    } else if inode.storage == 2 {
        let mut lower = Extent::key(serial, 0).to_vec();
        let upper = if serial == u64::MAX {
            vec![b'F']
        } else {
            Extent::key(serial + 1, 0).to_vec()
        };
        loop {
            let page = view.scan(&lower, &upper, 128)?;
            charge
                .resize((extents.len() + page.entries().len()) * std::mem::size_of::<Extent>())?;
            extents
                .try_reserve_exact(page.entries().len())
                .map_err(|_| WorkspaceError::Capacity)?;
            charge.resize(extents.capacity() * std::mem::size_of::<Extent>())?;
            for (key, value) in page.entries() {
                extents.push(Extent::parse(key, value, serial)?);
            }
            let Some((last, _)) = page.entries().last() else {
                break;
            };
            lower = last.clone();
            lower.push(0);
        }
    } else {
        return Err(WorkspaceError::Io);
    }
    let mut end = 0;
    for extent in &extents {
        if extent.start != end {
            return Err(WorkspaceError::Io);
        }
        end = extent.end;
    }
    if end != inode.length {
        return Err(WorkspaceError::Io);
    }
    Ok((extents, charge))
}

type BindingRows = Vec<(Vec<u8>, Option<u64>)>;

fn directory_rows(
    workspace: &Workspace,
    view: &ActiveSnapshot,
    parent: u64,
) -> Result<(BindingRows, Charge), WorkspaceError> {
    let mut lower = [vec![b'N'], parent.to_be_bytes().to_vec()].concat();
    let upper = if parent == u64::MAX {
        vec![b'O']
    } else {
        [vec![b'N'], (parent + 1).to_be_bytes().to_vec()].concat()
    };
    let mut rows = Vec::new();
    let mut charge = workspace.host.budget.reserve(0)?;
    let mut name_bytes = 0usize;
    loop {
        let page = view.scan(&lower, &upper, 128)?;
        for (key, value) in page.entries() {
            if key.len() < 10 || key[..9] != lower[..9] || namespace_key(parent, &key[9..])? != *key
            {
                return Err(WorkspaceError::Io);
            }
            let record = NamespaceRecord::parse(value)?;
            name_bytes = name_bytes
                .checked_add(key.len() - 9)
                .ok_or(WorkspaceError::Capacity)?;
            charge.resize(
                (rows.len() + 1) * std::mem::size_of::<(Vec<u8>, Option<u64>)>() + name_bytes,
            )?;
            rows.try_reserve_exact(1)
                .map_err(|_| WorkspaceError::Capacity)?;
            rows.push((
                key[9..].to_vec(),
                (!record.tombstone).then_some(record.serial),
            ));
            charge.resize(
                rows.capacity() * std::mem::size_of::<(Vec<u8>, Option<u64>)>() + name_bytes,
            )?;
        }
        let Some((last, _)) = page.entries().last() else {
            break;
        };
        lower = last.clone();
        lower.push(0);
    }
    Ok((rows, charge))
}

fn append(
    bytes: &mut Vec<u8>,
    charge: &mut Charge,
    more: usize,
    write: impl FnOnce(&mut Vec<u8>) -> Result<(), WorkspaceError>,
) -> Result<(), WorkspaceError> {
    charge.resize(
        bytes
            .len()
            .checked_add(more)
            .ok_or(WorkspaceError::Capacity)?,
    )?;
    bytes
        .try_reserve_exact(more)
        .map_err(|_| WorkspaceError::Capacity)?;
    write(bytes)?;
    charge.resize(bytes.capacity())?;
    Ok(())
}

pub(super) fn prepare<'a>(
    workspace: &Workspace,
    submission: &'a Submission,
    deadline: Instant,
    first_remote: &mut Option<OperationGuard>,
) -> Result<(PreparedChanges, super::stream::PreparedBody<'a>), WorkspaceError> {
    let captured = submission.capture()?;
    let view = captured.active_view()?;
    let (dirty, _dirty_charge) = scan_dirty(workspace, captured, &view)?;
    let mut saved = BTreeMap::<u64, (Root, Root)>::new();
    let _saved_charge = workspace.host.budget.reserve(captured.count * 96)?;
    for (serial, inode) in &dirty {
        if inode.kind == NodeKind::Directory {
            continue;
        }
        submission.phase(StagePhase::LocalBookkeeping, Some(*serial))?;
        let content = if inode.kind == NodeKind::Symlink {
            submission.phase(StagePhase::FileSave, Some(*serial))?;
            let target = read_symlink(workspace, &view, *serial, *inode, deadline)?;
            let remote = first_remote
                .take()
                .map_or_else(|| workspace.begin(true, deadline), Ok)?;
            let response = workspace.remote_call(
                (workspace.inner.store, captured.generation),
                Operation::ConstructSymlink { target },
                &mut &[][..],
                0,
                &mut io::sink(),
                deadline,
            );
            drop(remote);
            let Response::Saved { root, length, .. } = response? else {
                return Err(WorkspaceError::InvalidInput);
            };
            if length != inode.length {
                return Err(WorkspaceError::InvalidInput);
            }
            submission
                .state
                .lock()
                .map_err(|_| WorkspaceError::Io)?
                .status
                .saved_files += 1;
            root
        } else {
            let (extents, extent_charge) = scan_extents(workspace, &view, *serial, *inode)?;
            let replacement = extents
                .iter()
                .filter(|extent| extent.kind != ExtentKind::Base)
                .try_fold(0u64, |sum, extent| {
                    sum.checked_add(extent.end - extent.start)
                        .ok_or(WorkspaceError::Capacity)
                })?;
            let base_length = if inode.fresh {
                0
            } else {
                let remote = first_remote
                    .take()
                    .map_or_else(|| workspace.begin(true, deadline), Ok)?;
                let response = workspace.remote_call(
                    (workspace.inner.store, captured.generation),
                    Operation::Inspect {
                        root: inode.base,
                        query: Inspect::File,
                    },
                    &mut &[][..],
                    0,
                    &mut io::sink(),
                    deadline,
                );
                drop(remote);
                let Response::File { length, .. } = response? else {
                    return Err(WorkspaceError::InvalidInput);
                };
                length
            };
            if !inode.fresh
                && ((extents.len() == 1
                    && extents[0].kind == ExtentKind::Base
                    && extents[0].source_offset == 0
                    && inode.length == base_length)
                    || (extents.is_empty() && base_length == 0))
            {
                inode.base
            } else {
                submission.phase(StagePhase::FileSave, Some(*serial))?;
                let count = extents.len() as u64;
                let mut upload = ActiveUpload::new(
                    workspace.clone(),
                    view.clone(),
                    *serial,
                    extents,
                    replacement,
                    extent_charge,
                )?;
                let remote = first_remote
                    .take()
                    .map_or_else(|| workspace.begin(true, deadline), Ok)?;
                let response = workspace.remote_call(
                    (workspace.inner.store, captured.generation),
                    Operation::SaveFile {
                        base: (!inode.fresh).then_some(inode.base),
                        base_length,
                        length: inode.length,
                        extents: count,
                        replacement,
                    },
                    &mut upload,
                    0,
                    &mut io::sink(),
                    deadline,
                );
                drop(remote);
                if let Some(error) = upload.failure.take() {
                    submission
                        .state
                        .lock()
                        .map_err(|_| WorkspaceError::Io)?
                        .source_failure = Some(error);
                }
                let Response::Saved { root, length, .. } = response? else {
                    return Err(WorkspaceError::InvalidInput);
                };
                if length != inode.length || !upload.complete() {
                    return Err(WorkspaceError::InvalidInput);
                }
                submission
                    .state
                    .lock()
                    .map_err(|_| WorkspaceError::Io)?
                    .status
                    .saved_files += 1;
                root
            }
        };
        submission.phase(StagePhase::MetadataSave, Some(*serial))?;
        let metadata = save_metadata(workspace, submission, *inode, deadline, first_remote)?;
        saved.insert(*serial, (content, metadata));
        submission
            .state
            .lock()
            .map_err(|_| WorkspaceError::Io)?
            .pending = Some(SavedInode {
            serial: *serial,
            revision: inode.revision,
            length: inode.length,
            content,
            metadata: Some(metadata),
        });
        workspace.persist_saved(submission, deadline)?;
    }
    let mut bytes = Vec::new();
    let mut charge = workspace.host.budget.reserve(0)?;
    append(&mut bytes, &mut charge, 16, |out| {
        put_stream_tag(out).map_err(|_| WorkspaceError::Capacity)
    })?;
    let mut totals = PreparedTotals::default();
    let mut declared = Vec::new();
    for (serial, inode) in &dirty {
        if inode.kind != NodeKind::Directory {
            continue;
        }
        let (changes, _changes_charge) = directory_rows(workspace, &view, *serial)?;
        if changes.is_empty() && !inode.fresh {
            continue;
        }
        let more = 32
            + changes
                .iter()
                .map(|(name, _)| name.len() + 16)
                .sum::<usize>();
        append(&mut bytes, &mut charge, more, |out| {
            put_directory_row(out, *serial, &changes).map_err(|_| WorkspaceError::Capacity)
        })?;
        totals.directories += 1;
        totals.names += changes.len() as u64;
        totals.name_bytes += changes
            .iter()
            .map(|(name, _)| name.len() as u64 + 10)
            .sum::<u64>();
    }
    for (serial, inode) in &dirty {
        if inode.kind == NodeKind::Directory {
            let role = if inode.fresh {
                ROLE_DIRECTORY_DECLARATION
            } else {
                ROLE_DIRECTORY_PATCH
            };
            append(&mut bytes, &mut charge, 64, |out| {
                put_directory_identity(out, role, *serial, inode.mode, inode.seconds, inode.nanos)
                    .map_err(|_| WorkspaceError::Capacity)
            })?;
            if inode.fresh {
                if !captured.declared.contains(serial) {
                    return Err(WorkspaceError::Io);
                }
                declared.push(*serial);
                totals.declarations += 1;
                totals.fresh += 1;
            } else {
                totals.patches += 1;
            }
        } else {
            let (content, metadata) = saved.get(serial).ok_or(WorkspaceError::Io)?;
            let role = match (inode.fresh, inode.kind) {
                (true, NodeKind::File) => ROLE_FRESH_FILE,
                (false, NodeKind::File) => ROLE_EXISTING_FILE,
                (true, NodeKind::Symlink) => ROLE_FRESH_SYMLINK,
                (false, NodeKind::Symlink) => ROLE_EXISTING_SYMLINK,
                _ => return Err(WorkspaceError::Io),
            };
            append(&mut bytes, &mut charge, 80, |out| {
                put_rooted_identity(out, role, *serial, content, metadata)
                    .map_err(|_| WorkspaceError::Capacity)
            })?;
            if inode.fresh {
                totals.fresh += 1;
            }
        }
        totals.identities += 1;
    }
    totals.check().map_err(|_| WorkspaceError::Capacity)?;
    submission
        .state
        .lock()
        .map_err(|_| WorkspaceError::Io)?
        .declared = declared;
    if first_remote.is_none() {
        *first_remote = Some(workspace.begin(true, deadline)?);
    }
    let context = &captured.context;
    let changes = PreparedChanges {
        totals,
        workspace: workspace.inner.incarnation,
        branch: context.branch.branch,
        expected_head: context.branch.head_commit,
        expected_base: context.branch.base_layer,
        generation: captured.generation,
        base: context.effective_root,
        scope: context.scope,
        root_serial: context.root_serial.ok_or(WorkspaceError::Io)?,
    };
    Ok((
        changes,
        super::stream::PreparedBody::Active(ActiveStream {
            bytes,
            at: 0,
            _charge: charge,
        }),
    ))
}

fn read_symlink(
    workspace: &Workspace,
    view: &ActiveSnapshot,
    serial: u64,
    inode: HotInode,
    deadline: Instant,
) -> Result<Vec<u8>, WorkspaceError> {
    let size = usize::try_from(inode.length).map_err(|_| WorkspaceError::Capacity)?;
    if size > layerfs_bridge::contract::SYMLINK_TARGET_BYTES {
        return Err(WorkspaceError::Capacity);
    }
    let mut bytes = vec![0; size];
    let backing = workspace
        .inner
        .active
        .as_ref()
        .ok_or(WorkspaceError::Unsupported)?;
    let read = backing.read_file(
        serial,
        0,
        &mut bytes,
        Some(view),
        |_, _, _| Err(WorkspaceError::Io),
        |payload, start, output| {
            let mut reader = payload.reader(start..start + output.len() as u64)?;
            let mut done = 0;
            while done < output.len() {
                let count = Source::read(
                    &mut reader,
                    &mut output[done..],
                    deadline,
                    &workspace.inner.stopping,
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
    if read != size {
        return Err(WorkspaceError::Io);
    }
    Ok(bytes)
}

fn save_metadata(
    workspace: &Workspace,
    submission: &Submission,
    inode: HotInode,
    deadline: Instant,
    first_remote: &mut Option<OperationGuard>,
) -> Result<Root, WorkspaceError> {
    let kind = if inode.kind == NodeKind::Symlink {
        3
    } else {
        1
    };
    let remote = first_remote
        .take()
        .map_or_else(|| workspace.begin(true, deadline), Ok)?;
    let response = workspace.remote_call(
        (workspace.inner.store, submission.capture()?.generation),
        if inode.fresh {
            Operation::ConstructPortableMetadata {
                kind,
                mode: inode.mode,
                mtime_seconds: inode.seconds,
                mtime_nanoseconds: inode.nanos,
            }
        } else {
            Operation::UpdatePortableMetadata {
                base: inode.metadata,
                kind,
                mode: inode.mode,
                mtime_seconds: inode.seconds,
                mtime_nanoseconds: inode.nanos,
            }
        },
        &mut &[][..],
        0,
        &mut io::sink(),
        deadline,
    );
    drop(remote);
    let response = response?;
    let metadata = match response {
        Response::MetadataConstructed {
            kind: actual,
            mode,
            mtime_seconds,
            mtime_nanoseconds,
            metadata,
            ..
        } if inode.fresh
            && actual == kind
            && mode == inode.mode
            && mtime_seconds == inode.seconds
            && mtime_nanoseconds == inode.nanos =>
        {
            metadata
        }
        Response::MetadataSaved {
            base,
            kind: actual,
            mode,
            mtime_seconds,
            mtime_nanoseconds,
            metadata,
            ..
        } if !inode.fresh
            && base == inode.metadata
            && actual == kind
            && mode == inode.mode
            && mtime_seconds == inode.seconds
            && mtime_nanoseconds == inode.nanos =>
        {
            metadata
        }
        _ => return Err(WorkspaceError::InvalidInput),
    };
    submission
        .state
        .lock()
        .map_err(|_| WorkspaceError::Io)?
        .status
        .saved_metadata += 1;
    Ok(metadata)
}

pub(super) fn reconcile(
    workspace: &Workspace,
    submission: &Submission,
    attempt: &CommitAttempt,
    outcome: &CommitOutcomeWire,
    deadline: Instant,
) -> Result<u64, WorkspaceError> {
    crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
    let captured = submission.capture()?;
    let view = captured.active_view()?;
    let (dirty, _dirty_charge) = scan_dirty(workspace, captured, &view)?;
    drop(view);
    let (head, canonical) = match outcome {
        CommitOutcomeWire::Committed(commit) => (Some(commit.commit), commit.root),
        CommitOutcomeWire::UpToDate { head, root } => (*head, *root),
    };
    let next = match attempt.next.lock().map_err(|_| WorkspaceError::Io)?.take() {
        Some(mut next) => {
            next.snapshot.branch.head_commit = head;
            next.snapshot.head_root = head.map(|_| canonical);
            next.snapshot.effective_root = canonical;
            next
        }
        None => CommitAttempt::successor(workspace, submission, head, canonical)?,
    };
    let next = Arc::new(next);
    let active = workspace
        .inner
        .active
        .as_ref()
        .ok_or(WorkspaceError::Unsupported)?;
    let mut state = workspace.state()?;
    workspace.available(&state)?;
    if state.base != captured.context.effective_root
        || state
            .branch
            .as_ref()
            .is_none_or(|branch| !Arc::ptr_eq(branch, &captured.context))
        || state.generation != captured.generation + 1
        || state
            .submission
            .as_ref()
            .is_none_or(|held| !std::ptr::eq(held.as_ref(), submission))
        || active.generation_revision()? != (state.generation, state.revision)
    {
        return Err(WorkspaceError::Io);
    }
    let revision = state
        .revision
        .checked_add(1)
        .ok_or(WorkspaceError::Capacity)?;
    let baseline = state
        .baseline
        .checked_add(1)
        .ok_or(WorkspaceError::Capacity)?;
    let mut updates = BTreeMap::new();
    let mut node_updates = Vec::new();
    let current_view = active.pin_view()?;
    for (serial, original) in &dirty {
        updates.insert(dirty_key(captured.generation, *serial).to_vec(), None);
        let current = active.get(&inode_key(*serial))?.ok_or(WorkspaceError::Io)?;
        let mut current = HotInode::parse(&current)?;
        if current.kind != original.kind {
            return Err(WorkspaceError::Io);
        }
        if current.revision == original.revision && current.kind == NodeKind::File {
            let (extents, _charge) = scan_extents(workspace, &current_view, *serial, current)?;
            for extent in extents {
                if current.storage == 2 {
                    updates.insert(Extent::key(*serial, extent.start).to_vec(), None);
                }
                if let Some(key) = extent.inverse_key(*serial) {
                    updates.insert(key, None);
                }
            }
            current.storage = u8::from(current.length > 0);
            current.inline = if current.length > 0 {
                [Some(Extent::base(0, current.length)), None, None, None]
            } else {
                [None; 4]
            };
        }
        if original.kind != NodeKind::Directory {
            let host = workspace
                .host
                .metadata
                .as_ref()
                .ok_or(WorkspaceError::Unsupported)?;
            let mut lease = host.payloads.window(1, 3)?;
            let cell = captured
                .root
                .arena
                .find(
                    submission.result_ref()?,
                    &crate::backing::metadata_pages::result_key(*serial),
                    lease.window.as_mut().ok_or(WorkspaceError::Io)?,
                    deadline,
                )?
                .ok_or(WorkspaceError::Io)?;
            let saved = cell.value();
            if saved.len() != 80
                || u64::from_be_bytes(saved[..8].try_into().map_err(|_| WorkspaceError::Io)?)
                    != original.revision
                || u64::from_be_bytes(saved[8..16].try_into().map_err(|_| WorkspaceError::Io)?)
                    != original.length
            {
                return Err(WorkspaceError::Io);
            }
            let content: Root = saved[16..48].try_into().map_err(|_| WorkspaceError::Io)?;
            let metadata: Root = saved[48..80].try_into().map_err(|_| WorkspaceError::Io)?;
            current.base = content;
            current.metadata = metadata;
            node_updates.push((*serial, *original, content, metadata));
        } else {
            node_updates.push((*serial, *original, [0; 32], [0; 32]));
        }
        current.fresh = false;
        current.generation = state.generation;
        current.revision = revision;
        updates.insert(inode_key(*serial).to_vec(), Some(current.value()?.to_vec()));
    }
    drop(current_view);
    let mut status = attempt.status.lock().map_err(|_| WorkspaceError::Io)?;
    let publication = active.publish_reconcile(&updates.into_iter().collect::<Vec<_>>())?;
    let published = publication.revision;
    if published != revision {
        return Err(WorkspaceError::Io);
    }
    let old_branch = state.branch.replace(next);
    state.base = canonical;
    state.baseline = baseline;
    state.revision = revision;
    for (serial, original, content, metadata) in node_updates {
        if let Ok(node) = state.node_mut(serial) {
            node.original.size = original.length;
            node.original.mode = original.mode;
            node.original.mtime_seconds = original.seconds;
            node.original.mtime_nanoseconds = original.nanos;
            node.original.references = u64::from(original.links);
            if original.kind != NodeKind::Directory {
                node.content = content;
                node.metadata = metadata;
            }
            node.baseline = baseline;
        }
    }
    let accepted = &submission
        .state
        .lock()
        .map_err(|_| WorkspaceError::Io)?
        .declared;
    state.declared.retain(|serial| !accepted.contains(serial));
    status.installed_revision = Some(revision);
    drop(state);
    drop(status);
    drop(old_branch);
    captured.release_active()?;
    active.maintain_until(deadline)?;
    if let Some(error) = publication.cleanup_error {
        return Err(error);
    }
    Ok(revision)
}
