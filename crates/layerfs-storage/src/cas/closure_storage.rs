//! File-backed closure-object ordering and de-duplication.
//!
//! Closure identity belongs to the immutable CAS admission boundary. The
//! implementation stays private and retains the first concrete filesystem
//! failure so outer lifecycle code can return it after explicit cleanup.

use core::cmp::Ordering;

use super::{compare_closure_object_ids_v1, FsCasControlV1, FsCasErrorV1, FsOperationSpoolV1};
use crate::format::{validate_physical_object_len, PhysicalObjectKindV1};
use crate::limits::{FileSortEventV1, FileSortWorkV1, OperationCountersV1, OperationWorkControlV1};
use crate::object::TypedPhysicalObjectIdV1;
use crate::{CoreError, CoreResult};

const CLOSURE_OBJECT_RECORD_BYTES: u64 = 48;

#[derive(Clone, Copy)]
pub(crate) struct ClosureObjectRecordV1 {
    pub(crate) id: TypedPhysicalObjectIdV1,
    pub(crate) complete_len: u64,
}

impl ClosureObjectRecordV1 {
    pub(crate) const fn complete(id: TypedPhysicalObjectIdV1, complete_len: u64) -> Self {
        Self { id, complete_len }
    }

    /// A transaction-private queue entry whose occupied length has not yet
    /// been resolved. Pending records never cross the sort/fence boundary.
    pub(crate) const fn pending(id: TypedPhysicalObjectIdV1) -> Self {
        Self {
            id,
            complete_len: 0,
        }
    }

