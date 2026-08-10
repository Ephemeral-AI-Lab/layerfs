//! Lifecycle-owned filesystem preparation adapters for complete content operations.
//!
//! The shared lifecycle coordinator owns operation ordering; this child module
//! owns its concrete file-backed preparation artifacts and their explicit
//! terminal cleanup. Nothing here grants an operation slot or exposes an SDK.

use core::cmp::Ordering;

use crate::cas::{
    FileClosureObjectSpoolV1, FileGlobalSeenSpoolV1, FsCasControlV1, FsCasErrorV1, FsCasV1,
    FsOperationSpoolV1, FsStorageOperationTokenV1, GlobalSeenErrorV1,
};
use crate::content::{ChunkReferenceSpoolV1, PreparedChunkRefV1, PreparedSinkErrorV1};
use crate::format::{
    validate_chunk_refs_per_file, validate_chunk_refs_per_version, validate_entry_count,
    validate_extents_per_version, validate_logical_length,
};
use crate::identity::{
    FileNodeIdV1, LogicalChunkIdV1, PhysicalChunkIdV1, PhysicalFileIdV1, PhysicalTreeIdV1,
};
use crate::lifecycle::{BuiltDirectoryRecordV1, BuiltFileRecordV1, C3PreparationResidentBoundsV1};
use crate::limits::{FileSortEventV1, FileSortWorkV1, OperationCountersV1, OperationWorkControlV1};
use crate::pack::{FilePackIndexSpoolV1, PackIndexSpoolV1};
use crate::{CoreError, CoreResult};

const CHUNK_REFERENCE_RECORD_BYTES: u64 = 68;
const BUILT_FILE_RECORD_BYTES: u64 = 80;
const BUILT_DIRECTORY_RECORD_BYTES: u64 = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum C3PreparationErrorV1 {
    Core(CoreError),
    FsCas(FsCasErrorV1),
}

impl From<CoreError> for C3PreparationErrorV1 {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

impl From<FsCasErrorV1> for C3PreparationErrorV1 {
    fn from(error: FsCasErrorV1) -> Self {
        Self::FsCas(error)
    }
}

/// The single preparation lifecycle shared by complete one-file and tree C3.
/// Every filesystem artifact is opened here, after the root grant, and every
/// terminal path is cleaned here before that grant can be released.
pub(crate) struct C3OperationPreparationV1 {
    references: Option<FileChunkReferenceSpoolV1>,
    metadata: Option<FilePackIndexSpoolV1>,
    closure_objects: Option<FileClosureObjectSpoolV1>,
    global_seen: Option<FileGlobalSeenSpoolV1>,
    built_files: Option<FileBuiltFileSpoolV1>,
    built_directories: Option<FileBuiltDirectorySpoolV1>,
}

impl C3OperationPreparationV1 {
    pub(crate) fn begin<C>(
        cas: &FsCasV1,
        storage_token: FsStorageOperationTokenV1,
        global_seen_capacity: u32,
        bounds: C3PreparationResidentBoundsV1,
        control: &mut C,
    ) -> Result<Self, C3PreparationErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        if bounds.built_files.is_some() != bounds.built_directories.is_some() {
            return Err(CoreError::Schema.into());
        }
        let mut preparation = Self {
            references: None,
            metadata: None,
            closure_objects: None,
            global_seen: None,
            built_files: None,
            built_directories: None,
        };
        let opened = (|| -> Result<(), C3PreparationErrorV1> {
            preparation.references = Some(FileChunkReferenceSpoolV1::new(
                cas.begin_operation_spool_borrowed_v1("chunk-references", storage_token, control)?,
            ));
            if preparation
                .references_mut()
                .resident_memory_bound_bytes(0)?
                > bounds.references
            {
                return Err(CoreError::ResourceRefused.into());
            }

            preparation.metadata = Some(FilePackIndexSpoolV1::new(
                cas.begin_operation_spool_borrowed_v1("pack-index", storage_token, control)?,
            ));
            if preparation.metadata_mut().resident_memory_bound_bytes(0)? > bounds.metadata {
                return Err(CoreError::ResourceRefused.into());
            }

            preparation.closure_objects = Some(FileClosureObjectSpoolV1::new(
                cas.begin_operation_spool_borrowed_v1("closure-objects", storage_token, control)?,
            ));
            if preparation
                .closure_objects_mut()
                .resident_memory_bound_bytes()?
                > bounds.closure_objects
            {
                return Err(CoreError::ResourceRefused.into());
            }

            preparation.global_seen = Some(FileGlobalSeenSpoolV1::new(
                cas.begin_operation_spool_borrowed_v1("global-seen", storage_token, control)?,
            ));
            if preparation
                .global_seen_mut()
                .resident_memory_bound_bytes()?
                > bounds.global_seen
            {
                return Err(CoreError::ResourceRefused.into());
            }
            preparation
                .global_seen_mut()
                .initialize_controlled_v1(global_seen_capacity, control)
                .map_err(|error| match error {
                    GlobalSeenErrorV1::Core(error) => C3PreparationErrorV1::Core(error),
                    GlobalSeenErrorV1::FsCas(error) => C3PreparationErrorV1::FsCas(error),
                })?;

            if let Some(bound) = bounds.built_files {
                preparation.built_files = Some(FileBuiltFileSpoolV1::new(
                    cas.begin_operation_spool_borrowed_v1("built-files", storage_token, control)?,
                ));
                if preparation
                    .built_files_mut()
                    .resident_memory_bound_bytes()?
                    > bound
                {
                    return Err(CoreError::ResourceRefused.into());
                }
            }
            if let Some(bound) = bounds.built_directories {
                preparation.built_directories = Some(FileBuiltDirectorySpoolV1::new(
                    cas.begin_operation_spool_borrowed_v1(
                        "built-directories",
                        storage_token,
                        control,
                    )?,
                ));
                if preparation
                    .built_directories_mut()
                    .resident_memory_bound_bytes()?
                    > bound
                {
                    return Err(CoreError::ResourceRefused.into());
                }
            }
            Ok(())
        })();
        if let Err(error) = opened {
            return Err(match preparation.finish(control) {
                Ok(()) => error,
                Err(cleanup) => C3PreparationErrorV1::FsCas(cleanup),
            });
        }
        Ok(preparation)
    }

