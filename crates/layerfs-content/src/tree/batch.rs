//! Sorted final-state updates. Only the last sibling is kept private until its
//! neighbour establishes the final partition; untouched subtrees remain IDs.
#[cfg(test)]
use super::directory::codec::encode_directory_node;
use super::directory::codec::{decode_directory_node, DirectoryNodeV1};
#[cfg(test)]
use super::inode::codec::encode_inode_table_node;
use super::inode::codec::{decode_inode_table_node, InodeTableNodeV1};
use super::inode::InodeId;
use crate::file::rope::ObjectStore;
use crate::filesystem::delta_spool::Run;
use crate::object::ContentDigestWriter;
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{cell::Cell, marker::PhantomData, rc::Rc};

/// The caller reserves this together with its delta stream and other live state.
/// Every internal page/decode allocation is charged before allocation.
pub const SORTED_TREE_UPDATE_SCRATCH_BYTES: usize = 4 * 1024 * 1024;
const PAGE_ITEMS: usize = 234; // 8-KiB directory page, minimum 35-byte entry, plus overflow.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TreeBatchCounters {
    pub nodes_read: u64,
    pub nodes_created: u64,
    /// Published subtree roots forwarded into final parents, plus checked
    /// same-ID page results. Intermediate forwards are not counted again.
    pub nodes_reused: u64,
    pub delta_keys: u64,
    pub peak_scratch_bytes: usize,
    pub spill_write_bytes: u64,
    pub spill_read_bytes: u64,
    pub peak_spill_bytes: u64,
}

