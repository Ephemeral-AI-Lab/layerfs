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

struct DeferredInodes<'a, S> {
    store: &'a mut S,
    nodes: BTreeMap<ObjectId, Vec<u8>>,
}

impl<'a, S: ObjectStore> DeferredInodes<'a, S> {
    fn new(store: &'a mut S) -> Self {
        Self {
            store,
            nodes: BTreeMap::new(),
        }
    }

    fn commit(&mut self, root: ObjectId) -> CoreResult<u64> {
        let mut committed = BTreeSet::new();
        self.commit_node(root, &mut committed)?;
        u64::try_from(committed.len()).map_err(|_| CoreError::LengthOverflow)
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
        if self
            .nodes
            .insert(id, canonical.to_vec())
            .is_some_and(|prior| prior != canonical)
        {
            return Err(CoreError::IdentityMismatch);
        }
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
