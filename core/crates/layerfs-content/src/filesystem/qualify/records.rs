//! Pass state in the caller's indexed records: one row per inode and a FIFO of
//! directories still to list. Nothing here grows with the namespace in memory.
use super::proof::QualificationWork;
use crate::error::{ContentError, ContentResult};
use crate::filesystem::root::{FilesystemRoot, FilesystemRootId};
use crate::filesystem::state::{
    change, SerialState, QUALIFY_CONTEXT, QUALIFY_INODE, QUALIFY_QUEUE,
};
use crate::object::inode_leaf::{InodeKind, InodeValue};
use crate::object::ObjectId;
use crate::{ConstructionRecordChange, IndexedConstructionBacking};

const VERSION: u8 = 1;
const PLAIN: usize = 18;
const DIRECTORY: usize = PLAIN + 32;

/// One inode's declared and so-far observed binding counts.
pub(super) struct Row {
    pub kind: InodeKind,
    pub declared: u64,
    pub bound: u64,
    /// Listing root, retained for directories only.
    pub directory: Option<ObjectId>,
}
impl Row {
    pub fn new(value: InodeValue) -> Self {
        Self {
            kind: value.kind,
            declared: value.namespace_ref_count,
            bound: 0,
            directory: (value.kind == InodeKind::Directory).then_some(value.content_root),
        }
    }
    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.directory.map_or(PLAIN, |_| DIRECTORY));
        bytes.extend_from_slice(&[VERSION, self.kind.code()]);
        bytes.extend_from_slice(&self.declared.to_be_bytes());
        bytes.extend_from_slice(&self.bound.to_be_bytes());
        if let Some(root) = self.directory {
            bytes.extend_from_slice(root.as_bytes());
        }
        bytes
    }
    fn decode(bytes: &[u8]) -> ContentResult<Self> {
        let invalid = ContentError::InvalidRecord("qualification inode record");
        if bytes.len() < PLAIN || bytes[0] != VERSION {
            return Err(invalid);
        }
        let kind = InodeKind::from_code(bytes[1])?;
        let word = |at: usize| -> ContentResult<u64> {
            Ok(u64::from_be_bytes(
                bytes[at..at + 8]
                    .try_into()
                    .map_err(|_| ContentError::UnexpectedEof)?,
            ))
        };
        let directory = match (kind, bytes.len()) {
            (InodeKind::Directory, DIRECTORY) => Some(ObjectId::from_bytes(&bytes[PLAIN..])?),
            (InodeKind::Directory, _) | (_, DIRECTORY) => return Err(invalid),
            (_, PLAIN) => None,
            _ => return Err(invalid),
        };
        Ok(Self {
            kind,
            declared: word(2)?,
            bound: word(10)?,
            directory,
        })
    }
}

pub(super) struct Records<'a, 'w> {
    state: SerialState<'a>,
    work: &'w mut QualificationWork,
    head: u64,
    tail: u64,
}
impl<'a, 'w> Records<'a, 'w> {
    /// Claims the scope for exactly one pass. Records of an earlier pass in
    /// the same scope refuse here; the caller owns acquisition and release.
    pub fn begin(
        backing: &'a mut dyn IndexedConstructionBacking,
        id: FilesystemRootId,
        root: FilesystemRoot,
        work: &'w mut QualificationWork,
    ) -> ContentResult<Self> {
        let mut context = Vec::with_capacity(105);
        context.push(VERSION);
        context.extend_from_slice(id.0.as_bytes());
        context.extend_from_slice(root.scope().object().as_bytes());
        context.extend_from_slice(root.profile().as_bytes());
        context.extend_from_slice(&root.root_inode().serial().to_be_bytes());
        let mut records = Self {
            state: SerialState::new(Some(backing)),
            work,
            head: 0,
            tail: 0,
        };
        records.apply(vec![change(QUALIFY_CONTEXT, 0, None, context)])?;
        Ok(records)
    }
    pub fn work(&mut self) -> &mut QualificationWork {
        self.work
    }
    pub fn insert(serial: u64, row: &Row) -> ConstructionRecordChange {
        change(QUALIFY_INODE, serial, None, row.encode())
    }
    /// The current row and its exact original bytes for a guarded replacement.
    pub fn row(&mut self, serial: u64) -> ContentResult<Option<(Row, Vec<u8>)>> {
        self.work.record_reads = self.work.record_reads.saturating_add(1);
        self.state
            .read(QUALIFY_INODE, serial)?
            .map(|bytes| Ok((Row::decode(&bytes)?, bytes)))
            .transpose()
    }
    pub fn replace(serial: u64, old: Vec<u8>, row: &Row) -> ConstructionRecordChange {
        change(QUALIFY_INODE, serial, Some(old), row.encode())
    }
    /// Appends directories behind the inode changes of the same atomic batch.
    pub fn apply_with_queue(
        &mut self,
        mut changes: Vec<ConstructionRecordChange>,
        directories: &[u64],
    ) -> ContentResult<()> {
        let mut tail = self.tail;
        for serial in directories {
            changes.push(change(
                QUALIFY_QUEUE,
                tail,
                None,
                serial.to_be_bytes().to_vec(),
            ));
            tail = tail.checked_add(1).ok_or(ContentError::LengthOverflow)?;
        }
        self.apply(changes)?;
        self.tail = tail;
        Ok(())
    }
    pub fn apply(&mut self, changes: Vec<ConstructionRecordChange>) -> ContentResult<()> {
        let count = changes.len() as u64;
        self.work.record_batches = self.work.record_batches.saturating_add(1);
        self.work.record_changes = self.work.record_changes.saturating_add(count);
        self.work.peak_batch_changes = self.work.peak_batch_changes.max(count);
        self.state.apply(changes)
    }
    /// The next queued directory's serial and listing root, in bind order.
    pub fn next_directory(&mut self) -> ContentResult<Option<(u64, ObjectId)>> {
        if self.head == self.tail {
            return Ok(None);
        }
        self.work.record_reads = self.work.record_reads.saturating_add(1);
        let bytes = self
            .state
            .read(QUALIFY_QUEUE, self.head)?
            .ok_or(ContentError::InvalidRecord("qualification queue record"))?;
        let serial = u64::from_be_bytes(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| ContentError::InvalidRecord("qualification queue record"))?,
        );
        self.head += 1;
        let root = self
            .row(serial)?
            .and_then(|(row, _)| row.directory)
            .ok_or(ContentError::InvalidRecord("qualification queue record"))?;
        Ok(Some((serial, root)))
    }
}
