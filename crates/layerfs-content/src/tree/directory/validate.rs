use super::codec::{
    decode_directory_node, decode_directory_state, decode_symlink, encode_directory_node,
    encode_directory_state, profile_id, DirectoryNodeV1,
};
use super::node::{DirectoryStateRoot, DirectoryStateV1, NamespaceCounters, NodeSummary};
use super::read::visit_directory_entries;
use crate::file::rope::{
    read_range, state, validate_file, FileStateRoot, ObjectRead, ObjectStore, RopeCounters,
};
use crate::tree::inode::{InodeId, InodeKind, InodeRecordV1};
use crate::tree::metadata::{
    decode_apple_acl, visit_metadata_entries, PortableMetadataV1, SUPPORTED_BSD_FLAGS,
};
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};

pub(super) fn directory_node_shape(
    id: ObjectId,
    node: &DirectoryNodeV1,
    root: bool,
) -> CoreResult<NodeSummary> {
    let canonical_len = encode_directory_node(node)?.len();
    if !root && canonical_len * 5 < 8192 * 2 {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    match node {
        DirectoryNodeV1::Leaf { entries, .. } if !root && entries.is_empty() => {
            return Err(CoreError::NonCanonicalPagePartition)
        }
        DirectoryNodeV1::Branch { children, .. } if children.len() < 2 => {
            return Err(CoreError::NonCanonicalPagePartition)
        }
        _ => {}
    }
    let (max, entries, encoded_bytes, level) = node_fields(node);
    Ok(NodeSummary {
        id,
        min: node_min(node),
        max,
        entries,
        encoded_bytes,
        level,
    })
}

pub(super) fn node_fields(node: &DirectoryNodeV1) -> (Option<CanonicalName>, u64, u64, u8) {
    match node {
        DirectoryNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => (
            entries.last().map(|entry| entry.0.clone()),
            entries.len() as u64,
            *subtree_encoded_bytes,
            0,
        ),
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => (
            children.last().map(|entry| entry.0.clone()),
            *subtree_entry_count,
            *subtree_encoded_bytes,
            *level,
        ),
    }
}

pub(super) fn node_min(node: &DirectoryNodeV1) -> Option<CanonicalName> {
    match node {
        DirectoryNodeV1::Leaf { entries, .. } => entries.first().map(|entry| entry.0.clone()),
        DirectoryNodeV1::Branch { children, .. } => children.first().map(|entry| entry.0.clone()),
    }
}

pub(super) fn load_directory_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryNodeV1> {
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    store.with_authenticated_canonical(id, decode_directory_node)
}

pub(super) fn load_directory_state<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryStateV1> {
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    store.with_authenticated_canonical(root.0, decode_directory_state)
}

pub(super) fn store_directory_state<S: ObjectStore>(
    store: &mut S,
    node: NodeSummary,
) -> CoreResult<DirectoryStateRoot> {
    let state = DirectoryStateV1 {
        entry_count: node.entries,
        tree_level: node.level,
        profile_id: profile_id(),
        mapping_root: node.id,
    };
    let canonical = encode_directory_state(state)?;
    Ok(DirectoryStateRoot(store.put_owned(canonical)?))
}

pub fn validate_inode_record<S: ObjectRead>(
    store: &S,
    record: InodeRecordV1,
    root: bool,
    mut child_visitor: impl FnMut(InodeId) -> CoreResult<()>,
) -> CoreResult<()> {
    validate_inode_record_metadata(store, record, root)?;
    match record.kind {
        InodeKind::RegularFile => validate_file(store, FileStateRoot(record.content_root)),
        InodeKind::Symlink => store
            .with_authenticated_canonical(record.content_root, |canonical| {
                decode_symlink(canonical).map(drop)
            }),
        InodeKind::Directory => visit_directory_entries(
            store,
            DirectoryStateRoot(record.content_root),
            &mut NamespaceCounters::default(),
            |entries| {
                for (_, child) in entries {
                    child_visitor(*child)?;
                }
                Ok(())
            },
        ),
    }
}

pub fn validate_inode_record_metadata<S: ObjectRead>(
    store: &S,
    record: InodeRecordV1,
    root: bool,
) -> CoreResult<()> {
    record.validate(root)?;
    validate_metadata(store, record.metadata_root, record.kind)
}

fn validate_metadata<S: ObjectRead>(store: &S, root: ObjectId, kind: InodeKind) -> CoreResult<()> {
    let mut mode = None;
    let mut mtime = None;
    visit_metadata_entries(store, root, |entries| {
        for entry in entries {
            let root = FileStateRoot(entry.value_file_root);
            validate_file(store, root)?;
            let file = state(store, root, &mut RopeCounters::default())?;
            match (entry.key.domain.as_str(), entry.key.key.as_slice()) {
                ("portable", b"mode") if file.logical_len == 4 => {
                    let mut bytes = Vec::new();
                    read_range(store, root, 0..4, &mut bytes)?;
                    mode = Some(u32::from_be_bytes(bytes.try_into().unwrap()));
                }
                ("portable", b"mtime") if file.logical_len == 12 => {
                    let mut bytes = Vec::new();
                    read_range(store, root, 0..12, &mut bytes)?;
                    mtime = Some((
                        i64::from_be_bytes(bytes[..8].try_into().unwrap()),
                        u32::from_be_bytes(bytes[8..].try_into().unwrap()),
                    ));
                }
                ("apple.acl", b"") if file.logical_len <= 4_620 => {
                    let mut bytes = Vec::new();
                    read_range(store, root, 0..file.logical_len, &mut bytes)?;
                    decode_apple_acl(&bytes)?;
                }
                ("apple.bsd-flags", b"") if file.logical_len == 4 => {
                    let mut bytes = Vec::new();
                    read_range(store, root, 0..4, &mut bytes)?;
                    let flags = u32::from_be_bytes(bytes.try_into().unwrap());
                    if flags == 0 || flags & !SUPPORTED_BSD_FLAGS != 0 {
                        return Err(CoreError::InvalidRecord("BSD flags"));
                    }
                }
                ("apple.xattr", _) if file.logical_len <= 1024 * 1024 => {}
                _ => return Err(CoreError::InvalidRecord("metadata value")),
            }
        }
        Ok(())
    })?;
    let (seconds, nanoseconds) = mtime.ok_or(CoreError::InvalidRecord("mtime missing"))?;
    PortableMetadataV1 {
        permission_mode: mode.ok_or(CoreError::InvalidRecord("mode missing"))?,
        mtime_seconds: seconds,
        mtime_nanoseconds: nanoseconds,
    }
    .validate(kind)
}

pub(super) fn nearest_half(widths: Vec<usize>) -> usize {
    let total: usize = widths.iter().sum();
    let mut prefix = 0;
    let mut best = 1;
    let mut distance = usize::MAX;
    for (index, width) in widths.iter().enumerate().take(widths.len() - 1) {
        prefix += width;
        let next = total.abs_diff(prefix * 2);
        if next < distance {
            best = index + 1;
            distance = next;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::super::edit::{branch, leaf, try_directory_borrow};
    use super::*;
    use crate::tree::directory::{directory_insert, directory_page_after, empty_directory};
    use std::collections::BTreeMap;

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
    fn directory_pages_resume_without_rescanning_or_skipping() {
        let mut store = MemoryStore::default();
        let mut root = empty_directory(&mut store).unwrap();
        for serial in 0..300_u64 {
            let name = CanonicalName::new(&format!("entry-{serial:03}")).unwrap();
            root = directory_insert(
                &mut store,
                root,
                name,
                InodeId::allocate([0x31; 32], serial),
            )
            .unwrap()
            .0;
        }
        let mut after = None;
        let mut names = Vec::new();
        loop {
            let page = directory_page_after(
                &store,
                root,
                after.as_ref(),
                17,
                2048,
                &mut NamespaceCounters::default(),
            )
            .unwrap();
            names.extend(page.entries.iter().map(|entry| entry.0.as_str().to_owned()));
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(names.len(), 300);
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn variable_width_directory_borrows_until_both_branches_are_filled() {
        let mut store = MemoryStore::default();
        let mut serial = 0_u64;
        let mut children = |prefix: char, count: usize, width: usize, store: &mut MemoryStore| {
            (0..count)
                .map(|index| {
                    let text = if width == 4 {
                        format!("{prefix}{index:03}")
                    } else {
                        format!("{prefix}{}{index:04}", prefix.to_string().repeat(width - 5))
                    };
                    let name = CanonicalName::new(&text).unwrap();
                    let inode = InodeId::allocate([0x51; 32], serial);
                    serial += 1;
                    let id = store.put(
                        &encode_directory_node(&leaf(vec![(name.clone(), inode)])?).unwrap(),
                    )?;
                    Ok((name, id))
                })
                .collect::<CoreResult<Vec<_>>>()
        };
        let left_children = children('a', 131, 4, &mut store).unwrap();
        let right_children = children('m', 11, 255, &mut store).unwrap();
        let left = store
            .put(
                &encode_directory_node(
                    &branch(&store, 1, left_children, &mut NamespaceCounters::default()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let right = store
            .put(
                &encode_directory_node(
                    &branch(&store, 1, right_children, &mut NamespaceCounters::default()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let replacements = try_directory_borrow(
            &mut store,
            1,
            left,
            right,
            true,
            &mut NamespaceCounters::default(),
        )
        .unwrap()
        .unwrap();
        let counts = replacements
            .into_iter()
            .map(
                |summary| match decode_directory_node(&store.0[&summary.id]).unwrap() {
                    DirectoryNodeV1::Branch { children, .. } => children.len(),
                    _ => panic!("expected branch"),
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(counts, [129, 13]);
    }
}
