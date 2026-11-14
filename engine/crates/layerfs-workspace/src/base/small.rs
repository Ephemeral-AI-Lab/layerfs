//! Prototype (experiment `smallread`, FUSE profile probe): cold reads of many
//! small committed files. Switches come from `LAYERFS_PROBE_SMALL`, a comma
//! list; unset, none applies:
//!
//! - `lengths`: a READDIRPLUS window asks the lengths of all its inherited
//!   regular files in one demand before it composes their attributes,
//!   instead of one demand per entry.
//! - `sibling`: a READDIRPLUS window remembers, in listing order, the
//!   content roots of its inherited regular files of at most [`SMALL_BYTES`].
//!   When a base READ then misses the cache on one of them, the remembered
//!   siblings after it (at most [`SIBLING_IDS`] and [`SIBLING_FETCH_BYTES`])
//!   are fetched in one batch on a thread of their own over a fresh custody,
//!   marked in flight first, so the reads that follow find them in the cache
//!   or wait for that one fetch. A listing that reads no file fetches
//!   nothing. Objects are authenticated and cached exactly as any demand's.
//! - `sync`: as `sibling`, but the batch is fetched on the reading thread
//!   together with the missed object, in one demand.
//! - `eager` (implies `sibling`): a directory in which such a miss happened
//!   is being read; the later windows of that directory fetch their small
//!   files when they are listed, without waiting for a miss.
//! - `parts`: a sibling fetch is split into `LAYERFS_PROBE_READ_SESSIONS`
//!   contiguous parts, each fetched, checked and cached on its own thread,
//!   so the first files land before the whole batch has.
//!
//! - `push` (FUSE adapter): a read-only OPEN of an unchanged base regular
//!   file of at most [`SMALL_BYTES`] reads the whole file and stores it in
//!   the kernel page cache (`FUSE_NOTIFY_STORE`) before it replies, so the
//!   reader's `read` needs no READ request. A visit after the store checks
//!   the file is still the same base content; otherwise the inode's pages
//!   are invalidated.
//!
//! Every fetch is bounded: at most [`INFLIGHT_BYTES`] in flight in all.
use super::client::{probe, CanonicalClient, Flight};
use layerfs_content::{ContentError, ObjectId};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

/// Largest file whose content root a window remembers.
pub(crate) const SMALL_BYTES: u64 = 64 * 1024;
/// Most siblings one miss fetches.
const SIBLING_IDS: usize = 64;
/// Most bytes one miss fetches.
const SIBLING_FETCH_BYTES: u64 = 2 * 1024 * 1024;
/// Most bytes all sibling fetches may have in flight at once.
const INFLIGHT_BYTES: usize = 8 * 1024 * 1024;
/// Most windows remembered per cache allowance; the oldest goes first.
const WINDOWS: usize = 256;
/// Most directories remembered as being read (`eager`).
const ACTIVE: usize = 64;

#[derive(Clone, Copy, Default)]
pub(crate) struct Small {
    pub(crate) lengths: bool,
    pub(crate) sibling: bool,
    pub(crate) sync: bool,
    pub(crate) eager: bool,
    pub(crate) parts: bool,
    pub(crate) push: bool,
}
/// Whether `push` is set.
pub fn push() -> bool {
    small().push
}
pub(crate) fn small() -> Small {
    static SMALL: OnceLock<Small> = OnceLock::new();
    *SMALL.get_or_init(|| {
        let value = std::env::var("LAYERFS_PROBE_SMALL").unwrap_or_default();
        let has = |name: &str| value.split(',').any(|part| part.trim() == name);
        let (sync, eager) = (has("sync"), has("eager"));
        Small {
            lengths: has("lengths"),
            sibling: has("sibling") || sync || eager,
            sync,
            eager,
            parts: has("parts"),
            push: has("push"),
        }
    })
}

