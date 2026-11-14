//! Prototype (decoded-file cache, issue #684 experiment `passthrough`; not a
//! product contract and not for merge): whole unchanged committed regular
//! files, decoded, kept as plain files in a directory outside the one Store
//! file. Current product rules forbid committed content outside the Store; the
//! cache exists only behind `LAYERFS_PROBE_DECODED=<max bytes>` with its
//! directory in `LAYERFS_PROBE_DECODED_DIR`, both read once per process.
//!
//! A file is keyed by its content root, so one entry serves every Workspace
//! and Branch whose unchanged base names that root. It is written once, from
//! the windows of its first full read through a mount (bytes the Store read
//! path has already authenticated), in any order, and is named by its root
//! only when every byte is covered. A hit is one `pread` of the plain file:
//! no Store access, no hashing. Nothing is evicted; a file that does not fit
//! is not cached, and a file never read whole keeps its reserved bytes.
use std::{
    collections::{BTreeMap, HashMap},
    fs::{self, File, OpenOptions},
    os::unix::fs::FileExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Instant,
};

/// Process-wide counters, in [`NAMES`] order.
static COUNTERS: [AtomicU64; 10] = [const { AtomicU64::new(0) }; 10];
pub const NAMES: [&str; 10] = [
    "hit_windows",
    "hit_bytes",
    "hit_ns",
    "miss_windows",
    "fill_windows",
    "fill_bytes",
    "fill_ns",
    "fills_complete",
    "fills_abandoned",
    "passthrough_opens",
];
#[derive(Clone, Copy)]
pub enum Counter {
    HitWindows,
    HitBytes,
    HitNs,
    MissWindows,
    FillWindows,
    FillBytes,
    FillNs,
    FillsComplete,
    FillsAbandoned,
    PassthroughOpens,
}
pub fn count(counter: Counter, value: u64) {
    COUNTERS[counter as usize].fetch_add(value, Ordering::Relaxed);
}
pub fn snapshot() -> [u64; 10] {
    std::array::from_fn(|i| COUNTERS[i].load(Ordering::Relaxed))
}

pub struct Decoded {
    dir: PathBuf,
    limit: u64,
    state: Mutex<State>,
}
#[derive(Default)]
struct State {
    /// Bytes of complete files plus those reserved by fills in progress.
    used: u64,
    complete: HashMap<[u8; 32], (Arc<File>, u64)>,
    filling: HashMap<[u8; 32], Fill>,
}
struct Fill {
    file: File,
    /// Written ranges, start -> end, disjoint and not touching.
    covered: BTreeMap<u64, u64>,
    length: u64,
}
/// The process cache, when switched on.
pub fn get() -> Option<&'static Decoded> {
    static CACHE: OnceLock<Option<Decoded>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let limit = std::env::var("LAYERFS_PROBE_DECODED").ok()?.parse().ok()?;
            let dir = PathBuf::from(std::env::var_os("LAYERFS_PROBE_DECODED_DIR")?);
            fs::create_dir_all(&dir).ok()?;
            Some(Decoded {
                dir,
                limit,
                state: Mutex::default(),
            })
        })
        .as_ref()
}
/// Prototype: Linux FUSE passthrough of complete cached files, behind
/// `LAYERFS_PROBE_PASSTHROUGH=1` and only with the cache on.
pub fn passthrough() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        get().is_some() && std::env::var("LAYERFS_PROBE_PASSTHROUGH").as_deref() == Ok("1")
    })
}
fn name(root: &[u8; 32]) -> String {
    root.iter().map(|byte| format!("{byte:02x}")).collect()
}
impl Decoded {
    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// The complete plain file of `root` and its length.
    pub fn complete(&self, root: [u8; 32]) -> Option<(Arc<File>, u64)> {
        self.lock().complete.get(&root).cloned()
    }
    /// A window of a complete file; None on a miss or any I/O failure, when
    /// the caller reads the Store instead.
    pub fn read(&self, root: [u8; 32], offset: u64, length: u32) -> Option<Vec<u8>> {
        let started = Instant::now();
        let Some((file, size)) = self.complete(root) else {
            count(Counter::MissWindows, 1);
            return None;
        };
        let want = size.saturating_sub(offset).min(u64::from(length)) as usize;
        let mut data = vec![0; want];
        let mut done = 0;
        while done < want {
            match file.read_at(&mut data[done..], offset + done as u64) {
                Ok(0) | Err(_) => return None,
                Ok(n) => done += n,
            }
        }
        count(Counter::HitWindows, 1);
        count(Counter::HitBytes, want as u64);
        count(Counter::HitNs, started.elapsed().as_nanos() as u64);
        Some(data)
    }
    /// Offers one window of an unchanged file of `length` bytes, just read
    /// and authenticated from the Store. Windows may arrive in any order
    /// (several READs in flight); each is written at its offset and the file
    /// is complete when its windows cover it. A window again is harmless.
    pub fn fill(&self, root: [u8; 32], length: u64, offset: u64, data: &[u8]) {
        if length == 0 || data.is_empty() {
            return;
        }
        let started = Instant::now();
        let mut guard = self.lock();
        let state = &mut *guard;
        if state.complete.contains_key(&root) {
            return;
        }
        let part = self.dir.join(format!("{}.part", name(&root)));
        if !state.filling.contains_key(&root) {
            if state.used.saturating_add(length) > self.limit {
                return;
            }
            let Ok(file) = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&part)
            else {
                return;
            };
            state.used += length;
            let covered = BTreeMap::new();
            state.filling.insert(
                root,
                Fill {
                    file,
                    covered,
                    length,
                },
            );
        }
        let fill = state.filling.get_mut(&root).expect("inserted above");
        let end = offset + data.len() as u64;
        let written =
            fill.length == length && end <= length && fill.file.write_all_at(data, offset).is_ok();
        if !written {
            let fill = state.filling.remove(&root).expect("present");
            state.used -= fill.length;
            drop(fill);
            let _ = fs::remove_file(&part);
            count(Counter::FillsAbandoned, 1);
            return;
        }
        count(Counter::FillWindows, 1);
        count(Counter::FillBytes, data.len() as u64);
        // Merge [offset, end) into the covered ranges.
        let (mut from, mut to) = (offset, end);
        let touching: Vec<(u64, u64)> = fill
            .covered
            .range(..=to)
            .filter(|(_, until)| **until >= from)
            .map(|(at, until)| (*at, *until))
            .collect();
        for (at, until) in touching {
            fill.covered.remove(&at);
            from = from.min(at);
            to = to.max(until);
        }
        fill.covered.insert(from, to);
        if fill.covered.get(&0) == Some(&length) {
            let fill = state.filling.remove(&root).expect("present");
            drop(fill);
            let whole = self.dir.join(name(&root));
            match fs::rename(&part, &whole).and_then(|()| File::open(&whole)) {
                Ok(file) => {
                    state.complete.insert(root, (Arc::new(file), length));
                    count(Counter::FillsComplete, 1);
                }
                Err(_) => {
                    state.used -= length;
                    let _ = fs::remove_file(&part);
                    let _ = fs::remove_file(&whole);
                    count(Counter::FillsAbandoned, 1);
                }
            }
        }
        count(Counter::FillNs, started.elapsed().as_nanos() as u64);
    }
}