struct Budget {
    used: Cell<usize>,
    peak: Cell<usize>,
    limit: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            used: Cell::new(0),
            peak: Cell::new(0),
            limit: SORTED_TREE_UPDATE_SCRATCH_BYTES,
        }
    }
}
struct Lease {
    budget: Rc<Budget>,
    bytes: usize,
}
impl Budget {
    fn reserve(self: &Rc<Self>, bytes: usize) -> CoreResult<Lease> {
        let next = self
            .used
            .get()
            .checked_add(bytes)
            .ok_or(CoreError::LengthOverflow)?;
        if next > self.limit {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.used.set(next);
        self.peak.set(self.peak.get().max(next));
        Ok(Lease {
            budget: self.clone(),
            bytes,
        })
    }
}
impl Lease {
    fn grow(&mut self, bytes: usize) -> CoreResult<()> {
        let next = self
            .budget
            .used
            .get()
            .checked_add(bytes)
            .ok_or(CoreError::LengthOverflow)?;
        if next > self.budget.limit {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.budget.used.set(next);
        self.budget.peak.set(self.budget.peak.get().max(next));
        self.bytes += bytes;
        Ok(())
    }
    fn transfer(&mut self, to: &mut Lease, bytes: usize) {
        self.bytes -= bytes;
        to.bytes += bytes;
    }
    fn shrink(&mut self, bytes: usize) {
        self.bytes -= bytes;
        self.budget.used.set(self.budget.used.get() - bytes);
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.budget.used.set(self.budget.used.get() - self.bytes);
    }
}

struct Wire<K> {
    level: u8,
    count: u64,
    bytes: u64,
    entries: Vec<(K, ObjectId)>,
    size: usize,
}
struct ReadPage<K> {
    wire: Wire<K>,
    _lease: Lease,
}
trait Format {
    type Key: Ord + Clone;
    fn decode(bytes: &[u8]) -> CoreResult<Wire<Self::Key>>;
    fn encode(page: &Page<Self::Key>) -> CoreResult<Vec<u8>>;
    fn key_bytes(key: &Self::Key) -> &[u8];
    fn key_from_bytes(bytes: &[u8]) -> CoreResult<Self::Key>;
    fn encode_spilled(
        count: usize,
        bytes: u64,
        entries: impl Iterator<Item = CoreResult<(Self::Key, ObjectId)>>,
    ) -> CoreResult<Vec<u8>>;
    fn width(key: &Self::Key) -> usize;
    fn heap_bytes(key: &Self::Key) -> usize;
    fn decode_scratch(bytes: usize) -> usize;
    fn maximum_pairs() -> usize;
    fn filled(size: usize, count: usize) -> bool;
    fn fits(size: usize, count: usize) -> bool;
    fn empty_allowed() -> bool;
}

const SPILL_RECORD: usize = 289;
struct SpillPool {
    dir: PathBuf,
    limit: u64,
    used: Cell<u64>,
    peak: Cell<u64>,
    written: Cell<u64>,
    read: Cell<u64>,
    _lease: Lease,
}
struct DiskLease {
    pool: Rc<SpillPool>,
    bytes: u64,
}
impl DiskLease {
    fn reserve(&mut self, bytes: u64) -> CoreResult<()> {
        let used = self
            .pool
            .used
            .get()
            .checked_add(bytes)
            .ok_or(CoreError::LengthOverflow)?;
        if used > self.pool.limit {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.pool.used.set(used);
        self.pool.peak.set(self.pool.peak.get().max(used));
        self.bytes += bytes;
        Ok(())
    }
}
impl Drop for DiskLease {
    fn drop(&mut self) {
        self.pool.used.set(self.pool.used.get() - self.bytes);
    }
}
struct Spilled<K> {
    run: Run<SPILL_RECORD>,
    disk: DiskLease,
    max: K,
    items: usize,
    bytes: u64,
    size: usize,
    digest: [u8; 32],
}
struct SpillReader<K> {
    spilled: Spilled<K>,
    read: usize,
    _lease: Lease,
}
impl<K> SpillReader<K> {
    fn next<F: Format<Key = K>>(&mut self) -> CoreResult<Option<(K, ObjectId)>> {
        let record = self.spilled.run.next()?;
        if let Some(record) = record {
            if self.read >= self.spilled.items {
                return Err(CoreError::InvalidRecord("spilled leaf length"));
            }
            self.read += 1;
            self.spilled
                .disk
                .pool
                .read
                .set(self.spilled.disk.pool.read.get() + SPILL_RECORD as u64);
            let len = u16::from_be_bytes(record[..2].try_into().unwrap()) as usize;
            if len > 255 {
                return Err(CoreError::InvalidRecord("spilled leaf key"));
            }
            return Ok(Some((
                F::key_from_bytes(&record[2..2 + len])?,
                ObjectId::from_bytes(&record[257..])?,
            )));
        }
        if self.read != self.spilled.items {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(None)
    }
}
enum LeafInput<K> {
    Resident {
        entries: std::vec::IntoIter<(K, ObjectId)>,
        _lease: Lease,
    },
    Spilled(SpillReader<K>),
    Empty,
}
impl<K> LeafInput<K> {
    fn next<F: Format<Key = K>>(&mut self) -> CoreResult<Option<(K, ObjectId)>> {
        let next = match self {
            Self::Resident { entries, .. } => entries.next(),
            Self::Spilled(reader) => reader.next::<F>()?,
            Self::Empty => None,
        };
        if next.is_none() {
            *self = Self::Empty;
        }
        Ok(next)
    }
}
// Small fixed segments grow without holding old and replacement page arrays
// simultaneously. Splits move whole segments; only a boundary segment is copied.
struct Pairs<K> {
    chunks: Vec<Vec<(K, ObjectId)>>,
    len: usize,
}
impl<K> Pairs<K> {
    fn new() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
    fn len(&self) -> usize {
        self.len
    }
    fn is_empty(&self) -> bool {
        self.len == 0
    }
    fn last(&self) -> Option<&(K, ObjectId)> {
        self.chunks.last().and_then(|chunk| chunk.last())
    }
    fn last_mut(&mut self) -> Option<&mut (K, ObjectId)> {
        self.chunks.last_mut().and_then(|chunk| chunk.last_mut())
    }
    fn iter(&self) -> PairIter<'_, K> {
        PairIter {
            chunks: self.chunks.iter(),
            current: [].iter(),
            remaining: self.len,
        }
    }
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut (K, ObjectId)> {
        self.chunks.iter_mut().flatten()
    }
    fn push(&mut self, pair: (K, ObjectId), lease: &mut Lease, maximum: usize) -> CoreResult<()> {
        if self
            .chunks
            .last()
            .is_none_or(|chunk| chunk.len() == chunk.capacity())
        {
            let remaining =
                maximum.saturating_sub(self.chunks.iter().map(Vec::capacity).sum::<usize>());
            let capacity = self
                .chunks
                .last()
                .map_or(2, |chunk| (chunk.capacity() * 2).min(32))
                .min(remaining);
            if capacity == 0 {
                return Err(CoreError::ObjectLimitExceeded);
            }
            if self.chunks.len() == self.chunks.capacity() {
                let old = self.chunks.capacity();
                let new = (old.max(1) * 2).min(PAGE_ITEMS);
                lease.grow(new * std::mem::size_of::<Vec<(K, ObjectId)>>())?;
                self.chunks
                    .try_reserve_exact(new - old)
                    .map_err(|_| CoreError::ObjectLimitExceeded)?;
                lease.shrink(old * std::mem::size_of::<Vec<(K, ObjectId)>>());
            }
            lease.grow(capacity * std::mem::size_of::<(K, ObjectId)>())?;
            let mut chunk = Vec::new();
            chunk
                .try_reserve_exact(capacity)
                .map_err(|_| CoreError::ObjectLimitExceeded)?;
            self.chunks.push(chunk);
        }
        self.chunks.last_mut().unwrap().push(pair);
        self.len += 1;
        Ok(())
    }
    fn split_off(&mut self, index: usize, from: &mut Lease, to: &mut Lease) -> CoreResult<Self> {
        let mut local = index;
        let mut first = 0;
        while first < self.chunks.len() && local >= self.chunks[first].len() {
            local -= self.chunks[first].len();
            first += 1;
        }
        let capacity = self.chunks.len() - first;
        to.grow(capacity * std::mem::size_of::<Vec<(K, ObjectId)>>())?;
        let mut chunks = Vec::new();
        chunks
            .try_reserve_exact(capacity)
            .map_err(|_| CoreError::ObjectLimitExceeded)?;
        if local > 0 {
            let tail = self.chunks[first].len() - local;
            to.grow(tail * std::mem::size_of::<(K, ObjectId)>())?;
            chunks.push(self.chunks[first].split_off(local));
            first += 1;
        }
        for chunk in self.chunks.drain(first..) {
            from.transfer(to, chunk.capacity() * std::mem::size_of::<(K, ObjectId)>());
            chunks.push(chunk);
        }
        let len = self.len - index;
        self.len = index;
        Ok(Self { chunks, len })
    }
}
struct PairIter<'a, K> {
    chunks: std::slice::Iter<'a, Vec<(K, ObjectId)>>,
    current: std::slice::Iter<'a, (K, ObjectId)>,
    remaining: usize,
}
impl<K> Clone for PairIter<'_, K> {
    fn clone(&self) -> Self {
        Self {
            chunks: self.chunks.clone(),
            current: self.current.clone(),
            remaining: self.remaining,
        }
    }
}
impl<'a, K> Iterator for PairIter<'a, K> {
    type Item = &'a (K, ObjectId);
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(pair) = self.current.next() {
                self.remaining -= 1;
                return Some(pair);
            }
            self.current = self.chunks.next()?.iter();
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl<K> ExactSizeIterator for PairIter<'_, K> {}
struct Entry<K> {
    key: K,
    id: ObjectId,
    count: u64,
    bytes: u64,
    size: usize,
    pending: Option<Box<Page<K>>>,
    reuse: bool,
}
struct Child<K> {
    count: u64,
    bytes: u64,
    size: usize,
    pending: Option<Box<Page<K>>>,
    reuse: bool,
}
struct Page<K> {
    level: u8,
    entries: Pairs<K>,
    children: Vec<Child<K>>,
    origin: Option<ObjectId>,
    aggregate: Option<(u64, u64)>,
    _lease: Lease,
    spilled: Option<Box<Spilled<K>>>,
}
struct PageEntries<K> {
    entries: std::iter::Flatten<std::vec::IntoIter<Vec<(K, ObjectId)>>>,
    children: std::vec::IntoIter<Child<K>>,
    level: u8,
    _lease: Lease,
}
impl<K> Iterator for PageEntries<K> {
    type Item = Entry<K>;
    fn next(&mut self) -> Option<Self::Item> {
        let (key, id) = self.entries.next()?;
        let child = if self.level == 0 {
            Child {
                count: 1,
                bytes: 0,
                size: 0,
                pending: None,
                reuse: false,
            }
        } else {
            self.children.next().expect("one summary per branch child")
        };
        Some(Entry {
            key,
            id,
            count: child.count,
            bytes: child.bytes,
            size: child.size,
            pending: child.pending,
            reuse: child.reuse,
        })
    }
}
impl<K> Page<K> {
    fn into_entries(self) -> PageEntries<K> {
        assert!(self.spilled.is_none());
        PageEntries {
            entries: self.entries.chunks.into_iter().flatten(),
            children: self.children.into_iter(),
            level: self.level,
            _lease: self._lease,
        }
    }
}
struct Node<K> {
    max: Option<K>,
    id: Option<ObjectId>,
    level: u8,
    count: u64,
    bytes: u64,
    size: usize,
    items: usize,
    pending: Option<Box<Page<K>>>,
    reuse: bool,
}
impl<K: Clone> Node<K> {
    fn opaque(id: ObjectId, max: K, level: u8) -> Self {
        Self {
            max: Some(max),
            id: Some(id),
            level,
            count: 0,
            bytes: 0,
            size: 0,
            items: 0,
            pending: None,
            reuse: true,
        }
    }
    fn existing(id: ObjectId, wire: &Wire<K>) -> Self {
        Self {
            max: wire.entries.last().map(|v| v.0.clone()),
            id: Some(id),
            level: wire.level,
            count: wire.count,
            bytes: wire.bytes,
            size: wire.size,
            items: wire.entries.len(),
            pending: None,
            reuse: true,
        }
    }
}
struct Engine<'a, S, F> {
    store: &'a mut S,
    budget: Rc<Budget>,
    counters: TreeBatchCounters,
    format: PhantomData<F>,
    spill_options: Option<(&'a Path, u64)>,
    spills: Option<Rc<SpillPool>>,
}
impl<S: ObjectStore, F: Format> Engine<'_, S, F> {
    fn page(&self, level: u8) -> CoreResult<Page<F::Key>> {
        if level > 31 {
            return Err(CoreError::MappingDepthExceeded);
        }
        let lease = self.budget.reserve(std::mem::size_of::<Page<F::Key>>())?;
        Ok(Page {
            level,
            entries: Pairs::new(),
            children: Vec::new(),
            origin: None,
            aggregate: None,
            _lease: lease,
            spilled: None,
        })
    }
    fn spill_pool(&mut self) -> CoreResult<Rc<SpillPool>> {
        if let Some(pool) = &self.spills {
            return Ok(pool.clone());
        }
        let (dir, limit) = self.spill_options.ok_or(CoreError::ObjectLimitExceeded)?;
        let lease = self.budget.reserve(
            std::mem::size_of::<SpillPool>()
                + dir.as_os_str().as_encoded_bytes().len()
                + 2 * std::mem::size_of::<usize>(),
        )?;
        let pool = Rc::new(SpillPool {
            dir: dir.to_owned(),
            limit,
            used: Cell::new(0),
            peak: Cell::new(0),
            written: Cell::new(0),
            read: Cell::new(0),
            _lease: lease,
        });
        self.spills = Some(pool.clone());
        Ok(pool)
    }
    fn spill_page(&mut self, page: &mut Page<F::Key>) -> CoreResult<()> {
        if page.spilled.is_some() || page.entries.is_empty() {
            return Ok(());
        }
        if page.level > 0 {
            for child in &mut page.children {
                if let Some(page) = &mut child.pending {
                    self.spill_page(page)?;
                }
            }
            return Ok(());
        }
        let pool = self.spill_pool()?;
        let _writer = self.budget.reserve(
            std::mem::size_of::<ContentDigestWriter>()
                + SPILL_RECORD.max(2 * pool.dir.as_os_str().as_encoded_bytes().len() + 128),
        )?;
        let keep = std::mem::size_of::<Page<F::Key>>()
            + std::mem::size_of::<Spilled<F::Key>>()
            + 2 * F::heap_bytes(&page.entries.last().unwrap().0);
        page._lease.grow(
            std::mem::size_of::<Spilled<F::Key>>() + F::heap_bytes(&page.entries.last().unwrap().0),
        )?;
        let mut disk = DiskLease {
            pool: pool.clone(),
            bytes: 0,
        };
        disk.reserve(
            (page.entries.len() as u64)
                .checked_mul(SPILL_RECORD as u64)
                .ok_or(CoreError::LengthOverflow)?,
        )?;
        let mut run = Run::create(&pool.dir)?;
        let mut hash = ContentDigestWriter::new();
        let mut logical = 0u64;
        for (key, id) in page.entries.iter() {
            let bytes = F::key_bytes(key);
            if bytes.len() > 255 {
                return Err(CoreError::InvalidRecord("private tree key length"));
            }
            let mut record = [0u8; SPILL_RECORD];
            record[..2].copy_from_slice(&(bytes.len() as u16).to_be_bytes());
            record[2..2 + bytes.len()].copy_from_slice(bytes);
            record[257..].copy_from_slice(id.as_bytes());
            hash.write_all(&record).map_err(|_| CoreError::Io)?;
            run.push(&record)?;
            pool.written.set(pool.written.get() + SPILL_RECORD as u64);
            logical = logical
                .checked_add(F::width(key) as u64)
                .ok_or(CoreError::LengthOverflow)?;
        }
        let spilled = Spilled {
            run,
            disk,
            max: page.entries.last().unwrap().0.clone(),
            items: page.entries.len(),
            bytes: logical,
            size: 44 + usize::try_from(logical).map_err(|_| CoreError::LengthOverflow)?,
            digest: hash.finish(),
        };
        page.entries = Pairs::new();
        page._lease.shrink(page._lease.bytes.saturating_sub(keep));
        page.spilled = Some(Box::new(spilled));
        Ok(())
    }
    fn relieve(
        &mut self,
        page: &mut Page<F::Key>,
        pending: &mut Option<Node<F::Key>>,
    ) -> CoreResult<()> {
        if self.spill_options.is_none()
            || self.budget.limit.saturating_sub(self.budget.used.get()) >= 2 * 8192
        {
            return Ok(());
        }
        if let Some(node) = pending {
            if let Some(page) = &mut node.pending {
                self.spill_page(page)?;
            }
        }
        for child in &mut page.children {
            if let Some(page) = &mut child.pending {
                self.spill_page(page)?;
            }
        }
        Ok(())
    }
    fn spill_reader(&self, mut spilled: Spilled<F::Key>) -> CoreResult<SpillReader<F::Key>> {
        // The anonymous run has one owner and no writers after construction.
        // Verify it completely, then end the hash-state lifetime before any
        // consumer can allocate a canonical output or rebalance another page.
        self.verify_spill(&mut spilled)?;
        spilled.run.rewind()?;
        let lease = self
            .budget
            .reserve(std::mem::size_of::<SpillReader<F::Key>>() + 2 * SPILL_RECORD + 512)?;
        Ok(SpillReader {
            spilled,
            read: 0,
            _lease: lease,
        })
    }
    fn verify_spill(&self, spilled: &mut Spilled<F::Key>) -> CoreResult<()> {
        let _verify = self
            .budget
            .reserve(std::mem::size_of::<ContentDigestWriter>() + 2 * SPILL_RECORD)?;
        spilled.run.rewind()?;
        let mut hash = ContentDigestWriter::new();
        let mut count = 0usize;
        while let Some(record) = spilled.run.next()? {
            hash.write_all(&record).map_err(|_| CoreError::Io)?;
            count += 1;
            spilled
                .disk
                .pool
                .read
                .set(spilled.disk.pool.read.get() + SPILL_RECORD as u64);
        }
        if count != spilled.items || hash.finish() != spilled.digest {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(())
    }
    fn read(&mut self, id: ObjectId, root: bool) -> CoreResult<ReadPage<F::Key>> {
        self.counters.nodes_read += 1;
        let budget = self.budget.clone();
        let (wire, lease) = self.store.with_authenticated_canonical(id, |bytes| {
            if bytes.len() > 8192 {
                return Err(CoreError::ObjectLimitExceeded);
            }
            let lease = budget
                .reserve(F::decode_scratch(bytes.len()) + std::mem::size_of::<Wire<F::Key>>())?;
            Ok((F::decode(bytes)?, lease))
        })?;
        if (!root && !F::filled(wire.size, wire.entries.len()))
            || (wire.level > 0 && wire.entries.len() < 2)
            || (!F::empty_allowed() && wire.entries.is_empty())
        {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        Ok(ReadPage {
            wire,
            _lease: lease,
        })
    }
    fn complete_summaries(&mut self, page: &mut Page<F::Key>) -> CoreResult<()> {
        if page.level == 0 {
            return Ok(());
        }
        for ((key, id), child) in page.entries.iter().zip(&mut page.children) {
            if child.count == 0 {
                let read = self.read(*id, false)?;
                self.check_child(page.level, key, &read.wire)?;
                child.count = read.wire.count;
                child.bytes = read.wire.bytes;
                child.size = read.wire.size;
            }
        }
        Ok(())
    }
    fn node(&mut self, mut page: Page<F::Key>) -> CoreResult<Node<F::Key>> {
        if let Some(spilled) = &page.spilled {
            page._lease.grow(F::heap_bytes(&spilled.max))?;
            return Ok(Node {
                max: Some(spilled.max.clone()),
                id: page.origin,
                level: 0,
                count: spilled.items as u64,
                bytes: spilled.bytes,
                size: spilled.size,
                items: spilled.items,
                pending: Some(Box::new(page)),
                reuse: false,
            });
        }
        page._lease
            .grow(page.entries.last().map_or(0, |e| F::heap_bytes(&e.0)))?;
        if page.level > 0 && page.aggregate.is_none() {
            self.complete_summaries(&mut page)?;
        }
        let (count, bytes) = if page.level == 0 {
            (
                page.entries.len() as u64,
                page.entries.iter().try_fold(0u64, |n, e| {
                    n.checked_add(F::width(&e.0) as u64)
                        .ok_or(CoreError::LengthOverflow)
                })?,
            )
        } else {
            match page.aggregate {
                Some(summary) => summary,
                None => (
                    page.children.iter().try_fold(0u64, |n, e| {
                        n.checked_add(e.count).ok_or(CoreError::LengthOverflow)
                    })?,
                    page.children.iter().try_fold(0u64, |n, e| {
                        n.checked_add(e.bytes).ok_or(CoreError::LengthOverflow)
                    })?,
                ),
            }
        };
        page.aggregate = Some((count, bytes));
        let size = 44 + page.entries.iter().map(|e| F::width(&e.0)).sum::<usize>();
        Ok(Node {
            max: page.entries.last().map(|e| e.0.clone()),
            id: page.origin,
            level: page.level,
            count,
            bytes,
            size,
            items: page.entries.len(),
            pending: Some(Box::new(page)),
            reuse: false,
        })
    }
    fn filled(node: &Node<F::Key>) -> bool {
        node.pending.is_none() && node.count == 0 && node.reuse || F::filled(node.size, node.items)
    }
    fn persist(&mut self, mut node: Node<F::Key>) -> CoreResult<Node<F::Key>> {
        if let Some(mut page) = node.pending.take() {
            for ((_, id), child) in page.entries.iter_mut().zip(&mut page.children) {
                if let Some(final_id) = self.persist_child(child, page.level)? {
                    *id = final_id;
                } else if child.reuse {
                    self.counters.nodes_reused += 1;
                    child.reuse = false;
                }
            }
            if page.level == 0
                && page.spilled.is_none()
                && self.spill_options.is_some()
                && self.budget.used.get().saturating_add(node.size + 31) > self.budget.limit
            {
                self.spill_page(&mut page)?;
            }
            let reader = page
                .spilled
                .take()
                .map(|spilled| self.spill_reader(*spilled))
                .transpose()?;
            let _codec = self.budget.reserve(node.size + 31)?;
            let canonical = if let Some(mut reader) = reader {
                let input = std::iter::from_fn(|| reader.next::<F>().transpose());
                F::encode_spilled(node.items, node.bytes, input)?
            } else {
                F::encode(&page)?
            };
            drop(page);
            if node
                .id
                .is_some_and(|id| ObjectId::for_bytes(&canonical) == id)
            {
                self.counters.nodes_reused += 1;
            } else {
                node.id = Some(self.store.put_owned(canonical)?);
                self.counters.nodes_created += 1;
            }
        } else if node.reuse {
            self.counters.nodes_reused += 1;
        }
        node.reuse = false;
        Ok(node)
    }
    fn entry(&mut self, node: Node<F::Key>) -> CoreResult<Entry<F::Key>> {
        Ok(Entry {
            key: node.max.ok_or(CoreError::NonCanonicalPagePartition)?,
            id: node.id.unwrap_or_else(|| ObjectId::from_digest([0; 32])),
            count: node.count,
            bytes: node.bytes,
            size: node.size,
            pending: node.pending,
            reuse: node.reuse,
        })
    }
    fn persist_child(
        &mut self,
        child: &mut Child<F::Key>,
        level: u8,
    ) -> CoreResult<Option<ObjectId>> {
        if let Some(page) = child.pending.take() {
            if level == 0 {
                return Err(CoreError::WrongLogicalRole);
            }
            let node = self.node(*page)?;
            if !Self::filled(&node) {
                return Err(CoreError::NonCanonicalPagePartition);
            }
            return Ok(Some(
                self.persist(node)?.id.ok_or(CoreError::IdentityMismatch)?,
            ));
        }
        Ok(None)
    }
    fn child(entry: Entry<F::Key>, level: u8) -> Node<F::Key> {
        let items = entry.pending.as_ref().map_or_else(
            || entry.size.saturating_sub(44) / 64,
            |p| {
                p.spilled
                    .as_ref()
                    .map_or_else(|| p.entries.len(), |spill| spill.items)
            },
        );
        Node {
            max: Some(entry.key),
            id: Some(entry.id),
            level,
            count: entry.count,
            bytes: entry.bytes,
            size: entry.size,
            items,
            pending: entry.pending,
            reuse: entry.reuse,
        }
    }
    fn materialize(&mut self, node: Node<F::Key>) -> CoreResult<Page<F::Key>> {
        if let Some(mut page) = node.pending {
            if let Some(spilled) = page.spilled.take() {
                let mut reader = self.spill_reader(*spilled)?;
                while let Some((key, id)) = reader.next::<F>()? {
                    self.append_entry(
                        &mut page,
                        Entry {
                            key,
                            id,
                            count: 1,
                            bytes: 0,
                            size: 0,
                            pending: None,
                            reuse: false,
                        },
                    )?;
                }
            }
            return Ok(*page);
        }
        let id = node.id.ok_or(CoreError::IdentityMismatch)?;
        let read = self.read(id, node.count != 0)?;
        if read.wire.level != node.level
            || read.wire.entries.last().map(|entry| &entry.0) != node.max.as_ref()
        {
            return Err(CoreError::InvalidRecord("materialized tree child summary"));
        }
        self.page_from_wire(id, read)
    }
    fn page_from_wire(
        &mut self,
        id: ObjectId,
        mut read: ReadPage<F::Key>,
    ) -> CoreResult<Page<F::Key>> {
        let mut page = self.page(read.wire.level)?;
        page.origin = Some(id);
        if page.level == 0 {
            let len = read.wire.entries.len();
            let owned = read.wire.entries.capacity() * std::mem::size_of::<(F::Key, ObjectId)>()
                + read
                    .wire
                    .entries
                    .iter()
                    .map(|e| F::heap_bytes(&e.0))
                    .sum::<usize>();
            page._lease
                .grow(std::mem::size_of::<Vec<(F::Key, ObjectId)>>())?;
            read._lease.transfer(&mut page._lease, owned);
            page.entries = Pairs {
                chunks: vec![read.wire.entries],
                len,
            };
            return Ok(page);
        }
        for (key, value) in read.wire.entries {
            let child = self.read(value, false)?;
            self.check_child(page.level, &key, &child.wire)?;
            self.append_entry(
                &mut page,
                Entry {
                    key,
                    id: value,
                    count: child.wire.count,
                    bytes: child.wire.bytes,
                    size: child.wire.size,
                    pending: None,
                    reuse: true,
                },
            )?;
        }
        let count = page.children.iter().try_fold(0u64, |n, e| {
            n.checked_add(e.count).ok_or(CoreError::LengthOverflow)
        })?;
        let bytes = page.children.iter().try_fold(0u64, |n, e| {
            n.checked_add(e.bytes).ok_or(CoreError::LengthOverflow)
        })?;
        if count != read.wire.count || bytes != read.wire.bytes {
            return Err(CoreError::InvalidRecord("batched tree subtree summary"));
        }
        page.aggregate = Some((count, bytes));
        Ok(page)
    }
    fn check_child(&self, level: u8, max: &F::Key, child: &Wire<F::Key>) -> CoreResult<()> {
        if child.level.checked_add(1) != Some(level)
            || child.entries.last().map(|e| &e.0) != Some(max)
        {
            return Err(CoreError::InvalidRecord("batched tree child summary"));
        }
        Ok(())
    }
    fn append_entry(&self, page: &mut Page<F::Key>, entry: Entry<F::Key>) -> CoreResult<()> {
        page.aggregate = None;
        if page.level > 0 && page.children.len() == page.children.capacity() {
            let before = page.children.capacity();
            let capacity = (before.max(1) * 2).min(PAGE_ITEMS);
            page._lease
                .grow(capacity * std::mem::size_of::<Child<F::Key>>())?;
            page.children
                .try_reserve_exact(capacity - before)
                .map_err(|_| CoreError::ObjectLimitExceeded)?;
            page._lease
                .shrink(before * std::mem::size_of::<Child<F::Key>>());
        }
        page._lease.grow(F::heap_bytes(&entry.key))?;
        if page.level > 0 {
            page.children.push(Child {
                count: entry.count,
                bytes: entry.bytes,
                size: entry.size,
                pending: entry.pending,
                reuse: entry.reuse,
            });
        }
        page.entries
            .push((entry.key, entry.id), &mut page._lease, F::maximum_pairs())
    }
    fn push(
        &mut self,
        page: &mut Page<F::Key>,
        mut entry: Entry<F::Key>,
    ) -> CoreResult<Option<Node<F::Key>>> {
        if page.entries.len() == F::maximum_pairs() {
            return Err(CoreError::ObjectLimitExceeded);
        }
        // Keep the outer boundary children private. A neighbouring parent's
        // singleton underfull chain can still redistribute their entries.
        // Interior children cannot meet that chain after this final-state merge.
        if page.level > 0 && page.entries.len() > 1 {
            // The incoming child is already owned by this call even though it
            // is not in the parent yet. Release its boundary payload before
            // encoding the previous child; otherwise both buffers overlap.
            if self.spill_options.is_some()
                && self.budget.limit.saturating_sub(self.budget.used.get()) < 2 * 8192
            {
                if let Some(incoming) = &mut entry.pending {
                    self.spill_page(incoming)?;
                }
            }
            if let Some(id) = self.persist_child(page.children.last_mut().unwrap(), page.level)? {
                page.entries.last_mut().unwrap().1 = id;
            }
        }
        self.append_entry(page, entry)?;
        let size = 44 + page.entries.iter().map(|e| F::width(&e.0)).sum::<usize>();
        if F::fits(size, page.entries.len()) {
            return Ok(None);
        }
        self.complete_summaries(page)?;
        let mut right = self.page(page.level)?;
        let widths_lease = self
            .budget
            .reserve(page.entries.len() * std::mem::size_of::<usize>())?;
        let split =
            super::directory::nearest_half(page.entries.iter().map(|e| F::width(&e.0)).collect());
        drop(widths_lease);
        right.entries = page
            .entries
            .split_off(split, &mut page._lease, &mut right._lease)?;
        let names = right.entries.iter().map(|e| F::heap_bytes(&e.0)).sum();
        page._lease.transfer(&mut right._lease, names);
        if page.level > 0 {
            right
                ._lease
                .grow((page.children.len() - split) * std::mem::size_of::<Child<F::Key>>())?;
            right.children = page.children.split_off(split);
        }
        page.origin = None;
        let left = std::mem::replace(page, right);
        self.node(left).map(Some)
    }
    fn merge(
        &mut self,
        left: Node<F::Key>,
        right: Node<F::Key>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        let mut left = self.materialize(left)?;
        if left.level == 0
            && right
                .pending
                .as_ref()
                .is_some_and(|page| page.spilled.is_some())
        {
            let mut right = right.pending.unwrap();
            let mut reader = self.spill_reader(*right.spilled.take().unwrap())?;
            left.origin = None;
            while let Some((key, id)) = reader.next::<F>()? {
                if let Some(node) = self.push(
                    &mut left,
                    Entry {
                        key,
                        id,
                        count: 1,
                        bytes: 0,
                        size: 0,
                        pending: None,
                        reuse: false,
                    },
                )? {
                    output(self, node)?;
                }
            }
            // Neither the verified reader nor its anonymous run is needed by
            // final output; their ownership ends before canonical allocation.
            drop(reader);
            drop(right);
            let node = self.node(left)?;
            return output(self, node);
        }
        let right = self.materialize(right)?;
        if left.level != right.level {
            return Err(CoreError::WrongLogicalRole);
        }
        left.origin = None;
        if left.level == 0 {
            for entry in right.into_entries() {
                if let Some(node) = self.push(&mut left, entry)? {
                    output(self, node)?;
                }
            }
        } else {
            let mut page = self.page(left.level)?;
            let mut pending = None;
            let level = left.level - 1;
            for entry in left.into_entries().chain(right.into_entries()) {
                self.sibling(
                    &mut pending,
                    Self::child(entry, level),
                    &mut |engine, child| {
                        let entry = engine.entry(child)?;
                        if let Some(node) = engine.push(&mut page, entry)? {
                            output(engine, node)?;
                        }
                        Ok(())
                    },
                )?;
            }
            if let Some(child) = pending {
                let entry = self.entry(child)?;
                if let Some(node) = self.push(&mut page, entry)? {
                    output(self, node)?;
                }
            }
            left = page;
        }
        {
            let node = self.node(left)?;
            output(self, node)
        }
    }
    fn sibling(
        &mut self,
        pending: &mut Option<Node<F::Key>>,
        mut next: Node<F::Key>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        // Multiple output pages can come from one old leaf. Release retained
        // siblings here, before returning to that leaf's still-growing output.
        if self.spill_options.is_some()
            && self.budget.limit.saturating_sub(self.budget.used.get()) < 2 * 8192
        {
            if let Some(previous) = pending {
                if let Some(page) = &mut previous.pending {
                    self.spill_page(page)?;
                }
            }
            if let Some(page) = &mut next.pending {
                self.spill_page(page)?;
            }
        }
        let Some(previous) = pending.take() else {
            *pending = Some(next);
            return Ok(());
        };
        if Self::filled(&previous) && Self::filled(&next) {
            output(self, previous)?;
            *pending = Some(next);
        } else {
            self.merge(previous, next, &mut |engine, node| {
                if let Some(previous) = pending.replace(node) {
                    output(engine, previous)?;
                }
                Ok(())
            })?;
        }
        Ok(())
    }
    fn edit_leaf<I: Iterator<Item = CoreResult<(F::Key, Option<ObjectId>)>>>(
        &mut self,
        id: ObjectId,
        mut input: LeafInput<F::Key>,
        bound: Option<&F::Key>,
        deltas: &mut Deltas<I, F::Key>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        let mut page = self.page(0)?;
        page.origin = Some(id);
        let mut old = input.next::<F>()?;
        while old.is_some() || deltas.in_range(bound) {
            let delta_first = deltas.in_range(bound)
                && old
                    .as_ref()
                    .is_none_or(|old| old.0 >= deltas.next.as_ref().unwrap().0);
            let (key, value) = if delta_first {
                let (key, value) = deltas.take()?;
                self.counters.delta_keys += 1;
                if old.as_ref().is_some_and(|old| old.0 == key) {
                    old = input.next::<F>()?;
                }
                (key, value)
            } else {
                let (key, value) = old.take().unwrap();
                old = input.next::<F>()?;
                (key, Some(value))
            };
            if let Some(id) = value {
                if let Some(node) = self.push(
                    &mut page,
                    Entry {
                        bytes: F::width(&key) as u64,
                        key,
                        id,
                        count: 1,
                        size: 0,
                        pending: None,
                        reuse: false,
                    },
                )? {
                    output(self, node)?;
                }
            }
        }
        drop(input);
        if !page.entries.is_empty() {
            let node = self.node(page)?;
            output(self, node)?;
        }
        Ok(())
    }
    fn edit<I: Iterator<Item = CoreResult<(F::Key, Option<ObjectId>)>>>(
        &mut self,
        id: ObjectId,
        read: ReadPage<F::Key>,
        bound: Option<&F::Key>,
        deltas: &mut Deltas<I, F::Key>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        if !deltas.in_range(bound) {
            let node = Node::existing(id, &read.wire);
            drop(read);
            return output(self, node);
        }
        if read.wire.level == 0 {
            let pair_size = std::mem::size_of::<(F::Key, ObjectId)>();
            let source_bytes = read.wire.entries.capacity() * pair_size
                + read
                    .wire
                    .entries
                    .iter()
                    .map(|entry| F::heap_bytes(&entry.0))
                    .sum::<usize>();
            let reader_bytes = std::mem::size_of::<SpillReader<F::Key>>() + 2 * SPILL_RECORD + 512;
            // A final page plus one boundary-segment split can coexist before
            // output. The old page's size is not a bound on the new page's size.
            let working = (F::maximum_pairs() + 32) * pair_size
                + std::mem::size_of::<Page<F::Key>>()
                + 16 * std::mem::size_of::<Vec<(F::Key, ObjectId)>>();
            let input = if self.spill_options.is_some()
                && source_bytes > reader_bytes
                && self.budget.limit.saturating_sub(self.budget.used.get()) < working
            {
                // The old decoded page otherwise survives while its merged
                // output grows. Spill that input owner before constructing output.
                let mut source = self.page_from_wire(id, read)?;
                self.spill_page(&mut source)?;
                let reader = self.spill_reader(*source.spilled.take().unwrap())?;
                drop(source);
                LeafInput::Spilled(reader)
            } else {
                LeafInput::Resident {
                    entries: read.wire.entries.into_iter(),
                    _lease: read._lease,
                }
            };
            return self.edit_leaf(id, input, bound, deltas, output);
        }
        let mut page = self.page(read.wire.level)?;
        page.origin = Some(id);
        let level = page.level;
        let children = read.wire.entries.len();
        let (mut count, mut bytes) = (read.wire.count, read.wire.bytes);
        let mut pending = None;
        let mut split = false;
        let (mut source_count, mut source_bytes) = (0u64, 0u64);
        let mut opaque = false;
        let (mut emitted_count, mut emitted_bytes) = (0u64, 0u64);
        for (index, (key, child_id)) in read.wire.entries.into_iter().enumerate() {
            self.relieve(&mut page, &mut pending)?;
            let child_bound = if index + 1 == children {
                bound
            } else {
                Some(&key)
            };
            if deltas.in_range(child_bound) {
                let child = self.read(child_id, false)?;
                self.check_child(level, &key, &child.wire)?;
                source_count = source_count
                    .checked_add(child.wire.count)
                    .ok_or(CoreError::LengthOverflow)?;
                source_bytes = source_bytes
                    .checked_add(child.wire.bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                count = count
                    .checked_sub(child.wire.count)
                    .ok_or(CoreError::LengthOverflow)?;
                bytes = bytes
                    .checked_sub(child.wire.bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                self.edit(child_id, child, child_bound, deltas, &mut |engine, node| {
                    count = count
                        .checked_add(node.count)
                        .ok_or(CoreError::LengthOverflow)?;
                    bytes = bytes
                        .checked_add(node.bytes)
                        .ok_or(CoreError::LengthOverflow)?;
                    engine.sibling(&mut pending, node, &mut |engine, node| {
                        let entry = engine.entry(node)?;
                        if let Some(node) = engine.push(&mut page, entry)? {
                            split = true;
                            emitted_count = emitted_count
                                .checked_add(node.count)
                                .ok_or(CoreError::LengthOverflow)?;
                            emitted_bytes = emitted_bytes
                                .checked_add(node.bytes)
                                .ok_or(CoreError::LengthOverflow)?;
                            output(engine, node)?;
                        }
                        Ok(())
                    })
                })?;
            } else {
                opaque = true;
                self.sibling(
                    &mut pending,
                    Node::opaque(child_id, key, level - 1),
                    &mut |engine, node| {
                        let entry = engine.entry(node)?;
                        if let Some(node) = engine.push(&mut page, entry)? {
                            split = true;
                            emitted_count = emitted_count
                                .checked_add(node.count)
                                .ok_or(CoreError::LengthOverflow)?;
                            emitted_bytes = emitted_bytes
                                .checked_add(node.bytes)
                                .ok_or(CoreError::LengthOverflow)?;
                            output(engine, node)?;
                        }
                        Ok(())
                    },
                )?;
            }
        }
        if let Some(node) = pending {
            let entry = self.entry(node)?;
            if let Some(node) = self.push(&mut page, entry)? {
                split = true;
                emitted_count = emitted_count
                    .checked_add(node.count)
                    .ok_or(CoreError::LengthOverflow)?;
                emitted_bytes = emitted_bytes
                    .checked_add(node.bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                output(self, node)?;
            }
        }
        // Without a parent split, authenticated old totals plus changed ranges
        // are exact; untouched child pages need neither decode nor traversal.
        if !opaque && (source_count != read.wire.count || source_bytes != read.wire.bytes) {
            return Err(CoreError::InvalidRecord("batched tree subtree summary"));
        }
        if !split {
            page.aggregate = Some((count, bytes));
        } else {
            self.complete_summaries(&mut page)?;
            let tail_count = page.children.iter().try_fold(0u64, |n, e| {
                n.checked_add(e.count).ok_or(CoreError::LengthOverflow)
            })?;
            let tail_bytes = page.children.iter().try_fold(0u64, |n, e| {
                n.checked_add(e.bytes).ok_or(CoreError::LengthOverflow)
            })?;
            if emitted_count.checked_add(tail_count) != Some(count)
                || emitted_bytes.checked_add(tail_bytes) != Some(bytes)
            {
                return Err(CoreError::InvalidRecord("batched tree split summary"));
            }
            page.aggregate = Some((tail_count, tail_bytes));
        }
        drop(read._lease);
        if !page.entries.is_empty() {
            let node = self.node(page)?;
            output(self, node)?;
        }
        Ok(())
    }
}

struct Deltas<I, K> {
    source: I,
    next: Option<(K, Option<ObjectId>)>,
}
impl<I: Iterator<Item = CoreResult<(K, Option<ObjectId>)>>, K: Ord> Deltas<I, K> {
    fn new(mut source: I) -> CoreResult<Self> {
        let next = source.next().transpose()?;
        Ok(Self { source, next })
    }
    fn in_range(&self, bound: Option<&K>) -> bool {
        self.next
            .as_ref()
            .is_some_and(|v| bound.is_none_or(|bound| &v.0 <= bound))
    }
    fn take(&mut self) -> CoreResult<(K, Option<ObjectId>)> {
        let current = self.next.take().ok_or(CoreError::UnexpectedEof)?;
        self.next = self.source.next().transpose()?;
        if self.next.as_ref().is_some_and(|next| next.0 <= current.0) {
            return Err(CoreError::NonCanonicalOrdering);
        }
        Ok(current)
    }
}

#[cfg(test)]
fn apply<S: ObjectStore, F: Format>(
    store: &mut S,
    root: ObjectId,
    source: impl Iterator<Item = CoreResult<(F::Key, Option<ObjectId>)>>,
) -> CoreResult<(ObjectId, TreeBatchCounters)> {
    apply_budgeted::<S, F>(
        store,
        root,
        source,
        SORTED_TREE_UPDATE_SCRATCH_BYTES,
        None,
        None,
    )
}
fn apply_budgeted<S: ObjectStore, F: Format>(
    store: &mut S,
    root: ObjectId,
    source: impl Iterator<Item = CoreResult<(F::Key, Option<ObjectId>)>>,
    scratch_limit: usize,
    expected: Option<(u8, u64)>,
    spill_options: Option<(&Path, u64)>,
) -> CoreResult<(ObjectId, TreeBatchCounters)> {
    let mut deltas = Deltas::new(source)?;
    if deltas.next.is_none() {
        return Ok((root, TreeBatchCounters::default()));
    }
    let mut engine = Engine::<S, F> {
        store,
        budget: Rc::new(Budget {
            limit: scratch_limit,
            ..Budget::default()
        }),
        counters: TreeBatchCounters::default(),
        format: PhantomData,
        spill_options,
        spills: None,
    };
    let read = engine.read(root, true)?;
    if expected.is_some_and(|value| value != (read.wire.level, read.wire.count)) {
        return Err(CoreError::InvalidRecord("batched tree root summary"));
    }
    let level = read.wire.level;
    let mut first = None;
    let _frontier = engine
        .budget
        .reserve(32 * std::mem::size_of::<Option<Box<Page<F::Key>>>>())?;
    let mut levels: Vec<Option<Box<Page<F::Key>>>> = (0..32).map(|_| None).collect();
    engine.edit(root, read, None, &mut deltas, &mut |engine, node| {
        if first.is_none() && levels.iter().all(Option::is_none) {
            let mut node = node;
            if engine.spill_options.is_some()
                && Engine::<S, F>::filled(&node)
                && engine.budget.limit.saturating_sub(engine.budget.used.get()) < 2 * 8192
            {
                if let Some(page) = &mut node.pending {
                    engine.spill_page(page)?;
                }
            }
            first = Some(node);
            return Ok(());
        }
        if let Some(prior) = first.take() {
            append_root(engine, &mut levels, prior)?;
        }
        append_root(engine, &mut levels, node)
    })?;
    let mut root_node = if let Some(first) = first {
        first
    } else {
        let mut last = None;
        for index in usize::from(level) + 1..32 {
            if let Some(page) = levels[index].take() {
                let node = engine.node(*page)?;
                if levels[index + 1..].iter().all(Option::is_none) {
                    last = Some(node);
                    break;
                }
                append_root(&mut engine, &mut levels, node)?;
            }
        }
        match last {
            Some(node) => node,
            None if F::empty_allowed() => {
                let mut page = engine.page(0)?;
                page.origin = Some(root);
                engine.node(page)?
            }
            None => return Err(CoreError::InvalidRecord("empty inode table")),
        }
    };
    while root_node.level > 0 && root_node.items == 1 {
        let page = engine.materialize(root_node)?;
        root_node = {
            let level = page.level - 1;
            Engine::<S, F>::child(page.into_entries().next().unwrap(), level)
        };
    }
    let root = engine
        .persist(root_node)?
        .id
        .ok_or(CoreError::IdentityMismatch)?;
    engine.counters.peak_scratch_bytes = engine.budget.peak.get();
    if let Some(pool) = &engine.spills {
        engine.counters.spill_write_bytes = pool.written.get();
        engine.counters.spill_read_bytes = pool.read.get();
        engine.counters.peak_spill_bytes = pool.peak.get();
        if pool.used.get() != 0 {
            return Err(CoreError::InvalidRecord("unconsumed private tree pages"));
        }
    }
    Ok((root, engine.counters))
}
fn append_root<S: ObjectStore, F: Format>(
    engine: &mut Engine<'_, S, F>,
    levels: &mut [Option<Box<Page<F::Key>>>],
    mut node: Node<F::Key>,
) -> CoreResult<()> {
    loop {
        let level = node
            .level
            .checked_add(1)
            .filter(|level| *level <= 31)
            .ok_or(CoreError::MappingDepthExceeded)?;
        let entry = engine.entry(node)?;
        let slot = &mut levels[usize::from(level)];
        if slot.is_none() {
            *slot = Some(Box::new(engine.page(level)?));
        }
        match engine.push(slot.as_mut().unwrap(), entry)? {
            Some(next) => node = next,
            None => {
                engine.relieve(slot.as_mut().unwrap(), &mut None)?;
                return Ok(());
            }
        }
    }
}

struct Directory;
impl Format for Directory {
    type Key = CanonicalName;
    fn decode(bytes: &[u8]) -> CoreResult<Wire<Self::Key>> {
        let (level, count, logical, entries) = match decode_directory_node(bytes)? {
            DirectoryNodeV1::Leaf {
                subtree_encoded_bytes,
                entries,
            } => (
                0,
                entries.len() as u64,
                subtree_encoded_bytes,
                entries
                    .into_iter()
                    .map(|(k, v)| (k, ObjectId::from_digest(v.0)))
                    .collect(),
            ),
            DirectoryNodeV1::Branch {
                level,
                subtree_entry_count,
                subtree_encoded_bytes,
                children,
            } => (level, subtree_entry_count, subtree_encoded_bytes, children),
        };
        Ok(Wire {
            level,
            count,
            bytes: logical,
            entries,
            size: bytes.len(),
        })
    }
    fn encode(page: &Page<Self::Key>) -> CoreResult<Vec<u8>> {
        let count = if page.level == 0 {
            page.entries.len() as u64
        } else {
            page.aggregate
                .ok_or(CoreError::InvalidRecord("unresolved tree summary"))?
                .0
        };
        let bytes = if page.level == 0 {
            page.entries.iter().map(|e| Self::width(&e.0) as u64).sum()
        } else {
            page.aggregate
                .ok_or(CoreError::InvalidRecord("unresolved tree summary"))?
                .1
        };
        super::directory::codec::encode_directory_page(
            page.level,
            count,
            bytes,
            page.entries.iter().map(|(key, id)| (key, id.as_bytes())),
        )
    }
    fn key_bytes(key: &Self::Key) -> &[u8] {
        key.as_bytes()
    }
    fn key_from_bytes(bytes: &[u8]) -> CoreResult<Self::Key> {
        CanonicalName::from_bytes(bytes)
    }
    fn encode_spilled(
        count: usize,
        bytes: u64,
        entries: impl Iterator<Item = CoreResult<(Self::Key, ObjectId)>>,
    ) -> CoreResult<Vec<u8>> {
        super::directory::codec::encode_directory_leaf_from_iter(count, bytes, entries)
    }
    fn width(key: &Self::Key) -> usize {
        34 + key.as_bytes().len()
    }
    fn heap_bytes(key: &Self::Key) -> usize {
        key.owned_capacity_bytes()
    }
    fn decode_scratch(bytes: usize) -> usize {
        bytes * 3 + bytes.saturating_sub(44) / 35 * std::mem::size_of::<(CanonicalName, ObjectId)>()
    }
    fn maximum_pairs() -> usize {
        PAGE_ITEMS
    }
    fn filled(size: usize, _: usize) -> bool {
        size * 5 >= 8192 * 2
    }
    fn fits(size: usize, _: usize) -> bool {
        size <= 8192
    }
    fn empty_allowed() -> bool {
        true
    }
}
struct Inodes;
impl Format for Inodes {
    type Key = InodeId;
    fn decode(bytes: &[u8]) -> CoreResult<Wire<Self::Key>> {
        let (level, count, entries) = match decode_inode_table_node(bytes)? {
            InodeTableNodeV1::Leaf(entries) => (0, entries.len() as u64, entries),
            InodeTableNodeV1::Branch {
                level,
                subtree_entry_count,
                children,
            } => (level, subtree_entry_count, children),
        };
        Ok(Wire {
            level,
            count,
            bytes: count.checked_mul(64).ok_or(CoreError::LengthOverflow)?,
            entries,
            size: bytes.len(),
        })
    }
    fn encode(page: &Page<Self::Key>) -> CoreResult<Vec<u8>> {
        let count = if page.level == 0 {
            page.entries.len() as u64
        } else {
            page.aggregate
                .ok_or(CoreError::InvalidRecord("unresolved tree summary"))?
                .0
        };
        super::inode::codec::encode_inode_page(
            page.level,
            count,
            page.entries.iter().map(|(key, id)| (key, id.as_bytes())),
        )
    }
    fn key_bytes(key: &Self::Key) -> &[u8] {
        key.as_bytes()
    }
    fn key_from_bytes(bytes: &[u8]) -> CoreResult<Self::Key> {
        InodeId::from_slice(bytes)
    }
    fn encode_spilled(
        count: usize,
        _bytes: u64,
        entries: impl Iterator<Item = CoreResult<(Self::Key, ObjectId)>>,
    ) -> CoreResult<Vec<u8>> {
        super::inode::codec::encode_inode_leaf_from_iter(count, entries)
    }
    fn width(_: &Self::Key) -> usize {
        64
    }
    fn heap_bytes(_: &Self::Key) -> usize {
        0
    }
    fn decode_scratch(bytes: usize) -> usize {
        bytes.saturating_sub(44)
    }
    fn maximum_pairs() -> usize {
        128
    } // canonical127-pair page plus one overflow
    fn filled(_: usize, count: usize) -> bool {
        count >= 64
    }
    fn fits(_: usize, count: usize) -> bool {
        count <= 127
    }
    fn empty_allowed() -> bool {
        false
    }
}

/// Sorted unique final bindings; None ensures absence, Some upserts.
pub fn directory_apply_sorted<S: ObjectStore>(
    store: &mut S,
    root: super::directory::DirectoryStateRoot,
    deltas: impl Iterator<Item = CoreResult<(CanonicalName, Option<InodeId>)>>,
) -> CoreResult<(super::directory::DirectoryStateRoot, TreeBatchCounters)> {
    directory_apply_sorted_with_budget(store, root, deltas, SORTED_TREE_UPDATE_SCRATCH_BYTES)
}
/// Same engine with a caller-reserved simultaneous scratch ceiling.
pub fn directory_apply_sorted_with_budget<S: ObjectStore>(
    store: &mut S,
    root: super::directory::DirectoryStateRoot,
    deltas: impl Iterator<Item = CoreResult<(CanonicalName, Option<InodeId>)>>,
    scratch_limit: usize,
) -> CoreResult<(super::directory::DirectoryStateRoot, TreeBatchCounters)> {
    directory_apply_sorted_inner(store, root, deltas, scratch_limit, None)
}
pub fn directory_apply_sorted_with_spill<S: ObjectStore>(
    store: &mut S,
    root: super::directory::DirectoryStateRoot,
    deltas: impl Iterator<Item = CoreResult<(CanonicalName, Option<InodeId>)>>,
    scratch_limit: usize,
    dir: &Path,
    disk_limit: u64,
) -> CoreResult<(super::directory::DirectoryStateRoot, TreeBatchCounters)> {
    directory_apply_sorted_inner(store, root, deltas, scratch_limit, Some((dir, disk_limit)))
}
fn directory_apply_sorted_inner<S: ObjectStore>(
    store: &mut S,
    root: super::directory::DirectoryStateRoot,
    deltas: impl Iterator<Item = CoreResult<(CanonicalName, Option<InodeId>)>>,
    scratch_limit: usize,
    spill_options: Option<(&Path, u64)>,
) -> CoreResult<(super::directory::DirectoryStateRoot, TreeBatchCounters)> {
    use super::directory::codec::{decode_directory_state, encode_directory_state};
    let mut state = store.with_authenticated_canonical(root.0, decode_directory_state)?;
    let (mapping, mut counters) = apply_budgeted::<S, Directory>(
        store,
        state.mapping_root,
        deltas.map(|v| v.map(|(k, v)| (k, v.map(|v| ObjectId::from_digest(v.0))))),
        scratch_limit,
        Some((state.tree_level, state.entry_count)),
        spill_options,
    )?;
    counters.nodes_read += 1;
    if mapping == state.mapping_root {
        return Ok((root, counters));
    }
    let wire = store.with_authenticated_canonical(mapping, Directory::decode)?;
    counters.nodes_read += 1;
    state.mapping_root = mapping;
    state.entry_count = wire.count;
    state.tree_level = wire.level;
    Ok((
        super::directory::DirectoryStateRoot(store.put_owned(encode_directory_state(state)?)?),
        counters,
    ))
}
/// Sorted unique final record IDs, sharing exactly the directory mutation engine.
pub fn inode_table_apply_sorted<S: ObjectStore>(
    store: &mut S,
    root: super::inode::InodeTableRoot,
    deltas: impl Iterator<Item = CoreResult<(InodeId, Option<ObjectId>)>>,
) -> CoreResult<(super::inode::InodeTableRoot, TreeBatchCounters)> {
    inode_table_apply_sorted_with_budget(store, root, deltas, SORTED_TREE_UPDATE_SCRATCH_BYTES)
}
/// Same engine with a caller-reserved simultaneous scratch ceiling.
pub fn inode_table_apply_sorted_with_budget<S: ObjectStore>(
    store: &mut S,
    root: super::inode::InodeTableRoot,
    deltas: impl Iterator<Item = CoreResult<(InodeId, Option<ObjectId>)>>,
    scratch_limit: usize,
) -> CoreResult<(super::inode::InodeTableRoot, TreeBatchCounters)> {
    let (root, counters) =
        apply_budgeted::<S, Inodes>(store, root.0, deltas, scratch_limit, None, None)?;
    Ok((super::inode::InodeTableRoot(root), counters))
}

pub fn inode_table_apply_sorted_with_spill<S: ObjectStore>(
    store: &mut S,
    root: super::inode::InodeTableRoot,
    deltas: impl Iterator<Item = CoreResult<(InodeId, Option<ObjectId>)>>,
    scratch_limit: usize,
    dir: &Path,
    disk_limit: u64,
) -> CoreResult<(super::inode::InodeTableRoot, TreeBatchCounters)> {
    let (root, counters) = apply_budgeted::<S, Inodes>(
        store,
        root.0,
        deltas,
        scratch_limit,
        None,
        Some((dir, disk_limit)),
    )?;
    Ok((super::inode::InodeTableRoot(root), counters))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::directory::{
        directory_entries, directory_lookup, empty_directory, NamespaceCounters,
    };
    use crate::tree::inode::{inode_table_entries, InodeTableRoot};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MemoryStore {
        objects: BTreeMap<ObjectId, Vec<u8>>,
        puts: usize,
    }
    impl ObjectStore for MemoryStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.objects
                .get(&id)
                .cloned()
                .ok_or(CoreError::PathNotFound)
        }
        fn put(&mut self, bytes: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(bytes);
            self.puts += 1;
            self.objects.insert(id, bytes.to_vec());
            Ok(id)
        }
    }
    fn name(index: usize) -> CanonicalName {
        CanonicalName::new(&format!("entry-{index:08}")).unwrap()
    }
    fn inode(index: usize) -> InodeId {
        let mut bytes = [0; 32];
        bytes[24..].copy_from_slice(&(index as u64).to_be_bytes());
        InodeId(bytes)
    }
    fn value(index: usize) -> ObjectId {
        ObjectId::for_bytes(&(index as u64).to_le_bytes())
    }

    #[test]
    fn sorted_directory_dense_mixed_sparse_and_empty() {
        let mut store = MemoryStore::default();
        let empty = empty_directory(&mut store).unwrap();
        let (root, create) = directory_apply_sorted(
            &mut store,
            empty,
            (0..20000).map(|i| Ok((name(i), Some(inode(i))))),
        )
        .unwrap();
        assert!(create.nodes_created < 500, "{create:?}");
        assert!(create.peak_scratch_bytes < SORTED_TREE_UPDATE_SCRATCH_BYTES);
        let mut expected: BTreeMap<_, _> = (0..20000).map(|i| (name(i), inode(i))).collect();
        let changes: BTreeMap<_, _> = (0..20000)
            .filter(|i| i % 3 != 0)
            .map(|i| {
                (
                    name(i),
                    if i % 3 == 1 {
                        None
                    } else {
                        Some(inode(i + 30000))
                    },
                )
            })
            .chain((20000..24000).map(|i| (name(i), Some(inode(i)))))
            .collect();
        let (mixed, stats) = directory_apply_sorted(
            &mut store,
            root,
            changes.iter().map(|(k, v)| Ok((k.clone(), *v))),
        )
        .unwrap();
        for (key, value) in changes {
            match value {
                Some(value) => {
                    expected.insert(key, value);
                }
                None => {
                    expected.remove(&key);
                }
            }
        }
        assert_eq!(
            directory_entries(&store, mixed, &mut NamespaceCounters::default()).unwrap(),
            expected.clone().into_iter().collect::<Vec<_>>()
        );
        assert!(stats.nodes_created < 700, "{stats:?}");
        assert_eq!(
            directory_lookup(&store, root, &name(1), &mut NamespaceCounters::default()).unwrap(),
            Some(inode(1))
        );
        let (sparse, stats) = directory_apply_sorted(
            &mut store,
            mixed,
            std::iter::once(Ok((name(12000), Some(inode(90000))))),
        )
        .unwrap();
        assert!(stats.nodes_read < 1000, "{stats:?}");
        assert!(stats.nodes_created < 12, "{stats:?}");
        expected.insert(name(12000), inode(90000));
        assert_eq!(
            directory_entries(&store, sparse, &mut NamespaceCounters::default()).unwrap(),
            expected.clone().into_iter().collect::<Vec<_>>()
        );
        let before = store.puts;
        let (same, stats) = directory_apply_sorted(
            &mut store,
            sparse,
            std::iter::once(Ok((name(12000), Some(inode(90000))))),
        )
        .unwrap();
        assert_eq!(same, sparse);
        assert_eq!(stats.nodes_created, 0);
        assert_eq!(store.puts, before);
        let (cleared, _) = directory_apply_sorted(
            &mut store,
            sparse,
            expected.into_keys().map(|k| Ok((k, None))),
        )
        .unwrap();
        assert_eq!(cleared, empty);
    }

    #[test]
    fn sorted_inode_dense_delete_preserves_sparse_historical_records() {
        let mut store = MemoryStore::default();
        let root = InodeTableRoot(
            store
                .put(
                    &encode_inode_table_node(&InodeTableNodeV1::Leaf(vec![(inode(0), value(0))]))
                        .unwrap(),
                )
                .unwrap(),
        );
        let (root, stats) = inode_table_apply_sorted(
            &mut store,
            root,
            (1..20000).map(|i| Ok((inode(i), Some(value(i))))),
        )
        .unwrap();
        assert!(stats.nodes_created < 400, "{stats:?}");
        let (sparse, stats) = inode_table_apply_sorted(
            &mut store,
            root,
            std::iter::once(Ok((inode(9000), Some(value(90000))))),
        )
        .unwrap();
        assert!(stats.nodes_read <= 6, "{stats:?}");
        assert!(stats.nodes_created < 10, "{stats:?}");
        // Forwarded immutable children deliberately need no read. Count those
        // roots independently along the edited path rather than bounding reuse
        // by the number of pages decoded by the engine.
        let mut source = root.0;
        let mut expected_reuse = 0;
        while let InodeTableNodeV1::Branch { children, .. } =
            decode_inode_table_node(&store.get(source).unwrap()).unwrap()
        {
            expected_reuse += children.len() as u64 - 1;
            let index = children.partition_point(|(key, _)| *key < inode(9000))
                .min(children.len() - 1);
            source = children[index].1;
        }
        assert_eq!(stats.nodes_reused, expected_reuse);
        let (small, stats) = inode_table_apply_sorted(
            &mut store,
            sparse,
            (1..20000)
                .filter(|i| i % 777 != 0)
                .map(|i| Ok((inode(i), None))),
        )
        .unwrap();
        let expected: Vec<_> = std::iter::once((inode(0), value(0)))
            .chain(
                (1..20000)
                    .filter(|i| i % 777 == 0)
                    .map(|i| (inode(i), value(i))),
            )
            .collect();
        assert_eq!(
            inode_table_entries(
                &store,
                small,
                &mut super::super::inode::InodeTableCounters::default()
            )
            .unwrap(),
            expected
        );
        assert!(stats.nodes_created < 20, "{stats:?}");
        assert_eq!(
            inode_table_entries(
                &store,
                root,
                &mut super::super::inode::InodeTableCounters::default()
            )
            .unwrap()
            .len(),
            20000
        );
        let before = store.puts;
        let (same, stats) = inode_table_apply_sorted(
            &mut store,
            small,
            std::iter::once(Ok((inode(0), Some(value(0))))),
        )
        .unwrap();
        assert_eq!(same, small);
        assert_eq!(stats.nodes_created, 0);
        assert_eq!(before, store.puts);
    }

    #[test]
    fn sorted_updates_validate_input_and_reserve_before_allocation() {
        let budget = Rc::<Budget>::default();
        let held = budget.reserve(SORTED_TREE_UPDATE_SCRATCH_BYTES).unwrap();
        assert!(budget.reserve(1).is_err());
        assert_eq!(budget.used.get(), SORTED_TREE_UPDATE_SCRATCH_BYTES);
        drop(held);
        assert_eq!(budget.used.get(), 0);
        let mut store = MemoryStore::default();
        let root = empty_directory(&mut store).unwrap();
        assert!(matches!(
            directory_apply_sorted(
                &mut store,
                root,
                vec![Ok((name(2), Some(inode(2)))), Ok((name(1), Some(inode(1))))].into_iter()
            ),
            Err(CoreError::NonCanonicalOrdering)
        ));
        assert!(matches!(
            directory_apply_sorted(
                &mut store,
                root,
                vec![Ok((name(1), Some(inode(1)))), Ok((name(1), Some(inode(2))))].into_iter()
            ),
            Err(CoreError::NonCanonicalOrdering)
        ));
        assert_eq!(
            directory_apply_sorted(&mut store, root, std::iter::once(Ok((name(1), None))))
                .unwrap()
                .0,
            root
        );
    }
    fn reachable<F: Format>(
        store: &MemoryStore,
        id: ObjectId,
        output: &mut std::collections::BTreeSet<ObjectId>,
    ) {
        if !output.insert(id) {
            return;
        }
        let wire = F::decode(store.objects.get(&id).unwrap()).unwrap();
        if wire.level > 0 {
            for (_, child) in wire.entries {
                reachable::<F>(store, child, output);
            }
        }
    }

    #[test]
    fn sorted_mixed_boundaries_emit_only_final_reachable_pages() {
        // Two-level branches plus deletes leaving singleton underfull chains
        // force redistribution across old parent boundaries, not just leaf pairs.
        let mut store = MemoryStore::default();
        let root = store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Leaf(vec![(inode(0), value(0))]))
                    .unwrap(),
            )
            .unwrap();
        let (root, _) = apply::<_, Inodes>(
            &mut store,
            root,
            (1..20000).map(|i| Ok((inode(i), Some(value(i))))),
        )
        .unwrap();
        for seed in 1..=3 {
            let before: std::collections::BTreeSet<_> = store.objects.keys().copied().collect();
            let mut expected: BTreeMap<_, _> = (0..20000).map(|i| (inode(i), value(i))).collect();
            let mut random = seed as u64;
            let deltas: Vec<_> = (1..22000)
                .filter_map(|i| {
                    random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let final_value = if i > 20000 {
                        Some(value(i))
                    } else if i < 8000 || random % 9 < 7 {
                        None
                    } else if random % 9 == 7 {
                        Some(value(i + 30000))
                    } else {
                        return None;
                    };
                    match final_value {
                        Some(v) => {
                            expected.insert(inode(i), v);
                        }
                        None => {
                            expected.remove(&inode(i));
                        }
                    }
                    Some(Ok((inode(i), final_value)))
                })
                .collect();
            let (next, counters) =
                apply::<_, Inodes>(&mut store, root, deltas.into_iter()).unwrap();
            assert_eq!(
                inode_table_entries(
                    &store,
                    InodeTableRoot(next),
                    &mut super::super::inode::InodeTableCounters::default()
                )
                .unwrap(),
                expected.into_iter().collect::<Vec<_>>()
            );
            let mut final_pages = std::collections::BTreeSet::new();
            reachable::<Inodes>(&store, next, &mut final_pages);
            let unreachable: Vec<_> = store
                .objects
                .keys()
                .filter(|id| !before.contains(id) && !final_pages.contains(id))
                .collect();
            assert!(
                unreachable.is_empty(),
                "seed={seed}, unreachable={}, counters={counters:?}",
                unreachable.len()
            );
        }
    }

    #[test]
    fn sorted_tiny_inode_update_uses_actual_capacity() {
        let mut store = MemoryStore::default();
        let bytes = encode_inode_table_node(&InodeTableNodeV1::Leaf(vec![
            (inode(0), value(0)),
            (inode(1), value(1)),
        ]))
        .unwrap();
        let root = InodeTableRoot(store.put(&bytes).unwrap());
        let (next, counters) = inode_table_apply_sorted_with_budget(
            &mut store,
            root,
            std::iter::once(Ok((inode(1), Some(value(2))))),
            1024,
        )
        .unwrap();
        assert_ne!(next, root);
        assert!(counters.peak_scratch_bytes <= 1024, "{counters:?}");
        assert_eq!(
            inode_table_entries(
                &store,
                next,
                &mut super::super::inode::InodeTableCounters::default()
            )
            .unwrap(),
            vec![(inode(0), value(0)), (inode(1), value(2))]
        );
        assert!(matches!(
            inode_table_apply_sorted_with_budget(
                &mut store,
                root,
                std::iter::once(Ok((inode(1), Some(value(2))))),
                128
            ),
            Err(CoreError::ObjectLimitExceeded)
        ));
    }

    #[test]
    fn sorted_variable_names_balance_across_parent_boundaries() {
        let mut store = MemoryStore::default();
        let empty = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: 0,
                    entries: Vec::new(),
                })
                .unwrap(),
            )
            .unwrap();
        let key =
            |i: usize| CanonicalName::new(&format!("{i:08}-{}", "x".repeat((i % 5) * 50))).unwrap();
        let (root, _) = apply::<_, Directory>(
            &mut store,
            empty,
            (0..6000).map(|i| Ok((key(i), Some(ObjectId::from_digest(inode(i).0))))),
        )
        .unwrap();
        let before: std::collections::BTreeSet<_> = store.objects.keys().copied().collect();
        let mut expected: BTreeMap<_, _> = (0..6000).map(|i| (key(i), inode(i))).collect();
        let deltas = (0..6500)
            .filter_map(|i| {
                let final_value = if i >= 6000 {
                    Some(inode(i))
                } else if i < 3000 || i % 11 < 9 {
                    None
                } else {
                    return None;
                };
                match final_value {
                    Some(value) => {
                        expected.insert(key(i), value);
                    }
                    None => {
                        expected.remove(&key(i));
                    }
                }
                Some(Ok((
                    key(i),
                    final_value.map(|v| ObjectId::from_digest(v.0)),
                )))
            })
            .collect::<Vec<_>>();
        let (next, stats) = apply::<_, Directory>(&mut store, root, deltas.into_iter()).unwrap();
        let mut final_pages = std::collections::BTreeSet::new();
        reachable::<Directory>(&store, next, &mut final_pages);
        let unreachable = store
            .objects
            .keys()
            .filter(|id| !before.contains(id) && !final_pages.contains(id))
            .count();
        assert_eq!(unreachable, 0, "{stats:?}");
        let mut actual = BTreeMap::new();
        for page in final_pages {
            if let DirectoryNodeV1::Leaf { entries, .. } =
                decode_directory_node(&store.objects[&page]).unwrap()
            {
                actual.extend(entries);
            }
        }
        assert_eq!(actual, expected);
        let (cleared, _) = apply::<_, Directory>(
            &mut store,
            next,
            expected.into_keys().map(|key| Ok((key, None))),
        )
        .unwrap();
        assert_eq!(cleared, empty);
    }

    #[test]
    fn borrowed_page_codecs_match_canonical_framing_and_small_budget() {
        let mut store = MemoryStore::default();
        let empty = empty_directory(&mut store).unwrap();
        let (root, stats) = directory_apply_sorted_with_budget(
            &mut store,
            empty,
            (0..120).map(|i| Ok((name(i), Some(inode(i))))),
            16 * 1024,
        )
        .unwrap();
        assert!(stats.peak_scratch_bytes <= 16 * 1024, "{stats:?}");
        assert_eq!(
            directory_entries(&store, root, &mut NamespaceCounters::default())
                .unwrap()
                .len(),
            120
        );
        for level in [0, 1] {
            let entries = vec![(name(0), value(0)), (name(1), value(1))];
            let logical = entries
                .iter()
                .map(|(key, _)| Directory::width(key) as u64)
                .sum();
            let canonical = super::super::directory::codec::encode_directory_page(
                level,
                2,
                logical,
                entries.iter().map(|(key, id)| (key, id.as_bytes())),
            )
            .unwrap();
            let mut payload = super::super::directory::codec::node_header(
                b"LFS4NSP\0",
                if level == 0 { 1 } else { 2 },
                level,
                2,
                2,
                logical,
            )
            .unwrap();
            for (key, id) in &entries {
                super::super::directory::codec::put_bytes(&mut payload, key.as_bytes()).unwrap();
                payload.extend_from_slice(id.as_bytes());
            }
            assert_eq!(canonical, crate::encode_bytes_object(&payload).unwrap());
            let pairs = vec![(inode(0), value(0)), (inode(1), value(1))];
            let canonical = super::super::inode::codec::encode_inode_page(
                level,
                2,
                pairs.iter().map(|(key, id)| (key, id.as_bytes())),
            )
            .unwrap();
            let mut payload = super::super::directory::codec::node_header(
                b"LFS4INT\0",
                if level == 0 { 7 } else { 8 },
                level,
                2,
                2,
                128,
            )
            .unwrap();
            for (key, id) in pairs {
                payload.extend_from_slice(key.as_bytes());
                payload.extend_from_slice(id.as_bytes());
            }
            assert_eq!(canonical, crate::encode_bytes_object(&payload).unwrap());
        }
    }

    #[test]
    fn sorted_sibling_growth_fits_sixteen_kib() {
        let mut store = MemoryStore::default();
        let key = |i| InodeId::allocate([7; 32], i);
        let mut initial: BTreeMap<_, _> = (0..208).map(|i| (key(i), value(i as usize))).collect();
        let (first, record) = initial.pop_first().unwrap();
        let root = InodeTableRoot(
            store
                .put(
                    &encode_inode_table_node(&InodeTableNodeV1::Leaf(vec![(first, record)]))
                        .unwrap(),
                )
                .unwrap(),
        );
        let (root, _) = inode_table_apply_sorted(
            &mut store,
            root,
            initial.into_iter().map(|(k, v)| Ok((k, Some(v)))),
        )
        .unwrap();
        let changes: BTreeMap<_, _> = (208..328)
            .map(|i| (key(i), Some(value(i as usize))))
            .chain([(key(0), None), (key(1), Some(value(90000)))])
            .collect();
        let dir = std::env::temp_dir().join(format!("layerfs-tree-spill-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (reference, _) =
            inode_table_apply_sorted(&mut store, root, changes.iter().map(|(k, v)| Ok((*k, *v))))
                .unwrap();
        let (next, stats) = inode_table_apply_sorted_with_spill(
            &mut store,
            root,
            changes.iter().map(|(k, v)| Ok((*k, *v))),
            16 * 1024 - 256,
            &dir,
            1024 * 1024,
        )
        .unwrap();
        assert_eq!(next, reference);
        assert!(
            stats.spill_write_bytes > 0
                && stats.spill_read_bytes > 0
                && stats.peak_spill_bytes <= 1024 * 1024,
            "{stats:?}"
        );
        assert!(matches!(
            inode_table_apply_sorted_with_spill(
                &mut store,
                root,
                changes.into_iter().map(Ok),
                16 * 1024 - 256,
                &dir,
                128
            ),
            Err(CoreError::ObjectLimitExceeded)
        ));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir(dir).unwrap();
        assert!(stats.peak_scratch_bytes <= 16 * 1024 - 256, "{stats:?}");
        assert_eq!(
            inode_table_entries(
                &store,
                next,
                &mut super::super::inode::InodeTableCounters::default()
            )
            .unwrap()
            .len(),
            327
        );
    }

    #[test]
    fn spilled_boundary_identity_failure_admits_nothing_and_closes_run() {
        let dir =
            std::env::temp_dir().join(format!("layerfs-tree-spill-digest-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = MemoryStore::default();
        let mut engine = Engine::<_, Inodes> {
            store: &mut store,
            budget: Rc::new(Budget {
                limit: 16 * 1024,
                ..Budget::default()
            }),
            counters: TreeBatchCounters::default(),
            format: PhantomData,
            spill_options: Some((&dir, 1024 * 1024)),
            spills: None,
        };
        let mut page = engine.page(0).unwrap();
        for i in 0..70 {
            engine
                .append_entry(
                    &mut page,
                    Entry {
                        key: inode(i),
                        id: value(i),
                        count: 1,
                        bytes: 64,
                        size: 0,
                        pending: None,
                        reuse: false,
                    },
                )
                .unwrap();
        }
        engine.spill_page(&mut page).unwrap();
        page.spilled.as_mut().unwrap().digest[0] ^= 1;
        let node = engine.node(page).unwrap();
        assert!(matches!(
            engine.persist(node),
            Err(CoreError::IdentityMismatch)
        ));
        assert_eq!(engine.store.puts, 0);
        assert_eq!(engine.spills.as_ref().unwrap().used.get(), 0);
        drop(engine);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn full_source_leaf_and_output_share_sixteen_kib_via_private_input() {
        let dir =
            std::env::temp_dir().join(format!("layerfs-tree-full-source-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = MemoryStore::default();
        let entries = (0..127).map(|i| (inode(i), value(i))).collect();
        let root = InodeTableRoot(
            store
                .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(entries)).unwrap())
                .unwrap(),
        );
        let changes = || std::iter::once(Ok((inode(63), Some(value(90000)))));
        let (reference, _) = inode_table_apply_sorted(&mut store, root, changes()).unwrap();
        let (next, stats) = inode_table_apply_sorted_with_spill(
            &mut store,
            root,
            changes(),
            16 * 1024 - 256,
            &dir,
            1024 * 1024,
        )
        .unwrap();
        assert_eq!(reference, next);
        assert!(stats.peak_scratch_bytes <= 16 * 1024 - 256, "{stats:?}");
        assert!(
            stats.spill_write_bytes >= 127 * SPILL_RECORD as u64,
            "{stats:?}"
        );
        assert_eq!(
            inode_table_entries(
                &store,
                next,
                &mut super::super::inode::InodeTableCounters::default()
            )
            .unwrap()
            .len(),
            127
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn fully_visited_source_branch_rejects_inconsistent_count() {
        let mut store = MemoryStore::default();
        let left = store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Leaf(
                    (0..64).map(|i| (inode(i), value(i))).collect(),
                ))
                .unwrap(),
            )
            .unwrap();
        let right = store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Leaf(
                    (64..128).map(|i| (inode(i), value(i))).collect(),
                ))
                .unwrap(),
            )
            .unwrap();
        let root = InodeTableRoot(
            store
                .put(
                    &encode_inode_table_node(&InodeTableNodeV1::Branch {
                        level: 1,
                        subtree_entry_count: 127,
                        children: vec![(inode(63), left), (inode(127), right)],
                    })
                    .unwrap(),
                )
                .unwrap(),
        );
        assert!(matches!(
            inode_table_apply_sorted(
                &mut store,
                root,
                (0..128).map(|i| Ok((inode(i), Some(value(i + 1000)))))
            ),
            Err(CoreError::InvalidRecord("batched tree subtree summary"))
        ));
    }
}
