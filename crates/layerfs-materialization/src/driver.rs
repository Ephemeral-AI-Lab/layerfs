//! Object-safe native workspace boundary.

use std::any::Any;
use std::fmt;
use std::io::{self, Read, Seek, Write};
use std::path::Path;

pub const MAX_NATIVE_XATTR_BYTES: usize = 1024 * 1024;
const NATIVE_XATTR_CHUNK_BYTES: usize = 1024 * 1024;

pub trait DirectoryHandle: Send {
    fn as_any(&self) -> &dyn Any;
}
pub trait RegularFileHandle: Read + Write + Seek + Send {
    fn as_any(&self) -> &dyn Any;
}
pub trait OwnedTempHandle: Read + Write + Seek + Send {
    fn as_any(&self) -> &dyn Any;
    fn set_len(&mut self, len: u64) -> Result<()>;
    fn into_any(self: Box<Self>) -> Box<dyn Any>;
}
pub trait NamePreflight: Send {
    fn add(&mut self, name: &[u8]) -> Result<()>;
    fn finish(self: Box<Self>) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectionTimerAvailability {
    Available,
    #[default]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionTimer {
    pub availability: ProjectionTimerAvailability,
    pub nanoseconds: u64,
}

impl ProjectionTimer {
    pub const fn available() -> Self {
        Self {
            availability: ProjectionTimerAvailability::Available,
            nanoseconds: 0,
        }
    }

    fn checked_delta(self, before: Self) -> Option<Self> {
        (self.availability == before.availability).then_some(Self {
            availability: self.availability,
            nanoseconds: self.nanoseconds.checked_sub(before.nanoseconds)?,
        })
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        if self.availability == ProjectionTimerAvailability::Available
            && other.availability == ProjectionTimerAvailability::Available
        {
            Some(Self {
                availability: ProjectionTimerAvailability::Available,
                nanoseconds: self.nanoseconds.checked_add(other.nanoseconds)?,
            })
        } else {
            Some(Self::default())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DurabilityClass {
    ProcessCrashReconciled,
    HostCrashOrdered,
    DeviceFlushRequested,
    PowerLossQualified,
}

/// Directory durability for one atomic install. Deferral is valid only while
/// building a fresh tree that cannot become Complete until later bottom-up
/// directory barriers and root revalidation succeed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryDurability {
    ImmediateDirectoryDurability,
    DeferredToIncompleteTreeBoundary,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DurabilityClassCounts {
    pub process_crash_reconciled: u64,
    pub host_crash_ordered: u64,
    pub device_flush_requested: u64,
    pub power_loss_qualified: u64,
}

impl DurabilityClassCounts {
    pub fn get(&self, class: DurabilityClass) -> u64 {
        match class {
            DurabilityClass::ProcessCrashReconciled => self.process_crash_reconciled,
            DurabilityClass::HostCrashOrdered => self.host_crash_ordered,
            DurabilityClass::DeviceFlushRequested => self.device_flush_requested,
            DurabilityClass::PowerLossQualified => self.power_loss_qualified,
        }
    }

    fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            process_crash_reconciled: self
                .process_crash_reconciled
                .checked_sub(before.process_crash_reconciled)?,
            host_crash_ordered: self
                .host_crash_ordered
                .checked_sub(before.host_crash_ordered)?,
            device_flush_requested: self
                .device_flush_requested
                .checked_sub(before.device_flush_requested)?,
            power_loss_qualified: self
                .power_loss_qualified
                .checked_sub(before.power_loss_qualified)?,
        })
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            process_crash_reconciled: self
                .process_crash_reconciled
                .checked_add(other.process_crash_reconciled)?,
            host_crash_ordered: self
                .host_crash_ordered
                .checked_add(other.host_crash_ordered)?,
            device_flush_requested: self
                .device_flush_requested
                .checked_add(other.device_flush_requested)?,
            power_loss_qualified: self
                .power_loss_qualified
                .checked_add(other.power_loss_qualified)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionCallFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub wall: ProjectionTimer,
}

impl ProjectionCallFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            wall: ProjectionTimer::available(),
        }
    }

    fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionWriteFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub bytes: u64,
    pub wall: ProjectionTimer,
}

