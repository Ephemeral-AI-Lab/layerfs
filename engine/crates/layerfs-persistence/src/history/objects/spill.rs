//! Sorting in bounded memory for one mark, and for the compaction around it
//! (the pack lengths and the verification's checks by pack).
//!
//! A mark has to put several sets in order that grow with the Store: the
//! identities a wave reached, the objects of the next wave by locator, the
//! records it kept by pack. None of them may be held whole. A [`Sorter`]
//! keeps at most a fixed number of bytes of items; when they are full it
//! sorts them, folds equal neighbours and, if that freed too little, writes
//! them as one sorted run to a file of its own. Reading merges the runs with
//! what is still held, through one small window per run. A set that is read
//! many times, a few items at a time, is merged once into a [`Sealed`] run
//! that is found by key.
//!
//! The files have no name ([`Space`]): nothing is left behind by a mark that
//! ends, fails or is killed, and the space returns with the handle. They are
//! not Store state and nothing in them survives the mark.
use layerfs_history::error::{HistoryError, HistoryResult};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering as Atomic};
use std::sync::Arc;

/// Bytes of one run a merge holds at a time, at most.
const WINDOW_BYTES: usize = 64 * 1024;

/// A fixed-width value a [`Sorter`] can write to its file.
pub(crate) trait Item: Copy {
    /// The encoded width.
    const BYTES: usize;
    fn encode(&self, out: &mut [u8]);
    /// `None` when the bytes are not an encoded value.
    fn decode(bytes: &[u8]) -> Option<Self>;
    /// Folds `other`, which sorts equal to `self`, into `self`. `false`
    /// keeps both.
    fn absorb(&mut self, _other: &Self) -> bool {
        false
    }
    /// Whether [`Self::encode`] keeps every field whole. A value that does
    /// not fit is never written.
    fn fits(&self) -> bool {
        true
    }
}

/// One sorted run in the file.
#[derive(Clone, Copy)]
struct Run {
    offset: u64,
    items: u64,
}

/// A failed read or write of a sort file. A full disk, quota or file size
/// limit is the Store out of room; a directory that refuses the file is a
/// Store this process may not write beside; anything else contradicts what
/// the mark wrote.
fn io_error(error: std::io::Error) -> HistoryError {
    use std::io::ErrorKind as Kind;
    match error.kind() {
        Kind::StorageFull | Kind::QuotaExceeded | Kind::FileTooLarge => {
            HistoryError::Capacity("disk space")
        }
        Kind::PermissionDenied | Kind::ReadOnlyFilesystem => HistoryError::ContinuityUnavailable,
        _ => HistoryError::Integrity("mark sort file I/O"),
    }
}

/// The start of every name a sort file can have while it has one.
const NAME: &str = ".layerfs-mark-";

/// Where the sort files of one mark go, and how much they hold together.
///
/// On Linux a file is created without a name (`O_TMPFILE`). Where the
/// platform or the file system has no such file, it is created under a name
/// no other process chooses and unlinked at once; a process that dies
/// between the two calls leaves that name, and the next mark that writes a
/// sort file in the directory removes every such name first. Removing the
/// name of a file another process is about to unlink is harmless: the file
/// lives by its handle, and a missing name is what the unlink wanted.
#[derive(Debug)]
pub struct Space {
    directory: PathBuf,
    held: AtomicU64,
    peak: AtomicU64,
    swept: AtomicBool,
}

impl Space {
    /// A space whose files are created in `directory`.
    pub fn new(directory: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            directory,
            held: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            swept: AtomicBool::new(false),
        })
    }

    /// The most bytes the files of this space held at one time.
    pub fn peak_bytes(&self) -> u64 {
        self.peak.load(Atomic::Relaxed)
    }

    /// The bytes the files of this space hold now.
    pub fn held_bytes(&self) -> u64 {
        self.held.load(Atomic::Relaxed)
    }

    fn grow(&self, bytes: u64) {
        let held = self.held.fetch_add(bytes, Atomic::Relaxed) + bytes;
        self.peak.fetch_max(held, Atomic::Relaxed);
    }

    fn shrink(&self, bytes: u64) {
        self.held.fetch_sub(bytes, Atomic::Relaxed);
    }

    /// A new file with no name, open for reading and writing.
    fn file(&self) -> HistoryResult<File> {
        if !self.swept.swap(true, Atomic::Relaxed) {
            sweep(&self.directory);
        }
        unnamed(&self.directory).map_err(io_error)
    }
}

