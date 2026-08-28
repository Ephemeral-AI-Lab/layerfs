pub fn directory_insert<S: ObjectStore>(
    store: &mut S,
    root: DirectoryStateRoot,
    name: CanonicalName,
    inode: InodeId,
) -> CoreResult<(DirectoryStateRoot, NamespaceCounters)> {
    let mut counters = NamespaceCounters::default();
    let state = load_directory_state(store, root, &mut counters)?;
    let summary = load_directory_root_shallow(store, &state, &mut counters)?.summary;
    let mut replacements = insert_node(store, summary, true, name, inode, &mut counters)?;
    let next = if replacements.len() == 1 {
        replacements.remove(0)
    } else {
        emit_branch_from_summaries(
            store,
            state
                .tree_level
                .checked_add(1)
                .filter(|level| *level <= 31)
                .ok_or(CoreError::MappingDepthExceeded)?,
            replacements,
            &mut counters,
        )?
    };
    Ok((store_directory_state(store, next)?, counters))
}

pub fn directory_remove<S: ObjectStore>(
    store: &mut S,
    root: DirectoryStateRoot,
    name: &CanonicalName,
) -> CoreResult<(DirectoryStateRoot, InodeId, NamespaceCounters)> {
    let mut deferred = DeferredDirectory::new(store);
    let (root, removed, mut counters) = directory_remove_inner(&mut deferred, root, name)?;
    counters.nodes_created = deferred.commit(root)?;
    Ok((root, removed, counters))
}

fn directory_remove_inner<S: ObjectStore>(
    store: &mut S,
    root: DirectoryStateRoot,
    name: &CanonicalName,
) -> CoreResult<(DirectoryStateRoot, InodeId, NamespaceCounters)> {
    let mut counters = NamespaceCounters::default();
    let state = load_directory_state(store, root, &mut counters)?;
    let summary = load_directory_root_shallow(store, &state, &mut counters)?.summary;
    let (mut next, removed) = remove_node(store, summary, true, name, &mut counters)?;
    if let DirectoryNodeV1::Branch { children, .. } =
        load_directory_node(store, next.id, &mut counters)?
    {
        if children.len() == 1 {
            next = load_directory_summary(store, children[0].1, &mut counters)?;
        }
    }
    Ok((store_directory_state(store, next)?, removed, counters))
}

pub fn directory_rename<S: ObjectStore>(
    store: &mut S,
    root: DirectoryStateRoot,
    from: &CanonicalName,
    to: CanonicalName,
) -> CoreResult<(DirectoryStateRoot, NamespaceCounters)> {
    if directory_lookup(store, root, &to, &mut NamespaceCounters::default())?.is_some() {
        return Err(CoreError::NameCollision);
    }
    let mut deferred = DeferredDirectory::new(store);
    let (without, inode, mut counters) = directory_remove_inner(&mut deferred, root, from)?;
    let (renamed, inserted) = directory_insert(&mut deferred, without, to, inode)?;
    counters.nodes_read = counters
        .nodes_read
        .checked_add(inserted.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    counters.nodes_created = counters
        .nodes_created
        .checked_add(inserted.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    counters.nodes_created = deferred.commit(renamed)?;
    Ok((renamed, counters))
}

struct DeferredDirectory<'a, S> {
    store: &'a mut S,
    objects: BTreeMap<ObjectId, Vec<u8>>,
}

impl<'a, S: ObjectStore> DeferredDirectory<'a, S> {
    fn new(store: &'a mut S) -> Self {
        Self {
            store,
            objects: BTreeMap::new(),
        }
    }

    fn commit(&mut self, root: DirectoryStateRoot) -> CoreResult<u64> {
        let state_bytes = self
            .objects
            .get(&root.0)
            .cloned()
            .ok_or(CoreError::MissingObject)?;
        let state = decode_directory_state(&state_bytes)?;
        let mut committed = BTreeSet::new();
        self.commit_node(state.mapping_root, &mut committed)?;
        if self.store.put(&state_bytes)? != root.0 {
            return Err(CoreError::IdentityMismatch);
        }
        u64::try_from(committed.len()).map_err(|_| CoreError::LengthOverflow)
    }

    fn commit_node(&mut self, id: ObjectId, committed: &mut BTreeSet<ObjectId>) -> CoreResult<()> {
        let Some(canonical) = self.objects.get(&id).cloned() else {
            return Ok(());
        };
        if !committed.insert(id) {
            return Ok(());
        }
        if let DirectoryNodeV1::Branch { children, .. } = decode_directory_node(&canonical)? {
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

impl<S: ObjectStore> ObjectStore for DeferredDirectory<'_, S> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.objects
            .get(&id)
            .cloned()
            .map_or_else(|| self.store.get(id), Ok)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        if self
            .objects
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
        match self.objects.get(&id) {
            Some(bytes) if ObjectId::for_bytes(bytes) == id => callback(bytes),
            Some(_) => Err(CoreError::IdentityMismatch),
            None => self.store.with_authenticated_canonical(id, callback),
        }
    }
}
