//! Sandbox-owned payload backing for one workspace mount.
//!
//! Replacement bytes live in packed segment files under the workspace's
//! private backing directory (`/snapshots/<workspace-id>/`) until a Commit
//! transfers the frozen generation to the host builder. The backing is
//! disposable by contract: bytes are installed with ordinary positioned
//! writes through the scheduler's physical permits and no durability flush
//! (`fsync`/`fdatasync`/`sync_data`/`sync_all`) is ever issued for it.
//!
//! Segments are shared across files and bounded (`SEGMENT_CAPACITY`), so a
//! single surviving byte cannot pin an arbitrarily large dead segment: a
//! segment is retired once no live piece, frozen frontier or reader retains
//! a reference to it. Bytes are written before the piece that references the
//! range is applied, so every piece range is always physically readable.
//!
//! The backing is also bounded in *resident* terms. Every payload byte the
//! sandbox owns is written through the sandbox's page cache, so leaving that
//! cache alone makes the sandbox's memory charge grow with the workspace
//! payload and lets a later transfer read its own recent writes out of cache
//! instead of storage. Both are forbidden by the #151 resource contract: the
//! sandbox must not amplify memory, and a measured phase must not be flattered
//! by residual warmth. The spool therefore keeps only a bounded window of its
//! own bytes resident (see `CACHE_WINDOW_SEGMENTS`) and offers the rest back
//! with `POSIX_FADV_DONTNEED`.
//!
//! That hint is *not* a durability barrier: no flush is waited on, and the
//! kernel never discards dirty, in-writeback or mapped pages, so an eviction
//! can only drop bytes that are already on storage. Bytes evicted here are
//! simply re-read from storage by whoever needs them next.

use crate::{PortError, PortResult};
use layerfs_workspace_core::backing::{BackingId, BackingRef};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// One packed segment file. Shared by live pieces, the frozen frontier and
/// snapshot readers through the `BackingRef` arc graph; the registry holds
/// the only non-payload reference.
pub struct LocalSegment {
    file: File,
    path: PathBuf,
    /// Reserved high-water: every range below it is fully written.
    len: AtomicU64,
    capacity: u64,
}

pub struct LocalSpool {
    directory: PathBuf,
    inner: Mutex<SpoolInner>,
    /// Monotonic physical allocation for observation and teardown.
    physical: AtomicU64,
    physical_peak: AtomicU64,
}

struct SpoolInner {
    segments: HashMap<BackingId, BackingRef>,
    /// The active allocation target's id; the registry map holds the only
    /// non-payload reference, so `is_unique` remains a piece-only signal.
    current: Option<BackingId>,
    next_id: u64,
    /// Sealed segments still inside the resident window, oldest first, with
    /// the high-water they were sealed at.
    cache_window: VecDeque<(BackingId, u64)>,
}

/// Segment capacity bounds dead-range retention inside one shared segment.
pub(crate) const SEGMENT_CAPACITY: u64 = 1024 * 1024;

/// How many sealed segments stay inside the window the kernel may keep cached
/// on the spool's behalf. Sealing one segment offers the whole window back, so
/// a segment is offered repeatedly until it slides out; the first offer starts
/// writeback of any dirty pages in it and a later offer evicts them. The bound
/// is a constant, not a fraction of the payload: a 500 MiB workspace and a
/// 5 MiB workspace keep the same amount of the spool resident.
const CACHE_WINDOW_SEGMENTS: usize = 4;

fn io(_: std::io::Error) -> PortError {
    PortError::Io
}