impl ProjectionWriteFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            bytes: 0,
            wall: ProjectionTimer::available(),
        }
    }

    fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            bytes: self.bytes.checked_sub(before.bytes)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            bytes: self.bytes.checked_add(other.bytes)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionSyncFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub requested: DurabilityClassCounts,
    pub achieved: DurabilityClassCounts,
    pub wall: ProjectionTimer,
}

impl ProjectionSyncFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            requested: DurabilityClassCounts {
                process_crash_reconciled: 0,
                host_crash_ordered: 0,
                device_flush_requested: 0,
                power_loss_qualified: 0,
            },
            achieved: DurabilityClassCounts {
                process_crash_reconciled: 0,
                host_crash_ordered: 0,
                device_flush_requested: 0,
                power_loss_qualified: 0,
            },
            wall: ProjectionTimer::available(),
        }
    }

    fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            requested: self.requested.checked_delta(before.requested)?,
            achieved: self.achieved.checked_delta(before.achieved)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            requested: self.requested.checked_add(other.requested)?,
            achieved: self.achieved.checked_add(other.achieved)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionReplaceFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub requested_visible: u64,
    pub prior_visible: u64,
    pub visibility_ambiguous: u64,
    pub durability_ambiguous: u64,
    pub wall: ProjectionTimer,
}

impl ProjectionReplaceFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            requested_visible: 0,
            prior_visible: 0,
            visibility_ambiguous: 0,
            durability_ambiguous: 0,
            wall: ProjectionTimer::available(),
        }
    }

    fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            requested_visible: self
                .requested_visible
                .checked_sub(before.requested_visible)?,
            prior_visible: self.prior_visible.checked_sub(before.prior_visible)?,
            visibility_ambiguous: self
                .visibility_ambiguous
                .checked_sub(before.visibility_ambiguous)?,
            durability_ambiguous: self
                .durability_ambiguous
                .checked_sub(before.durability_ambiguous)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            requested_visible: self
                .requested_visible
                .checked_add(other.requested_visible)?,
            prior_visible: self.prior_visible.checked_add(other.prior_visible)?,
            visibility_ambiguous: self
                .visibility_ambiguous
                .checked_add(other.visibility_ambiguous)?,
            durability_ambiguous: self
                .durability_ambiguous
                .checked_add(other.durability_ambiguous)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionCleanupFacts {
    pub attempts: u64,
    pub successes: u64,
    pub failures: u64,
    pub residue: u64,
    pub wall: ProjectionTimer,
}

impl ProjectionCleanupFacts {
    pub const fn available() -> Self {
        Self {
            attempts: 0,
            successes: 0,
            failures: 0,
            residue: 0,
            wall: ProjectionTimer::available(),
        }
    }

    fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_sub(before.attempts)?,
            successes: self.successes.checked_sub(before.successes)?,
            failures: self.failures.checked_sub(before.failures)?,
            residue: self.residue.checked_sub(before.residue)?,
            wall: self.wall.checked_delta(before.wall)?,
        })
    }

    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            attempts: self.attempts.checked_add(other.attempts)?,
            successes: self.successes.checked_add(other.successes)?,
            failures: self.failures.checked_add(other.failures)?,
            residue: self.residue.checked_add(other.residue)?,
            wall: self.wall.checked_add(other.wall)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionFacts {
    pub workspace_setup: ProjectionCallFacts,
    pub workspace_root_create_open: ProjectionCallFacts,
    pub staging_create_open: ProjectionCallFacts,
    pub recovery_marker_create: ProjectionCallFacts,
    pub name_preflight: ProjectionCallFacts,
    pub temp_create: ProjectionCallFacts,
    pub workspace_marker_write: ProjectionWriteFacts,
    pub content_write: ProjectionWriteFacts,
    pub metadata_value_write: ProjectionWriteFacts,
    /// Inclusive report-only sum of marker, content, and metadata writes.
    pub aggregate_native_write: ProjectionWriteFacts,
    pub content_flush: ProjectionCallFacts,
    pub metadata_validate: ProjectionCallFacts,
    pub metadata_apply: ProjectionCallFacts,
    pub metadata_preinstall_verify: ProjectionCallFacts,
    pub metadata_postinstall_verify: ProjectionCallFacts,
    pub root_binding_revalidate: ProjectionCallFacts,
    pub regular_file_sync: ProjectionSyncFacts,
    pub directory_sync: ProjectionSyncFacts,
    pub recovery_marker_file_sync: ProjectionSyncFacts,
    pub content_temp_file_sync: ProjectionSyncFacts,
    pub post_hardlink_file_sync: ProjectionSyncFacts,
    pub staging_directory_sync: ProjectionSyncFacts,
    pub root_parent_directory_sync: ProjectionSyncFacts,
    pub install_parent_directory_sync: ProjectionSyncFacts,
    pub dirty_tree_directory_sync: ProjectionSyncFacts,
    pub final_root_directory_sync: ProjectionSyncFacts,
    pub replace: ProjectionReplaceFacts,
    pub authority_completion: ProjectionCallFacts,
    pub cleanup: ProjectionCleanupFacts,
}

impl ProjectionFacts {
    pub const fn available() -> Self {
        Self {
            workspace_setup: ProjectionCallFacts::available(),
            workspace_root_create_open: ProjectionCallFacts::available(),
            staging_create_open: ProjectionCallFacts::available(),
            recovery_marker_create: ProjectionCallFacts::available(),
            name_preflight: ProjectionCallFacts::available(),
            temp_create: ProjectionCallFacts::available(),
            workspace_marker_write: ProjectionWriteFacts::available(),
            content_write: ProjectionWriteFacts::available(),
            metadata_value_write: ProjectionWriteFacts::available(),
            aggregate_native_write: ProjectionWriteFacts::available(),
            content_flush: ProjectionCallFacts::available(),
            metadata_validate: ProjectionCallFacts::available(),
            metadata_apply: ProjectionCallFacts::available(),
            metadata_preinstall_verify: ProjectionCallFacts::available(),
            metadata_postinstall_verify: ProjectionCallFacts::available(),
            root_binding_revalidate: ProjectionCallFacts::available(),
            regular_file_sync: ProjectionSyncFacts::available(),
            directory_sync: ProjectionSyncFacts::available(),
            recovery_marker_file_sync: ProjectionSyncFacts::available(),
            content_temp_file_sync: ProjectionSyncFacts::available(),
            post_hardlink_file_sync: ProjectionSyncFacts::available(),
            staging_directory_sync: ProjectionSyncFacts::available(),
            root_parent_directory_sync: ProjectionSyncFacts::available(),
            install_parent_directory_sync: ProjectionSyncFacts::available(),
            dirty_tree_directory_sync: ProjectionSyncFacts::available(),
            final_root_directory_sync: ProjectionSyncFacts::available(),
            replace: ProjectionReplaceFacts::available(),
            authority_completion: ProjectionCallFacts::available(),
            cleanup: ProjectionCleanupFacts::available(),
        }
    }

