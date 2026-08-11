//! File-backed private pack-index spool.
//!
//! The lifecycle owner supplies the already-opened preparation file. This
//! pack-owned adapter performs bounded index serialization and sorting only.

use core::cmp::Ordering;

use crate::cas::{FsCasControlV1, FsCasErrorV1, FsOperationSpoolV1};
use crate::limits::{
    FileSortEventV1, FileSortWorkV1, ObservationScopeV1, OperationCountersV1,
    OperationWorkControlV1, OptionalU64ObservationV1,
};
use crate::CoreResult;

use super::{
    encode_index_entry, PackIndexEntryV1, PackIndexSpoolV1, PackPortErrorV1, PACK_INDEX_ENTRY_BYTES,
};

pub(crate) struct FilePackIndexSpoolV1 {
    storage: FsOperationSpoolV1,
    maximum: u32,
    count: u32,
    cursor: u32,
    first_error: Option<FsCasErrorV1>,
}

impl FilePackIndexSpoolV1 {
    pub(crate) const fn new(storage: FsOperationSpoolV1) -> Self {
        Self {
            storage,
            maximum: 0,
            count: 0,
            cursor: 0,
            first_error: None,
        }
    }

    pub(crate) const fn storage_bytes(&self) -> u64 {
        self.count as u64 * PACK_INDEX_ENTRY_BYTES
    }

    fn retain_error(&mut self, error: FsCasErrorV1) -> PackPortErrorV1 {
        self.first_error.get_or_insert(error);
        PackPortErrorV1::Failure
    }