/// Ask the kernel to drop its cached copy of a spool range. Linux only: the
/// crate's `nix` dependency and the sandbox placement itself are Linux only,
/// and on other targets the default caching behaviour stands. This is a hint
/// with no durability meaning, so a failure is deliberately ignored.
#[cfg(target_os = "linux")]
fn drop_cached_range(fd: RawFd, offset: u64, len: u64) {
    match (i64::try_from(offset), i64::try_from(len)) {
        (Ok(offset), Ok(len)) if len > 0 => {
            let _ = nix::fcntl::posix_fadvise(
                fd,
                offset,
                len,
                nix::fcntl::PosixFadviseAdvice::POSIX_FADV_DONTNEED,
            );
        }
        _ => {}
    }
}

#[cfg(not(target_os = "linux"))]
fn drop_cached_range(_fd: RawFd, _offset: u64, _len: u64) {}

impl LocalSpool {
    /// The directory is created with private permissions; it is removed by
    /// `destroy` at workspace End/Discard.
    pub fn new(directory: PathBuf) -> PortResult<Self> {
        std::fs::create_dir_all(&directory).map_err(io)?;
        Ok(Self {
            directory,
            inner: Mutex::new(SpoolInner {
                segments: HashMap::new(),
                current: None,
                next_id: 1,
                cache_window: VecDeque::new(),
            }),
            physical: AtomicU64::new(0),
            physical_peak: AtomicU64::new(0),
        })
    }

    pub fn directory(&self) -> &std::path::Path {
        &self.directory
    }

    pub fn physical_bytes(&self) -> u64 {
        self.physical.load(Ordering::Acquire)
    }

    pub fn physical_peak_bytes(&self) -> u64 {
        self.physical_peak.load(Ordering::Acquire)
    }

    pub fn segment_count(&self) -> PortResult<usize> {
        Ok(self.inner.lock().map_err(|_| PortError::Io)?.segments.len())
    }

    /// Reserve `bytes` of physical space in the active segment (or a fresh
    /// one) and return the owning reference plus the reserved start offset.
    /// The caller must write the bytes before applying the piece that
    /// references the range. Charging against the workspace spool policy
    /// happens in `LiveWorkspace::prepare_write` at apply validation.
    pub fn reserve(&self, bytes: u64) -> PortResult<(BackingRef, u64)> {
        if bytes == 0 || bytes > SEGMENT_CAPACITY {
            return Err(PortError::Invalid);
        }
        let mut inner = self.inner.lock().map_err(|_| PortError::Io)?;
        let reusable = inner
            .current
            .and_then(|id| inner.segments.get(&id))
            .and_then(|current| {
                let segment = current.resource::<LocalSegmentHandle>()?;
                let len = segment.len.load(Ordering::Acquire);
                (len.checked_add(bytes)
                    .is_some_and(|end| end <= segment.capacity))
                .then(|| current.clone())
            });
        if let Some(current) = reusable {
            let segment = current
                .resource::<LocalSegmentHandle>()
                .ok_or(PortError::Io)?;
            let start = segment.len.fetch_add(bytes, Ordering::AcqRel);
            self.note_physical(bytes);
            return Ok((current, start));
        }
        let id = BackingId(inner.next_id);
        inner.next_id = inner.next_id.checked_add(1).ok_or(PortError::NoSpace)?;
        let path = self.directory.join(format!("payload-{:08}.bin", id.0));
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(io)?;
        let segment = Arc::new(LocalSegment {
            file,
            path,
            len: AtomicU64::new(bytes),
            capacity: SEGMENT_CAPACITY.max(bytes),
        });
        let reference = BackingRef::new(id, LocalSegmentHandle(segment));
        inner.segments.insert(id, reference.clone());
        // Sealing the previous target is the moment its bytes stop growing and
        // can be offered back to the kernel; the new segment holds none yet.
        if let Some(previous) = inner.current.replace(id) {
            let sealed = inner
                .segments
                .get(&previous)
                .and_then(|segment| segment.resource::<LocalSegmentHandle>())
                .map(|segment| segment.len.load(Ordering::Acquire))
                .unwrap_or(0);
            inner.cache_window.push_back((previous, sealed));
        }
        drop(inner);
        self.evict_cache_window();
        self.note_physical(bytes);
        Ok((reference, 0))
    }

