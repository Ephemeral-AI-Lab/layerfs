use super::codec::{decode_inode_table_node, encode_inode_table_node, InodeTableNodeV1};
use super::cursor::{inode_node_min, inode_node_shape, load, load_shallow};
use super::{
    GeneratedInodeTable, InodeId, InodeKind, InodeRecordV1, InodeTableCounters, InodeTableRoot,
    Summary,
};
use crate::file::rope::{ObjectRead, ObjectStore};
use crate::{CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

pub fn inode_table_from_root<S: ObjectStore>(
    store: &mut S,
    root_inode: InodeId,
    record: ObjectId,
) -> CoreResult<InodeTableRoot> {
    let mut counters = InodeTableCounters::default();
    Ok(InodeTableRoot(
        emit(
            store,
            InodeTableNodeV1::Leaf(vec![(root_inode, record)]),
            &mut counters,
        )?
        .id,
    ))
}

pub fn inode_table_lookup<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    key: InodeId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<ObjectId>> {
    lookup_from(store, root.0, true, None, None, key, counters)
}

pub fn inode_table_lookup_many<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    keys: &[InodeId],
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<Option<ObjectId>>> {
    if keys.len() > 128 {
        return Err(CoreError::ObjectLimitExceeded);
    }
    struct Pending {
        slot: usize,
        key: InodeId,
        node: ObjectId,
        root: bool,
        expected_max: Option<InodeId>,
        expected_level: Option<u8>,
    }
    let mut pending = keys
        .iter()
        .copied()
        .enumerate()
        .map(|(slot, key)| Pending {
            slot,
            key,
            node: root.0,
            root: true,
            expected_max: None,
            expected_level: None,
        })
        .collect::<Vec<_>>();
    let mut output = vec![None; keys.len()];
    while !pending.is_empty() {
        let ids = pending
            .iter()
            .map(|pending| pending.node)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        counters.nodes_read = counters
            .nodes_read
            .checked_add(ids.len() as u64)
            .ok_or(CoreError::LengthOverflow)?;
        let mut nodes = BTreeMap::new();
        store.get_authenticated_batch(&ids, |id, payload| {
            nodes.insert(
                id,
                decode_inode_table_node(&crate::encode_bytes_object(payload)?)?,
            );
            Ok(())
        })?;
        if nodes.len() != ids.len() {
            return Err(CoreError::MissingObject);
        }
        let mut next = Vec::new();
        for lookup in pending {
            let node = nodes.get(&lookup.node).ok_or(CoreError::MissingObject)?;
            let summary = inode_node_shape(lookup.node, node, lookup.root)?;
            if lookup
                .expected_max
                .is_some_and(|expected| summary.max != expected)
                || lookup
                    .expected_level
                    .is_some_and(|expected| summary.level != expected)
            {
                return Err(CoreError::InvalidRecord("inode child summary"));
            }
            match node {
                InodeTableNodeV1::Leaf(entries) => {
                    output[lookup.slot] = leaf_lookup(entries, lookup.key);
                }
                InodeTableNodeV1::Branch {
                    level, children, ..
                } => {
                    let index = children
                        .partition_point(|entry| entry.0 < lookup.key)
                        .min(children.len() - 1);
                    next.push(Pending {
                        slot: lookup.slot,
                        key: lookup.key,
                        node: children[index].1,
                        root: false,
                        expected_max: Some(children[index].0),
                        expected_level: Some(
                            level
                                .checked_sub(1)
                                .ok_or(CoreError::InvalidRecord("inode child summary"))?,
                        ),
                    });
                }
            }
        }
        pending = next;
    }
    Ok(output)
}

pub(crate) fn inode_table_lookup_pair<S: ObjectRead>(
    store: &S,
    left: InodeTableRoot,
    right: InodeTableRoot,
    key: InodeId,
    counters: &mut InodeTableCounters,
) -> CoreResult<(Option<ObjectId>, Option<ObjectId>)> {
    if left == right {
        let value = inode_table_lookup(store, left, key, counters)?;
        return Ok((value, value));
    }
    let mut left = load_shallow(store, left.0, true, None, counters)?;
    let mut right = load_shallow(store, right.0, true, None, counters)?;
    loop {
        match (&left.node, &right.node) {
            (InodeTableNodeV1::Leaf(left), InodeTableNodeV1::Leaf(right)) => {
                return Ok((leaf_lookup(left, key), leaf_lookup(right, key)));
            }
            (
                InodeTableNodeV1::Branch {
                    level: left_level,
                    children: left_children,
                    ..
                },
                InodeTableNodeV1::Branch {
                    level: right_level,
                    children: right_children,
                    ..
                },
            ) => {
                let left_index = left_children
                    .partition_point(|entry| entry.0 < key)
                    .min(left_children.len() - 1);
                let right_index = right_children
                    .partition_point(|entry| entry.0 < key)
                    .min(right_children.len() - 1);
                let (left_max, left_id) = left_children[left_index];
                let (right_max, right_id) = right_children[right_index];
                if left_id == right_id {
                    if left_max != right_max || left_level != right_level {
                        return Err(CoreError::InvalidRecord("inode child summary"));
                    }
                    let child_level = left_level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("inode child summary"))?;
                    let value = lookup_from(
                        store,
                        left_id,
                        false,
                        Some(left_max),
                        Some(child_level),
                        key,
                        counters,
                    )?;
                    return Ok((value, value));
                }
                let next_left = load_shallow(store, left_id, false, None, counters)?;
                let next_right = load_shallow(store, right_id, false, None, counters)?;
                if next_left.summary.max != left_max
                    || next_left.summary.level.checked_add(1) != Some(*left_level)
                    || next_right.summary.max != right_max
                    || next_right.summary.level.checked_add(1) != Some(*right_level)
                {
                    return Err(CoreError::InvalidRecord("inode child summary"));
                }
                left = next_left;
                right = next_right;
            }
            _ => {
                return Ok((
                    inode_table_lookup(store, InodeTableRoot(left.summary.id), key, counters)?,
                    inode_table_lookup(store, InodeTableRoot(right.summary.id), key, counters)?,
                ));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lookup_from<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_max: Option<InodeId>,
    expected_level: Option<u8>,
    key: InodeId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<ObjectId>> {
    let mut current = load_shallow(store, id, root, None, counters)?;
    if expected_max.is_some_and(|expected| current.summary.max != expected)
        || expected_level.is_some_and(|expected| current.summary.level != expected)
    {
        return Err(CoreError::InvalidRecord("inode child summary"));
    }
    loop {
        match current.node {
            InodeTableNodeV1::Leaf(entries) => return Ok(leaf_lookup(&entries, key)),
            InodeTableNodeV1::Branch {
                level, children, ..
            } => {
                let index = children
                    .partition_point(|entry| entry.0 < key)
                    .min(children.len() - 1);
                let expected_max = children[index].0;
                let child = load_shallow(store, children[index].1, false, None, counters)?;
                if child.summary.max != expected_max
                    || child.summary.level.checked_add(1) != Some(level)
                {
                    return Err(CoreError::InvalidRecord("inode child summary"));
                }
                current = child;
            }
        }
    }
}

fn leaf_lookup(entries: &[(InodeId, ObjectId)], key: InodeId) -> Option<ObjectId> {
    entries
        .binary_search_by_key(&key, |entry| entry.0)
        .ok()
        .map(|index| entries[index].1)
}

pub fn inode_table_entries<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<(InodeId, ObjectId)>> {
    let mut output = Vec::new();
    visit_inode_table_entries(store, root, counters, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

pub fn visit_inode_table_entries<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    counters: &mut InodeTableCounters,
    mut visitor: impl FnMut(&[(InodeId, ObjectId)]) -> CoreResult<()>,
) -> CoreResult<()> {
    walk_inode_node(store, root.0, true, None, None, counters, &mut visitor).map(drop)
}

pub fn inode_table_upsert<S: ObjectStore>(
    store: &mut S,
    root: InodeTableRoot,
    key: InodeId,
    record: ObjectId,
) -> CoreResult<(InodeTableRoot, InodeTableCounters)> {
    let mut counters = InodeTableCounters::default();
    let summary = summary(store, root.0, &mut counters)?;
    upsert_validated(store, summary, key, record, counters)
}

pub fn inode_table_remove<S: ObjectStore>(
    store: &mut S,
    root: InodeTableRoot,
    key: InodeId,
) -> CoreResult<(InodeTableRoot, ObjectId, InodeTableCounters)> {
    let mut deferred = DeferredInodes::new(store);
    let mut counters = InodeTableCounters::default();
    let current = summary(&deferred, root.0, &mut counters)?;
    let (mut next, removed) = remove(&mut deferred, current, true, key, &mut counters)?;
    if let InodeTableNodeV1::Branch { children, .. } = load(&deferred, next.id, &mut counters)? {
        if children.len() == 1 {
            next = summary(&deferred, children[0].1, &mut counters)?;
        }
    }
    counters.nodes_created = deferred.commit(next.id)?;
    Ok((InodeTableRoot(next.id), removed, counters))
}

pub(crate) struct DeferredInodes<'a, S> {
    store: &'a mut S,
    nodes: BTreeMap<ObjectId, Vec<u8>>,
    charged_bytes: usize,
    peak_charged_bytes: usize,
    prunes: u64,
}

const DEFERRED_INODE_PRUNE_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const DEFERRED_INODE_MAX_BYTES: usize = 8 * 1024 * 1024 - 1;
const DEFERRED_OBJECT_CHARGE_BYTES: usize = 128;

impl<'a, S: ObjectStore> DeferredInodes<'a, S> {
    pub(crate) fn new(store: &'a mut S) -> Self {
        Self {
            store,
            nodes: BTreeMap::new(),
            charged_bytes: 0,
            peak_charged_bytes: 0,
            prunes: 0,
        }
    }

    pub(crate) fn prune_to(&mut self, root: ObjectId) -> CoreResult<()> {
        if self.charged_bytes <= DEFERRED_INODE_PRUNE_BYTES {
            return Ok(());
        }
        let mut reachable = BTreeSet::new();
        self.collect_node(root, &mut reachable)?;
        self.nodes.retain(|id, _| reachable.contains(id));
        self.charged_bytes = self.nodes.values().try_fold(0_usize, |total, bytes| {
            total
                .checked_add(object_charge(bytes.len())?)
                .ok_or(CoreError::LengthOverflow)
        })?;
        self.prunes = self
            .prunes
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        Ok(())
    }

    fn collect_node(&self, id: ObjectId, reachable: &mut BTreeSet<ObjectId>) -> CoreResult<()> {
        let Some(canonical) = self.nodes.get(&id) else {
            return Ok(());
        };
        if !reachable.insert(id) {
            return Ok(());
        }
        if let InodeTableNodeV1::Branch { children, .. } = decode_inode_table_node(canonical)? {
            for (_, child) in children {
                self.collect_node(child, reachable)?;
            }
        }
        Ok(())
    }

    pub(crate) fn peak_charged_bytes(&self) -> usize {
        self.peak_charged_bytes
    }

    pub(crate) fn prunes(&self) -> u64 {
        self.prunes
    }

    pub(crate) fn commit(&mut self, root: ObjectId) -> CoreResult<u64> {
        let mut committed = BTreeSet::new();
        self.commit_node(root, &mut committed)?;
        u64::try_from(committed.len()).map_err(|_| CoreError::LengthOverflow)
    }

    pub(crate) fn put_persistent(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        self.store.put(canonical)
    }

    fn commit_node(&mut self, id: ObjectId, committed: &mut BTreeSet<ObjectId>) -> CoreResult<()> {
        let Some(canonical) = self.nodes.get(&id).cloned() else {
            return Ok(());
        };
        if !committed.insert(id) {
            return Ok(());
        }
        if let InodeTableNodeV1::Branch { children, .. } = decode_inode_table_node(&canonical)? {
            for (_, child) in children {
                self.commit_node(child, committed)?;
            }
        }
        if self.store.put(&canonical)? != id {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(())
    }
}

impl<S: ObjectStore> ObjectStore for DeferredInodes<'_, S> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.nodes
            .get(&id)
            .cloned()
            .map_or_else(|| self.store.get(id), Ok)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        match self.nodes.get(&id) {
            Some(prior) if prior != canonical => return Err(CoreError::IdentityMismatch),
            Some(_) => return Ok(id),
            None => {}
        }
        let charged = object_charge(canonical.len())?;
        let next = self
            .charged_bytes
            .checked_add(charged)
            .ok_or(CoreError::LengthOverflow)?;
        if next > DEFERRED_INODE_MAX_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.nodes.insert(id, canonical.to_vec());
        self.charged_bytes = next;
        self.peak_charged_bytes = self.peak_charged_bytes.max(next);
        Ok(id)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        match self.nodes.get(&id) {
            Some(bytes) if ObjectId::for_bytes(bytes) == id => callback(bytes),
            Some(_) => Err(CoreError::IdentityMismatch),
            None => self.store.with_authenticated_canonical(id, callback),
        }
    }
}

fn object_charge(canonical_bytes: usize) -> CoreResult<usize> {
    canonical_bytes
        .checked_add(DEFERRED_OBJECT_CHARGE_BYTES)
        .ok_or(CoreError::LengthOverflow)
}

pub fn generated_inode_table_from_root<S: ObjectStore>(
    store: &mut S,
    root_inode: InodeId,
    record: ObjectId,
) -> CoreResult<GeneratedInodeTable> {
    inode_table_from_root(store, root_inode, record).map(GeneratedInodeTable)
}

pub fn generated_inode_table_upsert<S: ObjectStore>(
    store: &mut S,
    root: GeneratedInodeTable,
    key: InodeId,
    record: ObjectId,
) -> CoreResult<(GeneratedInodeTable, InodeTableCounters)> {
    let mut counters = InodeTableCounters::default();
    let summary = summary(store, root.0 .0, &mut counters)?;
    upsert_validated(store, summary, key, record, counters)
        .map(|(root, counters)| (GeneratedInodeTable(root), counters))
}

pub(crate) fn inode_table_apply_insertions<S: ObjectStore>(
    store: &mut S,
    initial: Vec<(InodeId, ObjectId)>,
    insertions: impl IntoIterator<Item = (InodeId, ObjectId)>,
    counters: &mut InodeTableCounters,
) -> CoreResult<InodeTableRoot> {
    inode_table_apply_insertions_fallible(store, initial, insertions.into_iter().map(Ok), counters)
        .map(|result| result.0)
}

#[doc(hidden)]
pub fn build_initial_inode_table_from_pairs<S: ObjectStore>(
    store: &mut S,
    root_inode: InodeId,
    pairs: impl IntoIterator<Item = CoreResult<(InodeId, ObjectId)>>,
) -> CoreResult<(InodeTableRoot, u64, u64)> {
    let mut pairs = pairs.into_iter();
    let root = pairs
        .next()
        .transpose()?
        .filter(|(inode, _)| *inode == root_inode)
        .ok_or(CoreError::InvalidRecord("initial root inode"))?;
    inode_table_apply_insertions_fallible(
        store,
        vec![root],
        pairs,
        &mut InodeTableCounters::default(),
    )
}

fn inode_table_apply_insertions_fallible<S: ObjectStore>(
    store: &mut S,
    initial: Vec<(InodeId, ObjectId)>,
    insertions: impl IntoIterator<Item = CoreResult<(InodeId, ObjectId)>>,
    counters: &mut InodeTableCounters,
) -> CoreResult<(InodeTableRoot, u64, u64)> {
    if initial.is_empty() || initial.len() > 127 {
        return Err(CoreError::InvalidRecord("initial inode table"));
    }
    let mut root = InsertNode::Leaf(initial);
    for insertion in insertions {
        let (inode, record) = insertion?;
        if let Some(right) = root.insert(inode, record) {
            let level = root
                .level()
                .checked_add(1)
                .ok_or(CoreError::MappingDepthExceeded)?;
            root = InsertNode::Branch {
                level,
                children: vec![root, right],
            };
        }
    }
    let (len_bytes, capacity_bytes) = root.allocation_bytes()?;
    emit_insert_node(store, root, counters)
        .map(|summary| (InodeTableRoot(summary.id), len_bytes, capacity_bytes))
}

enum InsertNode {
    Leaf(Vec<(InodeId, ObjectId)>),
    Branch {
        level: u8,
        children: Vec<InsertNode>,
    },
}

impl InsertNode {
    fn allocation_bytes(&self) -> CoreResult<(u64, u64)> {
        match self {
            Self::Leaf(entries) => Ok((
                u64::try_from(entries.len())
                    .map_err(|_| CoreError::LengthOverflow)?
                    .checked_mul(std::mem::size_of::<(InodeId, ObjectId)>() as u64)
                    .ok_or(CoreError::LengthOverflow)?,
                u64::try_from(entries.capacity())
                    .map_err(|_| CoreError::LengthOverflow)?
                    .checked_mul(std::mem::size_of::<(InodeId, ObjectId)>() as u64)
                    .ok_or(CoreError::LengthOverflow)?,
            )),
            Self::Branch { children, .. } => children.iter().try_fold(
                (
                    u64::try_from(children.len())
                        .map_err(|_| CoreError::LengthOverflow)?
                        .checked_mul(std::mem::size_of::<Self>() as u64)
                        .ok_or(CoreError::LengthOverflow)?,
                    u64::try_from(children.capacity())
                        .map_err(|_| CoreError::LengthOverflow)?
                        .checked_mul(std::mem::size_of::<Self>() as u64)
                        .ok_or(CoreError::LengthOverflow)?,
                ),
                |(len, capacity), child| {
                    let child = child.allocation_bytes()?;
                    Ok((
                        len.checked_add(child.0).ok_or(CoreError::LengthOverflow)?,
                        capacity
                            .checked_add(child.1)
                            .ok_or(CoreError::LengthOverflow)?,
                    ))
                },
            ),
        }
    }

    fn level(&self) -> u8 {
        match self {
            Self::Leaf(_) => 0,
            Self::Branch { level, .. } => *level,
        }
    }

    fn max(&self) -> CoreResult<InodeId> {
        match self {
            Self::Leaf(entries) => entries.last().map(|entry| entry.0),
            Self::Branch { children, .. } => children.last().map(InsertNode::max).transpose()?,
        }
        .ok_or(CoreError::InvalidRecord("empty inode table"))
    }

    fn insert(&mut self, inode: InodeId, record: ObjectId) -> Option<Self> {
        match self {
            Self::Leaf(entries) => {
                match entries.binary_search_by_key(&inode, |entry| entry.0) {
                    Ok(index) => entries[index].1 = record,
                    Err(index) => entries.insert(index, (inode, record)),
                }
                (entries.len() > 127).then(|| Self::Leaf(entries.split_off(64)))
            }
            Self::Branch { level, children } => {
                let index = children
                    .partition_point(|child| child.max().is_ok_and(|max| max < inode))
                    .min(children.len() - 1);
                if let Some(right) = children[index].insert(inode, record) {
                    children.insert(index + 1, right);
                }
                (children.len() > 127).then(|| Self::Branch {
                    level: *level,
                    children: children.split_off(64),
                })
            }
        }
    }
}

fn emit_insert_node<S: ObjectStore>(
    store: &mut S,
    node: InsertNode,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    match node {
        InsertNode::Leaf(entries) => emit(store, InodeTableNodeV1::Leaf(entries), counters),
        InsertNode::Branch { level, children } => {
            let children = children
                .into_iter()
                .map(|child| emit_insert_node(store, child, counters))
                .collect::<CoreResult<Vec<_>>>()?;
            emit_branch(store, level, children, counters)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTableDiff {
    pub inode: InodeId,
    pub before: Option<ObjectId>,
    pub after: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTableReconcileConflict {
    pub inode: InodeId,
    pub source: Option<ObjectId>,
    pub destination: Option<ObjectId>,
}

pub fn reconcile_inode_tables<S: ObjectStore>(
    store: &mut S,
    base: InodeTableRoot,
    source: InodeTableRoot,
    destination: InodeTableRoot,
) -> CoreResult<
    std::result::Result<
        (
            InodeTableRoot,
            InodeTableCounters,
            crate::tree::directory::NamespaceCounters,
        ),
        InodeTableReconcileConflict,
    >,
> {
    let mut counters = InodeTableCounters::default();
    let mut namespace = crate::tree::directory::NamespaceCounters::default();
    if source == base || source == destination {
        return Ok(Ok((destination, counters, namespace)));
    }
    if destination == base {
        return Ok(Ok((source, counters, namespace)));
    }
    let mut reconciled = destination;
    if let Some(conflict) = reconcile_inode_node_diffs(
        store,
        base.0,
        source.0,
        true,
        &mut reconciled,
        &mut counters,
        &mut namespace,
    )? {
        return Ok(Err(conflict));
    }
    Ok(Ok((reconciled, counters, namespace)))
}

fn reconcile_inode_node_diffs<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    root: bool,
    reconciled: &mut InodeTableRoot,
    counters: &mut InodeTableCounters,
    namespace: &mut crate::tree::directory::NamespaceCounters,
) -> CoreResult<Option<InodeTableReconcileConflict>> {
    use super::codec::InodeTableNodeV1;
    use super::cursor::{load_shallow, StreamingInodeDiff};
    if base == source {
        return Ok(None);
    }
    let base_node = load_shallow(store, base, root, None, counters)?;
    let source_node = load_shallow(store, source, root, None, counters)?;
    match (base_node.node, source_node.node) {
        (InodeTableNodeV1::Leaf(base), InodeTableNodeV1::Leaf(source)) => {
            let mut conflict = None;
            merge_inode_entries(
                base.into_iter().map(Ok),
                source.into_iter().map(Ok),
                &mut |change| {
                    if conflict.is_none() {
                        conflict = apply_inode_table_change(
                            store, reconciled, change, counters, namespace,
                        )?;
                    }
                    Ok(())
                },
            )?;
            Ok(conflict)
        }
        (
            InodeTableNodeV1::Branch {
                level: base_level,
                children: base_children,
                ..
            },
            InodeTableNodeV1::Branch {
                level: source_level,
                children: source_children,
                ..
            },
        ) if base_level == source_level
            && base_children.len() == source_children.len()
            && base_children
                .iter()
                .zip(&source_children)
                .all(|(base, source)| base.0 == source.0) =>
        {
            for ((_, base_child), (_, source_child)) in
                base_children.into_iter().zip(source_children)
            {
                if let Some(conflict) = reconcile_inode_node_diffs(
                    store,
                    base_child,
                    source_child,
                    false,
                    reconciled,
                    counters,
                    namespace,
                )? {
                    return Ok(Some(conflict));
                }
            }
            Ok(None)
        }
        _ => {
            let mut diffs = StreamingInodeDiff::new(base, source, root);
            while let Some(change) = diffs.next(store, counters)? {
                if let Some(conflict) =
                    apply_inode_table_change(store, reconciled, change, counters, namespace)?
                {
                    return Ok(Some(conflict));
                }
            }
            Ok(None)
        }
    }
}

fn apply_inode_table_change<S: ObjectStore>(
    store: &mut S,
    reconciled: &mut InodeTableRoot,
    change: InodeTableDiff,
    counters: &mut InodeTableCounters,
    namespace: &mut crate::tree::directory::NamespaceCounters,
) -> CoreResult<Option<InodeTableReconcileConflict>> {
    let mut lookup = InodeTableCounters::default();
    let destination = inode_table_lookup(store, *reconciled, change.inode, &mut lookup)?;
    add_inode_counters(counters, lookup)?;
    let selected = if destination == change.before {
        change.after
    } else if destination == change.after {
        if concurrent_namespace_identity_change(store, change.before, change.after)? {
            return Ok(Some(InodeTableReconcileConflict {
                inode: change.inode,
                source: change.after,
                destination,
            }));
        }
        destination
    } else if let (Some(base), Some(source), Some(destination)) =
        (change.before, change.after, destination)
    {
        match reconcile_inode_records(store, base, source, destination, namespace)? {
            Some(record) => Some(record),
            None => {
                return Ok(Some(InodeTableReconcileConflict {
                    inode: change.inode,
                    source: change.after,
                    destination: Some(destination),
                }));
            }
        }
    } else {
        return Ok(Some(InodeTableReconcileConflict {
            inode: change.inode,
            source: change.after,
            destination,
        }));
    };
    let Some(selected) = selected else {
        if destination.is_none() {
            return Ok(None);
        }
        let (next, _, changed) = inode_table_remove(store, *reconciled, change.inode)?;
        add_inode_counters(counters, changed)?;
        *reconciled = next;
        return Ok(None);
    };
    if Some(selected) == destination {
        return Ok(None);
    }
    let (next, changed) = inode_table_upsert(store, *reconciled, change.inode, selected)?;
    add_inode_counters(counters, changed)?;
    *reconciled = next;
    Ok(None)
}

fn concurrent_namespace_identity_change<S: ObjectStore>(
    store: &S,
    before: Option<ObjectId>,
    after: Option<ObjectId>,
) -> CoreResult<bool> {
    use super::codec::decode_inode_record;
    match (before, after) {
        (None, Some(_)) => Ok(true),
        (Some(before), Some(after)) => {
            let before = store.with_authenticated_canonical(before, decode_inode_record)?;
            let after = store.with_authenticated_canonical(after, decode_inode_record)?;
            Ok(before.namespace_ref_count != after.namespace_ref_count)
        }
        _ => Ok(false),
    }
}

fn reconcile_inode_records<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    destination: ObjectId,
    namespace: &mut crate::tree::directory::NamespaceCounters,
) -> CoreResult<Option<ObjectId>> {
    use super::codec::{decode_inode_record, encode_inode_record};
    use crate::tree::directory::{reconcile_directory_roots, DirectoryStateRoot};
    use crate::tree::metadata::reconcile_metadata_roots;
    let base = store.with_authenticated_canonical(base, decode_inode_record)?;
    let source = store.with_authenticated_canonical(source, decode_inode_record)?;
    let destination = store.with_authenticated_canonical(destination, decode_inode_record)?;
    let Some(kind) = reconcile_field(base.kind, source.kind, destination.kind) else {
        return Ok(None);
    };
    let Some(namespace_ref_count) = reconcile_namespace_ref_count(
        base.namespace_ref_count,
        source.namespace_ref_count,
        destination.namespace_ref_count,
    ) else {
        return Ok(None);
    };
    let content_root = match reconcile_field(
        base.content_root,
        source.content_root,
        destination.content_root,
    ) {
        Some(root) => root,
        None if [base.kind, source.kind, destination.kind]
            .into_iter()
            .all(|kind| kind == InodeKind::Directory) =>
        {
            let Some((root, counters)) = reconcile_directory_roots(
                store,
                DirectoryStateRoot(base.content_root),
                DirectoryStateRoot(source.content_root),
                DirectoryStateRoot(destination.content_root),
            )?
            else {
                return Ok(None);
            };
            add_namespace_counters(namespace, counters)?;
            root.0
        }
        None => return Ok(None),
    };
    let metadata_root = match reconcile_field(
        base.metadata_root,
        source.metadata_root,
        destination.metadata_root,
    ) {
        Some(root) => root,
        None => {
            let Some(root) = reconcile_metadata_roots(
                store,
                base.metadata_root,
                source.metadata_root,
                destination.metadata_root,
            )?
            else {
                return Ok(None);
            };
            root
        }
    };
    if namespace_ref_count == 0 && kind != InodeKind::Directory
        || namespace_ref_count != 1 && kind == InodeKind::Symlink
        || namespace_ref_count == 0 && kind == InodeKind::RegularFile
    {
        return Ok(None);
    }
    store
        .put(&encode_inode_record(InodeRecordV1 {
            kind,
            namespace_ref_count,
            content_root,
            metadata_root,
        })?)
        .map(Some)
}

fn add_namespace_counters(
    target: &mut crate::tree::directory::NamespaceCounters,
    source: crate::tree::directory::NamespaceCounters,
) -> CoreResult<()> {
    target.nodes_read = target
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    target.nodes_created = target
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn reconcile_field<T: Copy + Eq>(base: T, source: T, destination: T) -> Option<T> {
    if source == base || source == destination {
        Some(destination)
    } else if destination == base {
        Some(source)
    } else {
        None
    }
}

fn reconcile_namespace_ref_count(base: u64, source: u64, destination: u64) -> Option<u64> {
    if source == base {
        Some(destination)
    } else if destination == base {
        Some(source)
    } else {
        None
    }
}

fn add_inode_counters(
    target: &mut InodeTableCounters,
    source: InodeTableCounters,
) -> CoreResult<()> {
    target.nodes_read = target
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    target.nodes_created = target
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

pub fn diff_inode_table_entries<S: ObjectRead>(
    store: &S,
    old: InodeTableRoot,
    new: InodeTableRoot,
    mut visitor: impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<InodeTableCounters> {
    let mut counters = InodeTableCounters::default();
    if old == new {
        return Ok(counters);
    }
    diff_inode_nodes(store, old.0, new.0, true, &mut counters, &mut visitor)?;
    Ok(counters)
}

fn diff_inode_nodes<S: ObjectRead>(
    store: &S,
    old: ObjectId,
    new: ObjectId,
    root: bool,
    counters: &mut InodeTableCounters,
    visitor: &mut impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    use super::codec::InodeTableNodeV1;
    use super::cursor::{load_shallow, InodeEntryCursor};
    if old == new {
        return Ok(());
    }
    let old_node = load_shallow(store, old, root, None, counters)?;
    let new_node = load_shallow(store, new, root, None, counters)?;
    match (&old_node.node, &new_node.node) {
        (InodeTableNodeV1::Leaf(old), InodeTableNodeV1::Leaf(new)) => merge_inode_entries(
            old.iter().copied().map(Ok),
            new.iter().copied().map(Ok),
            visitor,
        ),
        (
            InodeTableNodeV1::Branch {
                level: old_level,
                children: old_children,
                ..
            },
            InodeTableNodeV1::Branch {
                level: new_level,
                children: new_children,
                ..
            },
        ) if old_level == new_level
            && old_children.len() == new_children.len()
            && old_children
                .iter()
                .zip(new_children)
                .all(|(old, new)| old.0 == new.0) =>
        {
            for ((_, old_child), (_, new_child)) in old_children.iter().zip(new_children) {
                diff_inode_nodes(store, *old_child, *new_child, false, counters, visitor)?;
            }
            Ok(())
        }
        _ => {
            let mut old_counters = InodeTableCounters::default();
            let mut new_counters = InodeTableCounters::default();
            let result = merge_inode_entries(
                InodeEntryCursor::new(store, old, root, &mut old_counters),
                InodeEntryCursor::new(store, new, root, &mut new_counters),
                visitor,
            );
            counters.nodes_read = counters
                .nodes_read
                .checked_add(old_counters.nodes_read)
                .and_then(|value| value.checked_add(new_counters.nodes_read))
                .ok_or(CoreError::LengthOverflow)?;
            result
        }
    }
}

fn merge_inode_entries(
    old: impl Iterator<Item = CoreResult<(InodeId, ObjectId)>>,
    new: impl Iterator<Item = CoreResult<(InodeId, ObjectId)>>,
    visitor: &mut impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    let mut old = old;
    let mut new = new;
    let mut old_entry = old.next().transpose()?;
    let mut new_entry = new.next().transpose()?;
    loop {
        match (old_entry, new_entry) {
            (None, None) => return Ok(()),
            (Some((old_key, before)), Some((new_key, after))) if old_key == new_key => {
                if before != after {
                    visitor(InodeTableDiff {
                        inode: old_key,
                        before: Some(before),
                        after: Some(after),
                    })?;
                }
                old_entry = old.next().transpose()?;
                new_entry = new.next().transpose()?;
            }
            (Some((old_key, before)), Some((new_key, after))) if old_key < new_key => {
                visitor(InodeTableDiff {
                    inode: old_key,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
                new_entry = Some((new_key, after));
            }
            (Some((old_key, before)), Some((new_key, after))) => {
                visitor(InodeTableDiff {
                    inode: new_key,
                    before: None,
                    after: Some(after),
                })?;
                old_entry = Some((old_key, before));
                new_entry = new.next().transpose()?;
            }
            (Some((old_key, before)), None) => {
                visitor(InodeTableDiff {
                    inode: old_key,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
            }
            (None, Some((new_key, after))) => {
                visitor(InodeTableDiff {
                    inode: new_key,
                    before: None,
                    after: Some(after),
                })?;
                new_entry = new.next().transpose()?;
            }
        }
    }
}

fn upsert_validated<S: ObjectStore>(
    store: &mut S,
    summary: Summary,
    key: InodeId,
    record: ObjectId,
    mut counters: InodeTableCounters,
) -> CoreResult<(InodeTableRoot, InodeTableCounters)> {
    let mut nodes = upsert(store, summary, true, key, record, &mut counters)?;
    let root = if nodes.len() == 1 {
        nodes.remove(0)
    } else {
        emit_branch(
            store,
            nodes[0]
                .level
                .checked_add(1)
                .ok_or(CoreError::MappingDepthExceeded)?,
            nodes,
            &mut counters,
        )?
    };
    Ok((InodeTableRoot(root.id), counters))
}

fn upsert<S: ObjectStore>(
    store: &mut S,
    current: Summary,
    root: bool,
    key: InodeId,
    record: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<Summary>> {
    let loaded = load_shallow(store, current.id, root, Some(&current), counters)?;
    match loaded.node {
        InodeTableNodeV1::Leaf(mut entries) => {
            match entries.binary_search_by_key(&key, |entry| entry.0) {
                Ok(index) => entries[index].1 = record,
                Err(index) => entries.insert(index, (key, record)),
            }
            if entries.len() <= 127 {
                Ok(vec![emit(
                    store,
                    InodeTableNodeV1::Leaf(entries),
                    counters,
                )?])
            } else {
                Ok(vec![
                    emit(
                        store,
                        InodeTableNodeV1::Leaf(entries[..64].to_vec()),
                        counters,
                    )?,
                    emit(
                        store,
                        InodeTableNodeV1::Leaf(entries[64..].to_vec()),
                        counters,
                    )?,
                ])
            }
        }
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            mut children,
        } => {
            let index = children
                .partition_point(|entry| entry.0 < key)
                .min(children.len() - 1);
            let old = load_shallow(store, children[index].1, false, None, counters)?.summary;
            if old.max != children[index].0 || old.level.checked_add(1) != Some(level) {
                return Err(CoreError::InvalidRecord("inode child summary"));
            }
            let replacements = upsert(store, old, false, key, record, counters)?;
            let replacement_count = replacements.iter().try_fold(0_u64, |sum, item| {
                sum.checked_add(item.entries)
                    .ok_or(CoreError::LengthOverflow)
            })?;
            children.splice(
                index..=index,
                replacements.iter().map(|item| (item.max, item.id)),
            );
            let count = subtree_entry_count
                .checked_sub(old.entries)
                .and_then(|count| count.checked_add(replacement_count))
                .ok_or(CoreError::LengthOverflow)?;
            if children.len() <= 127 {
                Ok(vec![emit(
                    store,
                    InodeTableNodeV1::Branch {
                        level,
                        subtree_entry_count: count,
                        children,
                    },
                    counters,
                )?])
            } else {
                let left = children[..64].to_vec();
                let right = children[64..].to_vec();
                Ok(vec![
                    emit_branch_descriptors(store, level, left, counters)?,
                    emit_branch_descriptors(store, level, right, counters)?,
                ])
            }
        }
    }
}

fn remove<S: ObjectStore>(
    store: &mut S,
    current: Summary,
    root: bool,
    key: InodeId,
    counters: &mut InodeTableCounters,
) -> CoreResult<(Summary, ObjectId)> {
    let loaded = load_shallow(store, current.id, root, Some(&current), counters)?;
    match loaded.node {
        InodeTableNodeV1::Leaf(mut entries) => {
            let index = entries
                .binary_search_by_key(&key, |entry| entry.0)
                .map_err(|_| CoreError::PathNotFound)?;
            let removed = entries.remove(index).1;
            if entries.is_empty() {
                return Err(CoreError::InvalidRecord("empty inode table"));
            }
            Ok((
                emit(store, InodeTableNodeV1::Leaf(entries), counters)?,
                removed,
            ))
        }
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            mut children,
        } => {
            let index = children
                .partition_point(|entry| entry.0 < key)
                .min(children.len() - 1);
            let old = load_shallow(store, children[index].1, false, None, counters)?.summary;
            if old.max != children[index].0 || old.level.checked_add(1) != Some(level) {
                return Err(CoreError::InvalidRecord("inode child summary"));
            }
            let (next, removed) = remove(store, old, false, key, counters)?;
            children[index] = (next.max, next.id);
            if children.len() > 1 && inode_underfull(store, next.id, counters)? {
                children = rebalance_inode_children(store, level - 1, children, index, counters)?;
            }
            let count = subtree_entry_count
                .checked_sub(1)
                .ok_or(CoreError::LengthOverflow)?;
            Ok((
                emit(
                    store,
                    InodeTableNodeV1::Branch {
                        level,
                        subtree_entry_count: count,
                        children,
                    },
                    counters,
                )?,
                removed,
            ))
        }
    }
}

fn inode_underfull<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<bool> {
    Ok(match load(store, id, counters)? {
        InodeTableNodeV1::Leaf(entries) => entries.len() < 64,
        InodeTableNodeV1::Branch { children, .. } => children.len() < 64,
    })
}

fn rebalance_inode_children<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    children: Vec<(InodeId, ObjectId)>,
    index: usize,
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<(InodeId, ObjectId)>> {
    if index > 0 {
        if let Some(replacements) = try_inode_borrow(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            true,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index - 1, replacements));
        }
    }
    if index + 1 < children.len() {
        if let Some(replacements) = try_inode_borrow(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            false,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index, replacements));
        }
    }
    if index > 0 {
        if let Some(replacement) = try_inode_merge(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index - 1, vec![replacement]));
        }
    }
    if index + 1 < children.len() {
        if let Some(replacement) = try_inode_merge(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index, vec![replacement]));
        }
    }
    Err(CoreError::NonCanonicalPagePartition)
}

fn try_inode_borrow<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    borrow_left: bool,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<Vec<Summary>>> {
    let (left, right) = match (
        load(store, left_id, counters)?,
        load(store, right_id, counters)?,
    ) {
        (InodeTableNodeV1::Leaf(mut left), InodeTableNodeV1::Leaf(mut right)) => {
            if borrow_left {
                if left.len() <= 64 {
                    return Ok(None);
                }
                right.insert(0, left.pop().unwrap());
            } else {
                if right.len() <= 64 {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            (InodeTableNodeV1::Leaf(left), InodeTableNodeV1::Leaf(right))
        }
        (
            InodeTableNodeV1::Branch {
                children: mut left, ..
            },
            InodeTableNodeV1::Branch {
                children: mut right,
                ..
            },
        ) => {
            if borrow_left {
                if left.len() <= 64 {
                    return Ok(None);
                }
                right.insert(0, left.pop().unwrap());
            } else {
                if right.len() <= 64 {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            (
                InodeTableNodeV1::Branch {
                    level: child_level,
                    subtree_entry_count: inode_children_count(store, &left, counters)?,
                    children: left,
                },
                InodeTableNodeV1::Branch {
                    level: child_level,
                    subtree_entry_count: inode_children_count(store, &right, counters)?,
                    children: right,
                },
            )
        }
        _ => return Err(CoreError::WrongLogicalRole),
    };
    Ok(Some(vec![
        emit(store, left, counters)?,
        emit(store, right, counters)?,
    ]))
}

fn try_inode_merge<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<Summary>> {
    let node = match (
        load(store, left_id, counters)?,
        load(store, right_id, counters)?,
    ) {
        (InodeTableNodeV1::Leaf(mut left), InodeTableNodeV1::Leaf(right)) => {
            left.extend(right);
            if left.len() > 127 {
                return Ok(None);
            }
            InodeTableNodeV1::Leaf(left)
        }
        (
            InodeTableNodeV1::Branch {
                children: mut left, ..
            },
            InodeTableNodeV1::Branch {
                children: right, ..
            },
        ) => {
            left.extend(right);
            if left.len() > 127 {
                return Ok(None);
            }
            InodeTableNodeV1::Branch {
                level: child_level,
                subtree_entry_count: inode_children_count(store, &left, counters)?,
                children: left,
            }
        }
        _ => return Err(CoreError::WrongLogicalRole),
    };
    emit(store, node, counters).map(Some)
}

fn inode_children_count<S: ObjectRead>(
    store: &S,
    children: &[(InodeId, ObjectId)],
    counters: &mut InodeTableCounters,
) -> CoreResult<u64> {
    children.iter().try_fold(0_u64, |sum, (_, id)| {
        sum.checked_add(summary(store, *id, counters)?.entries)
            .ok_or(CoreError::LengthOverflow)
    })
}

fn replace_inode_pair(
    mut children: Vec<(InodeId, ObjectId)>,
    start: usize,
    replacements: Vec<Summary>,
) -> Vec<(InodeId, ObjectId)> {
    children.splice(
        start..=start + 1,
        replacements.into_iter().map(|item| (item.max, item.id)),
    );
    children
}

fn emit_branch<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<Summary>,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let count = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let descriptors = children
        .into_iter()
        .map(|child| (child.max, child.id))
        .collect();
    emit(
        store,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count: count,
            children: descriptors,
        },
        counters,
    )
}

fn emit_branch_descriptors<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<(InodeId, ObjectId)>,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let mut count = 0_u64;
    for (_, id) in &children {
        count = count
            .checked_add(summary(store, *id, counters)?.entries)
            .ok_or(CoreError::LengthOverflow)?;
    }
    emit(
        store,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count: count,
            children,
        },
        counters,
    )
}

fn emit<S: ObjectStore>(
    store: &mut S,
    node: InodeTableNodeV1,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let (max, entries, level) = match &node {
        InodeTableNodeV1::Leaf(entries) => (
            entries
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode table"))?
                .0,
            entries.len() as u64,
            0,
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
        ),
    };
    let canonical = encode_inode_table_node(&node)?;
    let min = inode_node_min(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = counters
        .nodes_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(Summary {
        id,
        min,
        max,
        entries,
        level,
    })
}

fn summary<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    Ok(load_shallow(store, id, true, None, counters)?.summary)
}

fn walk_inode_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<InodeId>,
    counters: &mut InodeTableCounters,
    visitor: &mut impl FnMut(&[(InodeId, ObjectId)]) -> CoreResult<()>,
) -> CoreResult<Summary> {
    let mut loaded = load_shallow(store, id, root, None, counters)?;
    if expected_level.is_some_and(|level| loaded.summary.level != level)
        || expected_max.is_some_and(|maximum| loaded.summary.max != maximum)
    {
        return Err(CoreError::InvalidRecord("inode child summary"));
    }
    match &loaded.node {
        InodeTableNodeV1::Leaf(entries) => visitor(entries)?,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            children,
        } => {
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("inode child summary"))?;
            let mut count = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<InodeId> = None;
            for (maximum, child_id) in children {
                let child = walk_inode_node(
                    store,
                    *child_id,
                    false,
                    Some(child_level),
                    Some(*maximum),
                    counters,
                    visitor,
                )?;
                if previous_max.is_some_and(|previous| previous >= child.min) {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                minimum.get_or_insert(child.min);
                count = count
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = Some(child.max);
            }
            if count != *subtree_entry_count {
                return Err(CoreError::InvalidRecord("inode branch summary"));
            }
            loaded.summary.min = minimum.ok_or(CoreError::NonCanonicalPagePartition)?;
        }
    }
    Ok(loaded.summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

    impl ObjectStore for MemoryStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            self.0.insert(id, canonical.to_vec());
            Ok(id)
        }
    }

    #[test]
    fn batch_lookup_matches_sequential_with_fewer_node_reads() {
        let mut store = MemoryStore::default();
        let entries = (0..400_u64)
            .map(|index| {
                (
                    InodeId::allocate([91; 32], index),
                    ObjectId::for_bytes(&index.to_be_bytes()),
                )
            })
            .collect::<Vec<_>>();
        let mut table = inode_table_from_root(&mut store, entries[0].0, entries[0].1).unwrap();
        for (inode, record) in entries.iter().copied().skip(1) {
            table = inode_table_upsert(&mut store, table, inode, record)
                .unwrap()
                .0;
        }
        let mut keys = entries
            .iter()
            .step_by(4)
            .map(|entry| entry.0)
            .collect::<Vec<_>>();
        keys.push(InodeId::allocate([92; 32], 999));
        let mut sequential_counters = InodeTableCounters::default();
        let sequential = keys
            .iter()
            .map(|key| inode_table_lookup(&store, table, *key, &mut sequential_counters).unwrap())
            .collect::<Vec<_>>();
        let mut batch_counters = InodeTableCounters::default();
        let batch = inode_table_lookup_many(&store, table, &keys, &mut batch_counters).unwrap();
        assert_eq!(batch, sequential);
        assert!(batch_counters.nodes_read < sequential_counters.nodes_read);
    }
}
