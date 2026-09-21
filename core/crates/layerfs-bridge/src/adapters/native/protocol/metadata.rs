//! Checked binary metadata; every vector count is bounded before reservation.
use crate::contract::*;

pub struct Decoder<'a> {
    bytes: &'a [u8],
}
impl<'a> Decoder<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, Failure> {
        if bytes.len() > METADATA_BYTES {
            return Err(Code::Capacity.into());
        }
        Ok(Self { bytes })
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], Failure> {
        if n > self.bytes.len() {
            return Err(Code::InvalidInput.into());
        }
        let (v, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(v)
    }
    pub fn u8(&mut self) -> Result<u8, Failure> {
        Ok(self.take(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16, Failure> {
        Ok(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| Code::InvalidInput)?,
        ))
    }
    pub fn u32(&mut self) -> Result<u32, Failure> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| Code::InvalidInput)?,
        ))
    }
    pub fn u64(&mut self) -> Result<u64, Failure> {
        Ok(u64::from_be_bytes(
            self.take(8)?.try_into().map_err(|_| Code::InvalidInput)?,
        ))
    }
    pub fn root(&mut self) -> Result<Root, Failure> {
        self.take(32)?
            .try_into()
            .map_err(|_| Code::InvalidInput.into())
    }
    pub fn blob(&mut self, max: usize) -> Result<Vec<u8>, Failure> {
        let n = self.u16()? as usize;
        if n > max {
            return Err(Code::Capacity.into());
        }
        Ok(self.take(n)?.to_vec())
    }
    pub fn count(&mut self, max: usize, min_width: usize) -> Result<usize, Failure> {
        let n = self.u16()? as usize;
        if n > max || n.checked_mul(min_width).ok_or(Code::Capacity)? > self.bytes.len() {
            return Err(Code::Capacity.into());
        }
        Ok(n)
    }
    pub fn finish(self) -> Result<(), Failure> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(Code::InvalidInput.into())
        }
    }
}
pub struct Encoder {
    bytes: Vec<u8>,
    limit: usize,
}
impl Default for Encoder {
    fn default() -> Self {
        Self::bounded(METADATA_BYTES)
    }
}
impl Encoder {
    pub fn bounded(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
    pub fn put(&mut self, b: &[u8]) -> Result<(), Failure> {
        if self
            .bytes
            .len()
            .checked_add(b.len())
            .ok_or(Code::Capacity)?
            > self.limit
        {
            return Err(Code::Capacity.into());
        }
        self.bytes.extend_from_slice(b);
        Ok(())
    }
    pub fn u8(&mut self, n: u8) -> Result<(), Failure> {
        self.put(&[n])
    }
    pub fn u16(&mut self, n: u16) -> Result<(), Failure> {
        self.put(&n.to_be_bytes())
    }
    pub fn u32(&mut self, n: u32) -> Result<(), Failure> {
        self.put(&n.to_be_bytes())
    }
    pub fn u64(&mut self, n: u64) -> Result<(), Failure> {
        self.put(&n.to_be_bytes())
    }
    pub fn count(&mut self, n: usize) -> Result<(), Failure> {
        self.u16(u16::try_from(n).map_err(|_| Code::Capacity)?)
    }
    pub fn blob(&mut self, b: &[u8]) -> Result<(), Failure> {
        self.count(b.len())?;
        self.put(b)
    }
    pub fn finish(self) -> Vec<u8> {
        self.bytes
    }
}
pub fn encode_request(r: &Request) -> Result<Vec<u8>, Failure> {
    encode_request_with_budget(r, r.deadline_ms)
}
pub fn encode_request_with_budget(r: &Request, remaining_ms: u32) -> Result<Vec<u8>, Failure> {
    r.validate()?;
    if remaining_ms == 0 || remaining_ms > r.deadline_ms {
        return Err(Code::InvalidInput.into());
    }
    let mut e = match r.operation {
        Operation::WorkspaceStatus { .. } => Encoder::bounded(WORKSPACE_STATUS_REQUEST_BYTES),
        Operation::WorkspaceUnmount { .. } => Encoder::bounded(WORKSPACE_UNMOUNT_REQUEST_BYTES),
        Operation::WorkspaceMount { .. } => Encoder::bounded(WORKSPACE_MOUNT_REQUEST_BYTES),
        Operation::WorkspaceAttach { .. } => Encoder::bounded(WORKSPACE_ATTACH_REQUEST_BYTES),
        Operation::WorkspaceCloseClean { .. } => {
            Encoder::bounded(WORKSPACE_CLOSE_CLEAN_REQUEST_BYTES)
        }
        Operation::UpdatePortableMetadata { .. } => {
            Encoder::bounded(PORTABLE_METADATA_REQUEST_BYTES)
        }
        _ => Encoder::default(),
    };
    e.u64(r.generation)?;
    e.u32(r.store)?;
    e.u16(r.profile)?;
    e.u32(remaining_ms)?;
    e.u64(r.response_bytes)?;
    e.u8(r.operation.opcode())?;
    match &r.operation {
        Operation::ReadFile { root, start, end } => {
            e.put(root)?;
            e.u64(*start)?;
            e.u64(*end)?;
        }
        Operation::ConstructFile { length } => e.u64(*length)?,
        Operation::Inspect { root, query } => {
            e.put(root)?;
            match query {
                Inspect::File => e.u8(0)?,
                Inspect::Stat { path } => {
                    e.u8(1)?;
                    e.blob(path)?;
                }
                Inspect::List {
                    path,
                    after,
                    entries,
                    bytes,
                } => {
                    e.u8(2)?;
                    e.blob(path)?;
                    e.blob(after)?;
                    e.u16(*entries)?;
                    e.u32(*bytes)?;
                }
                Inspect::Readlink { path } => {
                    e.u8(3)?;
                    e.blob(path)?;
                }
                Inspect::Attributes { path } => {
                    e.u8(4)?;
                    e.blob(path)?;
                }
            }
        }
        Operation::EditFile {
            root,
            base_length,
            edits,
        } => {
            e.put(root)?;
            e.u64(*base_length)?;
            e.count(edits.len())?;
            for v in edits {
                e.u64(v.start)?;
                e.u64(v.end)?;
                e.u64(v.replacement)?;
            }
        }
        Operation::UpdatePreparedFilesystem {
            base,
            scope,
            root_serial,
            directories,
            inodes,
        } => {
            e.put(base)?;
            e.put(scope)?;
            e.u64(*root_serial)?;
            put_directories(&mut e, directories)?;
            put_inodes(&mut e, inodes)?;
        }
        Operation::HistoryQuery(query) => put_query(&mut e, query)?,
        Operation::HistoryCommand(command) => put_command(&mut e, command)?,
        Operation::WorkspaceStatus {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceUnmount {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceCloseClean {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceMount {
            workspace,
            incarnation,
        }
        | Operation::WorkspaceAttach {
            workspace,
            incarnation,
        } => {
            e.blob(workspace)?;
            e.put(incarnation)?;
        }
        Operation::UpdatePortableMetadata {
            base,
            kind,
            mode,
            mtime_seconds,
            mtime_nanoseconds,
        } => {
            e.put(base)?;
            e.u8(*kind)?;
            e.u32(*mode)?;
            e.u64(*mtime_seconds as u64)?;
            e.u32(*mtime_nanoseconds)?;
        }
    }
    Ok(e.finish())
}

/// Writes the final directory bindings of one prepared filesystem update.
fn put_directories(e: &mut Encoder, directories: &[DirectoryChange]) -> Result<(), Failure> {
    e.count(directories.len())?;
    for d in directories {
        e.u64(d.parent)?;
        e.count(d.changes.len())?;
        for (name, serial) in &d.changes {
            e.blob(name)?;
            e.u64(serial.unwrap_or(0))?;
        }
    }
    Ok(())
}

/// Reads the final directory bindings of one prepared filesystem update.
fn take_directories(d: &mut Decoder<'_>) -> Result<Vec<DirectoryChange>, Failure> {
    let count = d.count(128, 10)?;
    let mut directories = Vec::with_capacity(count);
    let mut total = 0;
    for _ in 0..count {
        let parent = d.u64()?;
        let n = d.count(128 - total, 10)?;
        total += n;
        let mut changes = Vec::with_capacity(n);
        for _ in 0..n {
            let name = d.blob(255)?;
            let serial = d.u64()?;
            changes.push((name, (serial != 0).then_some(serial)));
        }
        directories.push(DirectoryChange { parent, changes });
    }
    Ok(directories)
}

/// Writes the typed final inode values of one prepared filesystem update.
fn put_inodes(e: &mut Encoder, inodes: &[InodeChange]) -> Result<(), Failure> {
    e.count(inodes.len())?;
    for i in inodes {
        e.u64(i.serial)?;
        e.u8(i.kind)?;
        e.put(&i.content)?;
        e.put(&i.metadata)?;
    }
    Ok(())
}

/// Reads the typed final inode values of one prepared filesystem update.
fn take_inodes(d: &mut Decoder<'_>) -> Result<Vec<InodeChange>, Failure> {
    let n = d.count(128, 73)?;
    let mut inodes = Vec::with_capacity(n);
    for _ in 0..n {
        inodes.push(InodeChange {
            serial: d.u64()?,
            kind: d.u8()?,
            content: d.root()?,
            metadata: d.root()?,
        });
    }
    Ok(inodes)
}

/// Reads one fixed-width identity of `N` bytes.
fn take_array<const N: usize>(d: &mut Decoder<'_>) -> Result<[u8; N], Failure> {
    d.take(N)?.try_into().map_err(|_| Code::InvalidInput.into())
}

/// Writes an optional fixed-width identity.
pub(super) fn put_optional<const N: usize>(
    e: &mut Encoder,
    value: Option<&[u8; N]>,
) -> Result<(), Failure> {
    match value {
        Some(value) => {
            e.u8(1)?;
            e.put(value)
        }
        None => e.u8(0),
    }
}

/// Reads an optional fixed-width identity.
pub(super) fn take_optional<const N: usize>(
    d: &mut Decoder<'_>,
) -> Result<Option<[u8; N]>, Failure> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(take_array::<N>(d)?)),
        _ => Err(Code::InvalidInput.into()),
    }
}

/// Writes one page request: continuation then limit.
fn put_page(e: &mut Encoder, cursor: &[u8], limit: u16) -> Result<(), Failure> {
    e.blob(cursor)?;
    e.u16(limit)
}

fn take_page(d: &mut Decoder<'_>) -> Result<(Vec<u8>, u16), Failure> {
    let cursor = d.blob(CURSOR_BYTES)?;
    Ok((cursor, d.u16()?))
}

/// Writes the prepared filesystem update history carries.
fn put_prepared(e: &mut Encoder, changes: &PreparedChanges) -> Result<(), Failure> {
    e.put(&changes.workspace)?;
    e.put(&changes.branch)?;
    put_optional(e, changes.expected_head.as_ref())?;
    e.put(&changes.expected_base)?;
    e.u64(changes.generation)?;
    e.put(&changes.base)?;
    e.put(&changes.scope)?;
    e.u64(changes.root_serial)?;
    put_directories(e, &changes.directories)?;
    put_inodes(e, &changes.inodes)
}

fn take_prepared(d: &mut Decoder<'_>) -> Result<PreparedChanges, Failure> {
    Ok(PreparedChanges {
        workspace: take_array::<32>(d)?,
        branch: take_array::<17>(d)?,
        expected_head: take_optional::<33>(d)?,
        expected_base: take_array::<33>(d)?,
        generation: d.u64()?,
        base: d.root()?,
        scope: d.root()?,
        root_serial: d.u64()?,
        directories: take_directories(d)?,
        inodes: take_inodes(d)?,
    })
}

/// Writes one pathless manifest.
fn put_manifest(e: &mut Encoder, entries: &[ManifestEntry]) -> Result<(), Failure> {
    e.count(entries.len())?;
    for entry in entries {
        e.u16(entry.parent)?;
        e.blob(&entry.name)?;
        e.u8(entry.kind)?;
        e.u32(entry.mode)?;
        e.u64(entry.mtime_seconds as u64)?;
        e.u32(entry.mtime_nanoseconds)?;
        put_optional(e, entry.content.as_ref())?;
        e.blob(&entry.target)?;
    }
    Ok(())
}

/// Smallest encoded width of one manifest entry.
///
/// Parent, empty name, kind, mode, seconds, nanoseconds, an absent content flag
/// and an empty target. The pre-check uses this and not a typical width: a
/// manifest of small entries is legal and must not be refused before it is read.
const MANIFEST_ENTRY_MINIMUM: usize = 24;

fn take_manifest(d: &mut Decoder<'_>) -> Result<Vec<ManifestEntry>, Failure> {
    let count = d.count(MANIFEST_ENTRIES, MANIFEST_ENTRY_MINIMUM)?;
    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        entries.push(ManifestEntry {
            parent: d.u16()?,
            name: d.blob(255)?,
            kind: d.u8()?,
            mode: d.u32()?,
            mtime_seconds: d.u64()? as i64,
            mtime_nanoseconds: d.u32()?,
            content: take_optional::<32>(d)?,
            target: d.blob(MANIFEST_TARGET_BYTES)?,
        });
    }
    Ok(entries)
}