    fn note_physical(&self, bytes: u64) {
        let physical = self.physical.fetch_add(bytes, Ordering::AcqRel) + bytes;
        self.physical_peak.store(
            physical.max(self.physical_peak.load(Ordering::Acquire)),
            Ordering::Release,
        );
    }

    /// Install bytes at a reserved range. Positioned write; no durability
    /// flush. Must complete before the referencing piece is applied.
    pub fn write(&self, segment: &BackingRef, start: u64, bytes: &[u8]) -> PortResult<()> {
        let handle = segment
            .resource::<LocalSegmentHandle>()
            .ok_or(PortError::Io)?;
        let end = start
            .checked_add(bytes.len() as u64)
            .ok_or(PortError::Invalid)?;
        if end > handle.len.load(Ordering::Acquire)
            || handle.len.load(Ordering::Acquire) > handle.capacity
        {
            return Err(PortError::Invalid);
        }
        if bytes.is_empty() {
            return Ok(());
        }
        handle.file.write_all_at(bytes, start).map_err(io)?;
        Ok(())
    }

    /// Read an owned range. Ranges referenced by pieces are always fully
    /// written (write-before-apply), so a miss below the high-water is an
    /// integrity error, not a wait.
    pub fn read(&self, segment: &BackingRef, offset: u64, len: u64) -> PortResult<Vec<u8>> {
        let handle = segment
            .resource::<LocalSegmentHandle>()
            .ok_or(PortError::Io)?;
        let end = offset.checked_add(len).ok_or(PortError::Invalid)?;
        if end > handle.len.load(Ordering::Acquire) {
            return Err(PortError::Io);
        }
        let mut out = vec![0; len as usize];
        if len != 0 {
            handle.file.read_exact_at(&mut out, offset).map_err(io)?;
        }
        Ok(out)
    }

    /// Offer the resident window back to the kernel. Called when a segment is
    /// sealed: every segment still inside the window is offered again, so a
    /// range that was dirty at the previous offer is evicted by this one, and
    /// a segment leaves the window once enough newer segments have sealed.
    /// Best effort by construction: a refused or ineffective hint leaves pages
    /// resident, which is a memory outcome, never a correctness one.
    fn evict_cache_window(&self) {
        let offers: Vec<(RawFd, u64)> = {
            let Ok(mut inner) = self.inner.lock() else {
                return;
            };
            while inner.cache_window.len() > CACHE_WINDOW_SEGMENTS {
                inner.cache_window.pop_front();
            }
            inner
                .cache_window
                .iter()
                .filter_map(|(id, sealed)| {
                    let segment = inner.segments.get(id)?.resource::<LocalSegmentHandle>()?;
                    Some((segment.file.as_raw_fd(), *sealed))
                })
                .collect()
        };
        for (fd, sealed) in offers {
            drop_cached_range(fd, 0, sealed);
        }
    }

    /// Drop the resident copy of a range that was just served to the host.
    /// The bytes were read, so their pages are clean and the kernel can evict
    /// them at once; a later reader re-reads them from storage. This is what
    /// keeps a bulk transfer from re-inflating the sandbox's memory charge
    /// with the payload it is handing over.
    pub fn evict_served(&self, segment: &BackingRef, offset: u64, len: u64) {
        if let Some(segment) = segment.resource::<LocalSegmentHandle>() {
            drop_cached_range(segment.file.as_raw_fd(), offset, len);
        }
    }

    /// The registered reference for a segment id, for read-only serving of
    /// frozen payload ranges.
    pub fn segment_reference(&self, id: BackingId) -> PortResult<BackingRef> {
        let inner = self.inner.lock().map_err(|_| PortError::Io)?;
        inner.segments.get(&id).cloned().ok_or(PortError::NotFound)
    }

