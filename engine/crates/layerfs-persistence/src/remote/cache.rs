//! The block cache of one remote file (#510): fixed 64 KiB blocks, a bound
//! on the bytes kept, least recently used first out.
//!
//! A read takes the blocks it covers from the cache. Each run of missing
//! blocks is fetched from the source in ranged requests of at most
//! [`MAX_REQUEST`] bytes; a run's last request reads ahead into the missing
//! blocks after it, up to that same size, because SQLite reads one page
//! at a time and a table pushed with `VACUUM INTO` is one contiguous run.
//! The lock is held only to look blocks up and to insert them, never
//! across a request, so concurrent reads of one file proceed; two reads
//! missing the same block may both fetch it.
use super::{RemoteFailure, RemoteSource};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Bytes of one cached block.
pub const BLOCK: u64 = 64 * 1024;
/// Most bytes one ranged request asks for.
pub const MAX_REQUEST: u64 = 1024 * 1024;
/// The default bound of one file's cache.
pub const DEFAULT_CACHE: u64 = 64 * 1024 * 1024;

/// A run of blocks one request fetches: `count` blocks from `first`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Run {
    /// The first block's index.
    pub first: u64,
    /// How many blocks.
    pub count: u64,
}

/// The blocks `[first, last]` of a read, minus those `present`, as requests
/// of at most `MAX_REQUEST / BLOCK` blocks; the last request extends into
/// the following absent blocks up to that size, without passing `blocks`
/// (the file's block count) or a present block.
pub fn plan(first: u64, last: u64, blocks: u64, present: &dyn Fn(u64) -> bool) -> Vec<Run> {
    let most = MAX_REQUEST / BLOCK;
    let mut runs: Vec<Run> = Vec::new();
    for block in first..=last {
        if present(block) {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.first + run.count == block && run.count < most => run.count += 1,
            _ => runs.push(Run {
                first: block,
                count: 1,
            }),
        }
    }
    if let Some(run) = runs.last_mut() {
        while run.count < most && run.first + run.count < blocks && !present(run.first + run.count)
        {
            run.count += 1;
        }
    }
    runs
}

/// Least recently used blocks, bounded by a block count.
struct Lru {
    blocks: HashMap<u64, (Arc<[u8]>, u64)>,
    /// use tick -> block
    order: BTreeMap<u64, u64>,
    tick: u64,
}

impl Lru {
    fn touch(&mut self, block: u64) -> Option<Arc<[u8]>> {
        let (data, used) = self.blocks.get_mut(&block)?;
        self.order.remove(used);
        self.tick += 1;
        *used = self.tick;
        self.order.insert(self.tick, block);
        Some(data.clone())
    }

    fn insert(&mut self, block: u64, data: Arc<[u8]>, capacity: usize) {
        if self.touch(block).is_some() {
            return;
        }
        self.tick += 1;
        self.blocks.insert(block, (data, self.tick));
        self.order.insert(self.tick, block);
        while self.blocks.len() > capacity {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            self.blocks.remove(&oldest);
        }
    }
}

/// What one file's cache did.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheStats {
    /// Blocks a read found in the cache.
    pub hits: u64,
    /// Blocks a read did not find.
    pub misses: u64,
    /// Ranged requests the cache asked the source for.
    pub fetches: u64,
    /// Bytes those requests returned.
    pub fetched: u64,
    /// Bytes held now.
    pub resident: u64,
    /// The bound on bytes held.
    pub capacity: u64,
}

/// One remote file's blocks.
pub struct BlockCache {
    capacity: usize,
    lru: Mutex<Lru>,
    hits: AtomicU64,
    misses: AtomicU64,
    fetches: AtomicU64,
    fetched: AtomicU64,
}

impl BlockCache {
    /// A cache of at most `bytes` (rounded down to whole blocks, at least
    /// one request's worth).
    pub fn new(bytes: u64) -> Self {
        let blocks = (bytes / BLOCK).max(MAX_REQUEST / BLOCK);
        Self {
            capacity: usize::try_from(blocks).unwrap_or(usize::MAX),
            lru: Mutex::new(Lru {
                blocks: HashMap::new(),
                order: BTreeMap::new(),
                tick: 0,
            }),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            fetches: AtomicU64::new(0),
            fetched: AtomicU64::new(0),
        }
    }

    fn lru(&self) -> std::sync::MutexGuard<'_, Lru> {
        self.lru.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// Fills `out` with the file's bytes from `offset`, which must lie
    /// within the file.
    pub fn read(
        &self,
        source: &dyn RemoteSource,
        offset: u64,
        out: &mut [u8],
    ) -> Result<(), RemoteFailure> {
        if out.is_empty() {
            return Ok(());
        }
        let size = source.bytes();
        let end = offset + out.len() as u64;
        let (first, last) = (offset / BLOCK, (end - 1) / BLOCK);
        let blocks = size.div_ceil(BLOCK);
        let mut found: HashMap<u64, Arc<[u8]>> = HashMap::new();
        {
            let mut lru = self.lru();
            for block in first..=last {
                if let Some(data) = lru.touch(block) {
                    found.insert(block, data);
                }
            }
        }
        let wanted = last - first + 1;
        let hits = found.len() as u64;
        self.hits.fetch_add(hits, Ordering::Relaxed);
        self.misses.fetch_add(wanted - hits, Ordering::Relaxed);
        if hits < wanted {
            let runs = {
                let lru = self.lru();
                plan(first, last, blocks, &|block| {
                    found.contains_key(&block) || lru.blocks.contains_key(&block)
                })
            };
            for run in runs {
                let start = run.first * BLOCK;
                let stop = ((run.first + run.count) * BLOCK).min(size);
                let mut bytes = vec![0; (stop - start) as usize];
                self.fetches.fetch_add(1, Ordering::Relaxed);
                source.fetch(start, &mut bytes)?;
                self.fetched
                    .fetch_add(bytes.len() as u64, Ordering::Relaxed);
                let mut lru = self.lru();
                for (index, chunk) in bytes.chunks(BLOCK as usize).enumerate() {
                    let block = run.first + index as u64;
                    let data: Arc<[u8]> = Arc::from(chunk);
                    if (first..=last).contains(&block) {
                        found.insert(block, data.clone());
                    }
                    lru.insert(block, data, self.capacity);
                }
            }
        }
        for block in first..=last {
            let data = found
                .get(&block)
                .ok_or_else(|| RemoteFailure::failed("a block was not fetched"))?;
            let block_start = block * BLOCK;
            let from = offset.max(block_start);
            let to = end.min(block_start + data.len() as u64);
            if to <= from {
                return Err(RemoteFailure::failed("a block is shorter than the file"));
            }
            out[(from - offset) as usize..(to - offset) as usize]
                .copy_from_slice(&data[(from - block_start) as usize..(to - block_start) as usize]);
        }
        Ok(())
    }

    /// What this cache did so far, and what it holds.
    pub fn stats(&self) -> CacheStats {
        let resident = self
            .lru()
            .blocks
            .values()
            .map(|(data, _)| data.len() as u64)
            .sum();
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            fetches: self.fetches.load(Ordering::Relaxed),
            fetched: self.fetched.load(Ordering::Relaxed),
            resident,
            capacity: self.capacity as u64 * BLOCK,
        }
    }
}
