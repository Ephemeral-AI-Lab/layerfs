use super::codec::{decode_inode_table_node, InodeTableNodeV1};
use super::table::InodeTableDiff;
use super::{InodeId, InodeTableCounters, Summary, ValidatedNode};
use crate::file::rope::ObjectRead;
use crate::{CoreError, CoreResult, ObjectId};

pub(super) struct StreamingInodeDiff {
    base: StreamingInodeCursor,
    source: StreamingInodeCursor,
    base_entry: Option<(InodeId, ObjectId)>,
    source_entry: Option<(InodeId, ObjectId)>,
    initialized: bool,
}

impl StreamingInodeDiff {
    pub(super) fn new(base: ObjectId, source: ObjectId, root: bool) -> Self {
        Self {
            base: StreamingInodeCursor::new(base, root),
            source: StreamingInodeCursor::new(source, root),
            base_entry: None,
            source_entry: None,
            initialized: false,
        }
    }

    pub(super) fn next<S: ObjectRead>(
        &mut self,
        store: &S,
        counters: &mut InodeTableCounters,
    ) -> CoreResult<Option<InodeTableDiff>> {
        if !self.initialized {
            self.base_entry = self.base.next(store, counters)?;
            self.source_entry = self.source.next(store, counters)?;
            self.initialized = true;
        }
        loop {
            match (self.base_entry, self.source_entry) {
                (None, None) => return Ok(None),
                (Some((base_inode, before)), Some((source_inode, after)))
                    if base_inode == source_inode =>
                {
                    self.base_entry = self.base.next(store, counters)?;
                    self.source_entry = self.source.next(store, counters)?;
                    if before != after {
                        return Ok(Some(InodeTableDiff {
                            inode: base_inode,
                            before: Some(before),
                            after: Some(after),
                        }));
                    }
                }
                (Some((base_inode, before)), Some((source_inode, _)))
                    if base_inode < source_inode =>
                {
                    self.base_entry = self.base.next(store, counters)?;
                    return Ok(Some(InodeTableDiff {
                        inode: base_inode,
                        before: Some(before),
                        after: None,
                    }));
                }
                (Some(_), Some((source_inode, after))) => {
                    self.source_entry = self.source.next(store, counters)?;
                    return Ok(Some(InodeTableDiff {
                        inode: source_inode,
                        before: None,
                        after: Some(after),
                    }));
                }
                (Some((base_inode, before)), None) => {
                    self.base_entry = self.base.next(store, counters)?;
                    return Ok(Some(InodeTableDiff {
                        inode: base_inode,
                        before: Some(before),
                        after: None,
                    }));
                }
                (None, Some((source_inode, after))) => {
                    self.source_entry = self.source.next(store, counters)?;
                    return Ok(Some(InodeTableDiff {
                        inode: source_inode,
                        before: None,
                        after: Some(after),
                    }));
                }
            }
        }
    }
}

struct StreamingInodeCursor {
    stack: Vec<InodeWalkItem>,
    leaf: std::vec::IntoIter<(InodeId, ObjectId)>,
}

impl StreamingInodeCursor {
    fn new(root: ObjectId, root_context: bool) -> Self {
        Self {
            stack: vec![InodeWalkItem {
                id: root,
                root: root_context,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
        }
    }

    fn next<S: ObjectRead>(
        &mut self,
        store: &S,
        counters: &mut InodeTableCounters,
    ) -> CoreResult<Option<(InodeId, ObjectId)>> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Ok(Some(entry));
            }
            let Some(item) = self.stack.pop() else {
                return Ok(None);
            };
            let loaded = load_shallow(store, item.id, item.root, None, counters)?;
            if item
                .expected_level
                .is_some_and(|level| loaded.summary.level != level)
                || item
                    .expected_max
                    .is_some_and(|maximum| loaded.summary.max != maximum)
            {
                return Err(CoreError::InvalidRecord("inode child summary"));
            }
            match loaded.node {
                InodeTableNodeV1::Leaf(entries) => self.leaf = entries.into_iter(),
                InodeTableNodeV1::Branch {
                    level, children, ..
                } => {
                    let child_level = level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("inode child summary"))?;
                    self.stack
                        .extend(
                            children
                                .into_iter()
                                .rev()
                                .map(|(maximum, id)| InodeWalkItem {
                                    id,
                                    root: false,
                                    expected_level: Some(child_level),
                                    expected_max: Some(maximum),
                                }),
                        );
                }
            }
        }
    }
}

