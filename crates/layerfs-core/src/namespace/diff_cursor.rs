struct DirectoryWalkItem {
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<CanonicalName>,
}

struct StreamingDirectoryDiff {
    base: StreamingDirectoryCursor,
    source: StreamingDirectoryCursor,
    base_entry: Option<(CanonicalName, InodeId)>,
    source_entry: Option<(CanonicalName, InodeId)>,
    initialized: bool,
}

impl StreamingDirectoryDiff {
    fn new(base: ObjectId, source: ObjectId) -> Self {
        Self {
            base: StreamingDirectoryCursor::new(base),
            source: StreamingDirectoryCursor::new(source),
            base_entry: None,
            source_entry: None,
            initialized: false,
        }
    }

    fn next<S: ObjectRead>(
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

struct DirectoryEntryCursor<'a, S> {
    store: &'a S,
    stack: Vec<DirectoryWalkItem>,
    leaf: std::vec::IntoIter<(CanonicalName, InodeId)>,
    counters: &'a mut NamespaceCounters,
}

impl<'a, S> DirectoryEntryCursor<'a, S> {
    fn new(
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

    fn from_children(
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