    pub(crate) fn parts_mut(
        &mut self,
    ) -> (
        &mut FileChunkReferenceSpoolV1,
        &mut FilePackIndexSpoolV1,
        &mut FileClosureObjectSpoolV1,
        &mut FileGlobalSeenSpoolV1,
        Option<&mut FileBuiltFileSpoolV1>,
        Option<&mut FileBuiltDirectorySpoolV1>,
    ) {
        (
            self.references.as_mut().expect("opened reference spool"),
            self.metadata.as_mut().expect("opened metadata spool"),
            self.closure_objects.as_mut().expect("opened closure spool"),
            self.global_seen.as_mut().expect("opened global-seen spool"),
            self.built_files.as_mut(),
            self.built_directories.as_mut(),
        )
    }

    pub(crate) fn closure_objects_for_fence_mut(&mut self) -> &mut FileClosureObjectSpoolV1 {
        self.closure_objects
            .as_mut()
            .expect("opened closure-object spool")
    }

    fn references_mut(&mut self) -> &mut FileChunkReferenceSpoolV1 {
        self.references.as_mut().expect("opened reference spool")
    }

    fn metadata_mut(&mut self) -> &mut FilePackIndexSpoolV1 {
        self.metadata.as_mut().expect("opened metadata spool")
    }

    fn closure_objects_mut(&mut self) -> &mut FileClosureObjectSpoolV1 {
        self.closure_objects.as_mut().expect("opened closure spool")
    }

    fn global_seen_mut(&mut self) -> &mut FileGlobalSeenSpoolV1 {
        self.global_seen.as_mut().expect("opened global-seen spool")
    }

    fn built_files_mut(&mut self) -> &mut FileBuiltFileSpoolV1 {
        self.built_files.as_mut().expect("opened built-file spool")
    }

    fn built_directories_mut(&mut self) -> &mut FileBuiltDirectorySpoolV1 {
        self.built_directories
            .as_mut()
            .expect("opened built-directory spool")
    }