/// Removes the names a dead process left in `directory`. Best effort: a
/// name that stays costs disk, never correctness.
fn sweep(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(NAME) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[cfg(target_os = "linux")]
fn unnamed(directory: &Path) -> std::io::Result<File> {
    use nix::errno::Errno;
    use nix::fcntl::{open, OFlag};
    use nix::sys::stat::Mode;
    match open(
        directory,
        OFlag::O_TMPFILE | OFlag::O_RDWR | OFlag::O_CLOEXEC,
        Mode::S_IRUSR | Mode::S_IWUSR,
    ) {
        Ok(descriptor) => Ok(File::from(descriptor)),
        // A file system without unnamed files answers one of these.
        Err(Errno::EOPNOTSUPP | Errno::EISDIR | Errno::EINVAL) => named(directory),
        Err(error) => Err(std::io::Error::from(error)),
    }
}

#[cfg(not(target_os = "linux"))]
fn unnamed(directory: &Path) -> std::io::Result<File> {
    named(directory)
}

/// A file created under a fresh name and unlinked while the handle stays
/// open. The name carries the process id, a number drawn from the process's
/// random hashing keys and a counter, so processes that share a directory
/// and a process id (two PID namespaces) still choose different names.
fn named(directory: &Path) -> std::io::Result<File> {
    use std::hash::{BuildHasher, RandomState};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let next = NEXT.fetch_add(1, Atomic::Relaxed);
    let path = directory.join(format!(
        "{NAME}{}-{:016x}-{next}",
        std::process::id(),
        RandomState::new().hash_one(next),
    ));
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(file),
        // Another mark's sweep took the name first; the file has none now.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(file),
        Err(error) => Err(error),
    }
}

/// Items in one order, held in bounded memory. See the module comment.
pub(crate) struct Sorter<T: Item> {
    space: Arc<Space>,
    order: fn(&T, &T) -> Ordering,
    /// Items held before they are settled.
    limit: usize,
    held: Vec<T>,
    /// Whether `held` is sorted and folded.
    settled: bool,
    runs: Vec<Run>,
    file: Option<File>,
    written: u64,
    /// Items added since the last [`Self::clear`], before any folding.
    added: u64,
}

impl<T: Item> Sorter<T> {
    /// A sorter that holds at most about `bytes` of items in memory.
    pub(crate) fn new(space: &Arc<Space>, bytes: usize, order: fn(&T, &T) -> Ordering) -> Self {
        Self {
            space: space.clone(),
            order,
            limit: (bytes / std::mem::size_of::<T>().max(1)).max(16),
            held: Vec::new(),
            settled: true,
            runs: Vec::new(),
            file: None,
            written: 0,
            added: 0,
        }
    }

    /// Adds one item.
    pub(crate) fn push(&mut self, item: T) -> HistoryResult<()> {
        self.hold(item);
        self.relieve()
    }

    /// Adds one item without ever writing: for a caller that cannot fail
    /// here and calls [`Self::relieve`] soon after.
    pub(crate) fn hold(&mut self, item: T) {
        self.held.push(item);
        self.settled = false;
        self.added += 1;
    }

    /// Brings the held items back under the bound.
    pub(crate) fn relieve(&mut self) -> HistoryResult<()> {
        if self.held.len() < self.limit {
            return Ok(());
        }
        self.settle();
        // Folding freed less than half: write the run. Otherwise the items
        // stay, and the next fold meets them already in order.
        if self.held.len() * 2 > self.limit {
            self.write_run()?;
        }
        Ok(())
    }

