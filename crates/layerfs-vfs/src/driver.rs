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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspacePolicy {
    ManagedCreateOwned,
    ManagedPrivate,
    ExternalCooperative,
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
        policy: WorkspacePolicy,
        store_id: [u8; 32],
    ) -> Result<Box<dyn ProjectionWorkspace>>;
    fn recover_owned_workspaces(&self, _parent: &Path, _store_id: [u8; 32]) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_core::content::rope::ObjectRead;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct MemoryWorkspace {
        fail_replace: bool,
        fail_root: bool,
        fail_identity: bool,
        discarded: Arc<AtomicBool>,
        materialize: Option<Arc<Mutex<MaterializeState>>>,
        fault: Option<MaterializeFault>,
    }
    struct MemoryTemp(std::io::Cursor<Vec<u8>>);
    struct MemoryPreflight;
    impl NamePreflight for MemoryPreflight {
        fn add(&mut self, _name: &[u8]) -> Result<()> {
            Ok(())
        }
        fn finish(self: Box<Self>) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum MaterializeFault {
        TempCreate,
        WriteZero,
        WriteShort,
        WriteError,
        Metadata,
        FileSync,
        RenameBeforeVisibility,
        RenameAfterVisibility,
        DirectorySync(u64),
        RootRevalidation,
        HardLink(u64),
        HardLinkBeforeMetadata,
        HardLinkAfterMetadata,
        HardLinkAfterFinalSync,
    }

    #[derive(Default)]
    struct MaterializeState {
        events: Vec<String>,
        files: BTreeMap<Vec<u8>, Vec<u8>>,
        directories: Vec<Vec<u8>>,
        directory_syncs: Vec<Vec<u8>>,
        revalidations: u64,
        hard_links: u64,
        removed: bool,
    }

    struct Dir(Vec<u8>);
    impl DirectoryHandle for Dir {
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    struct MaterializeTemp {
        bytes: std::io::Cursor<Vec<u8>>,
        parent: Vec<u8>,
        fault: Option<MaterializeFault>,
        state: Arc<Mutex<MaterializeState>>,
    }

    impl Read for MaterializeTemp {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.bytes.read(bytes)
        }
    }
    impl Write for MaterializeTemp {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.state.lock().unwrap().events.push("write".into());
            match self.fault {
                Some(MaterializeFault::WriteZero) => Ok(0),
                Some(MaterializeFault::WriteShort) => {
                    self.bytes.write(&bytes[..bytes.len().min(1)])
                }
                Some(MaterializeFault::WriteError) => {
                    Err(io::Error::other("injected content-write failure"))
                }
                _ => self.bytes.write(bytes),
            }
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Seek for MaterializeTemp {
        fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
            self.bytes.seek(position)
        }
    }
    impl OwnedTempHandle for MaterializeTemp {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn set_len(&mut self, len: u64) -> Result<()> {
            self.bytes.get_mut().resize(
                usize::try_from(len).map_err(|_| DriverError::Unsupported)?,
                0,
            );
            Ok(())
        }
        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }
    }

    impl MemoryWorkspace {
        fn install_temp(
            &self,
            temp: Box<dyn OwnedTempHandle>,
            parent: &dyn DirectoryHandle,
            name: &[u8],
            requested: DirectoryDurability,
        ) -> Result<DirectoryDurability> {
            let temp = temp
                .into_any()
                .downcast::<MaterializeTemp>()
                .map_err(|_| DriverError::Conflict)?;
            let path = entry_path(parent, name)?;
            if temp.parent
                != parent
                    .as_any()
                    .downcast_ref::<Dir>()
                    .ok_or(DriverError::Conflict)?
                    .0
            {
                return Err(DriverError::Conflict);
            }
            let mut state = temp.state.lock().unwrap();
            state.events.push("file_sync".into());
            if self.fault == Some(MaterializeFault::FileSync) {
                return Err(DriverError::DurabilityAmbiguous);
            }
            state.events.push(format!("rename:{}", display_path(&path)));
            if self.fault == Some(MaterializeFault::RenameBeforeVisibility) {
                return Err(DriverError::Io(io::Error::other(
                    "injected pre-visibility rename failure",
                )));
            }
            state.files.insert(path, temp.bytes.into_inner());
            if self.fault == Some(MaterializeFault::RenameAfterVisibility) {
                return Err(DriverError::VisibilityAmbiguous);
            }
            if requested == DirectoryDurability::ImmediateDirectoryDurability {
                let parent = parent
                    .as_any()
                    .downcast_ref::<Dir>()
                    .ok_or(DriverError::Conflict)?
                    .0
                    .clone();
                state.directory_syncs.push(parent.clone());
                state
                    .events
                    .push(format!("directory_sync:{}", display_path(&parent)));
            }
            Ok(requested)
        }
    }

    fn entry_path(parent: &dyn DirectoryHandle, name: &[u8]) -> Result<Vec<u8>> {
        let parent = parent
            .as_any()
            .downcast_ref::<Dir>()
            .ok_or(DriverError::Conflict)?;
        let mut path = parent.0.clone();
        if !path.is_empty() {
            path.push(b'/');
        }
        path.extend_from_slice(name);
        Ok(path)
    }

    fn display_path(path: &[u8]) -> String {
        String::from_utf8_lossy(path).into_owned()
    }

    fn clear_owned_state(state: &mut MaterializeState) {
        state.events.push("owned_root_removed".into());
        state.files.clear();
        state.directories.clear();
        state.removed = true;
    }

    impl Read for MemoryTemp {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            self.0.read(bytes)
        }
    }
    impl Write for MemoryTemp {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    impl Seek for MemoryTemp {
        fn seek(&mut self, position: io::SeekFrom) -> io::Result<u64> {
            self.0.seek(position)
        }
    }
    impl OwnedTempHandle for MemoryTemp {
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn set_len(&mut self, len: u64) -> Result<()> {
            self.0.get_mut().resize(
                usize::try_from(len).map_err(|_| DriverError::Unsupported)?,
                0,
            );
            Ok(())
        }
        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }
    }

    impl ProjectionWorkspace for MemoryWorkspace {
        fn root_directory(&self) -> Result<Box<dyn DirectoryHandle>> {
            if self.fail_root {
                Err(DriverError::Io(io::Error::other(
                    "injected root-handle failure",
                )))
            } else {
                Ok(Box::new(Dir(Vec::new())))
            }
        }

        fn enumerate_at<'a>(
            &'a self,
            _parent: &'a dyn DirectoryHandle,
        ) -> Result<Box<dyn Iterator<Item = Result<NativeEntry>> + 'a>> {
            if self.materialize.is_some() {
                return Ok(Box::new(std::iter::empty()));
            }
            Ok(Box::new(
                [NativeEntry {
                    name: b"file".to_vec(),
                    kind: NativeKind::RegularFile,
                    token: vec![1],
                    hard_link_key: None,
                    link_count: 1,
                }]
                .into_iter()
                .map(Ok),
            ))
        }

        fn open_directory_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<Box<dyn DirectoryHandle>> {
            Err(DriverError::Unsupported)
        }
        fn duplicate_directory(
            &self,
            directory: &dyn DirectoryHandle,
        ) -> Result<Box<dyn DirectoryHandle>> {
            let directory = directory
                .as_any()
                .downcast_ref::<Dir>()
                .ok_or(DriverError::Conflict)?;
            Ok(Box::new(Dir(directory.0.clone())))
        }
        fn directory_token(&self, _directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
            Ok(vec![1])
        }
        fn directory_identity(&self, _directory: &dyn DirectoryHandle) -> Result<Vec<u8>> {
            if self.fail_identity {
                Err(DriverError::Io(io::Error::other(
                    "injected root-identity failure",
                )))
            } else {
                Ok(vec![1])
            }
        }
        fn revalidate_root_binding(&self) -> Result<()> {
            if let Some(state) = &self.materialize {
                let mut state = state.lock().unwrap();
                state.revalidations += 1;
                state.events.push("root_revalidate".into());
                if self.fault == Some(MaterializeFault::RootRevalidation)
                    && state.revalidations == 2
                {
                    return Err(DriverError::Conflict);
                }
            }
            Ok(())
        }
        fn begin_name_preflight(&self) -> Result<Box<dyn NamePreflight>> {
            Ok(Box::new(MemoryPreflight))
        }
        fn open_regular_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<Box<dyn RegularFileHandle>> {
            Err(DriverError::Unsupported)
        }
        fn open_regular_read_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<Box<dyn RegularFileHandle>> {
            Err(DriverError::Unsupported)
        }
        fn set_regular_len(&self, _file: &mut dyn RegularFileHandle, _len: u64) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn sync_regular(&self, _file: &mut dyn RegularFileHandle) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn read_link_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<Vec<u8>> {
            Err(DriverError::Unsupported)
        }
        fn read_metadata_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<NativeMetadata> {
            Err(DriverError::Unsupported)
        }
        fn token_at(&self, _parent: &dyn DirectoryHandle, _name: &[u8]) -> Result<Vec<u8>> {
            Err(DriverError::Unsupported)
        }
        fn identity_at(&self, _parent: &dyn DirectoryHandle, _name: &[u8]) -> Result<Vec<u8>> {
            entry_path(_parent, _name)
        }
        fn read_root_metadata(&self) -> Result<NativeMetadata> {
            Err(DriverError::Unsupported)
        }
        fn read_directory_metadata(
            &self,
            _directory: &dyn DirectoryHandle,
        ) -> Result<NativeMetadata> {
            Err(DriverError::Unsupported)
        }
        fn create_directory_at(
            &self,
            parent: &dyn DirectoryHandle,
            name: &[u8],
        ) -> Result<Box<dyn DirectoryHandle>> {
            let path = entry_path(parent, name)?;
            let Some(state) = &self.materialize else {
                return Err(DriverError::Unsupported);
            };
            let mut state = state.lock().unwrap();
            state.events.push(format!("mkdir:{}", display_path(&path)));
            state.directories.push(path.clone());
            Ok(Box::new(Dir(path)))
        }
        fn create_temp_at(&self, parent: &dyn DirectoryHandle) -> Result<Box<dyn OwnedTempHandle>> {
            let Some(state) = &self.materialize else {
                return Err(DriverError::Unsupported);
            };
            state.lock().unwrap().events.push("temp_create".into());
            if self.fault == Some(MaterializeFault::TempCreate) {
                return Err(DriverError::Io(io::Error::other(
                    "injected temp-create failure",
                )));
            }
            let parent = parent
                .as_any()
                .downcast_ref::<Dir>()
                .ok_or(DriverError::Conflict)?;
            Ok(Box::new(MaterializeTemp {
                bytes: std::io::Cursor::new(Vec::new()),
                parent: parent.0.clone(),
                fault: self.fault,
                state: state.clone(),
            }))
        }
        fn clone_temp_from_regular(
            &self,
            _source: &dyn RegularFileHandle,
        ) -> Result<Box<dyn OwnedTempHandle>> {
            Err(DriverError::Unsupported)
        }
        fn read_temp_metadata(&self, _temp: &dyn OwnedTempHandle) -> Result<NativeMetadata> {
            Err(DriverError::Unsupported)
        }
        fn set_temp_metadata(
            &self,
            _temp: &mut dyn OwnedTempHandle,
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            let Some(state) = &self.materialize else {
                return Err(DriverError::Unsupported);
            };
            state.lock().unwrap().events.push("metadata".into());
            if self.fault == Some(MaterializeFault::Metadata) {
                Err(DriverError::Io(io::Error::other(
                    "injected metadata failure",
                )))
            } else {
                Ok(())
            }
        }
        fn set_entry_metadata(
            &self,
            parent: &dyn DirectoryHandle,
            name: &[u8],
            _expected: &[u8],
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            let Some(state) = &self.materialize else {
                return Err(DriverError::Unsupported);
            };
            state.lock().unwrap().events.push(format!(
                "directory_metadata:{}",
                display_path(&entry_path(parent, name)?)
            ));
            Ok(())
        }
        fn atomic_replace(
            &self,
            temp: Box<dyn OwnedTempHandle>,
            parent: &dyn DirectoryHandle,
            name: &[u8],
        ) -> Result<()> {
            if self.fail_replace {
                Err(DriverError::DurabilityAmbiguous)
            } else if self.materialize.is_some() {
                self.install_temp(
                    temp,
                    parent,
                    name,
                    DirectoryDurability::ImmediateDirectoryDurability,
                )
                .map(drop)
            } else {
                Ok(())
            }
        }
        fn atomic_replace_with_directory_durability(
            &self,
            temp: Box<dyn OwnedTempHandle>,
            parent: &dyn DirectoryHandle,
            name: &[u8],
            requested: DirectoryDurability,
        ) -> Result<DirectoryDurability> {
            if self.materialize.is_some() {
                self.install_temp(temp, parent, name, requested)
            } else {
                self.atomic_replace(temp, parent, name)?;
                Ok(DirectoryDurability::ImmediateDirectoryDurability)
            }
        }
        fn atomic_replace_checked(
            &self,
            temp: Box<dyn OwnedTempHandle>,
            parent: &dyn DirectoryHandle,
            name: &[u8],
            _expected: Option<&[u8]>,
        ) -> Result<()> {
            if self.materialize.is_some() {
                self.install_temp(
                    temp,
                    parent,
                    name,
                    DirectoryDurability::ImmediateDirectoryDurability,
                )
                .map(drop)
            } else {
                Err(DriverError::Unsupported)
            }
        }
        fn create_symlink_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _target: &[u8],
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn atomic_replace_symlink(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: Option<&[u8]>,
            _target: &[u8],
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn create_hard_link_at(
            &self,
            source_parent: &dyn DirectoryHandle,
            source: &[u8],
            _source_expected: &[u8],
            target_parent: &dyn DirectoryHandle,
            target: &[u8],
        ) -> Result<()> {
            let Some(state) = &self.materialize else {
                return Err(DriverError::Unsupported);
            };
            let source = entry_path(source_parent, source)?;
            let target = entry_path(target_parent, target)?;
            let mut state = state.lock().unwrap();
            state.hard_links += 1;
            state
                .events
                .push(format!("hard_link:{}", display_path(&target)));
            if self.fault == Some(MaterializeFault::HardLink(state.hard_links)) {
                return Err(DriverError::Io(io::Error::other(
                    "injected hard-link failure",
                )));
            }
            let bytes = state
                .files
                .get(&source)
                .cloned()
                .ok_or(DriverError::Conflict)?;
            state.files.insert(target, bytes);
            Ok(())
        }
        fn finish_hard_link_at(
            &self,
            _source_parent: &dyn DirectoryHandle,
            _source: &[u8],
            _source_expected: &[u8],
            _metadata: &NativeMetadata,
        ) -> Result<()> {
            let Some(state) = &self.materialize else {
                return Err(DriverError::Unsupported);
            };
            let mut state = state.lock().unwrap();
            state.events.push("hard_link_before_metadata".into());
            if self.fault == Some(MaterializeFault::HardLinkBeforeMetadata) {
                return Err(DriverError::Io(io::Error::other(
                    "injected pre-metadata hard-link failure",
                )));
            }
            state.events.push("hard_link_metadata".into());
            if self.fault == Some(MaterializeFault::HardLinkAfterMetadata) {
                return Err(DriverError::Io(io::Error::other(
                    "injected post-metadata hard-link failure",
                )));
            }
            state.events.push("hard_link_final_sync".into());
            if self.fault == Some(MaterializeFault::HardLinkAfterFinalSync) {
                return Err(DriverError::DurabilityAmbiguous);
            }
            Ok(())
        }
        fn rename_at(
            &self,
            _source_parent: &dyn DirectoryHandle,
            _source: &[u8],
            _target_parent: &dyn DirectoryHandle,
            _target: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn unlink_regular_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn unlink_symlink_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn remove_directory_at(
            &self,
            _parent: &dyn DirectoryHandle,
            _name: &[u8],
            _expected: &[u8],
        ) -> Result<()> {
            Err(DriverError::Unsupported)
        }
        fn sync_directory(&self, directory: &dyn DirectoryHandle) -> Result<()> {
            if let Some(state) = &self.materialize {
                let directory = directory
                    .as_any()
                    .downcast_ref::<Dir>()
                    .ok_or(DriverError::Conflict)?;
                let mut state = state.lock().unwrap();
                state.directory_syncs.push(directory.0.clone());
                state
                    .events
                    .push(format!("directory_sync:{}", display_path(&directory.0)));
                let sync = u64::try_from(state.directory_syncs.len()).unwrap();
                if self.fault == Some(MaterializeFault::DirectorySync(sync)) {
                    return Err(DriverError::DurabilityAmbiguous);
                }
            }
            Ok(())
        }
        fn set_root_metadata(&self, _metadata: &NativeMetadata) -> Result<()> {
            if let Some(state) = &self.materialize {
                state.lock().unwrap().events.push("root_metadata".into());
                Ok(())
            } else {
                Err(DriverError::Unsupported)
            }
        }
        fn discard_owned_root(self: Box<Self>) -> Result<()> {
            self.discarded.store(true, Ordering::Release);
            if let Some(state) = &self.materialize {
                clear_owned_state(&mut state.lock().unwrap());
            }
            Ok(())
        }
        fn remove_owned_root(&self, expected_identity: &[u8]) -> Result<()> {
            let Some(state) = &self.materialize else {
                return Err(DriverError::Unsupported);
            };
            if expected_identity != [1] {
                return Err(DriverError::Conflict);
            }
            clear_owned_state(&mut state.lock().unwrap());
            Ok(())
        }
    }

    struct MemoryDriver;

    impl ProjectionDriver for MemoryDriver {
        fn open_workspace(
            &self,
            _path: &Path,
            _policy: WorkspacePolicy,
            _store_id: [u8; 32],
        ) -> Result<Box<dyn ProjectionWorkspace>> {
            Ok(Box::new(MemoryWorkspace::default()))
        }
    }

    struct MaterializeDriver {
        state: Arc<Mutex<MaterializeState>>,
        fault: Option<MaterializeFault>,
    }

    impl ProjectionDriver for MaterializeDriver {
        fn open_workspace(
            &self,
            _path: &Path,
            _policy: WorkspacePolicy,
            _store_id: [u8; 32],
        ) -> Result<Box<dyn ProjectionWorkspace>> {
            Ok(Box::new(MemoryWorkspace {
                materialize: Some(self.state.clone()),
                fault: self.fault,
                ..MemoryWorkspace::default()
            }))
        }
    }

    static TEST_SERIAL: AtomicU64 = AtomicU64::new(0);

    fn test_vfs(
        fault: Option<MaterializeFault>,
    ) -> (
        std::path::PathBuf,
        crate::workspace::LayerVfs,
        Arc<Mutex<MaterializeState>>,
    ) {
        let base = std::env::temp_dir().join(format!(
            "layerfs-vfs-fault-{}-{}",
            std::process::id(),
            TEST_SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&base).unwrap();
        let state = Arc::new(Mutex::new(MaterializeState::default()));
        let driver = Arc::new(MaterializeDriver {
            state: state.clone(),
            fault,
        });
        let vfs = crate::workspace::LayerVfs::open(&base.join("store.sqlite"), driver).unwrap();
        (base, vfs, state)
    }

    fn flat_file(vfs: &crate::workspace::LayerVfs) -> layerfs_engine::refs::RefState {
        let head = vfs.current_head("main").unwrap();
        vfs.replace_file(
            &head,
            &layerfs_core::CanonicalPath::new("file").unwrap(),
            io::Cursor::new(b"portable-fault-matrix"),
        )
        .unwrap()
        .0
    }

    fn nested_two_files(vfs: &crate::workspace::LayerVfs) -> layerfs_engine::refs::RefState {
        use layerfs_core::content::rope::build;
        use layerfs_core::inode::{
            inode_table_lookup, inode_table_upsert, InodeKind, InodeRecordV1, InodeTableCounters,
            InodeTableRoot,
        };
        use layerfs_core::namespace::{
            directory_insert, empty_directory, DirectoryStateRoot, NamespaceRootV1,
        };
        use layerfs_core::namespace_codec::{
            decode_inode_record, decode_namespace_root, encode_inode_record, encode_namespace_root,
        };
        use layerfs_core::CanonicalName;

        let head = vfs.current_head("main").unwrap();
        let mut publication = vfs.engine.begin_publication(Some(&head), "main").unwrap();
        let namespace = publication
            .with_authenticated_canonical(head.root, decode_namespace_root)
            .unwrap();
        let table = InodeTableRoot(namespace.inode_table_root);
        let mut visits = InodeTableCounters::default();
        let root_record_id = inode_table_lookup(
            &publication,
            table,
            namespace.root_directory_inode,
            &mut visits,
        )
        .unwrap()
        .unwrap();
        let root_record = publication
            .with_authenticated_canonical(root_record_id, decode_inode_record)
            .unwrap();

        let directory_inode = publication.allocate_inode_id().unwrap();
        let mut directory = empty_directory(&mut publication).unwrap();
        let mut file_records = Vec::new();
        for (name, bytes) in [(b"one".as_slice(), b"one".as_slice()), (b"two", b"two")] {
            let inode = publication.allocate_inode_id().unwrap();
            let (content, _) = build(&mut publication, io::Cursor::new(bytes)).unwrap();
            let metadata = crate::capture::put_metadata(
                &mut publication,
                InodeKind::RegularFile,
                &NativeMetadata {
                    mode: 0o644,
                    mtime_seconds: 0,
                    mtime_nanoseconds: 0,
                    xattrs: NativeXattrs::new(),
                    acl: None,
                    bsd_flags: 0,
                },
            )
            .unwrap();
            let record = publication
                .put_object(
                    &encode_inode_record(InodeRecordV1 {
                        kind: InodeKind::RegularFile,
                        namespace_ref_count: 1,
                        content_root: content.0,
                        metadata_root: metadata,
                    })
                    .unwrap(),
                )
                .unwrap();
            directory = directory_insert(
                &mut publication,
                directory,
                CanonicalName::from_bytes(name).unwrap(),
                inode,
            )
            .unwrap()
            .0;
            file_records.push((inode, record));
        }

        let directory_metadata = crate::capture::put_metadata(
            &mut publication,
            InodeKind::Directory,
            &NativeMetadata {
                mode: 0o755,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
                xattrs: NativeXattrs::new(),
                acl: None,
                bsd_flags: 0,
            },
        )
        .unwrap();
        let directory_record = publication
            .put_object(
                &encode_inode_record(InodeRecordV1 {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 1,
                    content_root: directory.0,
                    metadata_root: directory_metadata,
                })
                .unwrap(),
            )
            .unwrap();
        let root_directory = directory_insert(
            &mut publication,
            DirectoryStateRoot(root_record.content_root),
            CanonicalName::from_bytes(b"nested").unwrap(),
            directory_inode,
        )
        .unwrap()
        .0;
        let root_record = publication
            .put_object(
                &encode_inode_record(InodeRecordV1 {
                    content_root: root_directory.0,
                    ..root_record
                })
                .unwrap(),
            )
            .unwrap();
        let mut table = table;
        for (inode, record) in file_records {
            table = inode_table_upsert(&mut publication, table, inode, record)
                .unwrap()
                .0;
        }
        table = inode_table_upsert(&mut publication, table, directory_inode, directory_record)
            .unwrap()
            .0;
        table = inode_table_upsert(
            &mut publication,
            table,
            namespace.root_directory_inode,
            root_record,
        )
        .unwrap()
        .0;
        publication
            .publish_namespace(
                &encode_namespace_root(NamespaceRootV1 {
                    inode_table_root: table.0,
                    ..namespace
                })
                .unwrap(),
            )
            .unwrap()
    }

    fn three_hard_links(vfs: &crate::workspace::LayerVfs) -> layerfs_engine::refs::RefState {
        use layerfs_core::inode::{
            inode_table_lookup, inode_table_upsert, InodeKind, InodeRecordV1, InodeTableCounters,
            InodeTableRoot,
        };
        use layerfs_core::namespace::{directory_insert, directory_lookup, DirectoryStateRoot};
        use layerfs_core::namespace_codec::{
            decode_inode_record, decode_namespace_root, encode_inode_record, encode_namespace_root,
        };
        use layerfs_core::CanonicalName;

        let head = flat_file(vfs);
        let mut publication = vfs.engine.begin_publication(Some(&head), "main").unwrap();
        let namespace = publication
            .with_authenticated_canonical(head.root, decode_namespace_root)
            .unwrap();
        let mut inode_visits = InodeTableCounters::default();
        let table = InodeTableRoot(namespace.inode_table_root);
        let root_record_id = inode_table_lookup(
            &publication,
            table,
            namespace.root_directory_inode,
            &mut inode_visits,
        )
        .unwrap()
        .unwrap();
        let root_record = publication
            .with_authenticated_canonical(root_record_id, decode_inode_record)
            .unwrap();
        let directory = DirectoryStateRoot(root_record.content_root);
        let mut namespace_visits = layerfs_core::namespace::NamespaceCounters::default();
        let inode = directory_lookup(
            &publication,
            directory,
            &CanonicalName::from_bytes(b"file").unwrap(),
            &mut namespace_visits,
        )
        .unwrap()
        .unwrap();
        let record_id = inode_table_lookup(&publication, table, inode, &mut inode_visits)
            .unwrap()
            .unwrap();
        let record = publication
            .with_authenticated_canonical(record_id, decode_inode_record)
            .unwrap();
        let metadata = crate::capture::put_metadata(
            &mut publication,
            InodeKind::RegularFile,
            &NativeMetadata {
                mode: 0o644,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
                xattrs: NativeXattrs::new(),
                acl: None,
                bsd_flags: 0x2,
            },
        )
        .unwrap();
        let record = publication
            .put_object(
                &encode_inode_record(InodeRecordV1 {
                    namespace_ref_count: 3,
                    metadata_root: metadata,
                    ..record
                })
                .unwrap(),
            )
            .unwrap();
        let directory = directory_insert(
            &mut publication,
            directory,
            CanonicalName::from_bytes(b"link-b").unwrap(),
            inode,
        )
        .unwrap()
        .0;
        let directory = directory_insert(
            &mut publication,
            directory,
            CanonicalName::from_bytes(b"link-c").unwrap(),
            inode,
        )
        .unwrap()
        .0;
        let root_record = publication
            .put_object(
                &encode_inode_record(InodeRecordV1 {
                    content_root: directory.0,
                    ..root_record
                })
                .unwrap(),
            )
            .unwrap();
        let table = inode_table_upsert(&mut publication, table, inode, record)
            .unwrap()
            .0;
        let table = inode_table_upsert(
            &mut publication,
            table,
            namespace.root_directory_inode,
            root_record,
        )
        .unwrap()
        .0;
        publication
            .publish_namespace(
                &encode_namespace_root(layerfs_core::namespace::NamespaceRootV1 {
                    inode_table_root: table.0,
                    ..namespace
                })
                .unwrap(),
            )
            .unwrap()
    }

    fn assert_failed_fresh_materialization(
        fault: MaterializeFault,
        fixture: fn(&crate::workspace::LayerVfs) -> layerfs_engine::refs::RefState,
    ) {
        let (base, vfs, state) = test_vfs(Some(fault));
        let head = fixture(&vfs);
        assert!(
            vfs.materialize_managed(head.root).is_err(),
            "fault {fault:?}"
        );
        let state = state.lock().unwrap();
        assert!(state.removed, "fault {fault:?} left an owned root");
        assert!(state.files.is_empty(), "fault {fault:?} left visible files");
        assert!(
            state.directories.is_empty(),
            "fault {fault:?} left visible directories"
        );
        drop(state);
        drop(vfs);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn erased_driver_and_handles_are_object_safe() {
        let driver: Box<dyn ProjectionDriver> = Box::new(MemoryDriver);
        let workspace = driver
            .open_workspace(
                Path::new("unused"),
                WorkspacePolicy::ManagedPrivate,
                [0; 32],
            )
            .unwrap();
        let root = workspace.root_directory().unwrap();
        let entries = workspace
            .enumerate_at(root.as_ref())
            .unwrap()
            .collect::<Result<Vec<_>>>()
            .unwrap();
        assert_eq!(entries[0].name, b"file");
    }

    #[test]
    fn managed_root_handle_failure_discards_the_exact_opened_workspace() {
        let discarded = Arc::new(AtomicBool::new(false));
        let result = crate::workspace::admit_managed_root(Box::new(MemoryWorkspace {
            fail_root: true,
            discarded: discarded.clone(),
            ..MemoryWorkspace::default()
        }));
        assert!(matches!(result, Err(crate::VfsError::Driver(_))));
        assert!(discarded.load(Ordering::Acquire));
    }

    #[test]
    fn managed_root_identity_failure_discards_the_exact_opened_workspace() {
        let discarded = Arc::new(AtomicBool::new(false));
        let result = crate::workspace::admit_managed_root(Box::new(MemoryWorkspace {
            fail_identity: true,
            discarded: discarded.clone(),
            ..MemoryWorkspace::default()
        }));
        assert!(matches!(result, Err(crate::VfsError::Driver(_))));
        assert!(discarded.load(Ordering::Acquire));
    }

    #[test]
    fn fresh_materializer_faults_never_return_complete_or_leave_owned_residue() {
        for fault in [
            MaterializeFault::TempCreate,
            MaterializeFault::WriteZero,
            MaterializeFault::WriteError,
            MaterializeFault::Metadata,
            MaterializeFault::FileSync,
            MaterializeFault::RenameBeforeVisibility,
            MaterializeFault::RenameAfterVisibility,
            MaterializeFault::DirectorySync(1),
            MaterializeFault::RootRevalidation,
        ] {
            assert_failed_fresh_materialization(fault, flat_file);
        }
    }

    #[test]
    fn fresh_materializer_accepts_short_writes_and_completes_exact_bytes() {
        let (base, vfs, state) = test_vfs(Some(MaterializeFault::WriteShort));
        let head = flat_file(&vfs);
        let mut workspace = vfs.materialize_managed(head.root).unwrap();
        let state_guard = state.lock().unwrap();
        assert_eq!(
            state_guard.files.get(b"file".as_slice()).unwrap(),
            b"portable-fault-matrix"
        );
        assert!(
            state_guard
                .events
                .iter()
                .filter(|event| event.as_str() == "write")
                .count()
                > 1
        );
        assert_eq!(state_guard.directory_syncs, [Vec::<u8>::new()]);
        assert_eq!(state_guard.revalidations, 2);
        assert!(!state_guard.removed);
        drop(state_guard);
        workspace.discard().unwrap();
        assert!(state.lock().unwrap().removed);
        drop(workspace);
        drop(vfs);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn fresh_nested_tree_barriers_are_once_per_directory_and_bottom_up() {
        let (base, vfs, state) = test_vfs(None);
        let head = nested_two_files(&vfs);
        let mut workspace = vfs.materialize_managed(head.root).unwrap();
        let state_guard = state.lock().unwrap();
        assert_eq!(state_guard.files.len(), 2);
        assert_eq!(
            state_guard.directory_syncs,
            [b"nested".to_vec(), Vec::<u8>::new()]
        );
        let events = &state_guard.events;
        let second_rename = events
            .iter()
            .rposition(|event| event.starts_with("rename:nested/"))
            .unwrap();
        let nested_sync = events
            .iter()
            .position(|event| event == "directory_sync:nested")
            .unwrap();
        let root_metadata = events
            .iter()
            .position(|event| event == "root_metadata")
            .unwrap();
        let root_sync = events
            .iter()
            .position(|event| event == "directory_sync:")
            .unwrap();
        let final_revalidation = events
            .iter()
            .rposition(|event| event == "root_revalidate")
            .unwrap();
        assert!(second_rename < nested_sync);
        assert!(nested_sync < root_metadata);
        assert!(root_metadata < root_sync);
        assert!(root_sync < final_revalidation);
        drop(state_guard);
        workspace.discard().unwrap();
        drop(workspace);
        drop(vfs);
        std::fs::remove_dir_all(base).unwrap();

        assert_failed_fresh_materialization(MaterializeFault::DirectorySync(1), nested_two_files);
        assert_failed_fresh_materialization(MaterializeFault::DirectorySync(2), nested_two_files);
    }

    #[test]
    fn fresh_hard_link_cut_matrix_preserves_two_sync_order_and_cleans_residue() {
        for fault in [
            MaterializeFault::RenameBeforeVisibility,
            MaterializeFault::RenameAfterVisibility,
            MaterializeFault::HardLink(2),
            MaterializeFault::HardLinkBeforeMetadata,
            MaterializeFault::HardLinkAfterMetadata,
            MaterializeFault::HardLinkAfterFinalSync,
        ] {
            assert_failed_fresh_materialization(fault, three_hard_links);
        }

        let (base, vfs, state) = test_vfs(None);
        let head = three_hard_links(&vfs);
        let mut workspace = vfs.materialize_managed(head.root).unwrap();
        let state_guard = state.lock().unwrap();
        assert_eq!(state_guard.files.len(), 3);
        let events = &state_guard.events;
        let construction_sync = events
            .iter()
            .position(|event| event == "file_sync")
            .unwrap();
        let representative_install = events
            .iter()
            .position(|event| event == "rename:file")
            .unwrap();
        let first_alias = events
            .iter()
            .position(|event| event == "hard_link:link-b")
            .unwrap();
        let second_alias = events
            .iter()
            .position(|event| event == "hard_link:link-c")
            .unwrap();
        let restrictive_metadata = events
            .iter()
            .position(|event| event == "hard_link_metadata")
            .unwrap();
        let final_sync = events
            .iter()
            .position(|event| event == "hard_link_final_sync")
            .unwrap();
        let directory_barrier = events
            .iter()
            .position(|event| event == "directory_sync:")
            .unwrap();
        assert!(construction_sync < representative_install);
        assert!(representative_install < first_alias);
        assert!(first_alias < second_alias);
        assert!(second_alias < restrictive_metadata);
        assert!(restrictive_metadata < final_sync);
        assert!(final_sync < directory_barrier);
        drop(state_guard);
        workspace.discard().unwrap();
        drop(workspace);
        drop(vfs);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn checked_live_refresh_install_keeps_immediate_parent_durability() {
        let state = Arc::new(Mutex::new(MaterializeState::default()));
        let workspace = MemoryWorkspace {
            materialize: Some(state.clone()),
            ..MemoryWorkspace::default()
        };
        let root = Dir(Vec::new());
        workspace
            .atomic_replace_checked(
                Box::new(MaterializeTemp {
                    bytes: io::Cursor::new(b"refresh".to_vec()),
                    parent: Vec::new(),
                    fault: None,
                    state: state.clone(),
                }),
                &root,
                b"file",
                None,
            )
            .unwrap();
        assert_eq!(
            state.lock().unwrap().events,
            ["file_sync", "rename:file", "directory_sync:"]
        );
    }

    #[test]
    fn native_xattrs_are_compact_ordered_and_round_trip_the_full_envelope() {
        let entries = (0..1024)
            .map(|index| (format!("x{index:015}").into_bytes(), vec![9; 1008]))
            .collect::<Vec<_>>();
        let xattrs = NativeXattrs::from_entries(entries.clone()).unwrap();
        assert_eq!(xattrs.payload_bytes(), MAX_NATIVE_XATTR_BYTES);
        assert_eq!(xattrs.iter().collect::<Vec<_>>(), entries);
        assert!(xattrs
            .chunks
            .iter()
            .all(|chunk| chunk.len() <= NATIVE_XATTR_CHUNK_BYTES));

        let mut unordered = NativeXattrs::new();
        unordered.push(b"b", b"1").unwrap();
        assert!(matches!(
            unordered.push(b"a", b"2"),
            Err(DriverError::Unsupported)
        ));
    }

    #[test]
    fn projection_facts_delta_is_checked_and_preserves_unavailable_timers() {
        let before = ProjectionFacts::default();
        let mut after = before;
        after.content_write.attempts = 1;
        after.content_write.successes = 1;
        after.content_write.bytes = 4096;
        let delta = after.checked_delta(before).unwrap();
        assert_eq!(delta.content_write.bytes, 4096);
        assert_eq!(
            delta.content_write.wall.availability,
            ProjectionTimerAvailability::Unavailable
        );
        assert!(before.checked_delta(after).is_none());
    }

    #[test]
    fn portable_deferred_install_falls_back_to_safe_immediate_and_preserves_failure() {
        let directory = Dir(Vec::new());
        let achieved = MemoryWorkspace::default()
            .atomic_replace_with_directory_durability(
                Box::new(MemoryTemp(std::io::Cursor::new(Vec::new()))),
                &directory,
                b"file",
                DirectoryDurability::DeferredToIncompleteTreeBoundary,
            )
            .unwrap();
        assert_eq!(achieved, DirectoryDurability::ImmediateDirectoryDurability);

        assert!(matches!(
            (MemoryWorkspace {
                fail_replace: true,
                ..MemoryWorkspace::default()
            })
            .atomic_replace_with_directory_durability(
                Box::new(MemoryTemp(std::io::Cursor::new(Vec::new()))),
                &directory,
                b"file",
                DirectoryDurability::DeferredToIncompleteTreeBoundary,
            ),
            Err(DriverError::DurabilityAmbiguous)
        ));
    }
}