/// Writes one read-only history query.
fn put_query(e: &mut Encoder, query: &HistoryQuery) -> Result<(), Failure> {
    match query {
        HistoryQuery::GetStack { stack } => {
            e.u8(1)?;
            e.put(stack)?;
        }
        HistoryQuery::ListStacks { cursor, limit } => {
            e.u8(2)?;
            put_page(e, cursor, *limit)?;
        }
        HistoryQuery::GetBranch { branch } => {
            e.u8(3)?;
            e.put(branch)?;
        }
        HistoryQuery::ListBranches {
            stack,
            cursor,
            limit,
        } => {
            e.u8(4)?;
            e.put(stack)?;
            put_page(e, cursor, *limit)?;
        }
        HistoryQuery::GetCommit { commit } => {
            e.u8(5)?;
            e.put(commit)?;
        }
        HistoryQuery::CommitHistory {
            branch,
            start,
            cursor,
            limit,
        } => {
            e.u8(6)?;
            e.put(branch)?;
            put_optional(e, start.as_ref())?;
            put_page(e, cursor, *limit)?;
        }
        HistoryQuery::GetLayer { layer } => {
            e.u8(7)?;
            e.put(layer)?;
        }
        HistoryQuery::LayerHistory {
            stack,
            start,
            cursor,
            limit,
        } => {
            e.u8(8)?;
            e.put(stack)?;
            put_optional(e, start.as_ref())?;
            put_page(e, cursor, *limit)?;
        }
        HistoryQuery::GetStage { workspace } => {
            e.u8(9)?;
            e.put(workspace)?;
        }
        HistoryQuery::ListStages {
            branch,
            cursor,
            limit,
        } => {
            e.u8(10)?;
            e.put(branch)?;
            put_page(e, cursor, *limit)?;
        }
    }
    Ok(())
}