    /// Whether nothing was added since the last [`Self::clear`].
    pub(crate) fn is_empty(&self) -> bool {
        self.held.is_empty() && self.runs.is_empty()
    }

    /// How many items were added since the last [`Self::clear`]: at least
    /// the number [`Self::sorted`] yields, which folds equal ones.
    pub(crate) fn added(&self) -> u64 {
        self.added
    }

    fn settle(&mut self) {
        if self.settled {
            return;
        }
        let order = self.order;
        self.held.sort_unstable_by(order);
        let mut kept = 0;
        for index in 1..self.held.len() {
            let next = self.held[index];
            if order(&self.held[kept], &next) == Ordering::Equal && self.held[kept].absorb(&next) {
                continue;
            }
            kept += 1;
            self.held[kept] = next;
        }
        self.held.truncate((kept + 1).min(self.held.len()));
        self.settled = true;
    }

    fn write_run(&mut self) -> HistoryResult<()> {
        if self.file.is_none() {
            self.file = Some(self.space.file()?);
        }
        let Some(file) = self.file.as_mut() else {
            return Err(HistoryError::Integrity("mark sort file"));
        };
        file.seek(SeekFrom::Start(self.written)).map_err(io_error)?;
        let mut bytes = vec![0u8; WINDOW_BYTES.max(T::BYTES)];
        let per_write = bytes.len() / T::BYTES;
        for items in self.held.chunks(per_write) {
            for (item, out) in items.iter().zip(bytes.chunks_exact_mut(T::BYTES)) {
                if !item.fits() {
                    return Err(HistoryError::Integrity("mark sort record"));
                }
                item.encode(out);
            }
            file.write_all(&bytes[..items.len() * T::BYTES])
                .map_err(io_error)?;
        }
        let items = self.held.len() as u64;
        self.runs.push(Run {
            offset: self.written,
            items,
        });
        self.written += items * T::BYTES as u64;
        self.space.grow(items * T::BYTES as u64);
        self.held.clear();
        self.settled = true;
        Ok(())
    }

    /// Forgets every item and gives the file's space back.
    pub(crate) fn clear(&mut self) -> HistoryResult<()> {
        self.held.clear();
        self.settled = true;
        self.runs.clear();
        self.added = 0;
        self.space.shrink(std::mem::take(&mut self.written));
        if let Some(file) = &self.file {
            file.set_len(0).map_err(io_error)?;
        }
        Ok(())
    }

    /// Every item in order, equal items folded. May be read again.
    pub(crate) fn sorted(&mut self) -> HistoryResult<Merge<'_, T>> {
        self.settle();
        // A window per run, sized so that all of them stay within what the
        // sorter may hold.
        let window = (self.limit * std::mem::size_of::<T>() / self.runs.len().max(1))
            .clamp(T::BYTES, WINDOW_BYTES)
            / T::BYTES;
        let mut merge = Merge {
            order: self.order,
            file: self.file.as_ref(),
            runs: self
                .runs
                .iter()
                .map(|run| Reader {
                    offset: run.offset,
                    left: run.items,
                    window: Vec::new(),
                    at: 0,
                })
                .collect(),
            window,
            held: &self.held,
            held_at: 0,
            heads: BinaryHeap::new(),
        };
        for source in 0..merge.runs.len() {
            merge.advance(source)?;
        }
        merge.advance(usize::MAX)?;
        Ok(merge)
    }
}