    pub fn segment_len(&self, id: BackingId) -> PortResult<u64> {
        let inner = self.inner.lock().map_err(|_| PortError::Io)?;
        let reference = inner.segments.get(&id).ok_or(PortError::NotFound)?;
        Ok(reference
            .resource::<LocalSegmentHandle>()
            .ok_or(PortError::Io)?
            .len
            .load(Ordering::Acquire))
    }

    /// Retire segments whose only remaining reference is the registry's own:
    /// no live piece, frozen frontier or reader still needs their bytes.
    pub fn retire_idle(&self) -> PortResult<()> {
        let mut inner = self.inner.lock().map_err(|_| PortError::Io)?;
        let idle: Vec<_> = inner
            .segments
            .iter()
            .filter(|(_, reference)| reference.is_unique())
            .map(|(id, _)| *id)
            .collect();
        for id in idle {
            if inner.current == Some(id) {
                inner.current = None;
            }
            if let Some(reference) = inner.segments.remove(&id) {
                if let Some(handle) = reference.resource::<LocalSegmentHandle>() {
                    let len = handle.len.load(Ordering::Acquire);
                    let _ = std::fs::remove_file(&handle.path);
                    self.physical.fetch_sub(len, Ordering::AcqRel);
                }
            }
        }
        Ok(())
    }

    /// Explicitly drop a failed reservation's claim. Consumes the caller's
    /// reference: the segment is removed only when no live piece, frozen
    /// frontier or reader retains it (exactly the registry plus the passed
    /// reference remain); otherwise the dead range stays packed (bounded by
    /// segment capacity) until the segment retires.
    pub fn abandon(&self, segment: BackingRef) -> PortResult<()> {
        let mut inner = self.inner.lock().map_err(|_| PortError::Io)?;
        let removable = segment.strong_count() == 2
            && inner
                .segments
                .get(&segment.id())
                .is_some_and(|registered| *registered == segment);
        if !removable {
            return Ok(());
        }
        if inner.current == Some(segment.id()) {
            inner.current = None;
        }
        inner.segments.remove(&segment.id());
        if let Some(handle) = segment.resource::<LocalSegmentHandle>() {
            let len = handle.len.load(Ordering::Acquire);
            let _ = std::fs::remove_file(&handle.path);
            self.physical.fetch_sub(len, Ordering::AcqRel);
        }
        Ok(())
    }

    /// Remove the backing directory and all segment files. Called only at
    /// workspace End/Discard, after readers and the frozen frontier are
    /// released.
    pub fn destroy(&self) -> PortResult<()> {
        let mut inner = self.inner.lock().map_err(|_| PortError::Io)?;
        inner.current = None;
        inner.segments.clear();
        inner.cache_window.clear();
        self.physical.store(0, Ordering::Release);
        std::fs::remove_dir_all(&self.directory).map_err(io)?;
        Ok(())
    }
}

/// The `BackingRef` resource: shares one segment handle with every piece,
/// frozen record and read plan that references its ranges.
pub(crate) struct LocalSegmentHandle(Arc<LocalSegment>);