fn take_query(d: &mut Decoder<'_>) -> Result<HistoryQuery, Failure> {
    Ok(match d.u8()? {
        1 => HistoryQuery::GetStack {
            stack: take_array::<17>(d)?,
        },
        2 => {
            let (cursor, limit) = take_page(d)?;
            HistoryQuery::ListStacks { cursor, limit }
        }
        3 => HistoryQuery::GetBranch {
            branch: take_array::<17>(d)?,
        },
        4 => {
            let stack = take_array::<17>(d)?;
            let (cursor, limit) = take_page(d)?;
            HistoryQuery::ListBranches {
                stack,
                cursor,
                limit,
            }
        }
        5 => HistoryQuery::GetCommit {
            commit: take_array::<33>(d)?,
        },
        6 => {
            let branch = take_array::<17>(d)?;
            let start = take_optional::<33>(d)?;
            let (cursor, limit) = take_page(d)?;
            HistoryQuery::CommitHistory {
                branch,
                start,
                cursor,
                limit,
            }
        }
        7 => HistoryQuery::GetLayer {
            layer: take_array::<33>(d)?,
        },
        8 => {
            let stack = take_array::<17>(d)?;
            let start = take_optional::<33>(d)?;
            let (cursor, limit) = take_page(d)?;
            HistoryQuery::LayerHistory {
                stack,
                start,
                cursor,
                limit,
            }
        }
        9 => HistoryQuery::GetStage {
            workspace: take_array::<32>(d)?,
        },
        10 => {
            let branch = take_array::<17>(d)?;
            let (cursor, limit) = take_page(d)?;
            HistoryQuery::ListStages {
                branch,
                cursor,
                limit,
            }
        }
        _ => return Err(Code::Unsupported.into()),
    })
}