    pub(crate) fn take_first_error(&mut self) -> Option<FsCasErrorV1> {
        self.first_error.take()
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

    fn read_entry(&mut self, ordinal: u32) -> Result<PackIndexEntryV1, PackPortErrorV1> {
        if ordinal >= self.count {
            return Err(PackPortErrorV1::Failure);
        }
        let mut bytes = [0_u8; PACK_INDEX_ENTRY_BYTES as usize];
        self.storage
            .read_exact_at(u64::from(ordinal) * PACK_INDEX_ENTRY_BYTES, &mut bytes)
            .map_err(|error| self.retain_error(error))?;
        crate::pack::decode_index_entry(&bytes).map_err(|_| PackPortErrorV1::Failure)
    }

    fn write_entry(
        &mut self,
        ordinal: u32,
        entry: PackIndexEntryV1,
    ) -> Result<(), PackPortErrorV1> {
        self.storage
            .write_exact_at(
                u64::from(ordinal) * PACK_INDEX_ENTRY_BYTES,
                &encode_index_entry(entry),
            )
            .map_err(|error| self.retain_error(error))
    }

    fn sort(&mut self, order: SpoolOrderV1) -> Result<(), PackPortErrorV1> {
        let count = self.count;
        for start in (0..count / 2).rev() {
            self.sift_down(start, count, order)?;
        }
        for end in (1..count).rev() {
            let first = self.read_entry(0)?;
            let last = self.read_entry(end)?;
            self.write_entry(0, last)?;
            self.write_entry(end, first)?;
            self.sift_down(0, end, order)?;
        }
        self.cursor = 0;
        Ok(())
    }

    fn sort_controlled(
        &mut self,
        order: SpoolOrderV1,
        control: &mut dyn OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        let count = self.count;
        let mut work =
            FileSortWorkV1::begin(count, control, counters).map_err(map_work_error_v1)?;
        work.begin_pass(counters).map_err(map_work_error_v1)?;
        for start in (0..count / 2).rev() {
            self.sift_down_controlled(start, count, order, &mut work, control, counters)?;
        }
        work.begin_pass(counters).map_err(map_work_error_v1)?;
        for end in (1..count).rev() {
            let first = self.read_entry_controlled(0, &mut work, control, counters)?;
            let last = self.read_entry_controlled(end, &mut work, control, counters)?;
            self.write_entry_controlled(0, last, &mut work, control, counters)?;
            self.write_entry_controlled(end, first, &mut work, control, counters)?;
            self.sift_down_controlled(0, end, order, &mut work, control, counters)?;
        }
        work.finish(control, counters).map_err(map_work_error_v1)?;
        self.cursor = 0;
        Ok(())
    }

    fn read_entry_controlled(
        &mut self,
        ordinal: u32,
        work: &mut FileSortWorkV1,
        control: &mut dyn OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<PackIndexEntryV1, PackPortErrorV1> {
        work.begin_event(FileSortEventV1::RecordRead, control, counters)
            .map_err(map_work_error_v1)?;
        self.read_entry(ordinal)
    }

    fn write_entry_controlled(
        &mut self,
        ordinal: u32,
        entry: PackIndexEntryV1,
        work: &mut FileSortWorkV1,
        control: &mut dyn OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        work.begin_event(FileSortEventV1::RecordWrite, control, counters)
            .map_err(map_work_error_v1)?;
        self.write_entry(ordinal, entry)
    }

    fn compare_entry_controlled(
        left: PackIndexEntryV1,
        right: PackIndexEntryV1,
        order: SpoolOrderV1,
        work: &mut FileSortWorkV1,
        control: &mut dyn OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<Ordering, PackPortErrorV1> {
        work.begin_event(FileSortEventV1::Comparison, control, counters)
            .map_err(map_work_error_v1)?;
        Ok(compare_entry(left, right, order))
    }

    #[allow(clippy::too_many_arguments)]
    fn sift_down_controlled(
        &mut self,
        mut root: u32,
        end: u32,
        order: SpoolOrderV1,
        work: &mut FileSortWorkV1,
        control: &mut dyn OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        loop {
            let left = root
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .ok_or(PackPortErrorV1::Failure)?;
            if left >= end {
                return Ok(());
            }
            let right = left + 1;
            let mut child = left;
            let mut child_entry = self.read_entry_controlled(left, work, control, counters)?;
            if right < end {
                let right_entry = self.read_entry_controlled(right, work, control, counters)?;
                if Self::compare_entry_controlled(
                    child_entry,
                    right_entry,
                    order,
                    work,
                    control,
                    counters,
                )? == Ordering::Less
                {
                    child = right;
                    child_entry = right_entry;
                }
            }
            let root_entry = self.read_entry_controlled(root, work, control, counters)?;
            if Self::compare_entry_controlled(
                root_entry,
                child_entry,
                order,
                work,
                control,
                counters,
            )? != Ordering::Less
            {
                return Ok(());
            }
            self.write_entry_controlled(root, child_entry, work, control, counters)?;
            self.write_entry_controlled(child, root_entry, work, control, counters)?;
            root = child;
        }
    }

    fn sift_down(
        &mut self,
        mut root: u32,
        end: u32,
        order: SpoolOrderV1,
    ) -> Result<(), PackPortErrorV1> {
        loop {
            let left = root
                .checked_mul(2)
                .and_then(|value| value.checked_add(1))
                .ok_or(PackPortErrorV1::Failure)?;
            if left >= end {
                return Ok(());
            }
            let right = left + 1;
            let mut child = left;
            let mut child_entry = self.read_entry(left)?;
            if right < end {
                let right_entry = self.read_entry(right)?;
                if compare_entry(child_entry, right_entry, order) == Ordering::Less {
                    child = right;
                    child_entry = right_entry;
                }
            }
            let root_entry = self.read_entry(root)?;
            if compare_entry(root_entry, child_entry, order) != Ordering::Less {
                return Ok(());
            }
            self.write_entry(root, child_entry)?;
            self.write_entry(child, root_entry)?;
            root = child;
        }
    }
}

#[derive(Clone, Copy)]
enum SpoolOrderV1 {
    Key,
    Offset,
}

fn compare_entry(left: PackIndexEntryV1, right: PackIndexEntryV1, order: SpoolOrderV1) -> Ordering {
    match order {
        SpoolOrderV1::Key => left.compare_key(&right),
        SpoolOrderV1::Offset => left.compare_offset(&right),
    }
}

impl PackIndexSpoolV1 for FilePackIndexSpoolV1 {
    fn resident_memory_bound_bytes(&self, _maximum_entries: u32) -> CoreResult<u64> {
        self.storage.resident_memory_bound_bytes()
    }

    fn storage_bytes_observation(&self) -> CoreResult<OptionalU64ObservationV1> {
        Ok(OptionalU64ObservationV1::observed(
            self.storage_bytes(),
            "direct pack-index spool logical length",
            ObservationScopeV1::Operation,
        ))
    }

    fn take_storage_error_typed_v1(&mut self) -> Option<FsCasErrorV1> {
        self.take_first_error()
    }

    fn reset(&mut self, maximum_entries: u32) -> Result<(), PackPortErrorV1> {
        self.storage
            .set_len(0)
            .map_err(|error| self.retain_error(error))?;
        self.maximum = maximum_entries;
        self.count = 0;
        self.cursor = 0;
        Ok(())
    }

    fn push(&mut self, entry: PackIndexEntryV1) -> Result<(), PackPortErrorV1> {
        if self.count >= self.maximum {
            return Err(PackPortErrorV1::Failure);
        }
        self.write_entry(self.count, entry)?;
        self.count += 1;
        Ok(())
    }

    fn sort_by_key(&mut self) -> Result<(), PackPortErrorV1> {
        self.sort(SpoolOrderV1::Key)
    }

    fn sort_by_offset(&mut self) -> Result<(), PackPortErrorV1> {
        self.sort(SpoolOrderV1::Offset)
    }

    fn sort_by_key_controlled(
        &mut self,
        control: &mut dyn OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        self.sort_controlled(SpoolOrderV1::Key, control, counters)
    }

    fn sort_by_offset_controlled(
        &mut self,
        control: &mut dyn OperationWorkControlV1,
        counters: &mut OperationCountersV1,
    ) -> Result<(), PackPortErrorV1> {
        self.sort_controlled(SpoolOrderV1::Offset, control, counters)
    }

    fn rewind(&mut self) -> Result<(), PackPortErrorV1> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<Option<PackIndexEntryV1>, PackPortErrorV1> {
        if self.cursor >= self.count {
            return Ok(None);
        }
        let entry = self.read_entry(self.cursor)?;
        self.cursor += 1;
        Ok(Some(entry))
    }

    fn abort(&mut self) {
        if let Err(error) = self.storage.set_len(0) {
            self.first_error.get_or_insert(error);
        }
        self.maximum = 0;
        self.count = 0;
        self.cursor = 0;
    }
}

const fn map_work_error_v1(error: crate::CoreError) -> PackPortErrorV1 {
    match error {
        crate::CoreError::Cancelled => PackPortErrorV1::Cancelled,
        crate::CoreError::Deadline => PackPortErrorV1::Deadline,
        crate::CoreError::ResourceRefused => PackPortErrorV1::WorkExhausted,
        _ => PackPortErrorV1::Failure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cas::{FsCasControlV1, FsCasV1, FsOperationKindV1, FsStorageEnvelopeV1};
    use crate::format::PhysicalObjectKindV1;
    use crate::identity::ObjectChecksumV1;
    use crate::object::TypedPhysicalObjectIdV1;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

    static NEXT_SORT_ROOT: AtomicU64 = AtomicU64::new(1);

    #[derive(Clone, Copy)]
    enum StopV1 {
        Cancel,
        Deadline,
    }

    struct ContinueFsControlV1;

    impl FsCasControlV1 for ContinueFsControlV1 {
        fn cancellation_requested(&mut self) -> bool {
            false
        }

        fn deadline_exceeded(&mut self) -> bool {
            false
        }
    }

    struct StopAtSecondPollV1 {
        stop: StopV1,
        polls: u64,
    }

    impl OperationWorkControlV1 for StopAtSecondPollV1 {
        fn cancellation_requested_v1(&mut self) -> bool {
            self.polls += 1;
            matches!(self.stop, StopV1::Cancel) && self.polls >= 2
        }

        fn deadline_exceeded_v1(&mut self) -> bool {
            matches!(self.stop, StopV1::Deadline) && self.polls >= 2
        }
    }

    fn interrupted_file_sort_v1(stop: StopV1) {
        const RECORDS: u32 = 256;
        let sequence = NEXT_SORT_ROOT.fetch_add(1, AtomicOrdering::Relaxed);
        let parent = fs::canonicalize(std::env::temp_dir()).expect("canonical temp directory");
        let root = parent.join(format!(
            "layerfs-file-sort-control-{}-{sequence:016x}",
            std::process::id()
        ));
        let cas = FsCasV1::create_new(&root).expect("create isolated FsCas");
        let mut fs_control = ContinueFsControlV1;
        let mut counters = OperationCountersV1::default();
        let mut capability = cas
            .begin_operation_capability_v1(
                FsOperationKindV1::CompleteC3Tree,
                sequence,
                &mut counters,
                &mut fs_control,
            )
            .expect("acquire root-owned operation slot");
        capability
            .declare_storage_envelope_v1(
                FsStorageEnvelopeV1::new(u64::from(RECORDS) * PACK_INDEX_ENTRY_BYTES, 0, 1, 0)
                    .expect("checked sort storage envelope"),
            )
            .expect("reserve sort storage envelope");
        let token = capability.storage_token_v1().expect("storage token");
        let storage = cas
            .begin_operation_spool_borrowed_v1("file-sort-control", token, &mut fs_control)
            .expect("create post-grant sort spool");
        let mut spool = FilePackIndexSpoolV1::new(storage);
        spool.reset(RECORDS).expect("reset sort spool");
        for ordinal in (0..RECORDS).rev() {
            let mut digest = [0_u8; 32];
            digest[..4].copy_from_slice(&ordinal.to_be_bytes());
            spool
                .push(PackIndexEntryV1::from_validated_parts(
                    TypedPhysicalObjectIdV1::from_kind_and_digest(
                        PhysicalObjectKindV1::Chunk,
                        digest,
                    ),
                    u64::from(ordinal),
                    53,
                    ObjectChecksumV1::from_digest(digest),
                ))
                .expect("append index record");
        }

        let mut stop_control = StopAtSecondPollV1 { stop, polls: 0 };
        let expected = match stop {
            StopV1::Cancel => PackPortErrorV1::Cancelled,
            StopV1::Deadline => PackPortErrorV1::Deadline,
        };
        assert_eq!(
            spool.sort_by_key_controlled(&mut stop_control, &mut counters),
            Err(expected)
        );
        assert_eq!(counters.file_sort_control_polls, 2);
        assert!(counters.file_sort_work_units > 0);
        assert!(
            counters.file_sort_work_units < crate::limits::FILE_SORT_CONTROL_POLL_WORK_UNITS_V1
        );
        assert_eq!(
            counters.file_sort_work_units,
            counters.file_sort_comparisons
                + counters.file_sort_record_reads
                + counters.file_sort_record_writes
        );
        assert!(counters.file_sort_work_units <= counters.file_sort_maximum_work_budget);
        assert_eq!(counters.file_sort_temporary_bytes_high_water, 0);

        spool
            .cleanup_controlled_v1(&mut fs_control)
            .expect("explicit spool cleanup after interruption");
        capability
            .finish_storage_admission_v1(false, &mut counters, &mut fs_control)
            .expect("release storage lease after cleanup");
        capability
            .finish_operation_admission_v1(&mut counters, &mut fs_control)
            .expect("release operation slot after cleanup");
        drop(spool);
        drop(capability);
        drop(cas);
        fs::remove_dir_all(root).expect("remove isolated FsCas");
    }

    #[test]
    fn file_sort_cancellation_is_bounded_counted_and_cleanup_releases() {
        interrupted_file_sort_v1(StopV1::Cancel);
    }

    #[test]
    fn file_sort_deadline_is_bounded_counted_and_cleanup_releases() {
        interrupted_file_sort_v1(StopV1::Deadline);
    }
}
