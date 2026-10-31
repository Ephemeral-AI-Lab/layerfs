//! Read-only physical observations of one disposable SQLite file.
use crate::{OverlayError, OverlayResult};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::fs::MetadataExt;
use std::{
    cell::Cell,
    fs::File,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllocationWork {
    pub attempts: u64,
    pub requested_bytes: u64,
    pub admitted_jobs: u64,
    pub refusals: u64,
    /// Calls reading descriptor and path identity; each uses two metadata calls.
    pub observations: u64,
    /// Full-device checks after a write I/O failure, never write admission.
    pub capacity_checks: u64,
    /// Retained diagnostic field; reservation removal leaves this zero.
    pub freelist_queries: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocationState {
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    /// Largest allocation actually observed; not a phase-local residency peak.
    pub high_water_allocated_bytes: u64,
    pub reserved_tail_bytes: u64,
    pub cleanup_headroom_bytes: u64,
    pub work: AllocationWork,
}
pub(crate) struct Allocation {
    file: File,
    device: u64,
    inode: u64,
    path: PathBuf,
    work: Cell<AllocationWork>,
    high_water: Cell<u64>,
}
/// A checked-closed SQLite file's retained physical identity handle. It has no
/// SQLite connection and performs only read-only observations, never sync.
pub struct ClosedFile {
    pub(crate) allocation: Allocation,
}
impl ClosedFile {
    pub fn allocation(&self) -> OverlayResult<AllocationState> {
        self.allocation.state()
    }
    pub fn allocation_work(&self) -> AllocationWork {
        self.allocation.work()
    }
}
impl Allocation {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(crate) fn new(file: File, path: &Path) -> OverlayResult<Self> {
        let meta = file.metadata()?;
        if !meta.is_file() || meta.nlink() != 1 {
            return Err(OverlayError::Invalid("allocation file identity"));
        }
        Ok(Self {
            file,
            device: meta.dev(),
            inode: meta.ino(),
            path: path.to_owned(),
            work: Cell::new(AllocationWork::default()),
            high_water: Cell::new(meta.blocks().saturating_mul(512)),
        })
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(crate) fn state(&self) -> OverlayResult<AllocationState> {
        let mut work = self.work.get();
        work.observations = work.observations.saturating_add(1);
        self.work.set(work);
        let meta = self.file.metadata()?;
        let path = std::fs::symlink_metadata(&self.path)?;
        if !path.is_file() || path.dev() != meta.dev() || path.ino() != meta.ino() {
            return Err(OverlayError::Invalid("allocation path identity"));
        }
        if meta.dev() != self.device || meta.ino() != self.inode || meta.nlink() != 1 {
            return Err(OverlayError::Invalid("allocation file identity"));
        }
        let allocated = meta
            .blocks()
            .checked_mul(512)
            .ok_or(OverlayError::Invalid("allocation size"))?;
        self.high_water.set(self.high_water.get().max(allocated));
        Ok(AllocationState {
            logical_bytes: meta.len(),
            allocated_bytes: allocated,
            high_water_allocated_bytes: self.high_water.get(),
            reserved_tail_bytes: 0,
            cleanup_headroom_bytes: 0,
            work: self.work.get(),
        })
    }
    #[cfg(target_os = "windows")]
    pub(crate) fn new(file: File, path: &Path) -> OverlayResult<Self> {
        let identity = super::windows_identity::windows_handle_identity(&file)?;
        if identity.links != 1 {
            return Err(OverlayError::Invalid("allocation file identity"));
        }
        let allocated = super::windows_identity::windows_allocation_size(&file)?;
        Ok(Self {
            file,
            device: identity.device,
            inode: identity.index,
            path: path.to_owned(),
            work: Cell::new(AllocationWork::default()),
            high_water: Cell::new(allocated),
        })
    }
    #[cfg(target_os = "windows")]
    pub(crate) fn state(&self) -> OverlayResult<AllocationState> {
        let mut work = self.work.get();
        work.observations = work.observations.saturating_add(1);
        self.work.set(work);
        let meta = self.file.metadata()?;
        let by_path = super::windows_identity::windows_path_identity(&self.path)?;
        let by_handle = super::windows_identity::windows_handle_identity(&self.file)?;
        if by_path.device != by_handle.device
            || by_path.index != by_handle.index
            || by_handle.device != self.device
            || by_handle.index != self.inode
            || by_handle.links != 1
        {
            return Err(OverlayError::Invalid("allocation file identity"));
        }
        // The allocated footprint comes from FileStandardInfo on the same
        // retained handle (compression/sparse aware); the disposable overlay
        // never preallocates, so this tracks the tail's actual clusters.
        let allocated = super::windows_identity::windows_allocation_size(&self.file)?;
        self.high_water.set(self.high_water.get().max(allocated));
        Ok(AllocationState {
            logical_bytes: meta.len(),
            allocated_bytes: allocated,
            high_water_allocated_bytes: self.high_water.get(),
            reserved_tail_bytes: 0,
            cleanup_headroom_bytes: 0,
            work: self.work.get(),
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    pub(crate) fn new(_file: File, _path: &Path) -> OverlayResult<Self> {
        Err(OverlayError::UnsupportedPlatform)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    pub(crate) fn state(&self) -> OverlayResult<AllocationState> {
        Err(OverlayError::UnsupportedPlatform)
    }
    pub(crate) fn work(&self) -> AllocationWork {
        self.work.get()
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    pub(crate) fn device_full(&self) -> OverlayResult<bool> {
        self.state()?;
        let mut work = self.work.get();
        work.capacity_checks = work.capacity_checks.saturating_add(1);
        self.work.set(work);
        let filesystem = nix::sys::statvfs::fstatvfs(&self.file)
            .map_err(|error| std::io::Error::from_raw_os_error(error as i32))?;
        // Do not translate quota/size-limit/read/corruption failures. The
        // descriptor's filesystem must have no available blocks. ext4 may
        // still count reserved/internal blocks in blocks_free at real ENOSPC;
        // those are not ordinary writable capacity. Retain the original cause.
        Ok(filesystem.blocks_available() == 0)
    }
    #[cfg(target_os = "windows")]
    pub(crate) fn device_full(&self) -> OverlayResult<bool> {
        self.state()?;
        let mut work = self.work.get();
        work.capacity_checks = work.capacity_checks.saturating_add(1);
        self.work.set(work);
        let directory = self
            .path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let free = super::windows_identity::windows_free_bytes(directory)
            .map_err(|_| OverlayError::Invalid("capacity observation"))?;
        Ok(free == 0)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    pub(crate) fn device_full(&self) -> OverlayResult<bool> {
        Err(OverlayError::UnsupportedPlatform)
    }
}

impl AllocationWork {
    /// Cumulative observations since a prior snapshot of the same owner.
    pub fn since(self, before: Self) -> Self {
        Self {
            attempts: self.attempts.saturating_sub(before.attempts),
            requested_bytes: self.requested_bytes.saturating_sub(before.requested_bytes),
            admitted_jobs: self.admitted_jobs.saturating_sub(before.admitted_jobs),
            refusals: self.refusals.saturating_sub(before.refusals),
            observations: self.observations.saturating_sub(before.observations),
            capacity_checks: self.capacity_checks.saturating_sub(before.capacity_checks),
            freelist_queries: self
                .freelist_queries
                .saturating_sub(before.freelist_queries),
        }
    }
    pub fn accumulate(&mut self, delta: Self) {
        self.attempts = self.attempts.saturating_add(delta.attempts);
        self.requested_bytes = self.requested_bytes.saturating_add(delta.requested_bytes);
        self.admitted_jobs = self.admitted_jobs.saturating_add(delta.admitted_jobs);
        self.refusals = self.refusals.saturating_add(delta.refusals);
        self.observations = self.observations.saturating_add(delta.observations);
        self.capacity_checks = self.capacity_checks.saturating_add(delta.capacity_checks);
        self.freelist_queries = self.freelist_queries.saturating_add(delta.freelist_queries);
    }
}
