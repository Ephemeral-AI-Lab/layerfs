//! Closed operation profile. Root bytes are logical identities, never paths.
use super::{
    Code, Failure, HistoryCommand, HistoryForkSource, HistoryQuery, ManifestEntry, PreparedChanges,
    BRANCH_BYTES, COMMAND_OPCODE, COMMIT_BYTES, CURSOR_BYTES, HISTORY_PROFILE, LAYER_BYTES,
    MANIFEST_ENTRIES, MANIFEST_TARGET_BYTES, NAME_MAX_BYTES, PAGE_RECORDS, QUERY_OPCODE,
    STACK_BYTES,
};
pub const FRAME_BYTES: usize = 16384;
pub const METADATA_BYTES: usize = 32768;
pub const MAX_FILE: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_OPERATION_MS: u32 = 600_000;
pub const IO_PROGRESS_MS: u64 = 5_000;
/// Complete operation reservations; no queue or extra construction producer.
pub const MAX_OPERATIONS: usize = 2;
pub const MAX_REPLAY: u64 = 8 * 1024 * 1024;
/// Persistent/handshaking/closing sessions; the acceptor owns one extra refusal slot.
pub const MAX_SESSIONS: usize = 4;
/// Total application connection slots, including the synchronous accept/refusal owner.
pub const MAX_CONNECTIONS: usize = MAX_SESSIONS + 1;
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
    },
    HistoryQuery(HistoryQuery),
    HistoryCommand(HistoryCommand),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inspect {
    File,
    Stat {
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
            Self::EditFile { .. } => 4,
            Self::UpdatePreparedFilesystem { .. } => 5,
            Self::HistoryQuery(_) => QUERY_OPCODE,
            Self::HistoryCommand(_) => COMMAND_OPCODE,
        }
    }
    pub const fn label(&self) -> &'static str {
        match self {
            Self::ReadFile { .. } => "ReadFile",
            Self::Inspect { .. } => "Inspect",
            Self::ConstructFile { .. } => "ConstructFile",
            Self::EditFile { .. } => "EditFile",
            Self::UpdatePreparedFilesystem { .. } => "UpdatePreparedFilesystem",
            Self::HistoryQuery(_) => "HistoryQuery",
            Self::HistoryCommand(_) => "HistoryCommand",
        }
    }
    /// True for an operation that changes no persistent state.
    pub const fn read_only(&self) -> bool {
        match self {
            Self::ReadFile { .. } | Self::Inspect { .. } | Self::HistoryQuery(_) => true,
            Self::ConstructFile { .. }
            | Self::EditFile { .. }
            | Self::UpdatePreparedFilesystem { .. }
            | Self::HistoryCommand(_) => false,
        }
    }

    /// True for an operation that writes canonical content or a filesystem root.
    pub const fn content_mutation(&self) -> bool {
        match self {
            Self::ConstructFile { .. }
            | Self::EditFile { .. }
            | Self::UpdatePreparedFilesystem { .. } => true,
            Self::ReadFile { .. }
            | Self::Inspect { .. }
            | Self::HistoryQuery(_)
            | Self::HistoryCommand(_) => false,
        }
    }

    /// True for an operation that changes history metadata only.
    ///
    /// A metadata mutation never starts a content save; that is what keeps the
    /// two failure boundaries separate.
    pub const fn metadata_mutation(&self) -> bool {
        match self {
            Self::HistoryCommand(_) => true,
            Self::ReadFile { .. }
            | Self::Inspect { .. }
            | Self::ConstructFile { .. }
            | Self::EditFile { .. }
            | Self::UpdatePreparedFilesystem { .. }
            | Self::HistoryQuery(_) => false,
        }
    }

    /// True for any operation that intends a persistent change.
    pub const fn mutation(&self) -> bool {
        self.content_mutation() || self.metadata_mutation()
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
        let history = matches!(
            self.operation,
            Operation::HistoryQuery(_) | Operation::HistoryCommand(_)
        );
        if history {
            if self.profile != HISTORY_PROFILE {
                return Err(Code::Unsupported.into());
            }
        } else if self.profile != 1 {
            return Err(Code::Unsupported.into());
        }
        if self.id == 0 || self.deadline_ms == 0 || self.deadline_ms > MAX_OPERATION_MS {
            return Err(invalid());
        }
        if self.response_bytes > MAX_FILE || self.operation.input_length()? > MAX_FILE {
            return Err(Code::Capacity.into());
        }
        match &self.operation {
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
                Inspect::Stat { path } | Inspect::Readlink { path } => check_path(path)?,
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
                ..
            } => {
                if *root_serial == 0 || *root_serial > i64::MAX as u64 {
                    return Err(invalid());
                }
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
    check_prepared_lists(&changes.directories, &changes.inodes)
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
            if *count == 0 || *count > 65_536 {
                return Err(Code::Capacity.into());
            }
            Ok(())
        }
    }
}

fn check_name(name: &[u8]) -> Result<(), Failure> {
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