impl<T: Item> Sorter<T> {
    /// Merges every item into one run that is read by key: `key` must not
    /// decrease along the sorter's order. The sorter's own file is released.
    pub(crate) fn seal(mut self, key: fn(&T) -> i64) -> HistoryResult<Sealed<T>> {
        self.settle();
        let mut sealed = Sealed {
            space: self.space.clone(),
            key,
            memory: Vec::new(),
            file: None,
            items: 0,
            marks: Vec::new(),
            window: Vec::new(),
            first: 0,
            at: 0,
            passed: i64::MIN,
        };
        if self.runs.is_empty() {
            sealed.memory = std::mem::take(&mut self.held);
            return Ok(sealed);
        }
        let mut file = self.space.file()?;
        let mut bytes = vec![0u8; WINDOW_BYTES.max(T::BYTES) / T::BYTES * T::BYTES];
        let mut filled = 0;
        let mut merged = self.sorted()?;
        while let Some(item) = merged.next()? {
            if sealed.items % STRIDE == 0 {
                sealed.marks.push(key(&item));
            }
            if !item.fits() {
                return Err(HistoryError::Integrity("mark sort record"));
            }
            item.encode(&mut bytes[filled..filled + T::BYTES]);
            filled += T::BYTES;
            sealed.items += 1;
            if filled == bytes.len() {
                file.write_all(&bytes).map_err(io_error)?;
                sealed.space.grow(filled as u64);
                filled = 0;
            }
        }
        file.write_all(&bytes[..filled]).map_err(io_error)?;
        sealed.space.grow(filled as u64);
        sealed.file = Some(file);
        Ok(sealed)
    }
}

impl<T: Item> Drop for Sorter<T> {
    fn drop(&mut self) {
        self.space.shrink(self.written);
    }
}

/// Items of a [`Sealed`] run between two remembered keys.
const STRIDE: u64 = 128;

/// One sorted run, read a key at a time ([`Sorter::seal`]).
///
/// The run stays in its file. What is held is the key of every
/// [`STRIDE`]th item, eight bytes each, and one window of items. A set
/// that never left its sorter's memory stays there instead.
pub(crate) struct Sealed<T: Item> {
    space: Arc<Space>,
    key: fn(&T) -> i64,
    /// The items, when no file was written.
    memory: Vec<T>,
    file: Option<File>,
    items: u64,
    /// `marks[n]` is the key of item `n * STRIDE`.
    marks: Vec<i64>,
    /// The items held, the position of the first, and the next one to read.
    window: Vec<T>,
    first: u64,
    at: u64,
    /// The key of the last item read past; every item before `at` has a key
    /// at most this.
    passed: i64,
}

impl<T: Item> Sealed<T> {
    /// Hands every item whose key is `key` to `each`, in order.
    pub(crate) fn each_of(&mut self, key: i64, mut each: impl FnMut(T)) -> HistoryResult<()> {
        if self.file.is_none() {
            let of = self.key;
            let from = self.memory.partition_point(|item| of(item) < key);
            for item in self.memory[from..]
                .iter()
                .take_while(|item| of(item) == key)
            {
                each(*item);
            }
            return Ok(());
        }
        // The block whose first key is the last one below `key`: no earlier
        // item has the key. Reading goes on from where it stands when that
        // is inside the block or past it and still before the key.
        let block = self.marks.partition_point(|mark| *mark < key);
        let start = block.saturating_sub(1) as u64 * STRIDE;
        if self.at < start || self.passed >= key {
            self.at = start;
            self.passed = i64::MIN;
        }
        while self.at < self.items {
            if self.at < self.first || self.at >= self.first + self.window.len() as u64 {
                self.fill()?;
            }
            let item = self.window[(self.at - self.first) as usize];
            let found = (self.key)(&item);
            if found > key {
                break;
            }
            self.at += 1;
            if found == key {
                each(item);
            } else {
                self.passed = found;
            }
        }
        Ok(())
    }

