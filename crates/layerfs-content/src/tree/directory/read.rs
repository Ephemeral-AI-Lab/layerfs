use super::codec::DirectoryNodeV1;
use super::diff::DirectoryEntryDiff;
use super::edit::emit_directory_node;
use super::node::{
    DirectoryPage, DirectoryStateRoot, DirectoryStateV1, NamespaceCounters, NodeSummary,
    ValidatedNode,
};
use super::validate::{
    directory_node_shape, load_directory_node, load_directory_state, store_directory_state,
};
use crate::file::rope::{ObjectRead, ObjectStore};
use crate::tree::inode::InodeId;
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};

pub fn empty_directory<S: ObjectStore>(store: &mut S) -> CoreResult<DirectoryStateRoot> {
    let mut counters = NamespaceCounters::default();
    let node = emit_directory_node(
        store,
        DirectoryNodeV1::Leaf {
            subtree_encoded_bytes: 0,
            entries: Vec::new(),
        },
        &mut counters,
    )?;
    store_directory_state(store, node)
}

pub fn directory_lookup<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    name: &CanonicalName,
    counters: &mut NamespaceCounters,
) -> CoreResult<Option<InodeId>> {
    let state = load_directory_state(store, root, counters)?;
    let mut current = load_directory_root_shallow(store, &state, counters)?;
    loop {
        match current.node {
            DirectoryNodeV1::Leaf { entries, .. } => {
                return Ok(entries
                    .binary_search_by(|(candidate, _)| candidate.cmp(name))
                    .ok()
                    .map(|index| entries[index].1))
            }
            DirectoryNodeV1::Branch { children, .. } => {
                let index = children
                    .partition_point(|(maximum, _)| maximum < name)
                    .min(children.len().saturating_sub(1));
                let expected_max = children[index].0.clone();
                let child =
                    load_directory_node_shallow(store, children[index].1, false, None, counters)?;
                if child.summary.max.as_ref() != Some(&expected_max)
                    || child.summary.level.checked_add(1) != Some(current.summary.level)
                {
                    return Err(CoreError::InvalidRecord("directory child summary"));
                }
                current = child;
            }
        }
    }
}