    pub(crate) const fn is_pending(self) -> bool {
        self.complete_len == 0
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct ClosureObjectStatsV1 {
    pub(crate) count: u32,
    pub(crate) kind_counts: [u32; 5],
    pub(crate) physical_chunk_bytes: u64,
}

pub(crate) struct FileClosureObjectSpoolV1 {
    storage: FsOperationSpoolV1,
    pub(crate) count: u32,
    first_error: Option<FsCasErrorV1>,
}

impl FileClosureObjectSpoolV1 {
    pub(crate) fn new(storage: FsOperationSpoolV1) -> Self {
        Self {
            storage,
            count: 0,
            first_error: None,
        }
    }

    fn retain_sink_error(&mut self, error: FsCasErrorV1) -> CoreError {
        self.first_error.get_or_insert(error);
        CoreError::SinkRefused
    }

    fn retain_source_error(&mut self, error: FsCasErrorV1) -> CoreError {
        self.first_error.get_or_insert(error);
        CoreError::SourceFailure
    }

    pub(crate) fn take_first_error(&mut self) -> Option<FsCasErrorV1> {
        self.first_error.take()
    }

    pub(crate) fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        self.storage.resident_memory_bound_bytes()
    }

    pub(crate) fn cleanup_controlled_v1<C>(&mut self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.storage.cleanup_controlled_v1(control)
    }

    pub(crate) fn retained_cleanup_terminal_v1(&self) -> Option<FsCasErrorV1> {
        self.storage.retained_cleanup_terminal_v1()
    }

    pub(crate) fn storage_bytes(&self) -> u64 {
        u64::from(self.count) * CLOSURE_OBJECT_RECORD_BYTES
    }

    /// Reuse the same granted preparation file as the candidate-root graph
    /// queue. This is invoked only after every changed-object carrier has
    /// been sealed and admitted, so no construction record is discarded
    /// before it becomes readable through the operation's FsCas owner.
    pub(crate) fn clear_for_candidate_graph_v1(&mut self) -> CoreResult<()> {
        self.storage
            .set_len(0)
            .map_err(|error| self.retain_sink_error(error))?;
        self.count = 0;
        Ok(())
    }

    pub(crate) fn push(&mut self, record: ClosureObjectRecordV1) -> CoreResult<()> {
        let offset = u64::from(self.count)
            .checked_mul(CLOSURE_OBJECT_RECORD_BYTES)
            .ok_or(CoreError::IntegerOverflow)?;
        self.storage
            .write_exact_at(offset, &encode_closure_object_record(record))
            .map_err(|error| self.retain_sink_error(error))?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn read(&mut self, ordinal: u32) -> CoreResult<ClosureObjectRecordV1> {
        if ordinal >= self.count {
            return Err(CoreError::SourceFailure);
        }
        let mut bytes = [0_u8; CLOSURE_OBJECT_RECORD_BYTES as usize];
        self.storage
            .read_exact_at(
                u64::from(ordinal)
                    .checked_mul(CLOSURE_OBJECT_RECORD_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
                &mut bytes,
            )
            .map_err(|error| self.retain_source_error(error))?;
        decode_closure_object_record(&bytes)
    }

    pub(crate) fn complete_pending(&mut self, ordinal: u32, complete_len: u64) -> CoreResult<()> {
        validate_physical_object_len(complete_len)?;
        let pending = self.read(ordinal)?;
        if !pending.is_pending() {
            return Err(CoreError::IdMismatch);
        }
        self.write(
            ordinal,
            ClosureObjectRecordV1::complete(pending.id, complete_len),
        )
    }

    fn write(&mut self, ordinal: u32, record: ClosureObjectRecordV1) -> CoreResult<()> {
        if ordinal >= self.count {
            return Err(CoreError::SinkRefused);
        }
        self.storage
            .write_exact_at(
                u64::from(ordinal)
                    .checked_mul(CLOSURE_OBJECT_RECORD_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
                &encode_closure_object_record(record),
            )
            .map_err(|error| self.retain_sink_error(error))?;
        Ok(())
    }

    pub(crate) fn sort_unique<C>(
        &mut self,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<ClosureObjectStatsV1>
    where
        C: OperationWorkControlV1 + ?Sized,
    {
        let count = self.count;
        let mut work = FileSortWorkV1::begin(count, control, counters)?;
        work.begin_pass(counters)?;
        for start in (0..count / 2).rev() {
            self.sift_down(start, count, &mut work, control, counters)?;
        }
        work.begin_pass(counters)?;
        for end in (1..count).rev() {
            let first = self.read_counted(0, &mut work, control, counters)?;
            let last = self.read_counted(end, &mut work, control, counters)?;
            self.write_counted(0, last, &mut work, control, counters)?;
            self.write_counted(end, first, &mut work, control, counters)?;
            self.sift_down(0, end, &mut work, control, counters)?;
        }

        work.begin_pass(counters)?;
        let mut stats = ClosureObjectStatsV1::default();
        let mut previous: Option<ClosureObjectRecordV1> = None;
        for ordinal in 0..count {
            let record = self.read_counted(ordinal, &mut work, control, counters)?;
            validate_physical_object_len(record.complete_len)?;
            if let Some(left) = previous {
                work.begin_event(FileSortEventV1::Comparison, control, counters)?;
                match compare_closure_object_ids_v1(left.id, record.id) {
                    Ordering::Greater => return Err(CoreError::NonCanonicalOrder),
                    Ordering::Equal => {
                        if left.complete_len != record.complete_len {
                            return Err(CoreError::IdMismatch);
                        }
                        continue;
                    }
                    Ordering::Less => {}
                }
            }
            if stats.count != ordinal {
                self.write_counted(stats.count, record, &mut work, control, counters)?;
            }
            stats.kind_counts[kind_index(record.id.kind())] = stats.kind_counts
                [kind_index(record.id.kind())]
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
            if record.id.kind() == PhysicalObjectKindV1::Chunk {
                stats.physical_chunk_bytes = stats
                    .physical_chunk_bytes
                    .checked_add(
                        record
                            .complete_len
                            .checked_sub(crate::object::OBJECT_HEADER_BYTES)
                            .ok_or(CoreError::IntegerOverflow)?,
                    )
                    .ok_or(CoreError::IntegerOverflow)?;
            }
            stats.count = stats
                .count
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            previous = Some(record);
        }
        self.count = stats.count;
        self.storage
            .set_len(self.storage_bytes())
            .map_err(|error| self.retain_sink_error(error))?;
        work.finish(control, counters)?;
        Ok(stats)
    }

    fn read_counted<C>(
        &mut self,
        ordinal: u32,
        work: &mut FileSortWorkV1,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<ClosureObjectRecordV1>
    where
        C: OperationWorkControlV1 + ?Sized,
    {
        work.begin_event(FileSortEventV1::RecordRead, control, counters)?;
        self.read(ordinal)
    }

    fn write_counted<C>(
        &mut self,
        ordinal: u32,
        record: ClosureObjectRecordV1,
        work: &mut FileSortWorkV1,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<()>
    where
        C: OperationWorkControlV1 + ?Sized,
    {
        work.begin_event(FileSortEventV1::RecordWrite, control, counters)?;
        self.write(ordinal, record)
    }

    #[allow(clippy::too_many_arguments)]
    fn sift_down<C>(
        &mut self,
        mut root: u32,
        end: u32,
        work: &mut FileSortWorkV1,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<()>
    where
        C: OperationWorkControlV1 + ?Sized,
    {
        loop {
            let left = root
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .ok_or(CoreError::IntegerOverflow)?;
            if left >= end {
                return Ok(());
            }
            let right = left + 1;
            let mut child = left;
            let mut child_record = self.read_counted(left, work, control, counters)?;
            if right < end {
                let right_record = self.read_counted(right, work, control, counters)?;
                work.begin_event(FileSortEventV1::Comparison, control, counters)?;
                if compare_closure_object_ids_v1(child_record.id, right_record.id) == Ordering::Less
                {
                    child = right;
                    child_record = right_record;
                }
            }
            let root_record = self.read_counted(root, work, control, counters)?;
            work.begin_event(FileSortEventV1::Comparison, control, counters)?;
            if compare_closure_object_ids_v1(root_record.id, child_record.id) != Ordering::Less {
                return Ok(());
            }
            self.write_counted(root, child_record, work, control, counters)?;
            self.write_counted(child, root_record, work, control, counters)?;
            root = child;
        }
    }
}

fn kind_index(kind: PhysicalObjectKindV1) -> usize {
    match kind {
        PhysicalObjectKindV1::VersionRecord => 0,
        PhysicalObjectKindV1::Tree => 1,
        PhysicalObjectKindV1::File => 2,
        PhysicalObjectKindV1::Symlink => 3,
        PhysicalObjectKindV1::Chunk => 4,
    }
}

fn encode_closure_object_record(record: ClosureObjectRecordV1) -> [u8; 48] {
    let mut bytes = [0_u8; 48];
    bytes[0] = u8::try_from(kind_index(record.id.kind()) + 1).expect("five object kinds");
    bytes[8..40].copy_from_slice(record.id.as_bytes());
    bytes[40..48].copy_from_slice(&record.complete_len.to_be_bytes());
    bytes
}

fn decode_closure_object_record(bytes: &[u8; 48]) -> CoreResult<ClosureObjectRecordV1> {
    if bytes[1..8] != [0_u8; 7] {
        return Err(CoreError::Reserved);
    }
    let kind = match bytes[0] {
        1 => PhysicalObjectKindV1::VersionRecord,
        2 => PhysicalObjectKindV1::Tree,
        3 => PhysicalObjectKindV1::File,
        4 => PhysicalObjectKindV1::Symlink,
        5 => PhysicalObjectKindV1::Chunk,
        _ => return Err(CoreError::TypeDomain),
    };
    let digest = bytes[8..40].try_into().map_err(|_| CoreError::Schema)?;
    let complete_len = u64::from_be_bytes(bytes[40..48].try_into().map_err(|_| CoreError::Schema)?);
    // Zero is reserved for a private candidate-graph queue record. The
    // record is resolved and rewritten before sort/fence; all non-zero
    // lengths remain subject to the frozen object-length law immediately.
    if complete_len != 0 {
        validate_physical_object_len(complete_len)?;
    }
    Ok(ClosureObjectRecordV1 {
        id: TypedPhysicalObjectIdV1::from_kind_and_digest(kind, digest),
        complete_len,
    })
}
