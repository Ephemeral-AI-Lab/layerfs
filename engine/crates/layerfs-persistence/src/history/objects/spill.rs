//! Sorting in bounded memory for one mark.
//!
//! A mark has to put several sets in order that grow with the Store: the
//! identities a wave reached, the objects of the next wave by locator, the
//! records it kept by pack. None of them may be held whole. A [`Sorter`]
//! keeps at most a fixed number of bytes of items; when they are full it
//! sorts them, folds equal neighbours and, if that freed too little, writes
//! them as one sorted run to a file of its own. Reading merges the runs with
//! what is still held, through one small window per run.
//!
//! The file is created in the directory the engine names and unlinked at
//! once, so it has no name to leave behind and its space returns with the
//! handle. It is not Store state and nothing in it survives the mark.
use layerfs_history::error::{HistoryError, HistoryResult};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as Atomic};

/// Bytes of one run a merge holds at a time, at most.
const WINDOW_BYTES: usize = 64 * 1024;

/// A fixed-width value a [`Sorter`] can write to its file.
pub(super) trait Item: Copy {
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
}

/// One sorted run in the file.
#[derive(Clone, Copy)]
struct Run {
    offset: u64,
    items: u64,
}

fn io_error(error: std::io::Error) -> HistoryError {
    match error.raw_os_error() {
        Some(28) | Some(122) | Some(27) | Some(69) => HistoryError::Capacity("disk space"),
        _ => HistoryError::Integrity("mark sort file I/O"),
    }
}

/// A file with no name: created, then unlinked while the handle stays open.
fn unnamed(directory: &Path) -> std::io::Result<File> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        ".layerfs-mark-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Atomic::Relaxed)
    );
    let path = directory.join(name);
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&path)?;
    std::fs::remove_file(&path)?;
    Ok(file)
}

/// Items in one order, held in bounded memory. See the module comment.
pub(super) struct Sorter<T: Item> {
    directory: PathBuf,
    order: fn(&T, &T) -> Ordering,
    /// Items held before they are settled.
    limit: usize,
    held: Vec<T>,
    /// Whether `held` is sorted and folded.
    settled: bool,
    runs: Vec<Run>,
    file: Option<File>,
    written: u64,
    /// The most bytes the file has held.
    peak: u64,
}

impl<T: Item> Sorter<T> {
    /// A sorter that holds at most about `bytes` of items in memory.
    pub(super) fn new(directory: &Path, bytes: usize, order: fn(&T, &T) -> Ordering) -> Self {
        Self {
            directory: directory.to_path_buf(),
            order,
            limit: (bytes / std::mem::size_of::<T>().max(1)).max(16),
            held: Vec::new(),
            settled: true,
            runs: Vec::new(),
            file: None,
            written: 0,
            peak: 0,
        }
    }

    /// Adds one item.
    pub(super) fn push(&mut self, item: T) -> HistoryResult<()> {
        self.hold(item);
        self.relieve()
    }

    /// Adds one item without ever writing: for a caller that cannot fail
    /// here and calls [`Self::relieve`] soon after.
    pub(super) fn hold(&mut self, item: T) {
        self.held.push(item);
        self.settled = false;
    }

    /// Brings the held items back under the bound.
    pub(super) fn relieve(&mut self) -> HistoryResult<()> {
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
    pub(super) fn is_empty(&self) -> bool {
        self.held.is_empty() && self.runs.is_empty()
    }

    /// The most bytes this sorter's file has held.
    pub(super) fn peak_file_bytes(&self) -> u64 {
        self.peak
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
            self.file = Some(unnamed(&self.directory).map_err(io_error)?);
        }
        let Some(file) = self.file.as_mut() else {
            return Err(HistoryError::Integrity("mark sort file"));
        };
        file.seek(SeekFrom::Start(self.written)).map_err(io_error)?;
        let mut bytes = vec![0u8; WINDOW_BYTES.max(T::BYTES)];
        let per_write = bytes.len() / T::BYTES;
        for items in self.held.chunks(per_write) {
            for (item, out) in items.iter().zip(bytes.chunks_exact_mut(T::BYTES)) {
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
        self.peak = self.peak.max(self.written);
        self.held.clear();
        self.settled = true;
        Ok(())
    }

    /// Forgets every item and gives the file's space back.
    pub(super) fn clear(&mut self) -> HistoryResult<()> {
        self.held.clear();
        self.settled = true;
        self.runs.clear();
        self.written = 0;
        if let Some(file) = &self.file {
            file.set_len(0).map_err(io_error)?;
        }
        Ok(())
    }

    /// Every item in order, equal items folded. May be read again.
    pub(super) fn sorted(&mut self) -> HistoryResult<Merge<'_, T>> {
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
pub(super) struct Merge<'a, T: Item> {
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
    pub(super) fn next(&mut self) -> HistoryResult<Option<T>> {
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
    pub(super) fn peek(&self) -> Option<&T> {
        self.heads.peek().map(|head| &head.item)
    }
}