impl std::ops::Deref for LocalSegmentHandle {
    type Target = LocalSegment;
    fn deref(&self) -> &LocalSegment {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spool() -> LocalSpool {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-local-spool-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        LocalSpool::new(directory).unwrap()
    }

    #[test]
    fn reserve_write_read_roundtrip_and_packing() {
        let spool = spool();
        let (first, start) = spool.reserve(1024).unwrap();
        let _ = start;
        assert_eq!(start, 0);
        spool.write(&first, 0, b"hello world!!").unwrap();
        assert_eq!(
            spool.read(&first, 2, 5).unwrap(),
            b"llo w".to_vec(),
            "read from the shared segment, not just offset 0"
        );
        // A second reservation continues the same segment.
        let (second, start) = spool.reserve(2048).unwrap();
        assert_eq!(start, 1024);
        spool.write(&second, 1024, &[7u8; 2048]).unwrap();
        assert_eq!(spool.read(&second, 1024, 8).unwrap(), vec![7u8; 8]);
        assert_eq!(spool.segment_count().unwrap(), 1);
        assert_eq!(spool.physical_bytes(), 1024 + 2048);
        spool.destroy().unwrap();
    }

    #[test]
    fn reads_beyond_high_water_fail_and_capacity_bounds_segments() {
        let spool = spool();
        let (segment, start) = spool.reserve(SEGMENT_CAPACITY).unwrap();
        spool
            .write(&segment, 0, &[1u8; SEGMENT_CAPACITY as usize])
            .unwrap();
        assert!(spool.read(&segment, SEGMENT_CAPACITY - 1, 2).is_err());
        // No room for another byte in the current segment; a new one opens.
        let (next, start) = spool.reserve(1).unwrap();
        assert_eq!(start, 0);
        assert_ne!(next.id(), segment.id());
        assert_eq!(spool.segment_count().unwrap(), 2);
        spool.destroy().unwrap();
    }

    #[test]
    fn resident_cache_window_is_bounded_and_eviction_keeps_bytes_readable() {
        let spool = spool();
        let mut held = Vec::new();
        for index in 0..CACHE_WINDOW_SEGMENTS + 3 {
            let (segment, start) = spool.reserve(SEGMENT_CAPACITY).unwrap();
            spool.write(&segment, start, &[index as u8; 64]).unwrap();
            held.push((segment, start));
        }
        assert!(
            spool.inner.lock().unwrap().cache_window.len() <= CACHE_WINDOW_SEGMENTS,
            "the resident window must not grow with the number of sealed segments"
        );
        // Bytes stay readable whatever the kernel did with the hint, and an
        // eviction must not disturb contents.
        spool.evict_served(&held[0].0, 0, 64);
        for (index, (segment, start)) in held.iter().enumerate() {
            assert_eq!(
                spool.read(segment, *start, 64).unwrap(),
                vec![index as u8; 64]
            );
        }
    }

    #[test]
    fn idle_retirement_removes_unreferenced_segments_only() {
        let spool = spool();
        let (held, start) = spool.reserve(16).unwrap();
        spool.write(&held, start, b"0123456789abcdef").unwrap();
        // An unreferenced reservation in the same packed segment keeps the
        // segment alive until every reference drops; its dead range is
        // bounded by the segment capacity.
        let (dead, _) = spool.reserve(16).unwrap();
        assert_eq!(dead.id(), held.id());
        drop(dead);
        spool.retire_idle().unwrap();
        assert_eq!(spool.segment_count().unwrap(), 1, "held keeps the segment");
        assert_eq!(spool.physical_bytes(), 32);
        assert_eq!(
            spool.read(&held, 0, 16).unwrap(),
            b"0123456789abcdef".to_vec()
        );
        drop(held);
        spool.retire_idle().unwrap();
        assert_eq!(spool.segment_count().unwrap(), 0);
        assert_eq!(spool.physical_bytes(), 0);
        spool.destroy().unwrap();
    }

    #[test]
    fn abandon_drops_a_failed_reservation_without_touching_shared_ranges() {
        let spool = spool();
        let (segment, _) = spool.reserve(4096).unwrap();
        // A live piece retains the segment: the failed reservation's range
        // stays packed until the piece releases it.
        let shared = segment.clone();
        spool.abandon(segment.clone()).unwrap();
        assert_eq!(
            spool.segment_count().unwrap(),
            1,
            "still referenced by a piece"
        );
        drop(shared);
        // With only the registry and the caller's own reference left, the
        // abandoned segment is removed entirely.
        spool.abandon(segment).unwrap();
        assert_eq!(spool.segment_count().unwrap(), 0);
        assert_eq!(spool.physical_bytes(), 0);
        spool.destroy().unwrap();
    }
}