pub fn directory_entries<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<(CanonicalName, InodeId)>> {
    let mut output = Vec::new();
    visit_directory_entries(store, root, counters, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

/// Returns one bounded ordered page strictly after `exclusive_after`.
pub fn directory_page_after<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    exclusive_after: Option<&CanonicalName>,
    max_entries: usize,
    max_bytes: usize,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryPage> {
    if max_entries == 0 || max_bytes == 0 {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let state = load_directory_state(store, root, counters)?;
    let mut cursor =
        DirectoryEntryCursor::after(store, &state, exclusive_after, counters)?.peekable();
    let mut entries = Vec::new();
    let mut bytes = 0_usize;
    while entries.len() < max_entries {
        let Some(next) = cursor.peek() else {
            return Ok(DirectoryPage {
                entries,
                continuation: None,
            });
        };
        let width = match next {
            Ok((name, _)) => 34_usize
                .checked_add(name.as_bytes().len())
                .ok_or(CoreError::LengthOverflow)?,
            Err(_) => return Err(cursor.next().expect("peeked entry").unwrap_err()),
        };
        if bytes
            .checked_add(width)
            .is_none_or(|total| total > max_bytes)
        {
            if entries.is_empty() {
                return Err(CoreError::ObjectLimitExceeded);
            }
            break;
        }
        bytes += width;
        entries.push(cursor.next().expect("peeked entry")?);
    }
    Ok(DirectoryPage {
        continuation: entries.last().map(|entry| entry.0.clone()),
        entries,
    })
}

pub fn visit_directory_entries<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
    mut visitor: impl FnMut(&[(CanonicalName, InodeId)]) -> CoreResult<()>,
) -> CoreResult<()> {
    let state = load_directory_state(store, root, counters)?;
    let summary = walk_directory_node(
        store,
        state.mapping_root,
        true,
        None,
        None,
        counters,
        &mut visitor,
    )?;
    if summary.entries != state.entry_count || summary.level != state.tree_level {
        return Err(CoreError::InvalidRecord("directory state summary"));
    }
    Ok(())
}

pub(super) fn load_directory_summary<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<NodeSummary> {
    Ok(load_directory_node_shallow(store, id, true, None, counters)?.summary)
}

pub(super) fn load_directory_root_shallow<S: ObjectRead>(
    store: &S,
    state: &DirectoryStateV1,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    let loaded = load_directory_node_shallow(store, state.mapping_root, true, None, counters)?;
    if loaded.summary.entries != state.entry_count || loaded.summary.level != state.tree_level {
        return Err(CoreError::InvalidRecord("directory state summary"));
    }
    Ok(loaded)
}

pub(super) fn load_directory_node_expected_shallow<S: ObjectRead>(
    store: &S,
    expected: &NodeSummary,
    root: bool,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    load_directory_node_shallow(store, expected.id, root, Some(expected), counters)
}

pub(super) fn load_directory_node_shallow<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected: Option<&NodeSummary>,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    let node = load_directory_node(store, id, counters)?;
    let summary = directory_node_shape(id, &node, root)?;
    if expected.is_some_and(|expected| {
        summary.max != expected.max
            || summary.entries != expected.entries
            || summary.encoded_bytes != expected.encoded_bytes
            || summary.level != expected.level
    }) {
        return Err(CoreError::InvalidRecord("directory child summary"));
    }
    Ok(ValidatedNode { node, summary })
}

fn walk_directory_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&CanonicalName>,
    counters: &mut NamespaceCounters,
    visitor: &mut impl FnMut(&[(CanonicalName, InodeId)]) -> CoreResult<()>,
) -> CoreResult<NodeSummary> {
    let node = load_directory_node(store, id, counters)?;
    let mut summary = directory_node_shape(id, &node, root)?;
    if expected_level.is_some_and(|level| summary.level != level)
        || expected_max.is_some_and(|maximum| summary.max.as_ref() != Some(maximum))
    {
        return Err(CoreError::InvalidRecord("directory child summary"));
    }
    match &node {
        DirectoryNodeV1::Leaf { entries, .. } => visitor(entries)?,
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => {
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("directory child summary"))?;
            let mut entries = 0_u64;
            let mut bytes = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<CanonicalName> = None;
            for (maximum, child_id) in children {
                let child = walk_directory_node(
                    store,
                    *child_id,
                    false,
                    Some(child_level),
                    Some(maximum),
                    counters,
                    visitor,
                )?;
                if previous_max
                    .as_ref()
                    .zip(child.min.as_ref())
                    .is_some_and(|(previous, next)| previous >= next)
                {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                if minimum.is_none() {
                    minimum = child.min.clone();
                }
                entries = entries
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                bytes = bytes
                    .checked_add(child.encoded_bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = child.max;
            }
            if entries != *subtree_entry_count || bytes != *subtree_encoded_bytes {
                return Err(CoreError::InvalidRecord("directory branch summary"));
            }
            summary.min = minimum;
        }
    }
    Ok(summary)
}

struct DirectoryWalkItem {
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<CanonicalName>,
}

pub(super) struct StreamingDirectoryDiff {
    base: StreamingDirectoryCursor,
    source: StreamingDirectoryCursor,
    base_entry: Option<(CanonicalName, InodeId)>,
    source_entry: Option<(CanonicalName, InodeId)>,
    initialized: bool,
}

impl StreamingDirectoryDiff {
    pub(super) fn new(base: ObjectId, source: ObjectId) -> Self {
        Self {
            base: StreamingDirectoryCursor::new(base),
            source: StreamingDirectoryCursor::new(source),
            base_entry: None,
            source_entry: None,
            initialized: false,
        }
    }

    pub(super) fn next<S: ObjectRead>(
        &mut self,
        store: &S,
        counters: &mut NamespaceCounters,
    ) -> CoreResult<Option<DirectoryEntryDiff>> {
        if !self.initialized {
            self.base_entry = self.base.next(store, counters)?;
            self.source_entry = self.source.next(store, counters)?;
            self.initialized = true;
        }
        loop {
            match (&self.base_entry, &self.source_entry) {
                (None, None) => return Ok(None),
                (Some((base_name, before)), Some((source_name, after)))
                    if base_name == source_name =>
                {
                    let name = base_name.clone();
                    let (before, after) = (*before, *after);
                    self.base_entry = self.base.next(store, counters)?;
                    self.source_entry = self.source.next(store, counters)?;
                    if before != after {
                        return Ok(Some(DirectoryEntryDiff {
                            name,
                            before: Some(before),
                            after: Some(after),
                        }));
                    }
                }
                (Some((base_name, before)), Some((source_name, _))) if base_name < source_name => {
                    let change = DirectoryEntryDiff {
                        name: base_name.clone(),
                        before: Some(*before),
                        after: None,
                    };
                    self.base_entry = self.base.next(store, counters)?;
                    return Ok(Some(change));
                }
                (Some(_), Some((source_name, after))) => {
                    let change = DirectoryEntryDiff {
                        name: source_name.clone(),
                        before: None,
                        after: Some(*after),
                    };
                    self.source_entry = self.source.next(store, counters)?;
                    return Ok(Some(change));
                }
                (Some((base_name, before)), None) => {
                    let change = DirectoryEntryDiff {
                        name: base_name.clone(),
                        before: Some(*before),
                        after: None,
                    };
                    self.base_entry = self.base.next(store, counters)?;
                    return Ok(Some(change));
                }
                (None, Some((source_name, after))) => {
                    let change = DirectoryEntryDiff {
                        name: source_name.clone(),
                        before: None,
                        after: Some(*after),
                    };
                    self.source_entry = self.source.next(store, counters)?;
                    return Ok(Some(change));
                }
            }
        }
    }
}

struct StreamingDirectoryCursor {
    stack: Vec<DirectoryWalkItem>,
    leaf: std::vec::IntoIter<(CanonicalName, InodeId)>,
}

impl StreamingDirectoryCursor {
    fn new(root: ObjectId) -> Self {
        Self {
            stack: vec![DirectoryWalkItem {
                id: root,
                root: true,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
        }
    }

    fn next<S: ObjectRead>(
        &mut self,
        store: &S,
        counters: &mut NamespaceCounters,
    ) -> CoreResult<Option<(CanonicalName, InodeId)>> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Ok(Some(entry));
            }
            let Some(item) = self.stack.pop() else {
                return Ok(None);
            };
            let loaded = load_directory_node_shallow(store, item.id, item.root, None, counters)?;
            if item
                .expected_level
                .is_some_and(|level| loaded.summary.level != level)
                || item
                    .expected_max
                    .as_ref()
                    .is_some_and(|maximum| loaded.summary.max.as_ref() != Some(maximum))
            {
                return Err(CoreError::InvalidRecord("directory child summary"));
            }
            match loaded.node {
                DirectoryNodeV1::Leaf { entries, .. } => self.leaf = entries.into_iter(),
                DirectoryNodeV1::Branch {
                    level, children, ..
                } => {
                    let child_level = level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("directory child summary"))?;
                    self.stack
                        .extend(children.into_iter().rev().map(|(maximum, id)| {
                            DirectoryWalkItem {
                                id,
                                root: false,
                                expected_level: Some(child_level),
                                expected_max: Some(maximum),
                            }
                        }));
                }
            }
        }
    }
}