/// Writes one mutating history command.
fn put_command(e: &mut Encoder, command: &HistoryCommand) -> Result<(), Failure> {
    match command {
        HistoryCommand::InitLayerStack {
            stack,
            name,
            scope_seed,
            manifest,
        } => {
            e.u8(1)?;
            e.put(stack)?;
            e.blob(name)?;
            e.put(scope_seed)?;
            put_manifest(e, manifest)?;
        }
        HistoryCommand::Fork {
            stack,
            branch,
            name,
            source,
        } => {
            e.u8(2)?;
            e.put(stack)?;
            e.put(branch)?;
            e.blob(name)?;
            match source {
                HistoryForkSource::Layer(layer) => {
                    e.u8(1)?;
                    e.put(layer)?;
                }
                HistoryForkSource::Commit { branch, commit } => {
                    e.u8(2)?;
                    e.put(branch)?;
                    e.put(commit)?;
                }
            }
        }
        HistoryCommand::StageChanges(changes) => {
            e.u8(3)?;
            put_prepared(e, changes)?;
        }
        HistoryCommand::CommitStaged { workspace, token } => {
            e.u8(4)?;
            e.put(workspace)?;
            e.u64(*token)?;
        }
        HistoryCommand::Commit(changes) => {
            e.u8(5)?;
            put_prepared(e, changes)?;
        }
        HistoryCommand::AddLayer {
            stack,
            branch,
            commit,
            expected_stack_head,
            expected_branch_base,
        } => {
            e.u8(6)?;
            e.put(stack)?;
            e.put(branch)?;
            e.put(commit)?;
            e.put(expected_stack_head)?;
            e.put(expected_branch_base)?;
        }
        HistoryCommand::DiscardStage { workspace, token } => {
            e.u8(7)?;
            e.put(workspace)?;
            e.u64(*token)?;
        }
        HistoryCommand::ReserveInodes { scope, count } => {
            e.u8(8)?;
            e.put(scope)?;
            e.u64(*count)?;
        }
    }
    Ok(())
}

