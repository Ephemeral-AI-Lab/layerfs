impl MountedWorkspace {
    fn persist_content(
        spool: &mut Spool,
        publication: &mut WorkingCandidateWrite<'_>,
        node: &CheckpointNode,
        canonical_ids: &HashMap<MountedNodeId, InodeId>,
    ) -> Result<ObjectId, MountedError> {
        match &node.content {
            NodeContent::File {
                base,
                base_visible_len,
                logical_len,
                ranges,
                ..
            } => {
                let (mut root, mut current_len) = if let Some(root) = base {
                    let mut counters = RopeCounters::default();
                    let state =
                        layerfs_core::content::rope::state(publication, *root, &mut counters)?;
                    (*root, state.logical_len)
                } else {
                    let (root, _) = build(publication, Cursor::new(&[]))?;
                    (root, 0)
                };
                if current_len > *base_visible_len {
                    (root, _) = replace(
                        publication,
                        root,
                        *base_visible_len,
                        current_len - *base_visible_len,
                        Cursor::new(&[]),
                    )?;
                    current_len = *base_visible_len;
                }
                for (start, range) in ranges {
                    if *start > current_len {
                        let gap = *start - current_len;
                        (root, _) = replace(publication, root, current_len, 0, ZeroReader(gap))?;
                        current_len = *start;
                    }
                    let length = range.end - *start;
                    let delete = length.min(current_len.saturating_sub(*start));
                    let slice = spool.slice(range.spool_offset, length)?;
                    (root, _) = replace(publication, root, *start, delete, slice)?;
                    current_len = current_len.max(range.end);
                }
                match current_len.cmp(logical_len) {
                    Ordering::Greater => {
                        (root, _) = replace(
                            publication,
                            root,
                            *logical_len,
                            current_len - *logical_len,
                            Cursor::new(&[]),
                        )?;
                    }
                    Ordering::Less => {
                        (root, _) = replace(
                            publication,
                            root,
                            current_len,
                            0,
                            ZeroReader(*logical_len - current_len),
                        )?;
                    }
                    Ordering::Equal => {}
                }
                Ok(root.0)
            }
            NodeContent::Directory { base, changes } => {
                let root = match base {
                    Some(root) => *root,
                    None => empty_directory(publication)?,
                };
                let mut entries = Vec::with_capacity(changes.len());
                for (name, desired) in changes {
                    let inode = desired
                        .map(|child| {
                            canonical_ids
                                .get(&child)
                                .copied()
                                .ok_or(MountedError::Corrupt)
                        })
                        .transpose()?;
                    entries.push((name.clone(), inode));
                }
                Ok(
                    layerfs_core::logical::apply_directory_changes(publication, root, entries)?
                        .0
                         .0,
                )
            }
            NodeContent::Symlink { target } => Ok(layerfs_core::logical::symlink_content(
                publication,
                target.clone(),
            )?),
        }
    }
}
