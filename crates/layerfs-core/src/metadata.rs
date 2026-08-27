use crate::content::rope::{ObjectRead, ObjectStore};
use crate::inode::InodeKind;
use crate::namespace_codec::{decode_metadata_node, encode_metadata_node, MetadataNodeV1};
use crate::{CoreError, CoreResult, ObjectId};

pub const SUPPORTED_BSD_FLAGS: u32 = 0x0000_800f;
pub const ACL_RIGHTS_MASK: u64 = 0x0010_3ffe;
pub const ACL_FLAGS_MASK: u64 = 0x0002_01f0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortableMetadataV1 {
    pub permission_mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}

impl PortableMetadataV1 {
    pub fn validate(self, kind: InodeKind) -> CoreResult<()> {
        let mask = match kind {
            InodeKind::RegularFile => 0o777,
            InodeKind::Directory => 0o1777,
            InodeKind::Symlink => 0o777,
        };
        if self.permission_mode & !mask != 0
            || (kind == InodeKind::Symlink && self.permission_mode != 0o777)
            || self.mtime_nanoseconds > 999_999_999
        {
            return Err(CoreError::InvalidRecord("portable metadata"));
        }
        Ok(())
    }

    pub fn mode_bytes(self, kind: InodeKind) -> CoreResult<[u8; 4]> {
        self.validate(kind)?;
        Ok(self.permission_mode.to_be_bytes())
    }
    pub fn mtime_bytes(self) -> CoreResult<[u8; 12]> {
        if self.mtime_nanoseconds > 999_999_999 {
            return Err(CoreError::InvalidRecord("mtime"));
        }
        let mut bytes = [0; 12];
        bytes[..8].copy_from_slice(&self.mtime_seconds.to_be_bytes());
        bytes[8..].copy_from_slice(&self.mtime_nanoseconds.to_be_bytes());
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MetadataKey {
    pub domain: String,
    pub key: Vec<u8>,
}

impl MetadataKey {
    pub fn new(domain: String, key: Vec<u8>) -> CoreResult<Self> {
        if domain.is_empty()
            || domain.len() > 64
            || domain.as_bytes().contains(&0)
            || key.len() > 255
            || key.contains(&0)
        {
            return Err(CoreError::InvalidRecord("metadata key"));
        }
        let valid = match domain.as_str() {
            "portable" => key == b"mode" || key == b"mtime",
            "apple.xattr" => !key.is_empty() && key.len() <= 127,
            "apple.acl" | "apple.bsd-flags" => key.is_empty(),
            _ => false,
        };
        if !valid {
            return Err(CoreError::InvalidRecord("metadata domain"));
        }
        Ok(Self { domain, key })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataEntryV1 {
    pub key: MetadataKey,
    pub value_file_root: ObjectId,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MetadataCounters {
    pub nodes_read: u64,
    pub nodes_created: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AppleAclTag {
    Allow = 1,
    Deny = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppleAclEntryV1 {
    pub tag: AppleAclTag,
    pub flags: u64,
    pub rights: u64,
    pub qualifier_uuid: [u8; 16],
}

pub fn encode_apple_acl(entries: &[AppleAclEntryV1]) -> CoreResult<Vec<u8>> {
    if entries.is_empty() || entries.len() > 128 {
        return Err(CoreError::InvalidRecord("Apple ACL count"));
    }
    let mut bytes = Vec::with_capacity(12 + 36 * entries.len());
    bytes.extend_from_slice(b"LFS4ACL\0");
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    for entry in entries {
        if entry.flags & !ACL_FLAGS_MASK != 0 || entry.rights & !ACL_RIGHTS_MASK != 0 {
            return Err(CoreError::InvalidRecord("Apple ACL mask"));
        }
        bytes.extend_from_slice(&[entry.tag as u8, 1, 0, 0]);
        bytes.extend_from_slice(&entry.flags.to_be_bytes());
        bytes.extend_from_slice(&entry.rights.to_be_bytes());
        bytes.extend_from_slice(&entry.qualifier_uuid);
    }
    Ok(bytes)
}

pub fn decode_apple_acl(bytes: &[u8]) -> CoreResult<Vec<AppleAclEntryV1>> {
    if bytes.len() < 12 {
        return Err(CoreError::UnexpectedEof);
    }
    if &bytes[..8] != b"LFS4ACL\0" || u16::from_be_bytes([bytes[8], bytes[9]]) != 1 {
        return Err(CoreError::Unsupported);
    }
    let count = usize::from(u16::from_be_bytes([bytes[10], bytes[11]]));
    if count == 0 || count > 128 {
        return Err(CoreError::InvalidRecord("Apple ACL count"));
    }
    let expected = 12 + 36 * count;
    if bytes.len() < expected {
        return Err(CoreError::UnexpectedEof);
    }
    if bytes.len() > expected {
        return Err(CoreError::TrailingBytes);
    }
    bytes[12..]
        .chunks_exact(36)
        .map(|entry| {
            let tag = match entry[0] {
                1 => AppleAclTag::Allow,
                2 => AppleAclTag::Deny,
                _ => return Err(CoreError::InvalidRecord("Apple ACL tag")),
            };
            if entry[1] != 1 || entry[2..4] != [0, 0] {
                return Err(CoreError::InvalidRecord("Apple ACL qualifier"));
            }
            let flags = u64::from_be_bytes(entry[4..12].try_into().unwrap());
            let rights = u64::from_be_bytes(entry[12..20].try_into().unwrap());
            if flags & !ACL_FLAGS_MASK != 0 || rights & !ACL_RIGHTS_MASK != 0 {
                return Err(CoreError::InvalidRecord("Apple ACL mask"));
            }
            Ok(AppleAclEntryV1 {
                tag,
                flags,
                rights,
                qualifier_uuid: entry[20..36].try_into().unwrap(),
            })
        })
        .collect()
}

pub fn encode_bsd_flags(flags: u32) -> CoreResult<Option<[u8; 4]>> {
    if flags & !SUPPORTED_BSD_FLAGS != 0 {
        return Err(CoreError::InvalidRecord("BSD flags"));
    }
    Ok((flags != 0).then(|| flags.to_be_bytes()))
}

#[derive(Clone)]
struct MetadataSummary {
    id: ObjectId,
    min: Option<MetadataKey>,
    max: Option<MetadataKey>,
    entries: u64,
    encoded_bytes: u64,
    level: u8,
}

struct ValidatedMetadataNode {
    node: MetadataNodeV1,
    summary: MetadataSummary,
}

#[derive(Default)]
struct MetadataBranchPending {
    groups: Vec<Vec<MetadataSummary>>,
}

pub fn build_metadata_tree<S: ObjectStore>(
    store: &mut S,
    entries: &[MetadataEntryV1],
) -> CoreResult<ObjectId> {
    let mut builder = MetadataTreeBuilder::new();
    for entry in entries.iter().cloned() {
        builder.push(store, entry)?;
    }
    builder.finish(store)
}

pub struct MetadataTreeBuilder {
    groups: Vec<Vec<MetadataEntryV1>>,
    branches: Vec<MetadataBranchPending>,
    previous: Option<MetadataKey>,
    peak_pending_entries: usize,
    peak_pending_summaries: usize,
}

impl Default for MetadataTreeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl MetadataTreeBuilder {
    pub fn new() -> Self {
        Self {
            groups: Vec::new(),
            branches: Vec::new(),
            previous: None,
            peak_pending_entries: 0,
            peak_pending_summaries: 0,
        }
    }

    pub fn peak_pending_entries(&self) -> usize {
        self.peak_pending_entries
    }

    pub fn peak_pending_summaries(&self) -> usize {
        self.peak_pending_summaries
    }

    pub fn push<S: ObjectStore>(
        &mut self,
        store: &mut S,
        entry: MetadataEntryV1,
    ) -> CoreResult<()> {
        if self.previous.as_ref().is_some_and(|key| key >= &entry.key) {
            return Err(CoreError::NonCanonicalOrdering);
        }
        self.previous = Some(entry.key.clone());
        if self.groups.is_empty() {
            self.groups.push(Vec::new());
        }
        self.groups.last_mut().unwrap().push(entry);
        if encode_metadata_node(&metadata_leaf(self.groups.last().unwrap().clone())?).is_err() {
            let entry = self.groups.last_mut().unwrap().pop().unwrap();
            self.groups.push(vec![entry]);
        }
        if self.groups.len() == 3 {
            let sealed = self.groups.remove(0);
            let summary = emit_metadata(store, metadata_leaf(sealed)?)?;
            push_metadata_summary(
                store,
                &mut self.branches,
                0,
                summary,
                &mut self.peak_pending_summaries,
            )?;
        }
        self.peak_pending_entries = self
            .peak_pending_entries
            .max(self.groups.iter().map(Vec::len).sum());
        Ok(())
    }

    pub fn finish<S: ObjectStore>(mut self, store: &mut S) -> CoreResult<ObjectId> {
        if self.groups.is_empty() {
            return Ok(emit_metadata(store, metadata_leaf(Vec::new())?)?.id);
        }
        rebalance_leaf_tail(&mut self.groups)?;
        for group in self.groups {
            let summary = emit_metadata(store, metadata_leaf(group)?)?;
            push_metadata_summary(
                store,
                &mut self.branches,
                0,
                summary,
                &mut self.peak_pending_summaries,
            )?;
        }
        let mut index = 0_usize;
        loop {
            let pending = self
                .branches
                .get(index)
                .ok_or(CoreError::InvalidRecord("empty metadata level"))?;
            let children = pending.groups.iter().map(Vec::len).sum::<usize>();
            let higher = self.branches[index + 1..]
                .iter()
                .any(|pending| !pending.groups.is_empty());
            if children == 1 && !higher {
                return Ok(pending.groups[0][0].id);
            }
            if children == 0 {
                index = index
                    .checked_add(1)
                    .ok_or(CoreError::MappingDepthExceeded)?;
                continue;
            }
            let level = u8::try_from(index + 1).map_err(|_| CoreError::MappingDepthExceeded)?;
            let mut groups = std::mem::take(&mut self.branches[index].groups);
            rebalance_branch_tail(level, &mut groups)?;
            for group in groups {
                let summary = emit_metadata(store, metadata_branch(level, &group)?)?;
                push_metadata_summary(
                    store,
                    &mut self.branches,
                    index + 1,
                    summary,
                    &mut self.peak_pending_summaries,
                )?;
            }
            index = index
                .checked_add(1)
                .ok_or(CoreError::MappingDepthExceeded)?;
        }
    }
}

fn push_metadata_summary<S: ObjectStore>(
    store: &mut S,
    levels: &mut Vec<MetadataBranchPending>,
    index: usize,
    summary: MetadataSummary,
    peak: &mut usize,
) -> CoreResult<()> {
    if summary.level as usize != index {
        return Err(CoreError::InvalidRecord("metadata level"));
    }
    while levels.len() <= index {
        levels.push(MetadataBranchPending::default());
    }
    let sealed = {
        let groups = &mut levels[index].groups;
        if groups.is_empty() {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(summary);
        let branch_level = u8::try_from(index + 1).map_err(|_| CoreError::MappingDepthExceeded)?;
        if encode_metadata_node(&metadata_branch(branch_level, groups.last().unwrap())?).is_err() {
            let summary = groups.last_mut().unwrap().pop().unwrap();
            groups.push(vec![summary]);
        }
        (groups.len() == 3).then(|| groups.remove(0))
    };
    *peak = (*peak).max(
        levels
            .iter()
            .flat_map(|pending| &pending.groups)
            .map(Vec::len)
            .sum(),
    );
    if let Some(sealed) = sealed {
        let branch_level = u8::try_from(index + 1).map_err(|_| CoreError::MappingDepthExceeded)?;
        let parent = emit_metadata(store, metadata_branch(branch_level, &sealed)?)?;
        push_metadata_summary(store, levels, index + 1, parent, peak)?;
    }
    Ok(())
}

pub fn metadata_tree_entries<S: ObjectRead>(
    store: &S,
    root: ObjectId,
) -> CoreResult<Vec<MetadataEntryV1>> {
    let mut output = Vec::new();
    visit_metadata_entries(store, root, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

pub fn metadata_lookup<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    key: &MetadataKey,
) -> CoreResult<Option<MetadataEntryV1>> {
    metadata_lookup_with_counters(store, root, key, &mut MetadataCounters::default())
}

pub fn metadata_lookup_with_counters<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    key: &MetadataKey,
    counters: &mut MetadataCounters,
) -> CoreResult<Option<MetadataEntryV1>> {
    let mut id = root;
    let mut is_root = true;
    let mut expected_level = None;
    let mut expected_max = None;
    let mut exclusive_minimum = None;
    let mut ancestors = Vec::new();
    loop {
        if ancestors.contains(&id) {
            return Err(CoreError::MappingCycle);
        }
        ancestors.push(id);
        let loaded = load_metadata_counted(
            store,
            id,
            is_root,
            expected_level,
            expected_max.as_ref(),
            exclusive_minimum.as_ref(),
            counters,
        )?;
        match loaded.node {
            MetadataNodeV1::Leaf { entries, .. } => {
                return Ok(entries
                    .binary_search_by(|entry| entry.key.cmp(key))
                    .ok()
                    .map(|index| entries[index].clone()));
            }
            MetadataNodeV1::Branch {
                level, children, ..
            } => {
                let index = children.partition_point(|(maximum, _)| maximum < key);
                let Some((maximum, child)) = children.get(index) else {
                    return Ok(None);
                };
                exclusive_minimum = index
                    .checked_sub(1)
                    .map(|previous| children[previous].0.clone())
                    .or(exclusive_minimum);
                id = *child;
                is_root = false;
                expected_level = Some(
                    level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("metadata child summary"))?,
                );
                expected_max = Some(maximum.clone());
            }
        }
    }
}

pub fn replace_metadata_entry<S: ObjectStore>(
    store: &mut S,
    root: ObjectId,
    entry: MetadataEntryV1,
    counters: &mut MetadataCounters,
) -> CoreResult<ObjectId> {
    Ok(replace_metadata_node(store, root, true, None, None, None, &entry, counters)?.id)
}

#[allow(clippy::too_many_arguments)]
fn replace_metadata_node<S: ObjectStore>(
    store: &mut S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&MetadataKey>,
    exclusive_minimum: Option<&MetadataKey>,
    entry: &MetadataEntryV1,
    counters: &mut MetadataCounters,
) -> CoreResult<MetadataSummary> {
    let loaded = load_metadata_counted(
        store,
        id,
        root,
        expected_level,
        expected_max,
        exclusive_minimum,
        counters,
    )?;
    match loaded.node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            mut entries,
        } => {
            let index = entries
                .binary_search_by(|candidate| candidate.key.cmp(&entry.key))
                .map_err(|_| CoreError::InvalidRecord("metadata key missing"))?;
            if entries[index] == *entry {
                return Ok(loaded.summary);
            }
            entries[index] = entry.clone();
            emit_metadata_counted(
                store,
                MetadataNodeV1::Leaf {
                    subtree_encoded_bytes,
                    entries,
                },
                counters,
            )
        }
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            mut children,
        } => {
            let index = children.partition_point(|(maximum, _)| maximum < &entry.key);
            let (maximum, child) = children
                .get(index)
                .cloned()
                .ok_or(CoreError::InvalidRecord("metadata key missing"))?;
            let minimum = index
                .checked_sub(1)
                .map(|previous| &children[previous].0)
                .or(exclusive_minimum);
            let replacement = replace_metadata_node(
                store,
                child,
                false,
                Some(
                    level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("metadata child summary"))?,
                ),
                Some(&maximum),
                minimum,
                entry,
                counters,
            )?;
            if replacement.id == child {
                return Ok(loaded.summary);
            }
            if replacement.max.as_ref() != Some(&maximum) {
                return Err(CoreError::InvalidRecord("metadata replacement summary"));
            }
            children[index].1 = replacement.id;
            emit_metadata_counted(
                store,
                MetadataNodeV1::Branch {
                    level,
                    subtree_entry_count,
                    subtree_encoded_bytes,
                    children,
                },
                counters,
            )
        }
    }
}

/// Three-way merges ordered metadata entries with only bounded tree frontiers.
/// Conflicting values for the same key return `None`.
pub fn merge_metadata_roots<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    destination: ObjectId,
) -> CoreResult<Option<ObjectId>> {
    if source == base || source == destination {
        return Ok(Some(destination));
    }
    if destination == base {
        return Ok(Some(source));
    }
    let mut base_cursor = MetadataCursor::new(base);
    let mut source_cursor = MetadataCursor::new(source);
    let mut destination_cursor = MetadataCursor::new(destination);
    let mut base_entry = base_cursor.next(store)?;
    let mut source_entry = source_cursor.next(store)?;
    let mut destination_entry = destination_cursor.next(store)?;
    let mut builder = MetadataTreeBuilder::new();
    loop {
        let key = [
            base_entry.as_ref().map(|entry| &entry.key),
            source_entry.as_ref().map(|entry| &entry.key),
            destination_entry.as_ref().map(|entry| &entry.key),
        ]
        .into_iter()
        .flatten()
        .min()
        .cloned();
        let Some(key) = key else {
            return builder.finish(store).map(Some);
        };
        let base_value = if base_entry.as_ref().is_some_and(|entry| entry.key == key) {
            let value = base_entry.take();
            base_entry = base_cursor.next(store)?;
            value
        } else {
            None
        };
        let source_value = if source_entry.as_ref().is_some_and(|entry| entry.key == key) {
            let value = source_entry.take();
            source_entry = source_cursor.next(store)?;
            value
        } else {
            None
        };
        let destination_value = if destination_entry
            .as_ref()
            .is_some_and(|entry| entry.key == key)
        {
            let value = destination_entry.take();
            destination_entry = destination_cursor.next(store)?;
            value
        } else {
            None
        };
        let selected = if source_value == base_value || source_value == destination_value {
            destination_value
        } else if destination_value == base_value {
            source_value
        } else {
            return Ok(None);
        };
        if let Some(entry) = selected {
            builder.push(store, entry)?;
        }
    }
}

struct MetadataCursor {
    stack: Vec<MetadataWalkItem>,
    leaf: std::vec::IntoIter<MetadataEntryV1>,
}

struct MetadataWalkItem {
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<MetadataKey>,
}

impl MetadataCursor {
    fn new(root: ObjectId) -> Self {
        Self {
            stack: vec![MetadataWalkItem {
                id: root,
                root: true,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
        }
    }

    fn next<S: ObjectRead>(&mut self, store: &S) -> CoreResult<Option<MetadataEntryV1>> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Ok(Some(entry));
            }
            let Some(item) = self.stack.pop() else {
                return Ok(None);
            };
            let loaded = load_metadata_shallow(
                store,
                item.id,
                item.root,
                item.expected_level,
                item.expected_max.as_ref(),
            )?;
            match loaded.node {
                MetadataNodeV1::Leaf { entries, .. } => self.leaf = entries.into_iter(),
                MetadataNodeV1::Branch {
                    level, children, ..
                } => {
                    let child_level = level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("metadata child summary"))?;
                    self.stack
                        .extend(
                            children
                                .into_iter()
                                .rev()
                                .map(|(maximum, id)| MetadataWalkItem {
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

pub fn visit_metadata_entries<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    mut visitor: impl FnMut(&[MetadataEntryV1]) -> CoreResult<()>,
) -> CoreResult<()> {
    read_metadata_node(store, root, true, None, None, &mut Vec::new(), &mut visitor)?;
    Ok(())
}

fn read_metadata_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&MetadataKey>,
    ancestors: &mut Vec<ObjectId>,
    visitor: &mut impl FnMut(&[MetadataEntryV1]) -> CoreResult<()>,
) -> CoreResult<MetadataSummary> {
    if ancestors.contains(&id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(id);
    let loaded = load_metadata_shallow(store, id, root, expected_level, expected_max)?;
    let summary = match loaded.node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => {
            visitor(&entries)?;
            MetadataSummary {
                encoded_bytes: subtree_encoded_bytes,
                ..loaded.summary
            }
        }
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => {
            let mut count = 0_u64;
            let mut bytes = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<MetadataKey> = None;
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("metadata child summary"))?;
            for (maximum, child) in &children {
                let child = read_metadata_node(
                    store,
                    *child,
                    false,
                    Some(child_level),
                    Some(maximum),
                    ancestors,
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
                count = count
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                bytes = bytes
                    .checked_add(child.encoded_bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = child.max.clone();
            }
            if count != subtree_entry_count || bytes != subtree_encoded_bytes {
                return Err(CoreError::InvalidRecord("metadata branch summary"));
            }
            MetadataSummary {
                id,
                min: minimum,
                max: children.last().map(|child| child.0.clone()),
                entries: count,
                encoded_bytes: bytes,
                level,
            }
        }
    };
    ancestors.pop();
    Ok(summary)
}

fn load_metadata_shallow<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&MetadataKey>,
) -> CoreResult<ValidatedMetadataNode> {
    let node = store.with_authenticated_canonical(id, |canonical| {
        if !root && canonical.len() * 5 < 8192 * 2 {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        decode_metadata_node(canonical)
    })?;
    let summary = metadata_node_shape(id, &node, root)?;
    if expected_level.is_some_and(|level| summary.level != level)
        || expected_max.is_some_and(|maximum| summary.max.as_ref() != Some(maximum))
    {
        return Err(CoreError::InvalidRecord("metadata child summary"));
    }
    Ok(ValidatedMetadataNode { node, summary })
}

fn load_metadata_counted<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&MetadataKey>,
    exclusive_minimum: Option<&MetadataKey>,
    counters: &mut MetadataCounters,
) -> CoreResult<ValidatedMetadataNode> {
    let loaded = load_metadata_shallow(store, id, root, expected_level, expected_max)?;
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    if matches!(loaded.node, MetadataNodeV1::Leaf { .. })
        && exclusive_minimum
            .zip(loaded.summary.min.as_ref())
            .is_some_and(|(minimum, actual)| actual <= minimum)
    {
        return Err(CoreError::NonCanonicalOrdering);
    }
    Ok(loaded)
}

fn metadata_node_shape(
    id: ObjectId,
    node: &MetadataNodeV1,
    root: bool,
) -> CoreResult<MetadataSummary> {
    let (min, max, entries, encoded_bytes, level, count) = match node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => (
            entries.first().map(|entry| entry.key.clone()),
            entries.last().map(|entry| entry.key.clone()),
            entries.len() as u64,
            *subtree_encoded_bytes,
            0,
            entries.len(),
        ),
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => (
            children.first().map(|child| child.0.clone()),
            children.last().map(|child| child.0.clone()),
            *subtree_entry_count,
            *subtree_encoded_bytes,
            *level,
            children.len(),
        ),
    };
    if matches!(node, MetadataNodeV1::Branch { .. }) && count < 2 || !root && count == 0 {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    Ok(MetadataSummary {
        id,
        min,
        max,
        entries,
        encoded_bytes,
        level,
    })
}

fn metadata_leaf(entries: Vec<MetadataEntryV1>) -> CoreResult<MetadataNodeV1> {
    let bytes = entries.iter().try_fold(0_u64, |sum, entry| {
        sum.checked_add(37 + entry.key.domain.len() as u64 + entry.key.key.len() as u64)
            .ok_or(CoreError::LengthOverflow)
    })?;
    Ok(MetadataNodeV1::Leaf {
        subtree_encoded_bytes: bytes,
        entries,
    })
}

fn metadata_branch(level: u8, children: &[MetadataSummary]) -> CoreResult<MetadataNodeV1> {
    let count = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let bytes = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.encoded_bytes)
            .ok_or(CoreError::LengthOverflow)
    })?;
    Ok(MetadataNodeV1::Branch {
        level,
        subtree_entry_count: count,
        subtree_encoded_bytes: bytes,
        children: children
            .iter()
            .map(|child| {
                Ok((
                    child
                        .max
                        .clone()
                        .ok_or(CoreError::InvalidRecord("empty metadata child"))?,
                    child.id,
                ))
            })
            .collect::<CoreResult<Vec<_>>>()?,
    })
}

fn emit_metadata<S: ObjectStore>(
    store: &mut S,
    node: MetadataNodeV1,
) -> CoreResult<MetadataSummary> {
    let (max, entries, encoded_bytes, level) = match &node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => (
            entries.last().map(|entry| entry.key.clone()),
            entries.len() as u64,
            *subtree_encoded_bytes,
            0,
        ),
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => (
            Some(
                children
                    .last()
                    .ok_or(CoreError::InvalidRecord("empty metadata branch"))?
                    .0
                    .clone(),
            ),
            *subtree_entry_count,
            *subtree_encoded_bytes,
            *level,
        ),
    };
    let canonical = encode_metadata_node(&node)?;
    let id = store.put(&canonical)?;
    Ok(MetadataSummary {
        id,
        min: metadata_node_min(&node),
        max,
        entries,
        encoded_bytes,
        level,
    })
}

fn emit_metadata_counted<S: ObjectStore>(
    store: &mut S,
    node: MetadataNodeV1,
    counters: &mut MetadataCounters,
) -> CoreResult<MetadataSummary> {
    let summary = emit_metadata(store, node)?;
    counters.nodes_created = counters
        .nodes_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(summary)
}

fn metadata_node_min(node: &MetadataNodeV1) -> Option<MetadataKey> {
    match node {
        MetadataNodeV1::Leaf { entries, .. } => entries.first().map(|entry| entry.key.clone()),
        MetadataNodeV1::Branch { children, .. } => children.first().map(|entry| entry.0.clone()),
    }
}

fn rebalance_leaf_tail(groups: &mut [Vec<MetadataEntryV1>]) -> CoreResult<()> {
    if groups.len() < 2 {
        return Ok(());
    }
    let last = groups.len() - 1;
    while encode_metadata_node(&metadata_leaf(groups[last].clone())?)?.len() * 5 < 8192 * 2 {
        let moved = groups[last - 1]
            .pop()
            .ok_or(CoreError::NonCanonicalPagePartition)?;
        groups[last].insert(0, moved);
    }
    Ok(())
}

fn rebalance_branch_tail(level: u8, groups: &mut [Vec<MetadataSummary>]) -> CoreResult<()> {
    if groups.len() < 2 {
        return Ok(());
    }
    let last = groups.len() - 1;
    while encode_metadata_node(&metadata_branch(level, &groups[last])?)?.len() * 5 < 8192 * 2 {
        let moved = groups[last - 1]
            .pop()
            .ok_or(CoreError::NonCanonicalPagePartition)?;
        groups[last].insert(0, moved);
    }
    Ok(())
}
