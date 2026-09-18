//! Decoded pages, owned unfinished nodes and the engine's read paths.
//!
//! A stored page is read, authenticated, decoded and checked against its
//! root/non-root context. An unfinished page this operation built stays decoded
//! and private until the merge proves it final. Untouched subtrees stay as their
//! stored identity and their bytes are never read again, which is what makes a
//! localized update cost the changed path plus the siblings that prove the
//! partition, not the whole tree.

use std::marker::PhantomData;
use std::rc::Rc;

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::MAXIMUM_PAGE_BYTES;
use crate::filesystem::objects::FilesystemObjects;
use crate::filesystem::sorted::budget::{Budget, Lease};
use crate::filesystem::sorted::format::{
    node_header, Format, PageView, Wire, WireEntry, EMPTY_PAGE_BYTES,
};
use crate::object::{FinalizedObject, ObjectId, ObjectRole};

/// Largest work one operation may hold for its own unfinished pages.
///
/// The figure is the one ceiling the filesystem profile declares, not a second
/// copy of it: `limits` owns the number and every enforcement site names it.
pub const MAXIMUM_SCRATCH_BYTES: usize = crate::filesystem::limits::MAXIMUM_OPERATION_SCRATCH_BYTES;
/// Children read in one bounded authenticated batch.
///
/// The widest real demand is one branch page's children, which the directory
/// format bounds at 232 (1-byte names, `format.rs`), so 256 covers every legal
/// branch page in one wave. It is not the 4,096-id demand ceiling: a full-width
/// reserve of 256 x 8,280 B plus one decode slot is about 2.08 MiB against the
/// 4 MiB operation lease, while 4,096 x 8,280 B would be 33.9 MiB and the
/// narrowing loop below would clamp it on every call. A parent's batch is held
/// while the merge descends into its children, so a child's own batch narrows
/// instead of doubling the reservation.
pub const BATCH_CHILDREN: usize = 256;

/// One slot of the accumulating right spine: a private page, if one exists.
pub(crate) type PageSlot<K, V> = Option<Box<Page<K, V>>>;

/// One bounded group of children: how many were read, their canonical bytes and
/// the allocation lease retained while the caller descends into them.
pub(crate) type BatchChildren = (usize, Vec<(ObjectId, Vec<u8>)>, Option<Lease>);

/// Work one sorted tree operation performed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SortedWork {
    /// Pages read, including batched ones.
    pub pages_read: u64,
    /// Canonical read waves issued.
    pub read_waves: u64,
    /// Pages this operation created and emitted.
    pub pages_created: u64,
    /// Stored pages whose canonical bytes were reproduced exactly.
    pub pages_reused: u64,
    /// Final-state change keys consumed.
    pub change_keys: u64,
    /// Unchanged subtrees referenced by identity without being read.
    pub untouched_subtrees: u64,
    /// Largest simultaneous scratch reservation.
    pub peak_scratch_bytes: usize,
}

/// One decoded stored page.
pub(crate) struct ReadPage<K, V> {
    /// Decoded rows and recorded summaries.
    pub wire: Wire<K, V>,
    /// Retained decode scratch.
    pub _lease: Lease,
}

/// One entry of an unfinished page: a leaf value or a child page summary.
pub(crate) struct Entry<K, V> {
    /// Row key.
    pub key: K,
    /// Child page identity, present for a branch row.
    pub id: Option<ObjectId>,
    /// Typed leaf value, present for a leaf row.
    pub value: Option<V>,
    /// Entries in this child's subtree.
    pub count: u64,
    /// Encoded row bytes in this child's subtree.
    pub bytes: u64,
    /// Exact canonical width this child contributes to its parent.
    pub size: usize,
    /// Rows in this child.
    pub items: usize,
    /// This child's unfinished page, when it is private to this operation.
    pub pending: Option<Box<Page<K, V>>>,
}

/// One decoded unfinished page owned by this operation.
pub(crate) struct Page<K, V> {
    /// Page level.
    pub level: u8,
    /// Rows in key order.
    pub entries: Vec<Entry<K, V>>,
    /// Stored origin of this page, when it was materialized from one.
    pub origin: Option<ObjectId>,
    /// Retained page and row scratch.
    pub lease: Lease,
}