/// Counters since process start: windows remembered, misses that found a
/// window, sibling fetches, ids and bytes they fetched, fetches skipped
/// for the in-flight bound, failed fetches, length batches and their ids,
/// windows fetched when listed (`eager`).
pub const COUNTERS: usize = 14;
pub const COUNTER_NAMES: [&str; COUNTERS] = [
    "windows",
    "triggers",
    "fetches",
    "fetched_ids",
    "fetched_bytes",
    "skipped",
    "failed",
    "length_batches",
    "length_ids",
    "eager",
    "pushed",
    "pushed_bytes",
    "push_skipped",
    "push_invalidated",
];
const ZERO: AtomicU64 = AtomicU64::new(0);
static COUNTS: [AtomicU64; COUNTERS] = [ZERO; COUNTERS];
static INFLIGHT: AtomicUsize = AtomicUsize::new(0);
fn count(index: usize, by: u64) {
    COUNTS[index].fetch_add(by, Ordering::Relaxed);
}
/// `push` counters: stored files and bytes, skipped opens, invalidations.
pub fn counted_push(stored: u64, bytes: u64, skipped: u64, invalidated: u64) {
    count(10, stored);
    count(11, bytes);
    count(12, skipped);
    count(13, invalidated);
}
pub(crate) fn counted_lengths(ids: usize) {
    count(7, 1);
    count(8, ids as u64);
}
/// The counters so far.
pub fn snapshot() -> [u64; COUNTERS] {
    std::array::from_fn(|index| COUNTS[index].load(Ordering::Relaxed))
}

type Window = Vec<(ObjectId, u64)>;
/// Recently listed small files, per window in listing order, with lengths
/// and the directory listed; and the directories being read (`eager`).
#[derive(Default)]
pub(crate) struct Windows {
    windows: VecDeque<(u64, Window)>,
    active: VecDeque<u64>,
}
impl Windows {
    fn remember(&mut self, directory: u64, window: Window) {
        if self.windows.len() == WINDOWS {
            self.windows.pop_front();
        }
        self.windows.push_back((directory, window));
    }
    /// The siblings after `root` in the newest window holding it; that
    /// window is forgotten, so one listing triggers at most one fetch per
    /// window, and its directory is marked as being read.
    fn after(&mut self, root: ObjectId) -> Option<Window> {
        let at = self
            .windows
            .iter()
            .rposition(|(_, window)| window.iter().any(|(id, _)| *id == root))?;
        let (directory, window) = self.windows.remove(at)?;
        if !self.active.contains(&directory) {
            if self.active.len() == ACTIVE {
                self.active.pop_front();
            }
            self.active.push_back(directory);
        }
        let index = window.iter().position(|(id, _)| *id == root)?;
        Some(window[index + 1..].to_vec())
    }
}

