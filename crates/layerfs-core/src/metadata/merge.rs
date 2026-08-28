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
