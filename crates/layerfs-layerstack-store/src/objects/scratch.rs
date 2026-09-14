//! Optional charged scratch for private canonical construction. The canonical
//! codec and released callers remain unchanged; scoped callers reserve before
//! growth and keep allocations owned until both paths and descriptors release.
use crate::Result;
use std::collections::VecDeque;
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

const PAGE: u64 = 4096;
fn overflow() -> io::Error {
    io::Error::other("private scratch size overflow")
}
fn exhausted() -> io::Error {
    io::Error::new(io::ErrorKind::StorageFull, "private scratch capacity")
}
fn aligned(bytes: u64) -> io::Result<u64> {
    Ok(bytes.checked_add(PAGE - 1).ok_or_else(overflow)? / PAGE * PAGE)
}

#[derive(Clone, Copy, Debug)]
pub struct ScratchLimits {
    pub bytes: u64,
    pub files: usize,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct ScratchUsage {
    pub reserved_bytes: u64,
    pub reserved_files: usize,
    pub peak_reserved_bytes: u64,
    pub peak_reserved_files: usize,
    pub observed_allocated_bytes: u64,
    pub peak_observed_allocated_bytes: u64,
    /// Only ScratchFile pwrite calls; SQLite I/O is outside this counter.
    pub write_calls: u64,
    /// Only bytes returned by ScratchFile pwrite; not total Store/SQLite I/O.
    pub written_bytes: u64,
}
/// One attempt's byte/FD scope. Its external owner carries host admission.
pub struct ScratchBudget {
    limits: ScratchLimits,
    used: Mutex<ScratchUsage>,
    directory: Option<PathBuf>,
    _owner: Arc<dyn Send + Sync>,
}
impl ScratchBudget {
    pub fn new(limits: ScratchLimits, owner: Arc<dyn Send + Sync>) -> io::Result<Arc<Self>> {
        Self::with_directory(limits, owner, None)
    }
    /// Place this scope's scratch in an explicit owning directory (a Workspace
    /// runtime/spool path) instead of the process temporary directory. The
    /// directory must already exist; placement is deliberately explicit so no
    /// ambient or global context can silently redirect private scratch.
    pub fn with_directory(
        limits: ScratchLimits,
        owner: Arc<dyn Send + Sync>,
        directory: Option<PathBuf>,
    ) -> io::Result<Arc<Self>> {
        if limits.bytes < PAGE || limits.files == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "private scratch limits",
            ));
        }
        if let Some(path) = &directory {
            let metadata = std::fs::metadata(path)?;
            if !metadata.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "private scratch directory",
                ));
            }
        }
        Ok(Arc::new(Self {
            limits,
            used: Mutex::new(ScratchUsage::default()),
            directory,
            _owner: owner,
        }))
    }
    /// Owning scratch directory for this scope: the explicit placement when
    /// configured, otherwise the process temporary directory.
    pub fn directory(&self) -> PathBuf {
        self.directory.clone().unwrap_or_else(std::env::temp_dir)
    }
    pub fn usage(&self) -> ScratchUsage {
        *self.used.lock().unwrap()
    }
    fn acquire_file(self: &Arc<Self>) -> io::Result<FileCharge> {
        let mut used = self.used.lock().unwrap();
        let next = used.reserved_files.checked_add(1).ok_or_else(overflow)?;
        if next > self.limits.files {
            return Err(exhausted());
        }
        used.reserved_files = next;
        used.peak_reserved_files = used.peak_reserved_files.max(next);
        Ok(FileCharge(self.clone()))
    }
    #[cfg(test)]
    fn allocation(self: &Arc<Self>) -> Arc<Allocation> {
        Arc::new(Allocation {
            budget: self.clone(),
            state: Mutex::new(AllocationState::default()),
        })
    }
    fn named_allocation(self: &Arc<Self>, path: &Path) -> io::Result<Arc<Allocation>> {
        use std::os::unix::ffi::OsStrExt;
        if path.as_os_str().as_bytes().len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "scratch private path length",
            ));
        }
        let slot = CleanupSlot::acquire()?;
        Ok(Arc::new(Allocation {
            budget: self.clone(),
            state: Mutex::new(AllocationState {
                name: Some((path.to_owned(), slot)),
                ..Default::default()
            }),
        }))
    }
}
struct FileCharge(Arc<ScratchBudget>);
impl Drop for FileCharge {
    fn drop(&mut self) {
        self.0.used.lock().unwrap().reserved_files -= 1;
    }
}
#[derive(Default)]
struct AllocationState {
    reserved: u64,
    observed: u64,
    identity: Option<(u64, u64)>,
    created: bool,
    name: Option<(std::path::PathBuf, CleanupSlot)>,
}
/// Named files retain this owner even while no descriptor is open.
pub struct Allocation {
    budget: Arc<ScratchBudget>,
    state: Mutex<AllocationState>,
}
impl Allocation {
    fn created(&self) {
        self.state.lock().unwrap().created = true;
    }
    fn bind(&self, metadata: &Metadata) -> io::Result<()> {
        let identity = (metadata.dev(), metadata.ino());
        let mut state = self.state.lock().unwrap();
        match state.identity {
            Some(expected) if expected != identity => {
                Err(io::Error::other("private scratch inode changed"))
            }
            _ => {
                state.identity = Some(identity);
                Ok(())
            }
        }
    }
    fn reserve(&self, bytes: u64) -> io::Result<()> {
        let bytes = aligned(bytes)?;
        let mut state = self.state.lock().unwrap();
        if bytes <= state.reserved {
            return Ok(());
        }
        let mut used = self.budget.used.lock().unwrap();
        let next = used
            .reserved_bytes
            .checked_add(bytes - state.reserved)
            .ok_or_else(overflow)?;
        if next > self.budget.limits.bytes {
            return Err(exhausted());
        }
        used.reserved_bytes = next;
        used.peak_reserved_bytes = used.peak_reserved_bytes.max(next);
        state.reserved = bytes;
        Ok(())
    }
    fn observe(&self, metadata: &Metadata, release_unused: bool) -> io::Result<()> {
        self.bind(metadata)?;
        let allocated = metadata.blocks().checked_mul(512).ok_or_else(overflow)?;
        let mut state = self.state.lock().unwrap();
        if allocated > state.reserved {
            return Err(io::Error::other(
                "scratch allocation exceeds admitted growth",
            ));
        }
        let mut used = self.budget.used.lock().unwrap();
        used.observed_allocated_bytes = used.observed_allocated_bytes - state.observed + allocated;
        used.peak_observed_allocated_bytes = used
            .peak_observed_allocated_bytes
            .max(used.observed_allocated_bytes);
        state.observed = allocated;
        if release_unused {
            let keep = aligned(metadata.len())?.max(allocated);
            if keep < state.reserved {
                used.reserved_bytes -= state.reserved - keep;
                state.reserved = keep;
            }
        }
        Ok(())
    }
}
struct RetiredAllocation {
    budget: Arc<ScratchBudget>,
    reserved: u64,
    observed: u64,
}
impl Drop for RetiredAllocation {
    fn drop(&mut self) {
        let mut used = self.budget.used.lock().unwrap();
        used.reserved_bytes -= self.reserved;
        used.observed_allocated_bytes -= self.observed;
    }
}
impl Drop for Allocation {
    fn drop(&mut self) {
        let state = std::mem::take(self.state.get_mut().unwrap());
        let owner = RetiredAllocation {
            budget: self.budget.clone(),
            reserved: state.reserved,
            observed: state.observed,
        };
        if let Some((path, slot)) = state.name {
            if state.created && remove_owned(&path, state.identity).is_err() {
                cleanup_queue()
                    .lock()
                    .unwrap()
                    .pending
                    .push_back(PendingCleanup {
                        path,
                        identity: state.identity,
                        _owner: owner,
                        _slot: slot,
                    });
            }
        }
    }
}

