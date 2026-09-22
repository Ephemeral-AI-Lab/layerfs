//! Closed operation profile. Root bytes are logical identities, never paths.
use super::{
    Code, Failure, HistoryCommand, HistoryForkSource, HistoryQuery, ManifestEntry, PreparedChanges,
    BRANCH_BYTES, COMMAND_OPCODE, COMMIT_BYTES, CONSTRUCT_PORTABLE_METADATA_OPCODE, CURSOR_BYTES,
    HISTORY_PROFILE, LAYER_BYTES, MANIFEST_ENTRIES, MANIFEST_TARGET_BYTES, NAME_MAX_BYTES,
    PAGE_RECORDS, QUERY_OPCODE, STACK_BYTES, UPDATE_PORTABLE_METADATA_OPCODE,
    WORKSPACE_ATTACH_MAX_MS, WORKSPACE_ATTACH_OPCODE, WORKSPACE_CLOSE_CLEAN_MAX_MS,
    WORKSPACE_CLOSE_CLEAN_OPCODE, WORKSPACE_COMMIT_MAX_MS, WORKSPACE_COMMIT_OPCODE,
    WORKSPACE_MOUNT_MAX_MS, WORKSPACE_MOUNT_OPCODE, WORKSPACE_STATUS_MAX_MS,
    WORKSPACE_STATUS_OPCODE, WORKSPACE_STATUS_PROFILE, WORKSPACE_UNMOUNT_MAX_MS,
    WORKSPACE_UNMOUNT_OPCODE,
};
pub const FRAME_BYTES: usize = 16384;
pub const METADATA_BYTES: usize = 32768;
pub const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_OPERATION_MS: u32 = 600_000;
pub const IO_PROGRESS_MS: u64 = 5_000;
pub const CONSTRUCT_SYMLINK_OPCODE: u8 = 16;
pub const SYMLINK_TARGET_BYTES: usize = 4096;
/// Fixed request envelope and target length followed by the bounded target.
pub const CONSTRUCT_SYMLINK_REQUEST_BYTES: usize = 29 + SYMLINK_TARGET_BYTES;
/// Concurrent reads one service process admits without a writer permit.
///
/// A read holds one bounded decode workspace and one connection for its wave, and
/// that is the resource this bound protects - it is not a second writer budget
/// and it is not an operator setting. Writers are admitted per Store by the
/// Store's own configured budget (`max_concurrent_writes`, #216), so a busy
/// writer set no longer refuses reads.
pub const MAX_READ_OPERATIONS: usize = 2;
pub const MAX_REPLAY: u64 = 8 * 1024 * 1024;

/// Persistent/handshaking/closing sessions a transport admits for one Store.
///
/// The transport must not become an accidental lower ceiling for the configured
/// writer budget: one session can carry one operation, so the session space is
/// the Store's whole write budget plus the service's read bound. The acceptor
/// owns one extra refusal slot beyond this.
pub const fn session_capacity(max_concurrent_writes: u8) -> usize {
    max_concurrent_writes as usize + MAX_READ_OPERATIONS
}

/// Total application connection slots, including the synchronous accept/refusal owner.
pub const fn connection_capacity(max_concurrent_writes: u8) -> usize {
    session_capacity(max_concurrent_writes) + 1
}

