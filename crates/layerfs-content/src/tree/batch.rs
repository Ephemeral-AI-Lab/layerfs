//! Sorted final-state updates. Only the last sibling is kept private until its
//! neighbour establishes the final partition; untouched subtrees remain IDs.
use super::directory::codec::{decode_directory_node, encode_directory_node, DirectoryNodeV1};
use super::inode::codec::{decode_inode_table_node, encode_inode_table_node, InodeTableNodeV1};
use super::inode::InodeId;
use crate::file::rope::ObjectStore;
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};
use std::{cell::Cell, marker::PhantomData, rc::Rc};

/// The caller reserves this together with its delta stream and other live state.
/// Every internal page/decode allocation is charged before allocation.
pub const SORTED_TREE_UPDATE_SCRATCH_BYTES: usize = 4 * 1024 * 1024;
const PAGE_ITEMS: usize = 234; // 8-KiB directory page, minimum 35-byte entry, plus overflow.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TreeBatchCounters {
    pub nodes_read: u64,
    pub nodes_created: u64,
    pub nodes_reused: u64,
    pub delta_keys: u64,
    pub peak_scratch_bytes: usize,
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
    fn width(key: &Self::Key) -> usize;
    fn heap_bytes(key: &Self::Key) -> usize;
    fn decode_scratch(bytes: usize) -> usize;
    fn filled(size: usize, count: usize) -> bool;
    fn fits(size: usize, count: usize) -> bool;
    fn empty_allowed() -> bool;
}