struct SharedFile {
    file: Option<File>,
    allocation: Option<Arc<Allocation>>,
    _charge: Option<FileCharge>,
    #[cfg(test)]
    fault: std::sync::atomic::AtomicU8,
}
impl Drop for SharedFile {
    fn drop(&mut self) {
        drop(self.file.take());
    }
}
/// Clones own a separate logical cursor and share one real descriptor. This
/// avoids dup() and offset interference between buffered readers and writers.
pub struct ScratchFile {
    inner: Arc<SharedFile>,
    position: Mutex<u64>,
}
impl std::fmt::Debug for ScratchFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScratchFile")
            .field("file", self.file())
            .finish_non_exhaustive()
    }
}
impl ScratchFile {
    pub fn create(
        path: &Path,
        budget: Option<&Arc<ScratchBudget>>,
    ) -> io::Result<(Self, Option<Arc<Allocation>>)> {
        let charge = budget.map(ScratchBudget::acquire_file).transpose()?;
        let allocation = budget
            .map(|budget| budget.named_allocation(path))
            .transpose()?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        if let Some(a) = &allocation {
            a.created();
            a.bind(&file.metadata()?)?;
        }
        Ok((
            Self {
                inner: Arc::new(SharedFile {
                    file: Some(file),
                    allocation: allocation.clone(),
                    _charge: charge,
                    #[cfg(test)]
                    fault: Default::default(),
                }),
                position: Mutex::new(0),
            },
            allocation,
        ))
    }
    pub fn open(
        path: &Path,
        allocation: Option<Arc<Allocation>>,
        append: bool,
    ) -> io::Result<Self> {
        let charge = allocation
            .as_ref()
            .map(|a| a.budget.acquire_file())
            .transpose()?;
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        if let Some(a) = &allocation {
            a.bind(&file.metadata()?)?;
        }
        let position = if append { file.metadata()?.len() } else { 0 };
        Ok(Self {
            inner: Arc::new(SharedFile {
                file: Some(file),
                allocation,
                _charge: charge,
                #[cfg(test)]
                fault: Default::default(),
            }),
            position: Mutex::new(position),
        })
    }
    pub fn from_file(file: File) -> Self {
        Self {
            inner: Arc::new(SharedFile {
                file: Some(file),
                allocation: None,
                _charge: None,
                #[cfg(test)]
                fault: Default::default(),
            }),
            position: Mutex::new(0),
        }
    }
    fn file(&self) -> &File {
        self.inner.file.as_ref().unwrap()
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self {
            inner: self.inner.clone(),
            position: Mutex::new(*self.position.lock().unwrap()),
        })
    }
    pub fn metadata(&self) -> io::Result<Metadata> {
        self.file().metadata()
    }
    pub fn sync_data(&self) -> io::Result<()> {
        self.file().sync_data()
    }
    pub fn set_len(&self, length: u64) -> io::Result<()> {
        if let Some(a) = &self.inner.allocation {
            a.reserve(length)?;
        }
        self.file().set_len(length)?;
        #[cfg(test)]
        if self
            .inner
            .fault
            .swap(0, std::sync::atomic::Ordering::SeqCst)
            == 2
        {
            return Err(io::Error::other(
                "injected uncertain truncate acknowledgment",
            ));
        }
        if let Some(a) = &self.inner.allocation {
            a.observe(&self.file().metadata()?, true)?;
        }
        Ok(())
    }
    pub fn allocation(&self) -> Option<Arc<Allocation>> {
        self.inner.allocation.clone()
    }
    fn wrote(&self, bytes: usize) -> io::Result<()> {
        if let Some(a) = &self.inner.allocation {
            let mut used = a.budget.used.lock().unwrap();
            used.write_calls = used.write_calls.saturating_add(1);
            used.written_bytes = used.written_bytes.saturating_add(bytes as u64);
            drop(used);
            a.observe(&self.file().metadata()?, false)?;
        }
        Ok(())
    }
}
impl AsRawFd for ScratchFile {
    fn as_raw_fd(&self) -> RawFd {
        self.file().as_raw_fd()
    }
}
impl FileExt for ScratchFile {
    fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        self.file().read_at(buf, offset)
    }
    fn write_at(&self, buf: &[u8], offset: u64) -> io::Result<usize> {
        if let Some(a) = &self.inner.allocation {
            a.reserve(offset.checked_add(buf.len() as u64).ok_or_else(overflow)?)?;
        }
        // Failed/partial writes keep the prospective reservation. Releasing it
        // requires an authoritative successful resize or final allocation drop.
        #[cfg(test)]
        if self
            .inner
            .fault
            .swap(0, std::sync::atomic::Ordering::SeqCst)
            == 1
        {
            let count = self
                .file()
                .write_at(&buf[..buf.len().div_ceil(2)], offset)?;
            self.wrote(count)?;
            return Err(io::Error::other("injected partial write failure"));
        }
        let count = self.file().write_at(buf, offset)?;
        self.wrote(count)?;
        Ok(count)
    }
}
impl Read for &ScratchFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut position = self.position.lock().unwrap();
        let n = self.read_at(buf, *position)?;
        *position += n as u64;
        Ok(n)
    }
}
impl Read for ScratchFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&*self).read(buf)
    }
}
impl Write for &ScratchFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut position = self.position.lock().unwrap();
        let n = self.write_at(buf, *position)?;
        *position += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Write for ScratchFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        (&*self).write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Seek for &ScratchFile {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let mut position = self.position.lock().unwrap();
        let next = match from {
            SeekFrom::Start(n) => i128::from(n),
            SeekFrom::End(n) => i128::from(self.metadata()?.len()) + i128::from(n),
            SeekFrom::Current(n) => i128::from(*position) + i128::from(n),
        };
        *position = u64::try_from(next)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "scratch seek"))?;
        Ok(*position)
    }
}
impl Seek for ScratchFile {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        (&*self).seek(from)
    }
}