    pub fn checked_delta(self, before: Self) -> Option<Self> {
        Some(Self {
            workspace_setup: self.workspace_setup.checked_delta(before.workspace_setup)?,
            workspace_root_create_open: self
                .workspace_root_create_open
                .checked_delta(before.workspace_root_create_open)?,
            staging_create_open: self
                .staging_create_open
                .checked_delta(before.staging_create_open)?,
            recovery_marker_create: self
                .recovery_marker_create
                .checked_delta(before.recovery_marker_create)?,
            name_preflight: self.name_preflight.checked_delta(before.name_preflight)?,
            temp_create: self.temp_create.checked_delta(before.temp_create)?,
            workspace_marker_write: self
                .workspace_marker_write
                .checked_delta(before.workspace_marker_write)?,
            content_write: self.content_write.checked_delta(before.content_write)?,
            metadata_value_write: self
                .metadata_value_write
                .checked_delta(before.metadata_value_write)?,
            aggregate_native_write: self
                .aggregate_native_write
                .checked_delta(before.aggregate_native_write)?,
            content_flush: self.content_flush.checked_delta(before.content_flush)?,
            metadata_validate: self
                .metadata_validate
                .checked_delta(before.metadata_validate)?,
            metadata_apply: self.metadata_apply.checked_delta(before.metadata_apply)?,
            metadata_preinstall_verify: self
                .metadata_preinstall_verify
                .checked_delta(before.metadata_preinstall_verify)?,
            metadata_postinstall_verify: self
                .metadata_postinstall_verify
                .checked_delta(before.metadata_postinstall_verify)?,
            root_binding_revalidate: self
                .root_binding_revalidate
                .checked_delta(before.root_binding_revalidate)?,
            regular_file_sync: self
                .regular_file_sync
                .checked_delta(before.regular_file_sync)?,
            directory_sync: self.directory_sync.checked_delta(before.directory_sync)?,
            recovery_marker_file_sync: self
                .recovery_marker_file_sync
                .checked_delta(before.recovery_marker_file_sync)?,
            content_temp_file_sync: self
                .content_temp_file_sync
                .checked_delta(before.content_temp_file_sync)?,
            post_hardlink_file_sync: self
                .post_hardlink_file_sync
                .checked_delta(before.post_hardlink_file_sync)?,
            staging_directory_sync: self
                .staging_directory_sync
                .checked_delta(before.staging_directory_sync)?,
            root_parent_directory_sync: self
                .root_parent_directory_sync
                .checked_delta(before.root_parent_directory_sync)?,
            install_parent_directory_sync: self
                .install_parent_directory_sync
                .checked_delta(before.install_parent_directory_sync)?,
            dirty_tree_directory_sync: self
                .dirty_tree_directory_sync
                .checked_delta(before.dirty_tree_directory_sync)?,
            final_root_directory_sync: self
                .final_root_directory_sync
                .checked_delta(before.final_root_directory_sync)?,
            replace: self.replace.checked_delta(before.replace)?,
            authority_completion: self
                .authority_completion
                .checked_delta(before.authority_completion)?,
            cleanup: self.cleanup.checked_delta(before.cleanup)?,
        })
    }