    /// Attempt every fallible cleanup before the operation capability can be
    /// released. Drop remains only the lower adapter's unwind backstop.
    pub(crate) fn finish<C>(&mut self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        // Terminal cleanup closes and unlinks every spool directly. Do not
        // issue a pre-unlink truncate through the ordinary data port: a prior
        // cleanup fault may already have invalidated the root, and that stale
        // data-port refusal would mask the exact first lifecycle failure even
        // though unlink remains both possible and required.
        let first_io_error = self
            .references
            .as_mut()
            .and_then(FileChunkReferenceSpoolV1::take_first_error)
            .or_else(|| {
                self.metadata
                    .as_mut()
                    .and_then(FilePackIndexSpoolV1::take_first_error)
            })
            .or_else(|| {
                self.closure_objects
                    .as_mut()
                    .and_then(FileClosureObjectSpoolV1::take_first_error)
            })
            .or_else(|| {
                self.built_files
                    .as_mut()
                    .and_then(FileBuiltFileSpoolV1::take_first_error)
            })
            .or_else(|| {
                self.built_directories
                    .as_mut()
                    .and_then(FileBuiltDirectorySpoolV1::take_first_error)
            });
        let cleanup_directories = self
            .built_directories
            .as_mut()
            .map(|spool| spool.cleanup_controlled_v1(control));
        let cleanup_files = self
            .built_files
            .as_mut()
            .map(|spool| spool.cleanup_controlled_v1(control));
        let cleanup_seen = self
            .global_seen
            .as_mut()
            .map(|spool| spool.cleanup_controlled_v1(control));
        let cleanup_closure = self
            .closure_objects
            .as_mut()
            .map(|spool| spool.cleanup_controlled_v1(control));
        let cleanup_metadata = self
            .metadata
            .as_mut()
            .map(|spool| spool.cleanup_controlled_v1(control));
        let cleanup_references = self
            .references
            .as_mut()
            .map(|spool| spool.cleanup_controlled_v1(control));
        cleanup_directories
            .and_then(Result::err)
            .or_else(|| cleanup_files.and_then(Result::err))
            .or_else(|| cleanup_seen.and_then(Result::err))
            .or_else(|| cleanup_closure.and_then(Result::err))
            .or_else(|| cleanup_metadata.and_then(Result::err))
            .or_else(|| cleanup_references.and_then(Result::err))
            .or(first_io_error)
            .map_or(Ok(()), Err)
    }
}

pub(crate) struct FileChunkReferenceSpoolV1 {
    storage: FsOperationSpoolV1,
    maximum: u64,
    count: u64,
    cursor: u64,
    first_error: Option<FsCasErrorV1>,
}

impl FileChunkReferenceSpoolV1 {
    const fn new(storage: FsOperationSpoolV1) -> Self {
        Self {
            storage,
            maximum: 0,
            count: 0,
            cursor: 0,
            first_error: None,
        }
    }

    pub(crate) const fn storage_bytes(&self) -> u64 {
        self.count * CHUNK_REFERENCE_RECORD_BYTES
    }

    fn retain_error(&mut self, error: FsCasErrorV1) -> PreparedSinkErrorV1 {
        self.first_error.get_or_insert(error);
        PreparedSinkErrorV1::Refused
    }

    fn take_first_error(&mut self) -> Option<FsCasErrorV1> {
        self.first_error.take()
    }

    fn cleanup_controlled_v1<C>(&mut self, control: &mut C) -> Result<(), FsCasErrorV1>
    where
        C: FsCasControlV1 + ?Sized,
    {
        self.storage.cleanup_controlled_v1(control)
    }
}

impl ChunkReferenceSpoolV1 for FileChunkReferenceSpoolV1 {
    fn resident_memory_bound_bytes(&self, _maximum_refs: u64) -> CoreResult<u64> {
        self.storage.resident_memory_bound_bytes()
    }

    fn storage_bytes_observation(&self) -> CoreResult<Option<u64>> {
        Ok(Some(self.storage_bytes()))
    }

    fn begin(&mut self, maximum_refs: u64) -> Result<(), PreparedSinkErrorV1> {
        self.storage
            .set_len(0)
            .map_err(|error| self.retain_error(error))?;
        self.maximum = maximum_refs;
        self.count = 0;
        self.cursor = 0;
        Ok(())
    }