const CLEANUP_SLOTS: usize = 512;
struct CleanupQueue {
    active: usize,
    pending: VecDeque<PendingCleanup>,
}
fn cleanup_queue() -> &'static Mutex<CleanupQueue> {
    static QUEUE: OnceLock<Mutex<CleanupQueue>> = OnceLock::new();
    QUEUE.get_or_init(|| {
        Mutex::new(CleanupQueue {
            active: 0,
            pending: VecDeque::with_capacity(CLEANUP_SLOTS),
        })
    })
}
struct CleanupSlot;
impl CleanupSlot {
    fn acquire() -> io::Result<Self> {
        let mut queue = cleanup_queue().lock().unwrap();
        if queue.active == CLEANUP_SLOTS {
            return Err(exhausted());
        }
        queue.active += 1;
        Ok(Self)
    }
}
impl Drop for CleanupSlot {
    fn drop(&mut self) {
        cleanup_queue().lock().unwrap().active -= 1;
    }
}
struct PendingCleanup {
    path: std::path::PathBuf,
    identity: Option<(u64, u64)>,
    _owner: RetiredAllocation,
    _slot: CleanupSlot,
}
fn remove_owned(path: &Path, identity: Option<(u64, u64)>) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
        Ok(metadata) if Some((metadata.dev(), metadata.ino())) != identity => {
            return Err(io::Error::other("private scratch cleanup identity changed"))
        }
        Ok(_) => {}
    }
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