    pub fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            workspace_setup: self.workspace_setup.checked_add(other.workspace_setup)?,
            workspace_root_create_open: self
                .workspace_root_create_open
                .checked_add(other.workspace_root_create_open)?,
            staging_create_open: self
                .staging_create_open
                .checked_add(other.staging_create_open)?,
            recovery_marker_create: self
                .recovery_marker_create
                .checked_add(other.recovery_marker_create)?,
            name_preflight: self.name_preflight.checked_add(other.name_preflight)?,
            temp_create: self.temp_create.checked_add(other.temp_create)?,
            workspace_marker_write: self
                .workspace_marker_write
                .checked_add(other.workspace_marker_write)?,
            content_write: self.content_write.checked_add(other.content_write)?,
            metadata_value_write: self
                .metadata_value_write
                .checked_add(other.metadata_value_write)?,
            aggregate_native_write: self
                .aggregate_native_write
                .checked_add(other.aggregate_native_write)?,
            content_flush: self.content_flush.checked_add(other.content_flush)?,
            metadata_validate: self
                .metadata_validate
                .checked_add(other.metadata_validate)?,
            metadata_apply: self.metadata_apply.checked_add(other.metadata_apply)?,
            metadata_preinstall_verify: self
                .metadata_preinstall_verify
                .checked_add(other.metadata_preinstall_verify)?,
            metadata_postinstall_verify: self
                .metadata_postinstall_verify
                .checked_add(other.metadata_postinstall_verify)?,
            root_binding_revalidate: self
                .root_binding_revalidate
                .checked_add(other.root_binding_revalidate)?,
            regular_file_sync: self
                .regular_file_sync
                .checked_add(other.regular_file_sync)?,
            directory_sync: self.directory_sync.checked_add(other.directory_sync)?,
            recovery_marker_file_sync: self
                .recovery_marker_file_sync
                .checked_add(other.recovery_marker_file_sync)?,
            content_temp_file_sync: self
                .content_temp_file_sync
                .checked_add(other.content_temp_file_sync)?,
            post_hardlink_file_sync: self
                .post_hardlink_file_sync
                .checked_add(other.post_hardlink_file_sync)?,
            staging_directory_sync: self
                .staging_directory_sync
                .checked_add(other.staging_directory_sync)?,
            root_parent_directory_sync: self
                .root_parent_directory_sync
                .checked_add(other.root_parent_directory_sync)?,
            install_parent_directory_sync: self
                .install_parent_directory_sync
                .checked_add(other.install_parent_directory_sync)?,
            dirty_tree_directory_sync: self
                .dirty_tree_directory_sync
                .checked_add(other.dirty_tree_directory_sync)?,
            final_root_directory_sync: self
                .final_root_directory_sync
                .checked_add(other.final_root_directory_sync)?,
            replace: self.replace.checked_add(other.replace)?,
            authority_completion: self
                .authority_completion
                .checked_add(other.authority_completion)?,
            cleanup: self.cleanup.checked_add(other.cleanup)?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeKind {
    Directory,
    RegularFile,
    Symlink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeEntry {
    pub name: Vec<u8>,
    pub kind: NativeKind,
    pub token: Vec<u8>,
    pub hard_link_key: Option<Vec<u8>>,
    pub link_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeMetadata {
    pub mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
    pub xattrs: NativeXattrs,
    pub acl: Option<Vec<u8>>,
    pub bsd_flags: u32,
}

/// Compact native xattrs with no per-entry heap allocation. The accepted
/// name+value population remains one MiB; framing is split into <=1 MiB
/// chunks and is not canonical LayerFS storage.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NativeXattrs {
    chunks: Vec<Vec<u8>>,
    count: usize,
    payload_bytes: usize,
    last_name: Option<Vec<u8>>,
}

impl NativeXattrs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn payload_bytes(&self) -> usize {
        self.payload_bytes
    }

    pub fn from_entries(entries: impl IntoIterator<Item = (Vec<u8>, Vec<u8>)>) -> Result<Self> {
        let mut xattrs = Self::new();
        for (name, value) in entries {
            xattrs.push(&name, &value)?;
        }
        Ok(xattrs)
    }

    pub fn push(&mut self, name: &[u8], value: &[u8]) -> Result<()> {
        if name.is_empty()
            || name.len() > 127
            || name.contains(&0)
            || value.len() > MAX_NATIVE_XATTR_BYTES
            || self
                .last_name
                .as_deref()
                .is_some_and(|previous| previous >= name)
        {
            return Err(DriverError::Unsupported);
        }
        self.payload_bytes = self
            .payload_bytes
            .checked_add(name.len())
            .and_then(|total| total.checked_add(value.len()))
            .filter(|total| *total <= MAX_NATIVE_XATTR_BYTES)
            .ok_or(DriverError::Unsupported)?;
        append_varint(&mut self.chunks, name.len() as u32);
        append_varint(
            &mut self.chunks,
            u32::try_from(value.len()).map_err(|_| DriverError::Unsupported)?,
        );
        append_chunked(&mut self.chunks, name);
        append_chunked(&mut self.chunks, value);
        self.last_name = Some(name.to_vec());
        self.count = self.count.checked_add(1).ok_or(DriverError::Unsupported)?;
        Ok(())
    }

    pub fn iter(&self) -> NativeXattrIter<'_> {
        NativeXattrIter {
            xattrs: self,
            chunk: 0,
            offset: 0,
            remaining: self.count,
        }
    }

    pub fn names(&self) -> NativeXattrNameIter<'_> {
        NativeXattrNameIter { inner: self.iter() }
    }
}

impl<'a> IntoIterator for &'a NativeXattrs {
    type Item = (Vec<u8>, Vec<u8>);
    type IntoIter = NativeXattrIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct NativeXattrIter<'a> {
    xattrs: &'a NativeXattrs,
    chunk: usize,
    offset: usize,
    remaining: usize,
}