/// One page of the final tree, decoded or already stored.
pub(crate) struct Node<K, V> {
    /// Largest key in this subtree.
    pub max: Option<K>,
    /// Stored identity, when the page exists.
    pub id: Option<ObjectId>,
    /// Page level.
    pub level: u8,
    /// Entries in this subtree.
    pub count: u64,
    /// Encoded row bytes in this subtree.
    pub bytes: u64,
    /// Exact canonical width of this page.
    pub size: usize,
    /// Rows in this page.
    pub items: usize,
    /// This page's decoded rows, while still private.
    pub pending: Option<Box<Page<K, V>>>,
}

impl<K: Clone, V> Node<K, V> {
    /// Wraps one stored page summary.
    pub(crate) fn existing(id: ObjectId, wire: &Wire<K, V>) -> Self {
        Self {
            max: wire.entries.last().map(|entry| entry.key.clone()),
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

/// The bounded sorted B+tree engine over one page format.
pub(crate) struct Engine<'o, 'e, F: Format> {
    /// The operation's object boundary.
    pub objects: &'o mut FilesystemObjects<'e>,
    /// Observer of each original/final leaf value, for reference effects.
    pub observe: &'o mut dyn FnMut(Option<F::Value>, Option<F::Value>) -> ContentResult<()>,
    /// The operation's scratch budget.
    pub budget: Rc<Budget>,
    /// Work performed so far.
    pub work: SortedWork,
    /// The page format.
    pub format: PhantomData<F>,
}

#[allow(clippy::needless_lifetimes)]
impl<'o, 'e, F: Format> Engine<'o, 'e, F> {
    /// Builds an engine over the supplied boundary and observer.
    pub(crate) fn new(
        objects: &'o mut FilesystemObjects<'e>,
        observe: &'o mut dyn FnMut(Option<F::Value>, Option<F::Value>) -> ContentResult<()>,
        scratch_limit: usize,
    ) -> Self {
        Self {
            objects,
            observe,
            budget: Budget::new(scratch_limit),
            work: SortedWork::default(),
            format: PhantomData,
        }
    }

    /// An empty page at `level`, charged before it is allocated.
    pub(crate) fn page(&self, level: u8) -> ContentResult<Page<F::Key, F::Value>> {
        if level > crate::filesystem::limits::MAXIMUM_TREE_LEVEL {
            return Err(ContentError::MappingDepthExceeded);
        }
        let lease = self
            .budget
            .reserve(std::mem::size_of::<Page<F::Key, F::Value>>())?;
        Ok(Page {
            level,
            entries: Vec::new(),
            origin: None,
            lease,
        })
    }

    /// Reads, authenticates, decodes and context-checks one stored page.
    pub(crate) fn read(
        &mut self,
        id: ObjectId,
        root: bool,
    ) -> ContentResult<ReadPage<F::Key, F::Value>> {
        self.work.pages_read = self.work.pages_read.saturating_add(1);
        self.work.read_waves = self.work.read_waves.saturating_add(1);
        let canonical = self.objects.read(id)?;
        self.decode_page(root, &canonical)
    }

    /// Decodes one already fetched canonical page, charging the same scratch.
    pub(crate) fn decode_page(
        &mut self,
        root: bool,
        canonical: &[u8],
    ) -> ContentResult<ReadPage<F::Key, F::Value>> {
        if canonical.len() > MAXIMUM_PAGE_BYTES {
            return Err(ContentError::ObjectLimitExceeded {
                limit: MAXIMUM_PAGE_BYTES,
                actual: canonical.len(),
            });
        }
        let lease = self.budget.reserve(
            F::decode_scratch(canonical.len()) + std::mem::size_of::<Wire<F::Key, F::Value>>(),
        )?;
        let wire = F::decode(canonical)?;
        Self::check_page(root, &wire)?;
        Ok(ReadPage {
            wire,
            _lease: lease,
        })
    }

    /// Reads one bounded group of children as a single authenticated demand.
    ///
    /// The group is narrowed to the remaining budget when the full width does not
    /// fit, and the caller falls back to the ordinary point read only when not
    /// even one child fits. Every page is still read, authenticated, decoded and
    /// checked.
    pub(crate) fn batch_children(
        &mut self,
        entries: &[WireEntry<F::Key, F::Value>],
        width: usize,
    ) -> ContentResult<BatchChildren> {
        let associations =
            std::mem::size_of::<ObjectId>() + std::mem::size_of::<(ObjectId, Vec<u8>)>();
        let decode =
            F::decode_scratch(MAXIMUM_PAGE_BYTES) + std::mem::size_of::<Wire<F::Key, F::Value>>();
        for chunk in (1..=width).rev() {
            let reserved = chunk
                .checked_mul(MAXIMUM_PAGE_BYTES + associations)
                .and_then(|bytes| bytes.checked_add(decode))
                .ok_or(ContentError::LengthOverflow)?;
            let Ok(mut lease) = self.budget.reserve(reserved) else {
                continue;
            };
            let ids = entries[..chunk]
                .iter()
                .map(|entry| entry.child.ok_or(ContentError::InvalidRecord("branch row")))
                .collect::<ContentResult<Vec<_>>>()?;
            let fetched = self.objects.read_batch(&ids)?;
            let mut values = Vec::with_capacity(fetched.len());
            for (id, canonical) in ids.iter().zip(fetched) {
                values.push((*id, canonical));
            }
            let actual = values.capacity() * std::mem::size_of::<(ObjectId, Vec<u8>)>()
                + values
                    .iter()
                    .map(|(_, bytes)| bytes.capacity())
                    .sum::<usize>();
            if actual > reserved {
                lease.grow(actual - reserved)?;
            } else {
                lease.shrink(reserved - actual);
            }
            // One grouped demand is one wave and returns `chunk` pages: the
            // batched children are read, authenticated and decoded exactly like
            // a point read's single page, so they are charged like one.
            self.work.pages_read = self.work.pages_read.saturating_add(chunk as u64);
            self.work.read_waves = self.work.read_waves.saturating_add(1);
            return Ok((chunk, values, Some(lease)));
        }
        Ok((1, Vec::new(), None))
    }

    /// The canonical-context checks every read page must pass.
    pub(crate) fn check_page(root: bool, wire: &Wire<F::Key, F::Value>) -> ContentResult<()> {
        if (!root && !F::filled(wire.size, wire.entries.len(), wire.level))
            || (wire.level > 0 && wire.entries.len() < 2)
            || (!F::empty_allowed() && wire.entries.is_empty())
            || !F::fits(wire.size, wire.entries.len(), wire.level)
        {
            return Err(ContentError::NonCanonicalPagePartition);
        }
        Ok(())
    }

    /// Turns one decoded page into an unfinished node.
    pub(crate) fn node(
        &self,
        mut page: Page<F::Key, F::Value>,
    ) -> ContentResult<Node<F::Key, F::Value>> {
        page.lease.grow(
            page.entries
                .last()
                .map_or(0, |entry| F::heap_bytes(&entry.key)),
        )?;
        let count = page.entries.iter().try_fold(0_u64, |total, entry| {
            total
                .checked_add(entry.count)
                .ok_or(ContentError::LengthOverflow)
        })?;
        let bytes = page.entries.iter().try_fold(0_u64, |total, entry| {
            total
                .checked_add(entry.bytes)
                .ok_or(ContentError::LengthOverflow)
        })?;
        let size = EMPTY_PAGE_BYTES
            + page
                .entries
                .iter()
                .map(|entry| F::width(&entry.key, page.level))
                .sum::<usize>();
        Ok(Node {
            max: page.entries.last().map(|entry| entry.key.clone()),
            id: page.origin,
            level: page.level,
            count,
            bytes,
            size,
            items: page.entries.len(),
            pending: Some(Box::new(page)),
        })
    }

    /// True when a node satisfies the format's non-root fill rule.
    pub(crate) fn filled(node: &Node<F::Key, F::Value>) -> bool {
        F::filled(node.size, node.items, node.level)
    }

    /// Encodes and emits one final node, reusing its identity when unchanged.
    pub(crate) fn persist(
        &mut self,
        mut node: Node<F::Key, F::Value>,
    ) -> ContentResult<Node<F::Key, F::Value>> {
        let Some(mut page) = node.pending.take() else {
            self.work.pages_reused = self.work.pages_reused.saturating_add(1);
            return Ok(node);
        };
        for entry in &mut page.entries {
            self.persist_entry(entry, page.level)?;
        }
        let rows = page
            .entries
            .iter()
            .map(|entry| crate::filesystem::sorted::format::RowView {
                key: &entry.key,
                child: entry.id,
                value: entry.value.as_ref(),
            })
            .collect::<Vec<_>>();
        let _codec = self.budget.reserve(
            node.size * 2
                + page.entries.len() * std::mem::size_of::<(F::Key, Option<ObjectId>, F::Value)>()
                + page
                    .entries
                    .iter()
                    .map(|entry| F::heap_bytes(&entry.key))
                    .sum::<usize>(),
        )?;
        let canonical = F::encode(&PageView {
            level: page.level,
            count: node.count,
            bytes: node.bytes,
            rows: &rows,
        })?;
        let id = ObjectId::for_bytes(&canonical);
        if node.id == Some(id) {
            self.work.pages_reused = self.work.pages_reused.saturating_add(1);
        } else {
            let references = rows.iter().filter_map(|row| row.child).collect();
            let mut object = FinalizedObject::new(page_role::<F>(page.level), canonical)?
                .with_references(references);
            if let Some(origin) = page.origin {
                let mut predecessors = crate::object::AdvisoryPredecessors::new();
                let _ = predecessors.push(
                    origin,
                    crate::object::PredecessorProvenance::UnchangedPrefix,
                );
                object = object.with_predecessors(predecessors);
            }
            let emitted = self.objects.emit(object)?;
            debug_assert_eq!(emitted, id);
            node.id = Some(emitted);
            self.work.pages_created = self.work.pages_created.saturating_add(1);
        }
        Ok(node)
    }

    /// Publishes one private child page, or keeps a stored one as an identity.
    pub(crate) fn persist_entry(
        &mut self,
        entry: &mut Entry<F::Key, F::Value>,
        level: u8,
    ) -> ContentResult<()> {
        let Some(page) = entry.pending.take() else {
            return Ok(());
        };
        if level == 0 {
            return Err(ContentError::WrongLogicalRole);
        }
        let node = self.node(*page)?;
        if !Self::filled(&node) {
            return Err(ContentError::NonCanonicalPagePartition);
        }
        entry.id = self.persist(node)?.id;
        Ok(())
    }

    /// Reinterprets one entry as the child node it summarizes.
    pub(crate) fn child(entry: Entry<F::Key, F::Value>, level: u8) -> Node<F::Key, F::Value> {
        let items = entry
            .pending
            .as_ref()
            .map_or(entry.items, |page| page.entries.len());
        Node {
            max: Some(entry.key),
            id: entry.id,
            level,
            count: entry.count,
            bytes: entry.bytes,
            size: entry.size,
            items,
            pending: entry.pending,
        }
    }

    /// Turns one finalized node into the entry its parent holds.
    pub(crate) fn entry(
        &mut self,
        node: Node<F::Key, F::Value>,
    ) -> ContentResult<Entry<F::Key, F::Value>> {
        Ok(Entry {
            key: node.max.ok_or(ContentError::NonCanonicalPagePartition)?,
            id: node.id,
            value: None,
            count: node.count,
            bytes: node.bytes,
            size: node.size,
            items: node.items,
            pending: node.pending,
        })
    }

    /// Loads the decoded rows of a node, reading it when it is still stored.
    pub(crate) fn materialize(
        &mut self,
        node: Node<F::Key, F::Value>,
    ) -> ContentResult<Page<F::Key, F::Value>> {
        if let Some(page) = node.pending {
            return Ok(*page);
        }
        let id = node.id.ok_or(ContentError::IdentityMismatch)?;
        let read = self.read(id, true)?;
        self.page_from_wire(id, read)
    }

    /// Expands one decoded page into entries, reading each child summary.
    pub(crate) fn page_from_wire(
        &mut self,
        id: ObjectId,
        read: ReadPage<F::Key, F::Value>,
    ) -> ContentResult<Page<F::Key, F::Value>> {
        let mut page = self.page(read.wire.level)?;
        page.origin = Some(id);
        for entry in read.wire.entries {
            if page.level == 0 {
                self.append_entry(
                    &mut page,
                    Entry {
                        bytes: F::width(&entry.key, 0) as u64,
                        key: entry.key,
                        id: None,
                        value: entry.value,
                        count: 1,
                        size: 0,
                        items: 0,
                        pending: None,
                    },
                )?;
            } else {
                let child = entry
                    .child
                    .ok_or(ContentError::InvalidRecord("branch row"))?;
                let read = self.read(child, false)?;
                let key = entry.key.clone();
                self.check_child(page.level, &key, &read.wire)?;
                self.append_entry(
                    &mut page,
                    Entry {
                        bytes: read.wire.bytes,
                        key,
                        id: Some(child),
                        value: None,
                        count: read.wire.count,
                        size: read.wire.size,
                        items: read.wire.entries.len(),
                        pending: None,
                    },
                )?;
            }
        }
        let count = page.entries.iter().try_fold(0_u64, |total, entry| {
            total
                .checked_add(entry.count)
                .ok_or(ContentError::LengthOverflow)
        })?;
        let bytes = page.entries.iter().try_fold(0_u64, |total, entry| {
            total
                .checked_add(entry.bytes)
                .ok_or(ContentError::LengthOverflow)
        })?;
        if count != read.wire.count || bytes != read.wire.bytes {
            return Err(ContentError::InvalidRecord("tree subtree summary"));
        }
        Ok(page)
    }

    /// Checks one child summary against its parent's level and key.
    pub(crate) fn check_child(
        &self,
        level: u8,
        max: &F::Key,
        child: &Wire<F::Key, F::Value>,
    ) -> ContentResult<()> {
        if child.level.checked_add(1) != Some(level)
            || child.entries.last().map(|entry| &entry.key) != Some(max)
        {
            return Err(ContentError::InvalidRecord("tree child summary"));
        }
        Ok(())
    }

    /// Appends one row, charging the row storage before it is allocated.
    pub(crate) fn append_entry(
        &self,
        page: &mut Page<F::Key, F::Value>,
        entry: Entry<F::Key, F::Value>,
    ) -> ContentResult<()> {
        if page.entries.len() == page.entries.capacity() {
            let before = page.entries.capacity();
            let capacity = (before.max(1) * 2).min(F::page_items(page.level));
            let size = std::mem::size_of::<Entry<F::Key, F::Value>>();
            page.lease.grow(capacity * size)?;
            if page.entries.try_reserve_exact(capacity - before).is_err() {
                page.lease.shrink(capacity * size);
                return Err(ContentError::ObjectLimitExceeded {
                    limit: capacity,
                    actual: capacity + 1,
                });
            }
            page.lease.shrink(before * size);
        }
        page.lease.grow(F::heap_bytes(&entry.key))?;
        page.entries.push(entry);
        Ok(())
    }
}

/// Logical role of a finalized page at `level`.
fn page_role<F: Format>(level: u8) -> ObjectRole {
    match F::role(level) {
        crate::filesystem::sorted::format::DIRECTORY_LEAF_ROLE => ObjectRole::DirectoryLeaf,
        crate::filesystem::sorted::format::DIRECTORY_BRANCH_ROLE => ObjectRole::DirectoryBranch,
        crate::filesystem::sorted::format::INODE_LEAF_ROLE => ObjectRole::InodeLeaf,
        _ => ObjectRole::InodeBranch,
    }
}

/// Builds the canonical empty page of one format for a genuinely empty result.
pub(crate) fn empty_page<F: Format>(level: u8) -> ContentResult<Vec<u8>> {
    let value = node_header(F::magic(), F::role(level), level, 0, 0, 0)?;
    crate::filesystem::sorted::format::finish_node(value)
}