impl ScratchBudget {
    /// One bounded host maintenance assist. A failed unlink retains its
    /// allocation and external host reservation; no background worker or
    /// unbounded failed-operation history is introduced.
    pub fn reclaim_pending(limit: usize) -> io::Result<usize> {
        let count = cleanup_queue().lock().unwrap().pending.len().min(limit);
        let mut failure = None;
        for _ in 0..count {
            let Some(item) = cleanup_queue().lock().unwrap().pending.pop_front() else {
                break;
            };
            match remove_owned(&item.path, item.identity) {
                Ok(()) => drop(item),
                Err(error) => {
                    cleanup_queue().lock().unwrap().pending.push_back(item);
                    if failure.is_none() {
                        failure = Some(error)
                    }
                }
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(cleanup_queue().lock().unwrap().pending.len())
    }
    pub fn pending_cleanup_files() -> usize {
        cleanup_queue().lock().unwrap().pending.len()
    }
    pub fn pending_own_cleanup(&self) -> usize {
        cleanup_queue()
            .lock()
            .unwrap()
            .pending
            .iter()
            .filter(|item| std::ptr::eq(item._owner.budget.as_ref(), self))
            .count()
    }
}

/// Private SQLite owns one descriptor and one named allocation. Statements use
/// bounded rows; prospective page growth is admitted before their transaction.
pub(super) struct DatabaseCharge {
    allocation: Arc<Allocation>,
    _file: Option<FileCharge>,
}
impl DatabaseCharge {
    #[cfg(test)]
    pub(super) fn new(budget: &Arc<ScratchBudget>) -> io::Result<Self> {
        let file = budget.acquire_file()?;
        let allocation = budget.allocation();
        allocation.reserve(4 * PAGE)?;
        Ok(Self {
            allocation,
            _file: Some(file),
        })
    }
    pub(super) fn for_file(file: &ScratchFile) -> io::Result<Self> {
        let allocation = file
            .allocation()
            .ok_or_else(|| io::Error::other("uncharged SQLite observer"))?;
        let charge = allocation.budget.acquire_file()?;
        allocation.reserve(4 * PAGE)?;
        Ok(Self {
            allocation,
            _file: Some(charge),
        })
    }
    pub(super) fn settle_file(&self, file: &ScratchFile) -> Result<()> {
        self.allocation.observe(&file.metadata()?, true)?;
        Ok(())
    }
    pub(super) fn before_rows(&self, connection: &rusqlite::Connection, rows: usize) -> Result<()> {
        let page_size: i64 = connection.pragma_query_value(None, "page_size", |r| r.get(0))?;
        if page_size != PAGE as i64 {
            return Err(crate::StoreError::Integrity("charged SQLite page size"));
        }
        let pages: i64 = connection.pragma_query_value(None, "page_count", |r| r.get(0))?;
        let pages = u64::try_from(pages)
            .map_err(|_| crate::StoreError::Integrity("charged SQLite page count"))?;
        // Each fixed-width insertion can split at most the current B-tree
        // height. The on-disk page-count bound makes that height <=64.
        let extra = (rows as u64)
            .checked_mul(65)
            .and_then(|n| n.checked_add(2))
            .ok_or_else(overflow)?;
        let ceiling = pages.checked_add(extra).ok_or_else(overflow)?;
        self.allocation
            .reserve(ceiling.checked_mul(PAGE).ok_or_else(overflow)?)?;
        connection.pragma_update(
            None,
            "max_page_count",
            i64::try_from(ceiling).map_err(|_| overflow())?,
        )?;
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn settle(&self, path: &Path) -> Result<()> {
        self.allocation.observe(&std::fs::metadata(path)?, true)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Owner(Arc<AtomicUsize>);
    impl Drop for Owner {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    struct Fixture {
        path: std::path::PathBuf,
        budget: Arc<ScratchBudget>,
        dropped: Arc<AtomicUsize>,
    }
    impl Fixture {
        fn new(bytes: u64, files: usize) -> Self {
            let (file, path) =
                crate::objects::spill::temporary_file("charged-scratch-test").unwrap();
            drop(file);
            std::fs::remove_file(&path).unwrap();
            let dropped = Arc::new(AtomicUsize::new(0));
            let budget = ScratchBudget::new(
                ScratchLimits { bytes, files },
                Arc::new(Owner(dropped.clone())),
            )
            .unwrap();
            Self {
                path,
                budget,
                dropped,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
    /// Named scope scratch must be created inside the owning directory with
    /// private permissions and no extra link, so cleanup and custody are scoped
    /// to the Workspace that admitted it rather than to the shared temporary
    /// directory.
    #[test]
    fn named_scope_scratch_is_placed_in_the_owning_directory() {
        let root =
            std::env::temp_dir().join(format!("layerfs-scratch-placement-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let budget = ScratchBudget::with_directory(
            ScratchLimits {
                bytes: 1024 * 1024,
                files: 8,
            },
            Arc::new(Owner(Arc::new(AtomicUsize::new(0)))),
            Some(root.clone()),
        )
        .unwrap();
        assert_eq!(budget.directory(), root);
        let (file, path) =
            crate::objects::spill::temporary_output_file("placement-probe", &Some(budget.clone()))
                .unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert_eq!(path.parent(), Some(root.as_path()));
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
        assert_eq!(metadata.nlink(), 1);
        assert!(budget.usage().reserved_files >= 1);
        drop(file);
        drop(budget);
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// An explicit placement must be an existing directory; a bad placement is
    /// rejected rather than silently redirected.
    #[test]
    fn explicit_scratch_placement_rejects_a_missing_directory() {
        let missing =
            std::env::temp_dir().join(format!("layerfs-scratch-missing-{}", std::process::id()));
        assert!(ScratchBudget::with_directory(
            ScratchLimits {
                bytes: 1024 * 1024,
                files: 8,
            },
            Arc::new(Owner(Arc::new(AtomicUsize::new(0)))),
            Some(missing),
        )
        .is_err());
    }

    #[test]
    fn exact_growth_shared_fd_named_reopen_and_external_owner_are_charged() {
        let f = Fixture::new(8192, 1);
        let (mut writer, allocation) = ScratchFile::create(&f.path, Some(&f.budget)).unwrap();
        let allocation = allocation.unwrap();
        writer.write_all(&vec![7; 5000]).unwrap();
        let mut reader = writer.try_clone().unwrap();
        assert_eq!(writer.as_raw_fd(), reader.as_raw_fd());
        reader.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = [0; 3];
        reader.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, [7; 3]);
        writer.write_all(&[9]).unwrap();
        assert_eq!(reader.seek(SeekFrom::Current(0)).unwrap(), 3);
        assert_eq!(f.budget.usage().reserved_files, 1);
        assert!(ScratchFile::open(&f.path, Some(allocation.clone()), false).is_err());
        assert!(writer.write_at(&[1], 8192).is_err());
        assert_eq!(writer.metadata().unwrap().len(), 5001);
        drop(writer);
        drop(reader);
        let usage = f.budget.usage();
        assert_eq!(usage.reserved_files, 0);
        assert_eq!(usage.reserved_bytes, 8192);
        assert!(usage.observed_allocated_bytes > 0);
        let reader = ScratchFile::open(&f.path, Some(allocation.clone()), false).unwrap();
        reader.read_exact_at(&mut bytes, 4998).unwrap();
        assert_eq!(bytes, [7, 7, 9]);
        std::fs::remove_file(&f.path).unwrap();
        drop(allocation);
        drop(reader);
        assert_eq!(f.budget.usage().reserved_bytes, 0);
        assert_eq!(f.budget.usage().observed_allocated_bytes, 0);
        assert_eq!(f.dropped.load(Ordering::SeqCst), 0);
        println!("charged_named_peak {usage:?}");
        let (last, allocation) = ScratchFile::create(&f.path, Some(&f.budget)).unwrap();
        drop(allocation);
        let dropped = f.dropped.clone();
        drop(f);
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        drop(last);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn partial_write_and_uncertain_truncate_keep_prospective_charge_until_verified() {
        let f = Fixture::new(16384, 1);
        let (file, allocation) = ScratchFile::create(&f.path, Some(&f.budget)).unwrap();
        std::fs::remove_file(&f.path).unwrap();
        drop(allocation);
        file.write_all_at(&[3; 4096], 0).unwrap();
        file.inner.fault.store(1, Ordering::SeqCst);
        assert!(file.write_all_at(&[4; 8192], 4096).is_err());
        assert_eq!(file.metadata().unwrap().len(), 8192);
        assert_eq!(f.budget.usage().reserved_bytes, 12288);
        let mut prefix = [0; 1];
        file.read_exact_at(&mut prefix, 0).unwrap();
        assert_eq!(prefix, [3]);
        file.read_exact_at(&mut prefix, 4096).unwrap();
        assert_eq!(prefix, [4]);
        file.inner.fault.store(2, Ordering::SeqCst);
        assert!(file.set_len(4096).is_err());
        assert_eq!(f.budget.usage().reserved_bytes, 12288);
        file.set_len(4096).unwrap();
        assert_eq!(f.budget.usage().reserved_bytes, 4096);
        let usage = f.budget.usage();
        drop(file);
        assert_eq!(f.budget.usage().reserved_bytes, 0);
        println!("charged_failure_peak {usage:?}");
    }
    #[test]
    fn sqlite_growth_is_admitted_before_bounded_rows_and_observed_after_commit() {
        let f = Fixture::new(64 * 1024 * 1024, 1);
        let charge = DatabaseCharge::new(&f.budget).unwrap();
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&f.path)
            .unwrap();
        drop(file);
        let connection = rusqlite::Connection::open(&f.path).unwrap();
        crate::objects::spill::configure_scratch(&connection).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE rows (id BLOB PRIMARY KEY CHECK(length(id)=32)) WITHOUT ROWID;",
            )
            .unwrap();
        charge.settle(&f.path).unwrap();
        charge.before_rows(&connection, 128).unwrap();
        let reserved = f.budget.usage().reserved_bytes;
        connection.execute_batch("BEGIN;").unwrap();
        for id in 0u64..128 {
            let mut key = [0; 32];
            key[..8].copy_from_slice(&id.to_be_bytes());
            connection
                .execute("INSERT INTO rows VALUES (?1)", [&key[..]])
                .unwrap();
        }
        connection.execute_batch("COMMIT;").unwrap();
        charge.settle(&f.path).unwrap();
        let usage = f.budget.usage();
        assert!(usage.reserved_bytes < reserved);
        assert!(usage.observed_allocated_bytes > 0);
        assert_eq!(usage.reserved_files, 1);
        assert!(charge.before_rows(&connection, usize::MAX).is_err());
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM rows", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            128
        );
        drop(connection);
        std::fs::remove_file(&f.path).unwrap();
        drop(charge);
        assert_eq!(f.budget.usage().reserved_bytes, 0);
        assert_eq!(f.budget.usage().reserved_files, 0);
        println!("charged_sqlite_peak {usage:?}");
    }
    #[test]
    #[ignore = "native macOS SQLite rejects unlinked vnode with IOERR_VNODE; retained mechanism probe"]
    fn private_off_journal_sqlite_keeps_owned_unlinked_file_through_paging() {
        let f = Fixture::new(64 * 1024 * 1024, 2);
        let (file, allocation) = ScratchFile::create(&f.path, Some(&f.budget)).unwrap();
        drop(allocation);
        let charge = DatabaseCharge::for_file(&file).unwrap();
        let connection = rusqlite::Connection::open(&f.path).unwrap();
        crate::objects::spill::configure_scratch(&connection).unwrap();
        std::fs::remove_file(&f.path).unwrap();
        connection.pragma_update(None, "cache_size", -4).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE rows (id BLOB PRIMARY KEY CHECK(length(id)=32)) WITHOUT ROWID;",
            )
            .unwrap();
        for page in 0u64..8 {
            charge.before_rows(&connection, 128).unwrap();
            connection.execute_batch("BEGIN;").unwrap();
            for id in page * 128..(page + 1) * 128 {
                let mut key = [0; 32];
                key[..8].copy_from_slice(&id.to_be_bytes());
                connection
                    .execute("INSERT INTO rows VALUES (?1)", [&key[..]])
                    .unwrap();
            }
            connection.execute_batch("COMMIT;").unwrap();
            charge.settle_file(&file).unwrap();
        }
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM rows", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1024
        );
        assert!(!f.path.exists());
        let usage = f.budget.usage();
        assert_eq!(usage.reserved_files, 2);
        assert!(usage.observed_allocated_bytes > 4096);
        drop(connection);
        let mut header = [0; 16];
        file.read_exact_at(&mut header, 0).unwrap();
        assert_eq!(&header, b"SQLite format 3\0");
        drop(file);
        drop(charge);
        assert_eq!(f.budget.usage().reserved_bytes, 0);
        assert_eq!(f.budget.usage().reserved_files, 0);
        println!("unlinked_sqlite_paging {usage:?}");
    }
    #[test]
    fn replaced_private_name_is_preserved_and_cleanup_keeps_charge_until_owner_restored() {
        let f = Fixture::new(8192, 1);
        let (file, allocation) = ScratchFile::create(&f.path, Some(&f.budget)).unwrap();
        drop(allocation);
        file.write_all_at(&[5; 4096], 0).unwrap();
        let backup = f.path.with_extension("retained");
        std::fs::rename(&f.path, &backup).unwrap();
        std::fs::write(&f.path, b"unrelated").unwrap();
        drop(file);
        assert_eq!(f.budget.pending_own_cleanup(), 1);
        assert_eq!(f.budget.usage().reserved_files, 0);
        assert_eq!(f.budget.usage().reserved_bytes, 4096);
        assert!(ScratchBudget::reclaim_pending(512).is_err());
        assert_eq!(std::fs::read(&f.path).unwrap(), b"unrelated");
        std::fs::remove_file(&f.path).unwrap();
        std::fs::rename(&backup, &f.path).unwrap();
        let _ = ScratchBudget::reclaim_pending(512);
        assert_eq!(f.budget.pending_own_cleanup(), 0);
        assert_eq!(f.budget.usage().reserved_bytes, 0);
        assert!(!f.path.exists());
    }
    #[test]
    fn scoped_object_spill_seen_and_order_use_actual_owned_scratch() {
        use crate::objects::spill::{IdOrder, SpillableObjectSet};
        use crate::objects::{DeferredObjectStore, DeferredObjects};
        use layerfs_content::ObjectId;
        let f = Fixture::new(128 * 1024 * 1024, 16);
        let context = Some(f.budget.clone());
        let raw = [b"one".as_slice(), b"two", b"three", b"four"]
            .map(|b| layerfs_content::encode_bytes_object(b).unwrap());
        let ids = raw.each_ref().map(|b| ObjectId::for_bytes(b));
        let mut objects = DeferredObjectStore::new_all_reachable().unwrap();
        objects.scratch = context.clone();
        for i in 0..2 {
            objects.put(ids[i], &raw[i]).unwrap();
        }
        objects.spill().unwrap();
        let DeferredObjects::Spill(spill) = &mut objects.storage else {
            panic!("forced private spill")
        };
        assert!(!spill.path.exists());
        spill.index_limit = 2 * 64; // same exact transition fixture as the existing spill-index check
        objects.put(ids[2], &raw[2]).unwrap();
        objects.put(ids[3], &raw[3]).unwrap();
        for i in 0..4 {
            assert_eq!(objects.get(ids[i]).unwrap().unwrap(), raw[i]);
        }
        let DeferredObjects::Spill(spill) = &objects.storage else {
            panic!("forced private spill")
        };
        assert!(spill.disk_index.is_some());
        let mut order = IdOrder::empty();
        for id in ids {
            order.push_scoped(id, 64, &context).unwrap();
        }
        order.seal().unwrap();
        let mut read = Vec::new();
        order
            .visit(|id| {
                read.push(id);
                Ok(())
            })
            .unwrap();
        assert_eq!(read, ids);
        let mut seen =
            SpillableObjectSet::bounded_scoped(4 * 1024 * 1024, context.clone()).unwrap();
        assert_eq!(seen.insert_page(&ids).unwrap(), ids);
        assert!(seen.insert_page(&ids).unwrap().is_empty());
        let usage = f.budget.usage();
        assert!(usage.reserved_files >= 6);
        assert!(usage.observed_allocated_bytes >= 4 * 4096);
        drop(seen);
        drop(order);
        drop(objects);
        drop(context);
        assert_eq!(f.budget.usage().reserved_files, 0);
        assert_eq!(f.budget.usage().reserved_bytes, 0);
        assert_eq!(f.budget.pending_own_cleanup(), 0);
        println!("scoped_canonical_scratch {usage:?}");
    }
}
