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
/// Runs one level of a sorter holds, at most, whatever its size.
const MOST_FAN_IN: usize = 128;

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
pub(super) fn io_error(error: std::io::Error) -> HistoryError {
    use std::io::ErrorKind as Kind;
    match error.kind() {
        Kind::StorageFull | Kind::QuotaExceeded | Kind::FileTooLarge => {
            HistoryError::Capacity("disk space")
        }
        Kind::PermissionDenied | Kind::ReadOnlyFilesystem => HistoryError::ContinuityUnavailable,
        _ => HistoryError::Integrity("mark sort file I/O"),
    }
}

/// The budget of a run: what the sizes written beside each buffer and cache
/// of a mark and a compaction are stated for. They add up to less than this,
/// and not all of them are held at one time.
pub const BUDGET_BYTES: u64 = 256 * 1024 * 1024;
/// The least budget a space takes: one 4,096th of [`BUDGET_BYTES`].
pub const LEAST_BUDGET_BYTES: u64 = 64 * 1024;
/// The least a sorter holds, whatever the budget.
const LEAST_SORT_BYTES: usize = 32 * 1024;

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
///
/// It also carries the run's one budget: the number every buffer and cache
/// of a mark and of the compaction around it is sized from
/// ([`Self::scaled`]). The budget only sizes them; no size selects another
/// way of working, and none lets a set be held whole.
#[derive(Debug)]
pub struct Space {
    directory: PathBuf,
    budget: u64,
    held: AtomicU64,
    peak: AtomicU64,
    swept: AtomicBool,
    /// Pages a paged array ([`super::paged`]) was asked for and did not
    /// hold, pages it read back from its file, pages it wrote there, and
    /// how often the arrays that are gone were asked for a page at all.
    paging: [AtomicU64; 4],
}

impl Space {
    /// A space whose files are created in `directory`, with the budget
    /// every product run has ([`BUDGET_BYTES`]).
    pub fn new(directory: PathBuf) -> Arc<Self> {
        Self::sized(directory, BUDGET_BYTES)
    }

    /// A space with a budget of `budget` bytes, and at least
    /// [`LEAST_BUDGET_BYTES`].
    pub fn sized(directory: PathBuf, budget: u64) -> Arc<Self> {
        Arc::new(Self {
            directory,
            budget: budget.max(LEAST_BUDGET_BYTES),
            held: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            swept: AtomicBool::new(false),
            paging: Default::default(),
        })
    }

    /// The size, under this space's budget, of a buffer that is `bytes`
    /// under [`BUDGET_BYTES`]. Each buffer has its own least size on top.
    pub fn scaled(&self, bytes: usize) -> usize {
        let scaled = bytes as u128 * u128::from(self.budget) / u128::from(BUDGET_BYTES);
        usize::try_from(scaled).unwrap_or(usize::MAX)
    }

    /// The most bytes the files of this space held at one time.
    pub fn peak_bytes(&self) -> u64 {
        self.peak.load(Atomic::Relaxed)
    }

    /// The bytes the files of this space hold now.
    pub fn held_bytes(&self) -> u64 {
        self.held.load(Atomic::Relaxed)
    }

    /// What the paged arrays of this space did: the pages asked for that
    /// were not held, the pages read back from a file, the pages written to
    /// one, and every time a page was asked for, counted when its array is
    /// dropped. An array that fits its memory reads and writes none.
    pub fn paging(&self) -> [u64; 4] {
        [0, 1, 2, 3].map(|at| self.paging[at].load(Atomic::Relaxed))
    }

    pub(super) fn asked(&self, times: u64) {
        self.paging[3].fetch_add(times, Atomic::Relaxed);
    }

    pub(super) fn missed(&self) {
        self.paging[0].fetch_add(1, Atomic::Relaxed);
    }

    pub(super) fn paged_in(&self) {
        self.paging[1].fetch_add(1, Atomic::Relaxed);
    }

    pub(super) fn paged_out(&self) {
        self.paging[2].fetch_add(1, Atomic::Relaxed);
    }

    pub(super) fn grow(&self, bytes: u64) {
        let held = self.held.fetch_add(bytes, Atomic::Relaxed) + bytes;
        self.peak.fetch_max(held, Atomic::Relaxed);
    }

    pub(super) fn shrink(&self, bytes: u64) {
        self.held.fetch_sub(bytes, Atomic::Relaxed);
    }

