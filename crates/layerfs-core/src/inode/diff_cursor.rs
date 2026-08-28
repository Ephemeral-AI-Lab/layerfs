struct StreamingInodeDiff {
    base: StreamingInodeCursor,
    source: StreamingInodeCursor,
    base_entry: Option<(InodeId, ObjectId)>,
    source_entry: Option<(InodeId, ObjectId)>,
    initialized: bool,
}

impl StreamingInodeDiff {
    fn new(base: ObjectId, source: ObjectId, root: bool) -> Self {
        Self {
            base: StreamingInodeCursor::new(base, root),
            source: StreamingInodeCursor::new(source, root),
            base_entry: None,
            source_entry: None,
            initialized: false,
        }
    }

    fn next<S: ObjectRead>(
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

struct InodeEntryCursor<'a, S> {
    store: &'a S,
    stack: Vec<InodeWalkItem>,
    leaf: std::vec::IntoIter<(InodeId, ObjectId)>,
    counters: &'a mut InodeTableCounters,
}

impl<'a, S> InodeEntryCursor<'a, S> {
    fn new(
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
