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