    /// Holds the items from `at` on: a whole window when the read continues
    /// the one before, and one stride after a jump.
    fn fill(&mut self) -> HistoryResult<()> {
        let Some(mut file) = self.file.as_ref() else {
            return Err(HistoryError::Integrity("mark sort file"));
        };
        let continues = self.at == self.first + self.window.len() as u64 && !self.window.is_empty();
        let most = if continues {
            (WINDOW_BYTES / T::BYTES).max(1) as u64
        } else {
            STRIDE
        };
        let items = most.min(self.items - self.at) as usize;
        let mut bytes = vec![0u8; items * T::BYTES];
        file.seek(SeekFrom::Start(self.at * T::BYTES as u64))
            .map_err(io_error)?;
        file.read_exact(&mut bytes).map_err(io_error)?;
        self.window.clear();
        for encoded in bytes.chunks_exact(T::BYTES) {
            self.window
                .push(T::decode(encoded).ok_or(HistoryError::Integrity("mark sort file"))?);
        }
        self.first = self.at;
        Ok(())
    }
}

impl<T: Item> Drop for Sealed<T> {
    fn drop(&mut self) {
        if self.file.is_some() {
            self.space.shrink(self.items * T::BYTES as u64);
        }
    }
}

/// One run being read.
struct Reader<T> {
    offset: u64,
    left: u64,
    window: Vec<T>,
    at: usize,
}

/// The next item of one source, ordered so the heap yields the least first.
struct Head<T> {
    item: T,
    source: usize,
    order: fn(&T, &T) -> Ordering,
}

impl<T> PartialEq for Head<T> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<T> Eq for Head<T> {}
impl<T> PartialOrd for Head<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl<T> Ord for Head<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reversed: `BinaryHeap` is a max-heap. Ties go to the earlier run.
        (self.order)(&other.item, &self.item).then(other.source.cmp(&self.source))
    }
}

/// The merged, ordered items of a [`Sorter`].
pub(crate) struct Merge<'a, T: Item> {
    order: fn(&T, &T) -> Ordering,
    file: Option<&'a File>,
    runs: Vec<Reader<T>>,
    window: usize,
    held: &'a [T],
    held_at: usize,
    heads: BinaryHeap<Head<T>>,
}

impl<T: Item> Merge<'_, T> {
    /// Puts the next item of `source` on the heap; `usize::MAX` names the
    /// items still held in memory.
    fn advance(&mut self, source: usize) -> HistoryResult<()> {
        let item = if source == usize::MAX {
            let item = self.held.get(self.held_at).copied();
            self.held_at += 1;
            item
        } else {
            let reader = &mut self.runs[source];
            if reader.at == reader.window.len() && reader.left > 0 {
                let Some(mut file) = self.file else {
                    return Err(HistoryError::Integrity("mark sort file"));
                };
                let items = (self.window as u64).min(reader.left) as usize;
                let mut bytes = vec![0u8; items * T::BYTES];
                file.seek(SeekFrom::Start(reader.offset))
                    .map_err(io_error)?;
                file.read_exact(&mut bytes).map_err(io_error)?;
                reader.window.clear();
                for encoded in bytes.chunks_exact(T::BYTES) {
                    let item =
                        T::decode(encoded).ok_or(HistoryError::Integrity("mark sort file"))?;
                    reader.window.push(item);
                }
                reader.at = 0;
                reader.offset += bytes.len() as u64;
                reader.left -= items as u64;
            }
            let item = reader.window.get(reader.at).copied();
            reader.at += 1;
            item
        };
        if let Some(item) = item {
            self.heads.push(Head {
                item,
                source,
                order: self.order,
            });
        }
        Ok(())
    }

    /// The next item in order, or `None` at the end.
    pub(crate) fn next(&mut self) -> HistoryResult<Option<T>> {
        let Some(head) = self.heads.pop() else {
            return Ok(None);
        };
        self.advance(head.source)?;
        let mut item = head.item;
        while let Some(next) = self.heads.peek() {
            if (self.order)(&item, &next.item) != Ordering::Equal || !item.absorb(&next.item) {
                break;
            }
            let source = next.source;
            self.heads.pop();
            self.advance(source)?;
        }
        Ok(Some(item))
    }

    /// The next item without taking it.
    pub(crate) fn peek(&self) -> Option<&T> {
        self.heads.peek().map(|head| &head.item)
    }
}
