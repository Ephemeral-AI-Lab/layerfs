pub fn apply_inode_mutations(
    store: &mut impl ObjectStore,
    root: ObjectId,
    mutations: impl IntoIterator<Item = InodeMutation>,
) -> CoreResult<CandidateRoot> {
    let mut counters = LogicalCounters::default();
    let namespace = namespace(store, root)?;
    let mut table = InodeTableRoot(namespace.inode_table_root);
    for mutation in mutations {
        match mutation {
            InodeMutation::Upsert { inode, record } => {
                let record = store.put(&encode_inode_record(record)?)?;
                let (next, visits) = inode_table_upsert(store, table, inode, record)?;
                merge_inode(&mut counters, visits)?;
                table = next;
            }
            InodeMutation::Remove { inode } => {
                let (next, _, visits) = inode_table_remove(store, table, inode)?;
                merge_inode(&mut counters, visits)?;
                table = next;
            }
        }
    }
    let candidate = store.put(&encode_namespace_root(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })?)?;
    Ok(CandidateRoot {
        parent_root: root,
        root: candidate,
        counters,
    })
}

pub fn apply_directory_changes(
    store: &mut impl ObjectStore,
    mut root: DirectoryStateRoot,
    changes: impl IntoIterator<Item = (crate::CanonicalName, Option<InodeId>)>,
) -> CoreResult<(DirectoryStateRoot, crate::namespace::NamespaceCounters)> {
    let mut counters = crate::namespace::NamespaceCounters::default();
    for (name, desired) in changes {
        if directory_lookup(store, root, &name, &mut counters)?.is_some() {
            let (next, _, visits) = directory_remove(store, root, &name)?;
            merge_namespace_counters(&mut counters, visits)?;
            root = next;
        }
        if let Some(inode) = desired {
            let (next, visits) = directory_insert(store, root, name, inode)?;
            merge_namespace_counters(&mut counters, visits)?;
            root = next;
        }
    }
    Ok((root, counters))
}