    /// A new file with no name, open for reading and writing.
    pub(super) fn file(&self) -> HistoryResult<File> {
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

/// The runs of one level and the file they are in. A level's file is
/// emptied when its runs are merged into the level above.
struct Level {
    file: Option<File>,
    runs: Vec<Run>,
    written: u64,
}

/// Items in one order, held in bounded memory. See the module comment.
pub(crate) struct Sorter<T: Item> {
    space: Arc<Space>,
    order: fn(&T, &T) -> Ordering,
    /// Items held before they are settled: what `held` is allocated for.
    limit: usize,
    /// Bytes of windows one merge holds over the runs it reads.
    merge_bytes: usize,
    /// Runs one level holds before they are merged into one run of the
    /// level above. A read merges at most twice as many.
    fan_in: usize,
    held: Vec<T>,
    /// Whether `held` is sorted and folded.
    settled: bool,
    /// The runs by level: a run of level `n + 1` is `fan_in` runs of level
    /// `n` merged.
    levels: Vec<Level>,
    /// Items added since the last [`Self::clear`], before any folding.
    added: u64,
}

impl<T: Item> Sorter<T> {
    /// A sorter that holds at most `bytes` of items in memory under
    /// [`BUDGET_BYTES`], and that share of the space's own budget. A merge
    /// of its runs holds as much again in windows, and no more.
    pub(crate) fn new(space: &Arc<Space>, bytes: usize, order: fn(&T, &T) -> Ordering) -> Self {
        Self {
            space: space.clone(),
            order,
            limit: Self::holds(space, bytes),
            merge_bytes: Self::bytes(space, bytes),
            fan_in: Self::fan_in(space, bytes),
            held: Vec::new(),
            settled: true,
            levels: Vec::new(),
            added: 0,
        }
    }

    /// The bytes a sorter of `bytes` holds under `space`'s budget.
    fn bytes(space: &Space, bytes: usize) -> usize {
        space.scaled(bytes).max(LEAST_SORT_BYTES)
    }

    /// How many items a sorter of `bytes` holds in memory under `space`'s
    /// budget.
    fn holds(space: &Space, bytes: usize) -> usize {
        (Self::bytes(space, bytes) / std::mem::size_of::<T>().max(1)).max(16)
    }

    /// How many runs it merges at a time: as many as leave each a whole
    /// [`WINDOW_BYTES`] of the merge's memory, and never fewer than two.
    fn fan_in(space: &Space, bytes: usize) -> usize {
        (Self::bytes(space, bytes) / WINDOW_BYTES).clamp(2, MOST_FAN_IN)
    }

    /// The most the files of a sorter of `bytes` hold once `items` items
    /// that do not fold were added: it writes a run each time its memory is
    /// full, and keeps the rest. Once it has runs enough to merge some, the
    /// merged run and the runs it is made of exist together for a while.
    pub(crate) fn file_bytes(space: &Space, bytes: usize, items: u64) -> u64 {
        let held = Self::holds(space, bytes) as u64;
        let runs = items / held;
        let written = runs.saturating_mul(held).saturating_mul(T::BYTES as u64);
        if runs < Self::fan_in(space, bytes) as u64 {
            written
        } else {
            written.saturating_mul(2)
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
        if self.held.capacity() == 0 {
            // The whole bound at once: a buffer that grows by doubling
            // would be allocated for up to twice what it may hold.
            self.held.reserve_exact(self.limit);
        }
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
        self.held.is_empty() && self.runs() == 0
    }

    /// How many runs the files hold.
    fn runs(&self) -> usize {
        self.levels.iter().map(|level| level.runs.len()).sum()
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

    /// The level `level`, with a file.
    fn level(&mut self, level: usize) -> HistoryResult<&mut Level> {
        while self.levels.len() <= level {
            self.levels.push(Level {
                file: None,
                runs: Vec::new(),
                written: 0,
            });
        }
        let found = &mut self.levels[level];
        if found.file.is_none() {
            found.file = Some(self.space.file()?);
        }
        Ok(found)
    }

    /// Writes the held items as one run of the lowest level, then merges
    /// every level that is full into the level above it.
    fn write_run(&mut self) -> HistoryResult<()> {
        let held = std::mem::take(&mut self.held);
        let written = self.append(0, |at| held.get(at as usize).copied());
        self.held = held;
        written?;
        self.held.clear();
        self.settled = true;
        let mut level = 0;
        while level < self.levels.len() {
            if self.levels[level].runs.len() >= self.fan_in {
                self.merge(level, level)?;
            }
            level += 1;
        }
        Ok(())
    }

    /// Appends one run to level `level`: the items `next` hands over by
    /// position, in order, until it hands over `None`.
    fn append(
        &mut self,
        level: usize,
        mut next: impl FnMut(u64) -> Option<T>,
    ) -> HistoryResult<()> {
        let space = self.space.clone();
        let target = self.level(level)?;
        let offset = target.written;
        let Some(mut file) = target.file.as_ref() else {
            return Err(HistoryError::Integrity("mark sort file"));
        };
        file.seek(SeekFrom::Start(offset)).map_err(io_error)?;
        let mut bytes = vec![0u8; WINDOW_BYTES.max(T::BYTES) / T::BYTES * T::BYTES];
        let (mut filled, mut items) = (0, 0u64);
        while let Some(item) = next(items) {
            if !item.fits() {
                return Err(HistoryError::Integrity("mark sort record"));
            }
            item.encode(&mut bytes[filled..filled + T::BYTES]);
            filled += T::BYTES;
            items += 1;
            if filled == bytes.len() {
                file.write_all(&bytes).map_err(io_error)?;
                space.grow(filled as u64);
                target.written += filled as u64;
                filled = 0;
            }
        }
        file.write_all(&bytes[..filled]).map_err(io_error)?;
        space.grow(filled as u64);
        target.written += filled as u64;
        if items > 0 {
            target.runs.push(Run { offset, items });
        }
        Ok(())
    }

    /// Merges every run of the levels `from..=to` into one run of the level
    /// above `to`, and empties their files.
    fn merge(&mut self, from: usize, to: usize) -> HistoryResult<()> {
        // The level written to exists, with its file, before the runs
        // below it are borrowed.
        self.level(to + 1)?;
        let (below, above) = self.levels.split_at_mut(to + 1);
        let target = &mut above[0];
        let Some(mut file) = target.file.as_ref() else {
            return Err(HistoryError::Integrity("mark sort file"));
        };
        let offset = target.written;
        file.seek(SeekFrom::Start(offset)).map_err(io_error)?;
        let mut merged = Merge::over(self.order, &below[from..], &[], self.merge_bytes)?;
        let mut bytes = vec![0u8; WINDOW_BYTES.max(T::BYTES) / T::BYTES * T::BYTES];
        let (mut filled, mut items) = (0, 0u64);
        while let Some(item) = merged.next()? {
            if !item.fits() {
                return Err(HistoryError::Integrity("mark sort record"));
            }
            item.encode(&mut bytes[filled..filled + T::BYTES]);
            filled += T::BYTES;
            items += 1;
            if filled == bytes.len() {
                file.write_all(&bytes).map_err(io_error)?;
                self.space.grow(filled as u64);
                target.written += filled as u64;
                filled = 0;
            }
        }
        drop(merged);
        file.write_all(&bytes[..filled]).map_err(io_error)?;
        self.space.grow(filled as u64);
        target.written += filled as u64;
        if items > 0 {
            target.runs.push(Run { offset, items });
        }
        for level in &mut below[from..] {
            level.runs.clear();
            self.space.shrink(std::mem::take(&mut level.written));
            if let Some(file) = &level.file {
                file.set_len(0).map_err(io_error)?;
            }
        }
        Ok(())
    }

    /// Forgets every item and gives the files' space back.
    pub(crate) fn clear(&mut self) -> HistoryResult<()> {
        self.held.clear();
        self.settled = true;
        self.added = 0;
        for level in &mut self.levels {
            level.runs.clear();
            self.space.shrink(std::mem::take(&mut level.written));
            if let Some(file) = &level.file {
                file.set_len(0).map_err(io_error)?;
            }
        }
        Ok(())
    }

    /// Every item in order, equal items folded. May be read again.
    ///
    /// A read merges at most twice [`Self::fan_in`] runs, so each has a
    /// window of at least half a [`WINDOW_BYTES`] of the merge's memory (a
    /// quarter of a sorter at its least size): the lowest levels are merged
    /// upwards first when there are more.
    pub(crate) fn sorted(&mut self) -> HistoryResult<Merge<'_, T>> {
        self.settle();
        while self.runs() > 2 * self.fan_in {
            let Some(from) = self.levels.iter().position(|level| !level.runs.is_empty()) else {
                break;
            };
            let (mut to, mut runs) = (from, self.levels[from].runs.len());
            while to + 1 < self.levels.len() && runs < self.fan_in {
                to += 1;
                runs += self.levels[to].runs.len();
            }
            self.merge(from, to)?;
        }
        Merge::over(self.order, &self.levels, &self.held, self.merge_bytes)
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
            stride: STRIDE,
            marks: Vec::new(),
            window: Vec::new(),
            first: 0,
            at: 0,
            passed: i64::MIN,
        };
        if self.runs() == 0 {
            sealed.memory = std::mem::take(&mut self.held);
            return Ok(sealed);
        }
        // An even number of keys, so that halving them keeps every second.
        let most = (self.space.scaled(MARKS) / 2).max(8) * 2;
        let mut file = self.space.file()?;
        let mut bytes = vec![0u8; WINDOW_BYTES.max(T::BYTES) / T::BYTES * T::BYTES];
        let mut filled = 0;
        let mut merged = self.sorted()?;
        while let Some(item) = merged.next()? {
            if sealed.items % sealed.stride == 0 {
                if sealed.marks.len() >= most {
                    // Every other key goes and the stride doubles: the
                    // keys held never pass their bound.
                    let mut at = 0;
                    sealed.marks.retain(|_| {
                        at += 1;
                        at % 2 == 1
                    });
                    sealed.stride *= 2;
                }
                if sealed.items % sealed.stride == 0 {
                    sealed.marks.push(key(&item));
                }
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
        for level in &self.levels {
            self.space.shrink(level.written);
        }
    }
}

/// Items of a [`Sealed`] run between two remembered keys, to begin with.
const STRIDE: u64 = 128;
/// Keys a [`Sealed`] run remembers, at most, under [`BUDGET_BYTES`]: past
/// this many strides the stride doubles.
const MARKS: usize = 65_536;

/// One sorted run, read a key at a time ([`Sorter::seal`]).
///
/// The run stays in its file. What is held is the key of every
/// [`STRIDE`]th item, eight bytes each, and one window of items; a run of
/// more than [`MARKS`] strides remembers every second, fourth, eighth
/// stride instead, so the keys never pass [`MARKS`]. A set that never left
/// its sorter's memory stays there instead.
pub(crate) struct Sealed<T: Item> {
    space: Arc<Space>,
    key: fn(&T) -> i64,
    /// The items, when no file was written.
    memory: Vec<T>,
    file: Option<File>,
    items: u64,
    /// Items between two remembered keys.
    stride: u64,
    /// `marks[n]` is the key of item `n * stride`.
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
        let start = block.saturating_sub(1) as u64 * self.stride;
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
        // After a jump the key may be anywhere in one stride; a wide
        // stride is read a window at a time all the same.
        let window = (WINDOW_BYTES / T::BYTES).max(1) as u64;
        let most = if continues {
            window
        } else {
            self.stride.min(window)
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
struct Reader<'a, T> {
    file: &'a File,
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
    runs: Vec<Reader<'a, T>>,
    /// Items of one run held at a time.
    window: usize,
    held: &'a [T],
    held_at: usize,
    heads: BinaryHeap<Head<T>>,
}

impl<'a, T: Item> Merge<'a, T> {
    /// A merge of every run of `levels` and of `held`, which is in order,
    /// through windows of `bytes` together.
    fn over(
        order: fn(&T, &T) -> Ordering,
        levels: &'a [Level],
        held: &'a [T],
        bytes: usize,
    ) -> HistoryResult<Self> {
        let mut runs = Vec::new();
        for level in levels {
            if level.runs.is_empty() {
                continue;
            }
            let Some(file) = level.file.as_ref() else {
                return Err(HistoryError::Integrity("mark sort file"));
            };
            runs.extend(level.runs.iter().map(|run| Reader {
                file,
                offset: run.offset,
                left: run.items,
                window: Vec::new(),
                at: 0,
            }));
        }
        let window = (bytes / runs.len().max(1)).clamp(T::BYTES, WINDOW_BYTES) / T::BYTES;
        let mut merge = Merge {
            order,
            runs,
            window,
            held,
            held_at: 0,
            heads: BinaryHeap::new(),
        };
        for source in 0..merge.runs.len() {
            merge.advance(source)?;
        }
        merge.advance(usize::MAX)?;
        Ok(merge)
    }

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
                let mut file = reader.file;
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