/// Includes boundary slack and one terminal; finite even for tiny-frame abuse.
pub const fn frame_budget(bytes: u64) -> u64 {
    bytes.div_ceil(1024).saturating_add(257)
}
pub type Root = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub id: u64,
    pub generation: u64,
    pub store: u32,
    pub profile: u16,
    pub deadline_ms: u32,
    pub response_bytes: u64,
    pub operation: Operation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    ReadFile {
        root: Root,
        start: u64,
        end: u64,
    },
    Inspect {
        root: Root,
        query: Inspect,
    },
    ConstructFile {
        length: u64,
    },
    /// Saves an opaque symlink target; does not allocate or attach an inode.
    ConstructSymlink {
        /// Zero through 4,096 opaque bytes without NUL; no path normalization.
        target: Vec<u8>,
    },
    EditFile {
        root: Root,
        base_length: u64,
        edits: Vec<Edit>,
    },
    UpdatePreparedFilesystem {
        base: Root,
        scope: Root,
        root_serial: u64,
        directories: Vec<DirectoryChange>,
        inodes: Vec<InodeChange>,
        new_directories: Vec<DirectoryMetadata>,
        directory_metadata: Vec<DirectoryMetadata>,
        new_file_serials: Vec<u64>,
    },
    HistoryQuery(HistoryQuery),
    HistoryCommand(HistoryCommand),
    /// Read-only daemon control, independently authorized for one incarnation.
    WorkspaceStatus {
        workspace: Vec<u8>,
        incarnation: Root,
    },
    /// Authenticated daemon lifecycle mutation; no Store or history mutation.
    WorkspaceUnmount {
        workspace: Vec<u8>,
        incarnation: Root,
    },
    /// Closes a clean daemon-owned Workspace without saving or discarding edits.
    WorkspaceCloseClean {
        workspace: Vec<u8>,
        incarnation: Root,
    },
    /// Mounts the exact attached daemon Workspace with its configured profile.
    WorkspaceMount {
        workspace: Vec<u8>,
        incarnation: Root,
    },
    /// Attaches a new identity using the daemon's immutable configured profile.
    WorkspaceAttach {
        workspace: Vec<u8>,
        incarnation: Root,
    },
    /// Commits the selected writable Workspace through its configured service.
    WorkspaceCommit {
        workspace: Vec<u8>,
        incarnation: Root,
    },
    /// Saves an updated attribute tree; does not attach it to an inode or Branch.
    UpdatePortableMetadata {
        base: Root,
        kind: u8,
        mode: u32,
        mtime_seconds: i64,
        mtime_nanoseconds: u32,
    },
    /// Saves a fresh portable attribute tree; does not allocate or attach an inode.
    ConstructPortableMetadata {
        kind: u8,
        mode: u32,
        mtime_seconds: i64,
        mtime_nanoseconds: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inspect {
    File,
    Stat {
        path: Vec<u8>,
    },
    /// Complete portable attributes and exact logical size for one inode.
    Attributes {
        path: Vec<u8>,
    },
    List {
        path: Vec<u8>,
        after: Vec<u8>,
        entries: u16,
        bytes: u32,
    },
    Readlink {
        path: Vec<u8>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: u64,
    pub end: u64,
    pub replacement: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirectoryChange {
    pub parent: u64,
    pub changes: Vec<(Vec<u8>, Option<u64>)>,
}
/// Portable fields for a qualified new directory or an existing-directory patch.
/// New serials obey the caller's scope-wide allocator contract; an unbound new
/// declaration may be omitted from the result by the filesystem builder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectoryMetadata {
    pub serial: u64,
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InodeChange {
    pub serial: u64,
    pub kind: u8,
    pub content: Root,
    pub metadata: Root,
}
impl Operation {
    pub const fn opcode(&self) -> u8 {
        match self {
            Self::ReadFile { .. } => 1,
            Self::Inspect { .. } => 2,
            Self::ConstructFile { .. } => 3,
            Self::ConstructSymlink { .. } => CONSTRUCT_SYMLINK_OPCODE,
            Self::EditFile { .. } => 4,
            Self::UpdatePreparedFilesystem { .. } => 5,
            Self::HistoryQuery(_) => QUERY_OPCODE,
            Self::HistoryCommand(_) => COMMAND_OPCODE,
            Self::WorkspaceStatus { .. } => WORKSPACE_STATUS_OPCODE,
            Self::WorkspaceUnmount { .. } => WORKSPACE_UNMOUNT_OPCODE,
            Self::WorkspaceCloseClean { .. } => WORKSPACE_CLOSE_CLEAN_OPCODE,
            Self::WorkspaceMount { .. } => WORKSPACE_MOUNT_OPCODE,
            Self::WorkspaceAttach { .. } => WORKSPACE_ATTACH_OPCODE,
            Self::WorkspaceCommit { .. } => WORKSPACE_COMMIT_OPCODE,
            Self::UpdatePortableMetadata { .. } => UPDATE_PORTABLE_METADATA_OPCODE,
            Self::ConstructPortableMetadata { .. } => CONSTRUCT_PORTABLE_METADATA_OPCODE,
        }
    }
    pub const fn label(&self) -> &'static str {
        match self {
            Self::ReadFile { .. } => "ReadFile",
            Self::Inspect { .. } => "Inspect",
            Self::ConstructFile { .. } => "ConstructFile",
            Self::ConstructSymlink { .. } => "ConstructSymlink",
            Self::EditFile { .. } => "EditFile",
            Self::UpdatePreparedFilesystem { .. } => "UpdatePreparedFilesystem",
            Self::HistoryQuery(_) => "HistoryQuery",
            Self::HistoryCommand(_) => "HistoryCommand",
            Self::WorkspaceStatus { .. } => "WorkspaceStatus",
            Self::WorkspaceUnmount { .. } => "WorkspaceUnmount",
            Self::WorkspaceCloseClean { .. } => "WorkspaceCloseClean",
            Self::WorkspaceMount { .. } => "WorkspaceMount",
            Self::WorkspaceAttach { .. } => "WorkspaceAttach",
            Self::WorkspaceCommit { .. } => "WorkspaceCommit",
            Self::UpdatePortableMetadata { .. } => "UpdatePortableMetadata",
            Self::ConstructPortableMetadata { .. } => "ConstructPortableMetadata",
        }
    }
    /// True for an operation that changes neither stored nor daemon lifecycle state.
    pub const fn read_only(&self) -> bool {
        match self {
            Self::ReadFile { .. }
            | Self::Inspect { .. }
            | Self::HistoryQuery(_)
            | Self::WorkspaceStatus { .. } => true,
            Self::ConstructFile { .. }
            | Self::ConstructSymlink { .. }
            | Self::EditFile { .. }
            | Self::UpdatePreparedFilesystem { .. }
            | Self::UpdatePortableMetadata { .. }
            | Self::ConstructPortableMetadata { .. }
            | Self::WorkspaceUnmount { .. }
            | Self::WorkspaceCloseClean { .. }
            | Self::WorkspaceMount { .. }
            | Self::WorkspaceAttach { .. }
            | Self::WorkspaceCommit { .. }
            | Self::HistoryCommand(_) => false,
        }
    }

    /// True for an operation that writes canonical content or a filesystem root.
    pub const fn content_mutation(&self) -> bool {
        match self {
            Self::ConstructFile { .. }
            | Self::ConstructSymlink { .. }
            | Self::EditFile { .. }
            | Self::UpdatePreparedFilesystem { .. }
            | Self::UpdatePortableMetadata { .. }
            | Self::ConstructPortableMetadata { .. }
            | Self::HistoryCommand(
                HistoryCommand::InitLayerStack { .. }
                | HistoryCommand::StageChanges(_)
                | HistoryCommand::Commit(_),
            ) => true,
            Self::ReadFile { .. }
            | Self::Inspect { .. }
            | Self::WorkspaceStatus { .. }
            | Self::WorkspaceUnmount { .. }
            | Self::WorkspaceCloseClean { .. }
            | Self::WorkspaceMount { .. }
            | Self::WorkspaceAttach { .. }
            | Self::WorkspaceCommit { .. }
            | Self::HistoryQuery(_)
            | Self::HistoryCommand(
                HistoryCommand::Fork { .. }
                | HistoryCommand::CommitStaged { .. }
                | HistoryCommand::AddLayer { .. }
                | HistoryCommand::DiscardStage { .. }
                | HistoryCommand::ReserveInodes { .. },
            ) => false,
        }
    }

    /// True for an operation that changes history metadata only.
    ///
    /// A metadata mutation never starts a content save; that is what keeps the
    /// two failure boundaries separate.
    pub const fn metadata_mutation(&self) -> bool {
        match self {
            Self::HistoryCommand(
                HistoryCommand::Fork { .. }
                | HistoryCommand::CommitStaged { .. }
                | HistoryCommand::AddLayer { .. }
                | HistoryCommand::DiscardStage { .. }
                | HistoryCommand::ReserveInodes { .. },
            ) => true,
            Self::ReadFile { .. }
            | Self::Inspect { .. }
            | Self::WorkspaceStatus { .. }
            | Self::WorkspaceUnmount { .. }
            | Self::WorkspaceCloseClean { .. }
            | Self::WorkspaceMount { .. }
            | Self::WorkspaceAttach { .. }
            | Self::WorkspaceCommit { .. }
            | Self::ConstructFile { .. }
            | Self::ConstructSymlink { .. }
            | Self::EditFile { .. }
            | Self::UpdatePreparedFilesystem { .. }
            | Self::UpdatePortableMetadata { .. }
            | Self::ConstructPortableMetadata { .. }
            | Self::HistoryQuery(_)
            | Self::HistoryCommand(
                HistoryCommand::InitLayerStack { .. }
                | HistoryCommand::StageChanges(_)
                | HistoryCommand::Commit(_),
            ) => false,
        }
    }

    /// True for any stored-state or daemon lifecycle mutation. Lost delivery
    /// cannot establish that such an operation was refused or had no effect.
    pub const fn mutation(&self) -> bool {
        !self.read_only()
    }
    pub fn input_length(&self) -> Result<u64, Failure> {
        match self {
            Self::ConstructFile { length } => Ok(*length),
            Self::EditFile { edits, .. } => edits.iter().try_fold(0u64, |sum, e| {
                sum.checked_add(e.replacement).ok_or(Code::Capacity.into())
            }),
            _ => Ok(0),
        }
    }
}
impl Request {
    pub fn validate(&self) -> Result<(), Failure> {
        let invalid = || Failure::from(Code::InvalidInput);
        let profile = match &self.operation {
            Operation::HistoryQuery(_) | Operation::HistoryCommand(_) => HISTORY_PROFILE,
            Operation::WorkspaceStatus { .. }
            | Operation::WorkspaceUnmount { .. }
            | Operation::WorkspaceCloseClean { .. }
            | Operation::WorkspaceMount { .. }
            | Operation::WorkspaceAttach { .. }
            | Operation::WorkspaceCommit { .. } => WORKSPACE_STATUS_PROFILE,
            _ => 1,
        };
        if self.profile != profile {
            return Err(Code::Unsupported.into());
        }
        if self.id == 0 || self.deadline_ms == 0 || self.deadline_ms > MAX_OPERATION_MS {
            return Err(invalid());
        }
        if self.response_bytes > MAX_FILE || self.operation.input_length()? > MAX_FILE {
            return Err(Code::Capacity.into());
        }
        match &self.operation {
            Operation::ConstructSymlink { target } => {
                if target.len() > SYMLINK_TARGET_BYTES {
                    return Err(Code::Capacity.into());
                }
                if target.contains(&0) || self.response_bytes != 0 {
                    return Err(invalid());
                }
            }
            Operation::UpdatePortableMetadata {
                kind,
                mode,
                mtime_nanoseconds,
                ..
            }
            | Operation::ConstructPortableMetadata {
                kind,
                mode,
                mtime_nanoseconds,
                ..
            } => {
                super::metadata::check_portable_metadata(*kind, *mode, *mtime_nanoseconds)?;
                if self.response_bytes != 0 {
                    return Err(invalid());
                }
            }
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
            }
            | Operation::WorkspaceCommit {
                workspace,
                incarnation,
            } => {
                super::control::check_workspace_identity(workspace, incarnation)?;
                let maximum = match self.operation {
                    Operation::WorkspaceUnmount { .. } => WORKSPACE_UNMOUNT_MAX_MS,
                    Operation::WorkspaceCloseClean { .. } => WORKSPACE_CLOSE_CLEAN_MAX_MS,
                    Operation::WorkspaceMount { .. } => WORKSPACE_MOUNT_MAX_MS,
                    Operation::WorkspaceAttach { .. } => WORKSPACE_ATTACH_MAX_MS,
                    Operation::WorkspaceCommit { .. } => WORKSPACE_COMMIT_MAX_MS,
                    _ => WORKSPACE_STATUS_MAX_MS,
                };
                if self.store != 0
                    || self.generation != 0
                    || self.response_bytes != 0
                    || self.deadline_ms > maximum
                {
                    return Err(invalid());
                }
            }
            Operation::ReadFile { start, end, .. } => {
                if start > end || end - start > self.response_bytes {
                    return Err(invalid());
                }
            }
            Operation::EditFile {
                base_length, edits, ..
            } => {
                if edits.len() > 256
                    || *base_length > MAX_FILE
                    || self.operation.input_length()? > MAX_REPLAY
                {
                    return Err(Code::Capacity.into());
                }
                let mut length = *base_length;
                let mut previous = 0;
                for edit in edits {
                    if edit.start < previous || edit.start > edit.end || edit.end > length {
                        return Err(invalid());
                    }
                    length = length
                        .checked_sub(edit.end - edit.start)
                        .and_then(|n| n.checked_add(edit.replacement))
                        .ok_or_else(invalid)?;
                    previous = edit
                        .start
                        .checked_add(edit.replacement)
                        .ok_or_else(invalid)?;
                    if length > MAX_FILE {
                        return Err(Code::Capacity.into());
                    }
                }
            }
            Operation::Inspect { query, .. } => match query {
                Inspect::File => {}
                Inspect::Stat { path }
                | Inspect::Attributes { path }
                | Inspect::Readlink { path } => check_path(path)?,
                Inspect::List {
                    path,
                    after,
                    entries,
                    bytes,
                } => {
                    check_path(path)?;
                    if after.len() > 255
                        || *entries == 0
                        || *entries > 128
                        || *bytes == 0
                        || *bytes > 16384
                    {
                        return Err(Code::Capacity.into());
                    }
                }
            },
            Operation::UpdatePreparedFilesystem {
                root_serial,
                directories,
                inodes,
                new_directories,
                directory_metadata,
                new_file_serials,
                ..
            } => {
                if *root_serial == 0 || *root_serial > i64::MAX as u64 {
                    return Err(invalid());
                }
                if directories.len() > 128 || inodes.len() > 128 {
                    return Err(Code::Capacity.into());
                }
                check_prepared_additions(
                    *root_serial,
                    directories,
                    inodes,
                    new_directories,
                    directory_metadata,
                    new_file_serials,
                )?;
                let mut count = 0usize;
                for directory in directories {
                    count = count
                        .checked_add(directory.changes.len())
                        .ok_or(Code::Capacity)?;
                    if count > 128 {
                        return Err(Code::Capacity.into());
                    }
                    for (name, _) in &directory.changes {
                        if name.is_empty() || name.len() > 255 {
                            return Err(invalid());
                        }
                    }
                }
            }
            _ => {}
        }
        match &self.operation {
            Operation::HistoryQuery(query) => check_history_query(query)?,
            Operation::HistoryCommand(command) => check_history_command(command)?,
            _ => {}
        }
        Ok(())
    }
}

/// Checks one tagged identity of an exact frozen width.
fn check_identity(bytes: &[u8], width: usize, tag: u8) -> Result<(), Failure> {
    if bytes.len() != width || bytes[0] != tag {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}

fn check_page(cursor: &[u8], limit: u16) -> Result<(), Failure> {
    if limit == 0 {
        return Err(Code::InvalidInput.into());
    }
    if limit > PAGE_RECORDS || cursor.len() > CURSOR_BYTES {
        return Err(Code::Capacity.into());
    }
    Ok(())
}

fn check_history_query(query: &HistoryQuery) -> Result<(), Failure> {
    match query {
        HistoryQuery::GetStack { stack } => check_identity(stack, STACK_BYTES, 0x31),
        HistoryQuery::GetBranch { branch } => check_identity(branch, BRANCH_BYTES, 0x11),
        HistoryQuery::GetCommit { commit } => check_identity(commit, COMMIT_BYTES, 0x12),
        HistoryQuery::GetLayer { layer } => check_identity(layer, LAYER_BYTES, 0x32),
        HistoryQuery::GetStage { workspace } => {
            if workspace.iter().all(|byte| *byte == 0) {
                return Err(Code::InvalidInput.into());
            }
            Ok(())
        }
        HistoryQuery::ListStacks { cursor, limit } => check_page(cursor, *limit),
        HistoryQuery::ListBranches {
            stack,
            cursor,
            limit,
        } => {
            check_identity(stack, STACK_BYTES, 0x31)?;
            check_page(cursor, *limit)
        }
        HistoryQuery::ListStages {
            branch,
            cursor,
            limit,
        } => {
            check_identity(branch, BRANCH_BYTES, 0x11)?;
            check_page(cursor, *limit)
        }
        HistoryQuery::CommitHistory {
            branch,
            start,
            cursor,
            limit,
        } => {
            check_identity(branch, BRANCH_BYTES, 0x11)?;
            if let Some(start) = start {
                check_identity(start, COMMIT_BYTES, 0x12)?;
            }
            check_page(cursor, *limit)
        }
        HistoryQuery::LayerHistory {
            stack,
            start,
            cursor,
            limit,
        } => {
            check_identity(stack, STACK_BYTES, 0x31)?;
            if let Some(start) = start {
                check_identity(start, LAYER_BYTES, 0x32)?;
            }
            check_page(cursor, *limit)
        }
    }
}

fn check_prepared(changes: &PreparedChanges) -> Result<(), Failure> {
    if changes.workspace.iter().all(|byte| *byte == 0) {
        return Err(Code::InvalidInput.into());
    }
    check_identity(&changes.branch, BRANCH_BYTES, 0x11)?;
    if let Some(head) = &changes.expected_head {
        check_identity(head, COMMIT_BYTES, 0x12)?;
    }
    check_identity(&changes.expected_base, LAYER_BYTES, 0x32)?;
    if changes.root_serial == 0 || changes.root_serial > i64::MAX as u64 {
        return Err(Code::InvalidInput.into());
    }
    check_prepared_lists(&changes.directories, &changes.inodes)?;
    check_prepared_additions(
        changes.root_serial,
        &changes.directories,
        &changes.inodes,
        &changes.new_directories,
        &changes.directory_metadata,
        &changes.new_file_serials,
    )
}

fn check_prepared_lists(
    directories: &[DirectoryChange],
    inodes: &[InodeChange],
) -> Result<(), Failure> {
    if directories.len() > 128 || inodes.len() > 128 {
        return Err(Code::Capacity.into());
    }
    let mut count = 0usize;
    for directory in directories {
        count = count
            .checked_add(directory.changes.len())
            .ok_or(Code::Capacity)?;
        if count > 128 {
            return Err(Code::Capacity.into());
        }
        for (name, _) in &directory.changes {
            if name.is_empty() || name.len() > 255 {
                return Err(Code::InvalidInput.into());
            }
        }
    }
    if directories
        .windows(2)
        .any(|pair| pair[0].parent >= pair[1].parent)
    {
        return Err(Code::InvalidInput.into());
    }
    if inodes
        .windows(2)
        .any(|pair| pair[0].serial >= pair[1].serial)
    {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}

fn check_prepared_additions(
    root_serial: u64,
    directories: &[DirectoryChange],
    inodes: &[InodeChange],
    new_directories: &[DirectoryMetadata],
    directory_metadata: &[DirectoryMetadata],
    new_file_serials: &[u64],
) -> Result<(), Failure> {
    if inodes
        .len()
        .checked_add(new_directories.len())
        .and_then(|n| n.checked_add(directory_metadata.len()))
        .ok_or(Code::Capacity)?
        > 128
    {
        return Err(Code::Capacity.into());
    }
    if new_file_serials.len() > inodes.len() {
        return Err(Code::Capacity.into());
    }
    for records in [new_directories, directory_metadata] {
        if records
            .windows(2)
            .any(|pair| pair[0].serial >= pair[1].serial)
        {
            return Err(Code::InvalidInput.into());
        }
        for directory in records {
            if directory.serial == 0
                || directory.serial > i64::MAX as u64
                || inodes.iter().any(|inode| inode.serial == directory.serial)
            {
                return Err(Code::InvalidInput.into());
            }
            super::metadata::check_portable_metadata(
                2,
                directory.mode,
                directory.mtime_nanoseconds,
            )?;
        }
    }
    if (!new_directories.is_empty()
        || !directory_metadata.is_empty()
        || !new_file_serials.is_empty())
        && inodes
            .windows(2)
            .any(|pair| pair[0].serial >= pair[1].serial)
    {
        return Err(Code::InvalidInput.into());
    }
    if new_file_serials.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(Code::InvalidInput.into());
    }
    for serial in new_file_serials {
        if *serial == 0 || *serial > i64::MAX as u64 || *serial == root_serial {
            return Err(Code::InvalidInput.into());
        }
        let index = inodes
            .binary_search_by_key(serial, |inode| inode.serial)
            .map_err(|_| Code::InvalidInput)?;
        if inodes[index].kind != 1 {
            return Err(Code::InvalidInput.into());
        }
        // Directory declarations and patches above are disjoint from every I row.
    }
    for directory in new_directories {
        if directory.serial == root_serial
            || directory_metadata
                .binary_search_by_key(&directory.serial, |d| d.serial)
                .is_ok()
            || !directories
                .iter()
                .any(|changes| changes.parent == directory.serial)
        {
            return Err(Code::InvalidInput.into());
        }
    }
    Ok(())
}

fn check_manifest(entries: &[ManifestEntry]) -> Result<(), Failure> {
    if entries.is_empty() {
        return Err(Code::InvalidInput.into());
    }
    if entries.len() > MANIFEST_ENTRIES {
        return Err(Code::Capacity.into());
    }
    for (index, entry) in entries.iter().enumerate() {
        if index == 0 {
            if entry.kind != 2
                || !entry.name.is_empty()
                || entry.content.is_some()
                || !entry.target.is_empty()
            {
                return Err(Code::InvalidInput.into());
            }
        } else if entry.name.is_empty()
            || entry.name.len() > 255
            || entry.name.contains(&0)
            || usize::from(entry.parent) >= index
        {
            return Err(Code::InvalidInput.into());
        }
        if entry.mtime_nanoseconds > 999_999_999 {
            return Err(Code::InvalidInput.into());
        }
        let mask = match entry.kind {
            2 => 0o1777,
            1 | 3 => 0o777,
            _ => return Err(Code::InvalidInput.into()),
        };
        if entry.mode & !mask != 0 {
            return Err(Code::InvalidInput.into());
        }
        match entry.kind {
            2 => {
                if entry.content.is_some() || !entry.target.is_empty() {
                    return Err(Code::InvalidInput.into());
                }
            }
            1 => {
                if entry.content.is_none() || !entry.target.is_empty() {
                    return Err(Code::InvalidInput.into());
                }
            }
            _ => {
                if entry.content.is_some()
                    || entry.target.is_empty()
                    || entry.target.len() > MANIFEST_TARGET_BYTES
                    || entry.target.contains(&0)
                    || entry.mode != 0o777
                {
                    return Err(Code::InvalidInput.into());
                }
            }
        }
    }
    let mut seen: Vec<(u16, &[u8])> = Vec::with_capacity(entries.len());
    for entry in entries.iter().skip(1) {
        let name: &[u8] = &entry.name;
        if seen
            .iter()
            .any(|(parent, existing)| *parent == entry.parent && *existing == name)
        {
            return Err(Code::InvalidInput.into());
        }
        seen.push((entry.parent, name));
    }
    Ok(())
}

fn check_history_command(command: &HistoryCommand) -> Result<(), Failure> {
    match command {
        HistoryCommand::InitLayerStack { name, manifest, .. } => {
            check_name(name)?;
            check_manifest(manifest)
        }
        HistoryCommand::Fork {
            stack,
            name,
            source,
            ..
        } => {
            check_identity(stack, STACK_BYTES, 0x31)?;
            check_name(name)?;
            match source {
                HistoryForkSource::Layer(layer) => check_identity(layer, LAYER_BYTES, 0x32),
                HistoryForkSource::Commit { branch, commit } => {
                    check_identity(branch, BRANCH_BYTES, 0x11)?;
                    check_identity(commit, COMMIT_BYTES, 0x12)
                }
            }
        }
        HistoryCommand::StageChanges(changes) | HistoryCommand::Commit(changes) => {
            check_prepared(changes)
        }
        HistoryCommand::CommitStaged { workspace, token }
        | HistoryCommand::DiscardStage { workspace, token } => {
            if workspace.iter().all(|byte| *byte == 0) {
                return Err(Code::InvalidInput.into());
            }
            if *token == 0 || *token > i64::MAX as u64 {
                return Err(Code::InvalidInput.into());
            }
            Ok(())
        }
        HistoryCommand::AddLayer {
            stack,
            branch,
            commit,
            expected_stack_head,
            expected_branch_base,
        } => {
            check_identity(stack, STACK_BYTES, 0x31)?;
            check_identity(branch, BRANCH_BYTES, 0x11)?;
            check_identity(commit, COMMIT_BYTES, 0x12)?;
            check_identity(expected_stack_head, LAYER_BYTES, 0x32)?;
            check_identity(expected_branch_base, LAYER_BYTES, 0x32)
        }
        HistoryCommand::ReserveInodes { count, .. } => {
            if *count == 0 {
                return Err(Code::InvalidInput.into());
            }
            if *count > 65_536 {
                return Err(Code::Capacity.into());
            }
            Ok(())
        }
    }
}

pub(crate) fn check_name(name: &[u8]) -> Result<(), Failure> {
    if name.is_empty() || name.len() > NAME_MAX_BYTES {
        return Err(Code::InvalidInput.into());
    }
    let alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    if !name.is_ascii()
        || !alphanumeric(name[0])
        || !alphanumeric(name[name.len() - 1])
        || !name
            .iter()
            .all(|byte| alphanumeric(*byte) || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}
fn check_path(path: &[u8]) -> Result<(), Failure> {
    if path.len() > 4096 {
        Err(Code::Capacity.into())
    } else {
        Ok(())
    }
}