impl CanonicalClient {
    /// The remembered lengths of `roots`, in order; one not remembered is
    /// left out.
    pub(crate) fn remembered_lengths(&self, roots: &[ObjectId]) -> Window {
        let Ok(mut state) = self.state() else {
            return Vec::new();
        };
        roots
            .iter()
            .filter_map(|root| Some((*root, state.cache.length(*root)?)))
            .collect()
    }
    /// Remembers one READDIRPLUS window's small regular files of
    /// `directory`; with `eager`, a directory being read fetches them now.
    pub(crate) fn remember_window(&self, directory: u64, window: Window) {
        let window: Window = window
            .into_iter()
            .filter(|(_, length)| *length <= SMALL_BYTES)
            .collect();
        if window.len() < 2 || self.is_resident() {
            return;
        }
        let wanted = {
            let Ok(mut state) = self.state() else {
                return;
            };
            if !(small().eager && state.small.active.contains(&directory)) {
                state.small.remember(directory, window);
                count(0, 1);
                return;
            }
            count(9, 1);
            match Self::claim(&mut state, window) {
                Some(wanted) => wanted,
                None => return,
            }
        };
        self.spawn_fetch(wanted);
    }
    /// Up to the fetch bounds of `after` that is neither cached nor in
    /// flight, marked in flight and charged to the in-flight bytes.
    fn claim(state: &mut crate::cache::State, after: Window) -> Option<Window> {
        let mut bytes = 0;
        let mut wanted = Vec::new();
        for (id, length) in after {
            if wanted.len() == SIBLING_IDS || bytes + length > SIBLING_FETCH_BYTES {
                break;
            }
            if state.cache.within(id, usize::MAX) || state.inflight.contains(&id) {
                continue;
            }
            bytes += length;
            wanted.push((id, length));
        }
        if wanted.is_empty() {
            return None;
        }
        let bytes = bytes as usize;
        if INFLIGHT.fetch_add(bytes, Ordering::AcqRel) + bytes > INFLIGHT_BYTES {
            INFLIGHT.fetch_sub(bytes, Ordering::AcqRel);
            count(5, 1);
            return None;
        }
        state.inflight.extend(wanted.iter().map(|(id, _)| *id));
        Some(wanted)
    }
    /// Before a base READ demands `root`: when it is not cached and a
    /// remembered window holds it, fetch the siblings after it. With `sync`
    /// they join `root` in one demand here; otherwise they are fetched on
    /// their own thread and `root` is read on demand as before.
    pub(crate) fn sibling_prefetch(&self, root: ObjectId) {
        let switches = small();
        if !switches.sibling || self.is_resident() {
            return;
        }
        let wanted = {
            let Ok(mut state) = self.state() else {
                return;
            };
            if state.cache.within(root, usize::MAX) || state.inflight.contains(&root) {
                return;
            }
            let Some(after) = state.small.after(root) else {
                return;
            };
            count(1, 1);
            let Some(wanted) = Self::claim(&mut state, after) else {
                return;
            };
            if switches.sync {
                state.inflight.insert(root);
            }
            wanted
        };
        if switches.sync {
            let bytes: u64 = wanted.iter().map(|(_, length)| *length).sum();
            let mut ids: Vec<ObjectId> = wanted.into_iter().map(|(id, _)| id).collect();
            ids.insert(0, root);
            let flight = Flight::new(&self.cache, ids.clone());
            let fetched = self.fetch_parts(&ids, 1);
            self.land(&ids, fetched, bytes);
            drop(flight);
            return;
        }
        self.spawn_fetch(wanted);
    }
    /// Fetches claimed `wanted` off this thread: in one batch (split into
    /// read-session parts that land together), or with `parts` in parts
    /// that each land on their own.
    fn spawn_fetch(&self, wanted: Window) {
        let parts = if small().parts {
            probe().parts.min(wanted.len()).max(1)
        } else {
            1
        };
        let size = wanted.len().div_ceil(parts);
        for part in wanted.chunks(size) {
            let bytes: u64 = part.iter().map(|(_, length)| *length).sum();
            let ids: Vec<ObjectId> = part.iter().map(|(id, _)| *id).collect();
            let Some(client) = self.forked() else {
                self.release(&ids, bytes);
                continue;
            };
            let split = if parts == 1 { probe().parts } else { 1 };
            let spawned = std::thread::Builder::new()
                .name("layerfs-sibling".into())
                .spawn({
                    let ids = ids.clone();
                    move || {
                        let flight = Flight::new(&client.cache, ids.clone());
                        let fetched = client.fetch_parts(&ids, split);
                        client.land(&ids, fetched, bytes);
                        drop(flight);
                    }
                });
            if spawned.is_err() {
                self.release(&ids, bytes);
            }
        }
    }
    /// Unmarks `ids` in flight and returns their byte budget.
    fn release(&self, ids: &[ObjectId], bytes: u64) {
        drop(Flight::new(&self.cache, ids.to_vec()));
        INFLIGHT.fetch_sub(bytes as usize, Ordering::AcqRel);
    }
    /// Checks and caches one fetch; a failed or mismatched batch caches
    /// nothing, and those objects are then read on demand.
    fn land(&self, ids: &[ObjectId], fetched: Result<Vec<Vec<u8>>, ContentError>, bytes: u64) {
        INFLIGHT.fetch_sub(bytes as usize, Ordering::AcqRel);
        let checked = fetched.and_then(|values| {
            if values.len() != ids.len() {
                return Err(ContentError::BatchCardinality {
                    requested: ids.len(),
                    returned: values.len(),
                });
            }
            if !probe().noverify {
                for (id, value) in ids.iter().zip(&values) {
                    if ObjectId::for_bytes(value) != *id {
                        return Err(ContentError::IdentityMismatch);
                    }
                }
            }
            Ok(values)
        });
        let Ok(values) = checked else {
            count(6, 1);
            return;
        };
        count(2, 1);
        count(3, ids.len() as u64);
        let mut total = 0;
        let Ok(mut state) = self.state() else {
            return;
        };
        for (id, value) in ids.iter().zip(values) {
            let length = value.len() as u64;
            total += length;
            let evictions = state.cache.insert_shared(*id, Arc::new(value));
            state.work.evictions = state.work.evictions.saturating_add(evictions);
            state.work.cache_misses = state.work.cache_misses.saturating_add(1);
            state.work.authenticated_bytes = state.work.authenticated_bytes.saturating_add(length);
        }
        state.work.upstream_batches = state.work.upstream_batches.saturating_add(1);
        count(4, total);
    }
}