pub struct NativeXattrNameIter<'a> {
    inner: NativeXattrIter<'a>,
}

impl Iterator for NativeXattrNameIter<'_> {
    type Item = Vec<u8>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.inner.remaining == 0 {
            return None;
        }
        let name_len = self.inner.read_varint()? as usize;
        let value_len = self.inner.read_varint()? as usize;
        let name = self.inner.read_vec(name_len)?;
        self.inner.skip_bytes(value_len)?;
        self.inner.remaining -= 1;
        Some(name)
    }
}

impl Iterator for NativeXattrIter<'_> {
    type Item = (Vec<u8>, Vec<u8>);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let name_len = self.read_varint()? as usize;
        let value_len = self.read_varint()? as usize;
        let name = self.read_vec(name_len)?;
        let value = self.read_vec(value_len)?;
        self.remaining -= 1;
        Some((name, value))
    }
}

impl NativeXattrIter<'_> {
    fn read_byte(&mut self) -> Option<u8> {
        while self
            .xattrs
            .chunks
            .get(self.chunk)
            .is_some_and(|chunk| self.offset == chunk.len())
        {
            self.chunk += 1;
            self.offset = 0;
        }
        let byte = *self.xattrs.chunks.get(self.chunk)?.get(self.offset)?;
        self.offset += 1;
        Some(byte)
    }

    fn read_varint(&mut self) -> Option<u32> {
        let mut value = 0_u32;
        for shift in (0..=28).step_by(7) {
            let byte = self.read_byte()?;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Some(value);
            }
        }
        None
    }

    fn read_vec(&mut self, len: usize) -> Option<Vec<u8>> {
        let mut value = Vec::with_capacity(len);
        for _ in 0..len {
            value.push(self.read_byte()?);
        }
        Some(value)
    }

    fn skip_bytes(&mut self, len: usize) -> Option<()> {
        for _ in 0..len {
            self.read_byte()?;
        }
        Some(())
    }
}

fn append_varint(chunks: &mut Vec<Vec<u8>>, mut value: u32) {
    let mut bytes = [0_u8; 5];
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        bytes[len] = byte;
        len += 1;
        if value == 0 {
            append_chunked(chunks, &bytes[..len]);
            return;
        }
    }
}

fn append_chunked(chunks: &mut Vec<Vec<u8>>, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        if chunks
            .last()
            .is_none_or(|chunk| chunk.len() == NATIVE_XATTR_CHUNK_BYTES)
        {
            chunks.push(Vec::with_capacity(
                NATIVE_XATTR_CHUNK_BYTES.min(bytes.len()),
            ));
        }
        let chunk = chunks.last_mut().unwrap();
        let take = bytes
            .len()
            .min(NATIVE_XATTR_CHUNK_BYTES.saturating_sub(chunk.len()));
        chunk.extend_from_slice(&bytes[..take]);
        bytes = &bytes[take..];
    }
}

