fn directory_node_shape(
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

fn node_fields(node: &DirectoryNodeV1) -> (Option<CanonicalName>, u64, u64, u8) {
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

fn node_min(node: &DirectoryNodeV1) -> Option<CanonicalName> {
    match node {
        DirectoryNodeV1::Leaf { entries, .. } => entries.first().map(|entry| entry.0.clone()),
        DirectoryNodeV1::Branch { children, .. } => children.first().map(|entry| entry.0.clone()),
    }
}

fn load_directory_node<S: ObjectRead>(
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

fn load_directory_state<S: ObjectRead>(
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

fn store_directory_state<S: ObjectStore>(
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
    Ok(DirectoryStateRoot(store.put(&canonical)?))
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