    fn push(&mut self, chunk: PreparedChunkRefV1) -> Result<(), PreparedSinkErrorV1> {
        if self.count >= self.maximum {
            return Err(PreparedSinkErrorV1::Refused);
        }
        let mut bytes = [0_u8; CHUNK_REFERENCE_RECORD_BYTES as usize];
        bytes[..32].copy_from_slice(chunk.logical_id().as_bytes());
        bytes[32..64].copy_from_slice(chunk.physical_id().as_bytes());
        bytes[64..68].copy_from_slice(&chunk.len().to_be_bytes());
        self.storage
            .write_exact_at(self.count * CHUNK_REFERENCE_RECORD_BYTES, &bytes)
            .map_err(|error| self.retain_error(error))?;
        self.count += 1;
        Ok(())
    }

    fn rewind(&mut self) -> Result<(), PreparedSinkErrorV1> {
        self.cursor = 0;
        Ok(())
    }

    fn next(&mut self) -> Result<Option<PreparedChunkRefV1>, PreparedSinkErrorV1> {
        if self.cursor >= self.count {
            return Ok(None);
        }
        let mut bytes = [0_u8; CHUNK_REFERENCE_RECORD_BYTES as usize];
        self.storage
            .read_exact_at(self.cursor * CHUNK_REFERENCE_RECORD_BYTES, &mut bytes)
            .map_err(|error| self.retain_error(error))?;
        self.cursor += 1;
        Ok(Some(PreparedChunkRefV1::from_parts(
            LogicalChunkIdV1::from_digest(bytes[..32].try_into().expect("fixed id")),
            PhysicalChunkIdV1::from_digest(bytes[32..64].try_into().expect("fixed id")),
            u32::from_be_bytes(bytes[64..68].try_into().expect("fixed length")),
        )))
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

#[derive(Clone, Copy, Default)]
pub(crate) struct BuiltFileStatsV1 {
    pub(crate) unique_file_count: u32,
    pub(crate) logical_file_bytes: u64,
    pub(crate) extent_count: u32,
    pub(crate) chunk_ref_count: u32,
}

pub(crate) struct FileBuiltFileSpoolV1 {
    storage: FsOperationSpoolV1,
    count: u32,
    first_error: Option<FsCasErrorV1>,
}

impl FileBuiltFileSpoolV1 {
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

    fn take_first_error(&mut self) -> Option<FsCasErrorV1> {
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

    pub(crate) fn push(&mut self, record: BuiltFileRecordV1) -> CoreResult<()> {
        let offset = u64::from(self.count)
            .checked_mul(BUILT_FILE_RECORD_BYTES)
            .ok_or(CoreError::IntegerOverflow)?;
        self.storage
            .write_exact_at(offset, &encode_built_file_record(record))
            .map_err(|error| self.retain_sink_error(error))?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    pub(crate) fn read(&mut self, ordinal: u32) -> CoreResult<BuiltFileRecordV1> {
        if ordinal >= self.count {
            return Err(CoreError::SourceFailure);
        }
        let mut bytes = [0_u8; BUILT_FILE_RECORD_BYTES as usize];
        self.storage
            .read_exact_at(
                u64::from(ordinal)
                    .checked_mul(BUILT_FILE_RECORD_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
                &mut bytes,
            )
            .map_err(|error| self.retain_source_error(error))?;
        decode_built_file_record(&bytes)
    }

    fn write(&mut self, ordinal: u32, record: BuiltFileRecordV1) -> CoreResult<()> {
        if ordinal >= self.count {
            return Err(CoreError::SinkRefused);
        }
        self.storage
            .write_exact_at(
                u64::from(ordinal)
                    .checked_mul(BUILT_FILE_RECORD_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
                &encode_built_file_record(record),
            )
            .map_err(|error| self.retain_sink_error(error))
    }

    pub(crate) fn sort_unique_stats<C>(
        &mut self,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<BuiltFileStatsV1>
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
        let mut stats = BuiltFileStatsV1::default();
        let mut previous: Option<BuiltFileRecordV1> = None;
        for ordinal in 0..count {
            let record = self.read_counted(ordinal, &mut work, control, counters)?;
            if let Some(left) = previous {
                work.begin_event(FileSortEventV1::Comparison, control, counters)?;
                match left.physical.as_bytes().cmp(record.physical.as_bytes()) {
                    Ordering::Greater => return Err(CoreError::NonCanonicalOrder),
                    Ordering::Equal => {
                        if left != record {
                            return Err(CoreError::IdMismatch);
                        }
                        continue;
                    }
                    Ordering::Less => {}
                }
            }
            stats.unique_file_count = stats
                .unique_file_count
                .checked_add(1)
                .ok_or(CoreError::IntegerOverflow)?;
            stats.logical_file_bytes = stats
                .logical_file_bytes
                .checked_add(record.logical_len)
                .ok_or(CoreError::IntegerOverflow)?;
            stats.extent_count = stats
                .extent_count
                .checked_add(u32::from(record.extent_count))
                .ok_or(CoreError::IntegerOverflow)?;
            stats.chunk_ref_count = stats
                .chunk_ref_count
                .checked_add(record.chunk_count)
                .ok_or(CoreError::IntegerOverflow)?;
            previous = Some(record);
        }
        validate_extents_per_version(u64::from(stats.extent_count))?;
        validate_chunk_refs_per_version(u64::from(stats.chunk_ref_count))?;
        work.finish(control, counters)?;
        Ok(stats)
    }

    fn read_counted<C>(
        &mut self,
        ordinal: u32,
        work: &mut FileSortWorkV1,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<BuiltFileRecordV1>
    where
        C: OperationWorkControlV1 + ?Sized,
    {
        work.begin_event(FileSortEventV1::RecordRead, control, counters)?;
        self.read(ordinal)
    }

    fn write_counted<C>(
        &mut self,
        ordinal: u32,
        record: BuiltFileRecordV1,
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
                if child_record.physical.as_bytes() < right_record.physical.as_bytes() {
                    child = right;
                    child_record = right_record;
                }
            }
            let root_record = self.read_counted(root, work, control, counters)?;
            work.begin_event(FileSortEventV1::Comparison, control, counters)?;
            if root_record.physical.as_bytes() >= child_record.physical.as_bytes() {
                return Ok(());
            }
            self.write_counted(root, child_record, work, control, counters)?;
            self.write_counted(child, root_record, work, control, counters)?;
            root = child;
        }
    }
}

fn encode_built_file_record(record: BuiltFileRecordV1) -> [u8; 80] {
    let mut bytes = [0_u8; 80];
    bytes[..32].copy_from_slice(record.logical.as_bytes());
    bytes[32..64].copy_from_slice(record.physical.as_bytes());
    bytes[64..72].copy_from_slice(&record.logical_len.to_be_bytes());
    bytes[72..76].copy_from_slice(&record.chunk_count.to_be_bytes());
    bytes[76] = record.extent_count;
    bytes
}

fn decode_built_file_record(bytes: &[u8; 80]) -> CoreResult<BuiltFileRecordV1> {
    if bytes[77..] != [0_u8; 3] {
        return Err(CoreError::Reserved);
    }
    let logical_len = u64::from_be_bytes(bytes[64..72].try_into().map_err(|_| CoreError::Schema)?);
    validate_logical_length(logical_len)?;
    let chunk_count = u32::from_be_bytes(bytes[72..76].try_into().map_err(|_| CoreError::Schema)?);
    validate_chunk_refs_per_file(u64::from(chunk_count))?;
    let extent_count = bytes[76];
    if extent_count > 1 || extent_count != u8::from(logical_len != 0) {
        return Err(CoreError::Schema);
    }
    Ok(BuiltFileRecordV1 {
        logical: FileNodeIdV1::from_digest(bytes[..32].try_into().map_err(|_| CoreError::Schema)?),
        physical: PhysicalFileIdV1::from_digest(
            bytes[32..64].try_into().map_err(|_| CoreError::Schema)?,
        ),
        logical_len,
        chunk_count,
        extent_count,
    })
}

pub(crate) struct FileBuiltDirectorySpoolV1 {
    storage: FsOperationSpoolV1,
    count: u32,
    first_error: Option<FsCasErrorV1>,
}

impl FileBuiltDirectorySpoolV1 {
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

    fn take_first_error(&mut self) -> Option<FsCasErrorV1> {
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

    pub(crate) fn push(&mut self, record: BuiltDirectoryRecordV1) -> CoreResult<()> {
        let offset = u64::from(self.count)
            .checked_mul(BUILT_DIRECTORY_RECORD_BYTES)
            .ok_or(CoreError::IntegerOverflow)?;
        self.storage
            .write_exact_at(offset, &encode_built_directory_record(record))
            .map_err(|error| self.retain_sink_error(error))?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    fn read(&mut self, ordinal: u32) -> CoreResult<BuiltDirectoryRecordV1> {
        if ordinal >= self.count {
            return Err(CoreError::SourceFailure);
        }
        let mut bytes = [0_u8; BUILT_DIRECTORY_RECORD_BYTES as usize];
        self.storage
            .read_exact_at(
                u64::from(ordinal)
                    .checked_mul(BUILT_DIRECTORY_RECORD_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
                &mut bytes,
            )
            .map_err(|error| self.retain_source_error(error))?;
        decode_built_directory_record(&bytes)
    }

    fn write(&mut self, ordinal: u32, record: BuiltDirectoryRecordV1) -> CoreResult<()> {
        if ordinal >= self.count {
            return Err(CoreError::SinkRefused);
        }
        self.storage
            .write_exact_at(
                u64::from(ordinal)
                    .checked_mul(BUILT_DIRECTORY_RECORD_BYTES)
                    .ok_or(CoreError::IntegerOverflow)?,
                &encode_built_directory_record(record),
            )
            .map_err(|error| self.retain_sink_error(error))
    }

    pub(crate) fn sort_unique_entry_count<C>(
        &mut self,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<u32>
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
        let mut total = 0_u32;
        let mut previous: Option<BuiltDirectoryRecordV1> = None;
        for ordinal in 0..count {
            let record = self.read_counted(ordinal, &mut work, control, counters)?;
            if let Some(left) = previous {
                work.begin_event(FileSortEventV1::Comparison, control, counters)?;
                match left.physical.as_bytes().cmp(record.physical.as_bytes()) {
                    Ordering::Greater => return Err(CoreError::NonCanonicalOrder),
                    Ordering::Equal => {
                        if left != record {
                            return Err(CoreError::IdMismatch);
                        }
                        continue;
                    }
                    Ordering::Less => {}
                }
            }
            total = total
                .checked_add(record.entry_count)
                .ok_or(CoreError::IntegerOverflow)?;
            previous = Some(record);
        }
        validate_entry_count(u64::from(total))?;
        work.finish(control, counters)?;
        Ok(total)
    }

    fn read_counted<C>(
        &mut self,
        ordinal: u32,
        work: &mut FileSortWorkV1,
        control: &mut C,
        counters: &mut OperationCountersV1,
    ) -> CoreResult<BuiltDirectoryRecordV1>
    where
        C: OperationWorkControlV1 + ?Sized,
    {
        work.begin_event(FileSortEventV1::RecordRead, control, counters)?;
        self.read(ordinal)
    }

    fn write_counted<C>(
        &mut self,
        ordinal: u32,
        record: BuiltDirectoryRecordV1,
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
                if child_record.physical.as_bytes() < right_record.physical.as_bytes() {
                    child = right;
                    child_record = right_record;
                }
            }
            let root_record = self.read_counted(root, work, control, counters)?;
            work.begin_event(FileSortEventV1::Comparison, control, counters)?;
            if root_record.physical.as_bytes() >= child_record.physical.as_bytes() {
                return Ok(());
            }
            self.write_counted(root, child_record, work, control, counters)?;
            self.write_counted(child, root_record, work, control, counters)?;
            root = child;
        }
    }
}

fn encode_built_directory_record(record: BuiltDirectoryRecordV1) -> [u8; 40] {
    let mut bytes = [0_u8; 40];
    bytes[..32].copy_from_slice(record.physical.as_bytes());
    bytes[32..36].copy_from_slice(&record.entry_count.to_be_bytes());
    bytes
}

fn decode_built_directory_record(bytes: &[u8; 40]) -> CoreResult<BuiltDirectoryRecordV1> {
    if bytes[36..] != [0_u8; 4] {
        return Err(CoreError::Reserved);
    }
    let entry_count = u32::from_be_bytes(bytes[32..36].try_into().map_err(|_| CoreError::Schema)?);
    validate_entry_count(u64::from(entry_count))?;
    Ok(BuiltDirectoryRecordV1 {
        physical: PhysicalTreeIdV1::from_digest(
            bytes[..32].try_into().map_err(|_| CoreError::Schema)?,
        ),
        entry_count,
    })
}
