//! Physical capacity before SQL, on the daemon's one exclusive disposable file.
use crate::{OverlayError, OverlayResult};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::os::unix::fs::MetadataExt;
use std::{
    cell::Cell,
    fs::File,
    path::{Path, PathBuf},
};
/// Rounded conservative bounds for the selected bounded DML jobs.
pub const MUTATION_GROWTH: u64 = 128 << 20;
pub const CLEANUP_HEADROOM: u64 = 128 << 20;
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AllocationWork {
    pub attempts: u64,
    pub requested_bytes: u64,
    pub admitted_jobs: u64,
    pub refusals: u64,
    /// Calls reading descriptor and path identity; each uses two metadata calls.
    pub observations: u64,
    /// Committed freelist cookies read for pre-BEGIN admission.
    pub freelist_queries: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocationState {
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    /// Largest st_blocks allocation observed since daemon startup. It includes
    /// the full reservation and is not a phase-local residency peak.
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
    guaranteed_end: Cell<u64>,
    observed: Cell<(u64, u64)>,
    high_water: Cell<u64>,
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
            guaranteed_end: Cell::new(0),
            observed: Cell::new((meta.len(), meta.blocks())),
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
        let previous = self.observed.replace((meta.len(), meta.blocks()));
        if meta.len() < previous.0 || meta.blocks() < previous.1 {
            // Rollback/truncation may discard preallocation. Never infer its
            // position from st_blocks (which may include metadata).
            self.guaranteed_end
                .set(self.guaranteed_end.get().min(meta.len()));
        }
        Ok(AllocationState {
            logical_bytes: meta.len(),
            allocated_bytes: allocated,
            high_water_allocated_bytes: self.high_water.get(),
            reserved_tail_bytes: self
                .guaranteed_end
                .get()
                .saturating_sub(meta.len())
                .min(allocated.saturating_sub(meta.len())),
            cleanup_headroom_bytes: CLEANUP_HEADROOM,
            work: self.work.get(),
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub(crate) fn new(_file: File, _path: &Path) -> OverlayResult<Self> {
        Err(OverlayError::UnsupportedPlatform)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub(crate) fn state(&self) -> OverlayResult<AllocationState> {
        Err(OverlayError::UnsupportedPlatform)
    }
    pub(crate) fn work(&self) -> AllocationWork {
        self.work.get()
    }
    pub(crate) fn freelist_query(&self) {
        let mut work = self.work.get();
        work.freelist_queries = work.freelist_queries.saturating_add(1);
        self.work.set(work);
    }
    /// Failed admission changes no logical bytes or SQL state. Preserve exact
    /// original allocation failure; never zero-fill, retry or estimate free disk.
    pub(crate) fn admit(&self, cleanup: bool, reusable_bytes: u64) -> OverlayResult<()> {
        let before = self.state()?;
        let budget = CLEANUP_HEADROOM + if cleanup { 0 } else { MUTATION_GROWTH };
        // Reusable committed SQLite pages are already physically owned. They
        // replenish cleanup capacity without requiring free filesystem blocks.
        let required = budget.saturating_sub(reusable_bytes);
        let mut work = self.work.get();
        // Linux establishes the precise range on every admission. An already
        // allocated range needs no new disk space; no block-total inference.
        if required != 0 && (cfg!(target_os = "linux") || before.reserved_tail_bytes < required) {
            let need = required.saturating_sub(before.reserved_tail_bytes);
            let requested = if cfg!(target_os = "linux") {
                required
            } else {
                need
            };
            work.attempts = work.attempts.saturating_add(1);
            work.requested_bytes = work.requested_bytes.saturating_add(requested);
            self.work.set(work);
            if let Err(cause) = self.allocate(before.logical_bytes, need, required) {
                work.refusals = work.refusals.saturating_add(1);
                self.work.set(work);
                return Err(OverlayError::Reservation {
                    required_bytes: required,
                    allocated_bytes: before.allocated_bytes,
                    cause: Box::new(cause),
                });
            }
            // The successful primitive establishes a range, not merely a sum
            // of disk blocks. On Darwin the exclusively owned fresh dense file
            // extends from physical EOF, which is at least the known end.
            let established = before
                .logical_bytes
                .checked_add(required)
                .ok_or(OverlayError::Invalid("allocation end"))?;
            // A smaller cleanup request does not discard an already established
            // larger range. state() has already invalidated any truncated credit.
            self.guaranteed_end
                .set(self.guaranteed_end.get().max(established));
            let after = self.state()?;
            if after.logical_bytes != before.logical_bytes {
                return Err(OverlayError::Invalid(
                    "preallocation changed logical length",
                ));
            }
            if after.reserved_tail_bytes < required {
                return Err(OverlayError::Invalid("physical allocation short readback"));
            }
        }
        work = self.work.get();
        work.admitted_jobs = work.admitted_jobs.saturating_add(1);
        self.work.set(work);
        Ok(())
    }
    #[cfg(target_os = "linux")]
    fn allocate(&self, logical: u64, _need: u64, required: u64) -> OverlayResult<()> {
        nix::fcntl::fallocate(
            &self.file,
            nix::fcntl::FallocateFlags::FALLOC_FL_KEEP_SIZE,
            i64::try_from(logical).map_err(|_| OverlayError::Invalid("allocation offset"))?,
            i64::try_from(required).map_err(|_| OverlayError::Invalid("allocation length"))?,
        )
        .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    fn allocate(&self, _logical: u64, need: u64, _required: u64) -> OverlayResult<()> {
        let mut request = nix::libc::fstore_t {
            fst_flags: nix::libc::F_ALLOCATEALL,
            fst_posmode: nix::libc::F_PEOFPOSMODE,
            fst_offset: 0,
            fst_length: i64::try_from(need)
                .map_err(|_| OverlayError::Invalid("allocation length"))?,
            fst_bytesalloc: 0,
        };
        nix::fcntl::fcntl(
            &self.file,
            nix::fcntl::FcntlArg::F_PREALLOCATE(&mut request),
        )
        .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
        if request.fst_bytesalloc != request.fst_length {
            return Err(OverlayError::Invalid("physical allocation short result"));
        }
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn allocate(&self, _logical: u64, _need: u64, _required: u64) -> OverlayResult<()> {
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
        self.freelist_queries = self.freelist_queries.saturating_add(delta.freelist_queries);
    }
}