#[derive(Debug)]
pub enum DriverError {
    Unsupported,
    NativeProtected,
    Conflict,
    VisibilityAmbiguous,
    DurabilityAmbiguous,
    Io(io::Error),
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported => f.write_str("native operation is unsupported"),
            Self::NativeProtected => f.write_str("native object is protected"),
            Self::Conflict => f.write_str("native object changed"),
            Self::VisibilityAmbiguous => f.write_str("native visibility is ambiguous"),
            Self::DurabilityAmbiguous => f.write_str("native durability is ambiguous"),
            Self::Io(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<io::Error> for DriverError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, DriverError>;

pub trait ProjectionWorkspace: Send {
    fn projection_facts(&self) -> ProjectionFacts {
        ProjectionFacts::default()
    }
    fn root_directory(&self) -> Result<Box<dyn DirectoryHandle>>;
    fn enumerate_at<'a>(
        &'a self,
        parent: &'a dyn DirectoryHandle,
    ) -> Result<Box<dyn Iterator<Item = Result<NativeEntry>> + 'a>>;
    fn open_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn DirectoryHandle>>;
    fn duplicate_directory(
        &self,
        directory: &dyn DirectoryHandle,
    ) -> Result<Box<dyn DirectoryHandle>>;
    fn directory_token(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>>;
    fn directory_identity(&self, directory: &dyn DirectoryHandle) -> Result<Vec<u8>>;
    fn revalidate_root_binding(&self) -> Result<()>;
    fn begin_name_preflight(&self) -> Result<Box<dyn NamePreflight>>;
    fn open_regular_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn RegularFileHandle>>;
    fn open_regular_read_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Box<dyn RegularFileHandle>>;
    fn set_regular_len(&self, file: &mut dyn RegularFileHandle, len: u64) -> Result<()>;
    fn sync_regular(&self, file: &mut dyn RegularFileHandle) -> Result<()>;
    fn read_link_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<Vec<u8>>;
    fn read_metadata_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<NativeMetadata>;
    fn token_at(&self, parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>>;
    fn identity_at(&self, parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>>;
    fn read_root_metadata(&self) -> Result<NativeMetadata>;
    fn read_directory_metadata(&self, directory: &dyn DirectoryHandle) -> Result<NativeMetadata>;
    fn create_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
    ) -> Result<Box<dyn DirectoryHandle>>;
    fn create_temp_at(&self, parent: &dyn DirectoryHandle) -> Result<Box<dyn OwnedTempHandle>>;
    fn clone_temp_from_regular(
        &self,
        source: &dyn RegularFileHandle,
    ) -> Result<Box<dyn OwnedTempHandle>>;
    fn read_temp_metadata(&self, temp: &dyn OwnedTempHandle) -> Result<NativeMetadata>;
    fn set_temp_metadata(
        &self,
        temp: &mut dyn OwnedTempHandle,
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn set_entry_metadata(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn atomic_replace(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
    ) -> Result<()>;
    fn atomic_replace_with_directory_durability(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        _requested: DirectoryDurability,
    ) -> Result<DirectoryDurability> {
        self.atomic_replace(temp, parent, name)?;
        Ok(DirectoryDurability::ImmediateDirectoryDurability)
    }
    fn atomic_replace_checked(
        &self,
        temp: Box<dyn OwnedTempHandle>,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
    ) -> Result<()>;
    fn create_symlink_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        target: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn atomic_replace_symlink(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: Option<&[u8]>,
        target: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn create_hard_link_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        source_expected: &[u8],
        target_parent: &dyn DirectoryHandle,
        target: &[u8],
    ) -> Result<()>;
    fn finish_hard_link_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        source_expected: &[u8],
        metadata: &NativeMetadata,
    ) -> Result<()>;
    fn rename_at(
        &self,
        source_parent: &dyn DirectoryHandle,
        source: &[u8],
        target_parent: &dyn DirectoryHandle,
        target: &[u8],
    ) -> Result<()>;
    fn unlink_regular_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()>;
    fn unlink_symlink_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()>;
    fn remove_directory_at(
        &self,
        parent: &dyn DirectoryHandle,
        name: &[u8],
        expected: &[u8],
    ) -> Result<()>;
    fn sync_directory(&self, directory: &dyn DirectoryHandle) -> Result<()>;
    fn set_root_metadata(&self, metadata: &NativeMetadata) -> Result<()>;
    /// Discards the root created by `ManagedCreateOwned` when admission fails
    /// before the portable layer can obtain a root handle or stable identity.
    /// The workspace retains the creation-time identity needed to remove only
    /// that exact owned root.
    fn discard_owned_root(self: Box<Self>) -> Result<()>;
    fn remove_owned_root(&self, expected_identity: &[u8]) -> Result<()>;
}

pub trait ProjectionDriver: Send + Sync {
    fn projection_facts(&self) -> ProjectionFacts {
        ProjectionFacts::default()
    }
    fn open_workspace(
        &self,
        path: &Path,
        store_id: [u8; 32],
    ) -> Result<Box<dyn ProjectionWorkspace>>;
    fn recover_owned_workspaces(&self, _parent: &Path, _store_id: [u8; 32]) -> Result<()> {
        Ok(())
    }
}