struct Entry<K> {
    key: K,
    id: ObjectId,
    count: u64,
    bytes: u64,
    size: usize,
    pending: Option<Box<Page<K>>>,
}
struct Page<K> {
    level: u8,
    entries: Vec<Entry<K>>,
    origin: Option<ObjectId>,
    _lease: Lease,
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
}
impl<K: Clone> Node<K> {
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
        }
    }
}
struct Engine<'a, S, F> {
    store: &'a mut S,
    budget: Rc<Budget>,
    counters: TreeBatchCounters,
    format: PhantomData<F>,
}
impl<S: ObjectStore, F: Format> Engine<'_, S, F> {
    fn page(&self, level: u8) -> CoreResult<Page<F::Key>> {
        if level > 31 {
            return Err(CoreError::MappingDepthExceeded);
        }
        let lease = self.budget.reserve(std::mem::size_of::<Page<F::Key>>())?;
        Ok(Page {
            level,
            entries: Vec::new(),
            origin: None,
            _lease: lease,
        })
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
    fn node(&self, mut page: Page<F::Key>) -> CoreResult<Node<F::Key>> {
        page._lease
            .grow(page.entries.last().map_or(0, |e| F::heap_bytes(&e.key)))?;
        let count = page.entries.iter().try_fold(0u64, |n, e| {
            n.checked_add(e.count).ok_or(CoreError::LengthOverflow)
        })?;
        let bytes = page.entries.iter().try_fold(0u64, |n, e| {
            n.checked_add(e.bytes).ok_or(CoreError::LengthOverflow)
        })?;
        let size = 44 + page.entries.iter().map(|e| F::width(&e.key)).sum::<usize>();
        Ok(Node {
            max: page.entries.last().map(|e| e.key.clone()),
            id: page.origin,
            level: page.level,
            count,
            bytes,
            size,
            items: page.entries.len(),
            pending: Some(Box::new(page)),
        })
    }
    fn filled(node: &Node<F::Key>) -> bool {
        F::filled(node.size, node.items)
    }
    fn persist(&mut self, mut node: Node<F::Key>) -> CoreResult<Node<F::Key>> {
        if let Some(mut page) = node.pending.take() {
            for entry in &mut page.entries {
                self.persist_entry(entry, page.level)?;
            }
            let _codec = self.budget.reserve(
                node.size * 2
                    + page.entries.len() * std::mem::size_of::<(F::Key, ObjectId)>()
                    + page
                        .entries
                        .iter()
                        .map(|e| F::heap_bytes(&e.key))
                        .sum::<usize>(),
            )?;
            let canonical = F::encode(&page)?;
            if node
                .id
                .is_some_and(|id| ObjectId::for_bytes(&canonical) == id)
            {
                self.counters.nodes_reused += 1;
            } else {
                node.id = Some(self.store.put_owned(canonical)?);
                self.counters.nodes_created += 1;
            }
        } else {
            self.counters.nodes_reused += 1;
        }
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
        })
    }
    fn persist_entry(&mut self, entry: &mut Entry<F::Key>, level: u8) -> CoreResult<()> {
        if let Some(page) = entry.pending.take() {
            if level == 0 {
                return Err(CoreError::WrongLogicalRole);
            }
            let node = self.node(*page)?;
            if !Self::filled(&node) {
                return Err(CoreError::NonCanonicalPagePartition);
            }
            entry.id = self.persist(node)?.id.ok_or(CoreError::IdentityMismatch)?;
        }
        Ok(())
    }
    fn child(entry: Entry<F::Key>, level: u8) -> Node<F::Key> {
        let items = entry
            .pending
            .as_ref()
            .map_or_else(|| (entry.size - 44) / 64, |p| p.entries.len());
        Node {
            max: Some(entry.key),
            id: Some(entry.id),
            level,
            count: entry.count,
            bytes: entry.bytes,
            size: entry.size,
            items,
            pending: entry.pending,
        }
    }
    fn materialize(&mut self, node: Node<F::Key>) -> CoreResult<Page<F::Key>> {
        if let Some(page) = node.pending {
            return Ok(*page);
        }
        let id = node.id.ok_or(CoreError::IdentityMismatch)?;
        let read = self.read(id, true)?;
        self.page_from_wire(id, read)
    }
    fn page_from_wire(&mut self, id: ObjectId, read: ReadPage<F::Key>) -> CoreResult<Page<F::Key>> {
        let mut page = self.page(read.wire.level)?;
        page.origin = Some(id);
        for (key, value) in read.wire.entries {
            if page.level == 0 {
                self.append_entry(
                    &mut page,
                    Entry {
                        bytes: F::width(&key) as u64,
                        key,
                        id: value,
                        count: 1,
                        size: 0,
                        pending: None,
                    },
                )?;
            } else {
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
                    },
                )?;
            }
        }
        let count = page.entries.iter().try_fold(0u64, |n, e| {
            n.checked_add(e.count).ok_or(CoreError::LengthOverflow)
        })?;
        let bytes = page.entries.iter().try_fold(0u64, |n, e| {
            n.checked_add(e.bytes).ok_or(CoreError::LengthOverflow)
        })?;
        if count != read.wire.count || bytes != read.wire.bytes {
            return Err(CoreError::InvalidRecord("batched tree subtree summary"));
        }
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
        if page.entries.len() == page.entries.capacity() {
            let before = page.entries.capacity();
            let capacity = (before.max(1) * 2).min(PAGE_ITEMS);
            let size = std::mem::size_of::<Entry<F::Key>>();
            page._lease.grow(capacity * size)?;
            if page.entries.try_reserve_exact(capacity - before).is_err() {
                page._lease.shrink(capacity * size);
                return Err(CoreError::ObjectLimitExceeded);
            }
            page._lease.shrink(before * size);
        }
        page._lease.grow(F::heap_bytes(&entry.key))?;
        page.entries.push(entry);
        Ok(())
    }
    fn push(
        &mut self,
        page: &mut Page<F::Key>,
        entry: Entry<F::Key>,
    ) -> CoreResult<Option<Node<F::Key>>> {
        if page.entries.len() == PAGE_ITEMS {
            return Err(CoreError::ObjectLimitExceeded);
        }
        // Keep the outer boundary children private. A neighbouring parent's
        // singleton underfull chain can still redistribute their entries.
        // Interior children cannot meet that chain after this final-state merge.
        if page.level > 0 && page.entries.len() > 1 {
            self.persist_entry(page.entries.last_mut().unwrap(), page.level)?;
        }
        self.append_entry(page, entry)?;
        let size = 44 + page.entries.iter().map(|e| F::width(&e.key)).sum::<usize>();
        if F::fits(size, page.entries.len()) {
            return Ok(None);
        }
        let mut right = self.page(page.level)?;
        let split =
            super::directory::nearest_half(page.entries.iter().map(|e| F::width(&e.key)).collect());
        for entry in page.entries.drain(split..) {
            self.append_entry(&mut right, entry)?;
        }
        page.origin = None;
        let left = std::mem::replace(page, right);
        self.node(left).map(Some)
    }
    #[allow(clippy::type_complexity)]
    fn merge(
        &mut self,
        left: Node<F::Key>,
        right: Node<F::Key>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        let mut left = self.materialize(left)?;
        let right = self.materialize(right)?;
        if left.level != right.level {
            return Err(CoreError::WrongLogicalRole);
        }
        left.origin = None;
        if left.level == 0 {
            for entry in right.entries {
                if let Some(node) = self.push(&mut left, entry)? {
                    output(self, node)?;
                }
            }
        } else {
            let mut page = self.page(left.level)?;
            let mut pending = None;
            let level = left.level - 1;
            for entry in left.entries.into_iter().chain(right.entries) {
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
    #[allow(clippy::type_complexity)]
    fn sibling(
        &mut self,
        pending: &mut Option<Node<F::Key>>,
        next: Node<F::Key>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key>) -> CoreResult<()>,
    ) -> CoreResult<()> {
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
    #[allow(clippy::type_complexity)]
    fn edit<I: Iterator<Item = CoreResult<(F::Key, Option<ObjectId>)>>>(
        &mut self,
        id: ObjectId,
        read: ReadPage<F::Key>,
        bound: Option<&F::Key>,
        deltas: &mut Deltas<I, F::Key>,
        output: &mut dyn FnMut(&mut Self, Node<F::Key>) -> CoreResult<()>,
    ) -> CoreResult<()> {
        if !deltas.in_range(bound) {
            return output(self, Node::existing(id, &read.wire));
        }
        let mut page = self.page(read.wire.level)?;
        page.origin = Some(id);
        if page.level == 0 {
            let mut old = read.wire.entries.into_iter().peekable();
            while old.peek().is_some() || deltas.in_range(bound) {
                let delta_first = deltas.in_range(bound)
                    && old
                        .peek()
                        .is_none_or(|old| old.0 >= deltas.next.as_ref().unwrap().0);
                let (key, value) = if delta_first {
                    let (key, value) = deltas.take()?;
                    self.counters.delta_keys += 1;
                    let existed = old.peek().is_some_and(|old| old.0 == key);
                    if existed {
                        old.next();
                    }
                    (key, value)
                } else {
                    let (key, value) = old.next().unwrap();
                    (key, Some(value))
                };
                if let Some(value) = value {
                    let entry = Entry {
                        bytes: F::width(&key) as u64,
                        key,
                        id: value,
                        count: 1,
                        size: 0,
                        pending: None,
                    };
                    if let Some(node) = self.push(&mut page, entry)? {
                        output(self, node)?;
                    }
                }
            }
        } else {
            let level = page.level;
            let count = read.wire.entries.len();
            let mut old_count = 0u64;
            let mut old_bytes = 0u64;
            let mut pending = None;
            for (index, (key, child_id)) in read.wire.entries.into_iter().enumerate() {
                let child = self.read(child_id, false)?;
                self.check_child(level, &key, &child.wire)?;
                old_count = old_count
                    .checked_add(child.wire.count)
                    .ok_or(CoreError::LengthOverflow)?;
                old_bytes = old_bytes
                    .checked_add(child.wire.bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                let child_bound = if index + 1 == count {
                    bound
                } else {
                    Some(&key)
                };
                self.edit(child_id, child, child_bound, deltas, &mut |engine, node| {
                    engine.sibling(&mut pending, node, &mut |engine, node| {
                        let entry = engine.entry(node)?;
                        if let Some(node) = engine.push(&mut page, entry)? {
                            output(engine, node)?;
                        }
                        Ok(())
                    })
                })?;
            }
            if old_count != read.wire.count || old_bytes != read.wire.bytes {
                return Err(CoreError::InvalidRecord("batched tree subtree summary"));
            }
            if let Some(node) = pending {
                let entry = self.entry(node)?;
                if let Some(node) = self.push(&mut page, entry)? {
                    output(self, node)?;
                }
            }
        }
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
    apply_budgeted::<S, F>(store, root, source, SORTED_TREE_UPDATE_SCRATCH_BYTES, None)
}
fn apply_budgeted<S: ObjectStore, F: Format>(
    store: &mut S,
    root: ObjectId,
    source: impl Iterator<Item = CoreResult<(F::Key, Option<ObjectId>)>>,
    scratch_limit: usize,
    expected: Option<(u8, u64)>,
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
        root_node = Engine::<S, F>::child(page.entries.into_iter().next().unwrap(), page.level - 1);
    }
    let root = engine
        .persist(root_node)?
        .id
        .ok_or(CoreError::IdentityMismatch)?;
    engine.counters.peak_scratch_bytes = engine.budget.peak.get();
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
            None => return Ok(()),
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
        let bytes = page.entries.iter().try_fold(0u64, |n, e| {
            n.checked_add(e.bytes).ok_or(CoreError::LengthOverflow)
        })?;
        let node = if page.level == 0 {
            DirectoryNodeV1::Leaf {
                subtree_encoded_bytes: bytes,
                entries: page
                    .entries
                    .iter()
                    .map(|e| (e.key.clone(), InodeId(e.id.to_bytes())))
                    .collect(),
            }
        } else {
            DirectoryNodeV1::Branch {
                level: page.level,
                subtree_entry_count: page.entries.iter().try_fold(0u64, |n, e| {
                    n.checked_add(e.count).ok_or(CoreError::LengthOverflow)
                })?,
                subtree_encoded_bytes: bytes,
                children: page.entries.iter().map(|e| (e.key.clone(), e.id)).collect(),
            }
        };
        encode_directory_node(&node)
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
        let entries = page.entries.iter().map(|e| (e.key, e.id)).collect();
        let node = if page.level == 0 {
            InodeTableNodeV1::Leaf(entries)
        } else {
            InodeTableNodeV1::Branch {
                level: page.level,
                subtree_entry_count: page.entries.iter().try_fold(0u64, |n, e| {
                    n.checked_add(e.count).ok_or(CoreError::LengthOverflow)
                })?,
                children: entries,
            }
        };
        encode_inode_table_node(&node)
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
    use super::directory::codec::{decode_directory_state, encode_directory_state};
    let mut state = store.with_authenticated_canonical(root.0, decode_directory_state)?;
    let (mapping, mut counters) = apply_budgeted::<S, Directory>(
        store,
        state.mapping_root,
        deltas.map(|v| v.map(|(k, v)| (k, v.map(|v| ObjectId::from_digest(v.0))))),
        scratch_limit,
        Some((state.tree_level, state.entry_count)),
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
    let (root, counters) = apply_budgeted::<S, Inodes>(store, root.0, deltas, scratch_limit, None)?;
    Ok((super::inode::InodeTableRoot(root), counters))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::build_initial_directory;
    use crate::tree::directory::{
        directory_entries, directory_lookup, empty_directory, NamespaceCounters,
    };
    use crate::tree::inode::{inode_table_entries, inode_table_upsert, InodeTableRoot};
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
    fn sorted_builds_match_existing_canonical_split_boundaries() {
        for count in [1, 127, 128, 169, 170, 171, 254, 255] {
            let entries = (0..count)
                .map(|index| (name(index), inode(index)))
                .collect::<Vec<_>>();
            let mut existing = MemoryStore::default();
            let expected = build_initial_directory(&mut existing, entries.iter().cloned()).unwrap();
            let mut sorted = MemoryStore::default();
            let empty = empty_directory(&mut sorted).unwrap();
            let actual = directory_apply_sorted(
                &mut sorted,
                empty,
                entries
                    .into_iter()
                    .map(|entry| Ok((entry.0, Some(entry.1)))),
            )
            .unwrap()
            .0;
            assert_eq!(actual, expected, "directory count={count}");
            assert_eq!(
                sorted.objects.get(&actual.0),
                existing.objects.get(&expected.0),
                "directory canonical bytes count={count}"
            );
        }

        for count in [63, 64, 65, 126, 127, 128, 129, 254, 255] {
            let leaf = encode_inode_table_node(&InodeTableNodeV1::Leaf(vec![(inode(0), value(0))]))
                .unwrap();
            let mut existing = MemoryStore::default();
            let mut expected = InodeTableRoot(existing.put(&leaf).unwrap());
            for index in 1..count {
                expected = inode_table_upsert(&mut existing, expected, inode(index), value(index))
                    .unwrap()
                    .0;
            }
            let mut sorted = MemoryStore::default();
            let initial = InodeTableRoot(sorted.put(&leaf).unwrap());
            let actual = inode_table_apply_sorted(
                &mut sorted,
                initial,
                (1..count).map(|index| Ok((inode(index), Some(value(index))))),
            )
            .unwrap()
            .0;
            assert_eq!(actual, expected, "inode count={count}");
            assert_eq!(
                sorted.objects.get(&actual.0),
                existing.objects.get(&expected.0),
                "inode canonical bytes count={count}"
            );
        }
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
        assert!(stats.nodes_read < 500, "{stats:?}");
        assert!(stats.nodes_created < 10, "{stats:?}");
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
}