pub(super) struct DirectoryEntryCursor<'a, S> {
    store: &'a S,
    stack: Vec<DirectoryWalkItem>,
    leaf: std::vec::IntoIter<(CanonicalName, InodeId)>,
    counters: &'a mut NamespaceCounters,
}

impl<'a, S> DirectoryEntryCursor<'a, S> {
    pub(super) fn new(
        store: &'a S,
        root: ObjectId,
        root_context: bool,
        counters: &'a mut NamespaceCounters,
    ) -> Self {
        Self {
            store,
            stack: vec![DirectoryWalkItem {
                id: root,
                root: root_context,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
            counters,
        }
    }

    pub(super) fn from_children(
        store: &'a S,
        children: &[(CanonicalName, ObjectId)],
        child_level: u8,
        counters: &'a mut NamespaceCounters,
    ) -> Self {
        Self {
            store,
            stack: children
                .iter()
                .rev()
                .map(|(maximum, id)| DirectoryWalkItem {
                    id: *id,
                    root: false,
                    expected_level: Some(child_level),
                    expected_max: Some(maximum.clone()),
                })
                .collect(),
            leaf: Vec::new().into_iter(),
            counters,
        }
    }

    fn after(
        store: &'a S,
        state: &DirectoryStateV1,
        exclusive_after: Option<&CanonicalName>,
        counters: &'a mut NamespaceCounters,
    ) -> CoreResult<Self>
    where
        S: ObjectRead,
    {
        let mut loaded = load_directory_root_shallow(store, state, counters)?;
        let mut stack = Vec::new();
        loop {
            match loaded.node {
                DirectoryNodeV1::Leaf { entries, .. } => {
                    let start = exclusive_after
                        .map(|after| entries.partition_point(|entry| entry.0 <= *after))
                        .unwrap_or(0);
                    return Ok(Self {
                        store,
                        stack,
                        leaf: entries
                            .into_iter()
                            .skip(start)
                            .collect::<Vec<_>>()
                            .into_iter(),
                        counters,
                    });
                }
                DirectoryNodeV1::Branch {
                    level, children, ..
                } => {
                    let child_level = level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("directory child summary"))?;
                    let selected = exclusive_after
                        .map(|after| children.partition_point(|entry| entry.0 <= *after))
                        .unwrap_or(0);
                    if selected == children.len() {
                        return Ok(Self {
                            store,
                            stack,
                            leaf: Vec::new().into_iter(),
                            counters,
                        });
                    }
                    stack.extend(children[selected + 1..].iter().rev().map(|(maximum, id)| {
                        DirectoryWalkItem {
                            id: *id,
                            root: false,
                            expected_level: Some(child_level),
                            expected_max: Some(maximum.clone()),
                        }
                    }));
                    let (maximum, id) = &children[selected];
                    loaded = load_directory_node_shallow(store, *id, false, None, counters)?;
                    if loaded.summary.level != child_level
                        || loaded.summary.max.as_ref() != Some(maximum)
                    {
                        return Err(CoreError::InvalidRecord("directory child summary"));
                    }
                }
            }
        }
    }
}

impl<S: ObjectRead> Iterator for DirectoryEntryCursor<'_, S> {
    type Item = CoreResult<(CanonicalName, InodeId)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Some(Ok(entry));
            }
            let item = self.stack.pop()?;
            let loaded = match load_directory_node_shallow(
                self.store,
                item.id,
                item.root,
                None,
                self.counters,
            ) {
                Ok(loaded) => loaded,
                Err(error) => return Some(Err(error)),
            };
            if item
                .expected_level
                .is_some_and(|level| loaded.summary.level != level)
                || item
                    .expected_max
                    .as_ref()
                    .is_some_and(|maximum| loaded.summary.max.as_ref() != Some(maximum))
            {
                return Some(Err(CoreError::InvalidRecord("directory child summary")));
            }
            match loaded.node {
                DirectoryNodeV1::Leaf { entries, .. } => self.leaf = entries.into_iter(),
                DirectoryNodeV1::Branch {
                    level, children, ..
                } => {
                    let Some(child_level) = level.checked_sub(1) else {
                        return Some(Err(CoreError::InvalidRecord("directory child summary")));
                    };
                    self.stack
                        .extend(children.into_iter().rev().map(|(maximum, id)| {
                            DirectoryWalkItem {
                                id,
                                root: false,
                                expected_level: Some(child_level),
                                expected_max: Some(maximum),
                            }
                        }));
                }
            }
        }
    }
}