struct InodeWalkItem {
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<InodeId>,
}

pub(super) struct InodeEntryCursor<'a, S> {
    store: &'a S,
    stack: Vec<InodeWalkItem>,
    leaf: std::vec::IntoIter<(InodeId, ObjectId)>,
    counters: &'a mut InodeTableCounters,
}

impl<'a, S> InodeEntryCursor<'a, S> {
    pub(super) fn new(
        store: &'a S,
        root: ObjectId,
        root_context: bool,
        counters: &'a mut InodeTableCounters,
    ) -> Self {
        Self {
            store,
            stack: vec![InodeWalkItem {
                id: root,
                root: root_context,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
            counters,
        }
    }
}

impl<S: ObjectRead> Iterator for InodeEntryCursor<'_, S> {
    type Item = CoreResult<(InodeId, ObjectId)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Some(Ok(entry));
            }
            let item = self.stack.pop()?;
            let loaded = match load_shallow(self.store, item.id, item.root, None, self.counters) {
                Ok(loaded) => loaded,
                Err(error) => return Some(Err(error)),
            };
            if item
                .expected_level
                .is_some_and(|level| loaded.summary.level != level)
                || item
                    .expected_max
                    .is_some_and(|maximum| loaded.summary.max != maximum)
            {
                return Some(Err(CoreError::InvalidRecord("inode child summary")));
            }
            match loaded.node {
                InodeTableNodeV1::Leaf(entries) => self.leaf = entries.into_iter(),
                InodeTableNodeV1::Branch {
                    level, children, ..
                } => {
                    let Some(child_level) = level.checked_sub(1) else {
                        return Some(Err(CoreError::InvalidRecord("inode child summary")));
                    };
                    self.stack
                        .extend(
                            children
                                .into_iter()
                                .rev()
                                .map(|(maximum, id)| InodeWalkItem {
                                    id,
                                    root: false,
                                    expected_level: Some(child_level),
                                    expected_max: Some(maximum),
                                }),
                        );
                }
            }
        }
    }
}

pub(super) fn load_shallow<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected: Option<&Summary>,
    counters: &mut InodeTableCounters,
) -> CoreResult<ValidatedNode> {
    let node = load(store, id, counters)?;
    let summary = inode_node_shape(id, &node, root)?;
    if expected.is_some_and(|expected| {
        summary.max != expected.max
            || summary.entries != expected.entries
            || summary.level != expected.level
    }) {
        return Err(CoreError::InvalidRecord("inode child summary"));
    }
    Ok(ValidatedNode { node, summary })
}

pub(super) fn inode_node_shape(
    id: ObjectId,
    node: &InodeTableNodeV1,
    root: bool,
) -> CoreResult<Summary> {
    let (max, entries, level, count) = match node {
        InodeTableNodeV1::Leaf(entries) => (
            entries
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode table"))?
                .0,
            entries.len() as u64,
            0,
            entries.len(),
        ),
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            children,
        } => (
            children
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode branch"))?
                .0,
            *subtree_entry_count,
            *level,
            children.len(),
        ),
    };
    if (root && matches!(node, InodeTableNodeV1::Branch { .. }) && count < 2)
        || (!root && !(64..=127).contains(&count))
    {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    Ok(Summary {
        id,
        min: inode_node_min(node)?,
        max,
        entries,
        level,
    })
}

pub(super) fn inode_node_min(node: &InodeTableNodeV1) -> CoreResult<InodeId> {
    match node {
        InodeTableNodeV1::Leaf(entries) => entries
            .first()
            .map(|entry| entry.0)
            .ok_or(CoreError::InvalidRecord("empty inode table")),
        InodeTableNodeV1::Branch { children, .. } => children
            .first()
            .map(|entry| entry.0)
            .ok_or(CoreError::InvalidRecord("empty inode branch")),
    }
}

pub(super) fn load<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<InodeTableNodeV1> {
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    store.with_authenticated_canonical(id, decode_inode_table_node)
}