fn take_command(d: &mut Decoder<'_>) -> Result<HistoryCommand, Failure> {
    Ok(match d.u8()? {
        1 => HistoryCommand::InitLayerStack {
            stack: take_array::<16>(d)?,
            name: d.blob(NAME_MAX_BYTES)?,
            scope_seed: d.root()?,
            manifest: take_manifest(d)?,
        },
        2 => {
            let stack = take_array::<17>(d)?;
            let branch = take_array::<16>(d)?;
            let name = d.blob(NAME_MAX_BYTES)?;
            let source = match d.u8()? {
                1 => HistoryForkSource::Layer(take_array::<33>(d)?),
                2 => HistoryForkSource::Commit {
                    branch: take_array::<17>(d)?,
                    commit: take_array::<33>(d)?,
                },
                _ => return Err(Code::Unsupported.into()),
            };
            HistoryCommand::Fork {
                stack,
                branch,
                name,
                source,
            }
        }
        3 => HistoryCommand::StageChanges(take_prepared(d)?),
        4 => HistoryCommand::CommitStaged {
            workspace: take_array::<32>(d)?,
            token: d.u64()?,
        },
        5 => HistoryCommand::Commit(take_prepared(d)?),
        6 => HistoryCommand::AddLayer {
            stack: take_array::<17>(d)?,
            branch: take_array::<17>(d)?,
            commit: take_array::<33>(d)?,
            expected_stack_head: take_array::<33>(d)?,
            expected_branch_base: take_array::<33>(d)?,
        },
        7 => HistoryCommand::DiscardStage {
            workspace: take_array::<32>(d)?,
            token: d.u64()?,
        },
        8 => HistoryCommand::ReserveInodes {
            scope: d.root()?,
            count: d.u64()?,
        },
        _ => return Err(Code::Unsupported.into()),
    })
}
pub fn decode_request(id: u64, b: &[u8]) -> Result<Request, Failure> {
    let mut d = Decoder::new(b)?;
    let generation = d.u64()?;
    let store = d.u32()?;
    let profile = d.u16()?;
    let deadline_ms = d.u32()?;
    let response_bytes = d.u64()?;
    let opcode = d.u8()?;
    if opcode == WORKSPACE_STATUS_OPCODE && b.len() > WORKSPACE_STATUS_REQUEST_BYTES {
        return Err(Code::Capacity.into());
    }
    if opcode == WORKSPACE_UNMOUNT_OPCODE && b.len() > WORKSPACE_UNMOUNT_REQUEST_BYTES {
        return Err(Code::Capacity.into());
    }
    if opcode == WORKSPACE_CLOSE_CLEAN_OPCODE && b.len() > WORKSPACE_CLOSE_CLEAN_REQUEST_BYTES {
        return Err(Code::Capacity.into());
    }
    if opcode == WORKSPACE_MOUNT_OPCODE && b.len() > WORKSPACE_MOUNT_REQUEST_BYTES {
        return Err(Code::Capacity.into());
    }
    if opcode == WORKSPACE_ATTACH_OPCODE && b.len() > WORKSPACE_ATTACH_REQUEST_BYTES {
        return Err(Code::Capacity.into());
    }
    if opcode == UPDATE_PORTABLE_METADATA_OPCODE && b.len() > PORTABLE_METADATA_REQUEST_BYTES {
        return Err(Code::Capacity.into());
    }
    let operation = match opcode {
        1 => Operation::ReadFile {
            root: d.root()?,
            start: d.u64()?,
            end: d.u64()?,
        },
        2 => {
            let root = d.root()?;
            let query = match d.u8()? {
                0 => Inspect::File,
                1 => Inspect::Stat {
                    path: d.blob(4096)?,
                },
                2 => Inspect::List {
                    path: d.blob(4096)?,
                    after: d.blob(255)?,
                    entries: d.u16()?,
                    bytes: d.u32()?,
                },
                3 => Inspect::Readlink {
                    path: d.blob(4096)?,
                },
                4 => Inspect::Attributes {
                    path: d.blob(4096)?,
                },
                _ => return Err(Code::Unsupported.into()),
            };
            Operation::Inspect { root, query }
        }
        3 => Operation::ConstructFile { length: d.u64()? },
        4 => {
            let root = d.root()?;
            let base_length = d.u64()?;
            let count = d.count(256, 24)?;
            let mut edits = Vec::with_capacity(count);
            for _ in 0..count {
                edits.push(Edit {
                    start: d.u64()?,
                    end: d.u64()?,
                    replacement: d.u64()?,
                });
            }
            Operation::EditFile {
                root,
                base_length,
                edits,
            }
        }
        5 => {
            let base = d.root()?;
            let scope = d.root()?;
            let root_serial = d.u64()?;
            let directories = take_directories(&mut d)?;
            let inodes = take_inodes(&mut d)?;
            Operation::UpdatePreparedFilesystem {
                base,
                scope,
                root_serial,
                directories,
                inodes,
            }
        }
        6 => Operation::HistoryQuery(take_query(&mut d)?),
        7 => Operation::HistoryCommand(take_command(&mut d)?),
        WORKSPACE_STATUS_OPCODE => Operation::WorkspaceStatus {
            workspace: d.blob(WORKSPACE_ID_BYTES)?,
            incarnation: d.root()?,
        },
        WORKSPACE_UNMOUNT_OPCODE => Operation::WorkspaceUnmount {
            workspace: d.blob(WORKSPACE_ID_BYTES)?,
            incarnation: d.root()?,
        },
        WORKSPACE_CLOSE_CLEAN_OPCODE => Operation::WorkspaceCloseClean {
            workspace: d.blob(WORKSPACE_ID_BYTES)?,
            incarnation: d.root()?,
        },
        WORKSPACE_MOUNT_OPCODE => Operation::WorkspaceMount {
            workspace: d.blob(WORKSPACE_ID_BYTES)?,
            incarnation: d.root()?,
        },
        WORKSPACE_ATTACH_OPCODE => Operation::WorkspaceAttach {
            workspace: d.blob(WORKSPACE_ID_BYTES)?,
            incarnation: d.root()?,
        },
        UPDATE_PORTABLE_METADATA_OPCODE => Operation::UpdatePortableMetadata {
            base: d.root()?,
            kind: d.u8()?,
            mode: d.u32()?,
            mtime_seconds: d.u64()? as i64,
            mtime_nanoseconds: d.u32()?,
        },
        _ => return Err(Code::Unsupported.into()),
    };
    d.finish()?;
    let r = Request {
        id,
        generation,
        store,
        profile,
        deadline_ms,
        response_bytes,
        operation,
    };
    r.validate()?;
    Ok(r)
}
