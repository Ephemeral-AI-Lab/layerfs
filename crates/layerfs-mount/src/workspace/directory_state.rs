impl MountedWorkspace {
    fn detach_node(&mut self, node: MountedNodeId) -> Result<(), MountedError> {
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
        entry.namespace_refs = entry.namespace_refs.saturating_sub(1);
        if entry.namespace_refs != 0 {
            entry.dirty_links = entry.canonical.is_some();
            self.sync_node_state(node);
            return Ok(());
        }
        entry.deleted = true;
        if entry.canonical.is_some() {
            entry.dirty_links = true;
        } else {
            self.counters.created_then_deleted += 1;
            entry.dirty_content = false;
            entry.dirty_metadata = false;
            entry.dirty_links = false;
        }
        let removed_logical = match &entry.content {
            NodeContent::File { logical_len, .. } => *logical_len,
            _ => 0,
        };
        let _ = entry;
        self.logical_workspace_bytes =
            match self.logical_workspace_bytes.checked_sub(removed_logical) {
                Some(bytes) => bytes,
                None => {
                    self.lifecycle = MountedLifecycle::Incomplete;
                    return Err(MountedError::Indeterminate);
                }
            };
        self.sync_node_state(node);
        if let Err(error) = self.finalize_deleted_pending(node) {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.reclaim_node(node);
        Ok(())
    }

    fn directory_is_empty(&mut self, node: MountedNodeId) -> Result<bool, MountedError> {
        let (base, changes) = match &self.nodes.get(&node).ok_or(MountedError::NotFound)?.content {
            NodeContent::Directory { base, changes } => (*base, changes.clone()),
            _ => return Err(MountedError::NotDirectory),
        };
        if changes.values().any(Option::is_some) {
            return Ok(false);
        }
        let Some(base) = base else {
            return Ok(true);
        };
        let mut after = None;
        loop {
            let page = directory_page_after(
                &self.working,
                base,
                after.as_ref(),
                DIRECTORY_PAGE_ENTRIES,
                DIRECTORY_PAGE_BYTES,
                &mut NamespaceCounters::default(),
            )?;
            if page
                .entries
                .iter()
                .any(|(name, _)| !matches!(changes.get(name), Some(None)))
            {
                return Ok(false);
            }
            after = page.continuation;
            if after.is_none() {
                return Ok(true);
            }
        }
    }

    fn next_directory_entry(
        &mut self,
        cursor: &mut DirectoryCursor,
    ) -> Result<Option<MountedDirEntry>, MountedError> {
        if cursor.cookie == 0 {
            cursor.cookie = 1;
            return Ok(Some(MountedDirEntry {
                node: cursor.node,
                name: b".".to_vec(),
                kind: MountedFileType::Directory,
                next_offset: cursor.cookie,
            }));
        }
        if cursor.cookie == 1 {
            cursor.cookie = 2;
            let parent = self
                .nodes
                .get(&cursor.node)
                .ok_or(MountedError::NotFound)?
                .parent;
            return Ok(Some(MountedDirEntry {
                node: parent,
                name: b"..".to_vec(),
                kind: MountedFileType::Directory,
                next_offset: cursor.cookie,
            }));
        }
        loop {
            self.fill_directory_base(cursor)?;
            let change = match &self
                .nodes
                .get(&cursor.node)
                .ok_or(MountedError::NotFound)?
                .content
            {
                NodeContent::Directory { changes, .. } => changes
                    .range((
                        cursor.scan_after.as_ref().map_or(Unbounded, Excluded),
                        Unbounded,
                    ))
                    .next()
                    .map(|(name, node)| (name.clone(), *node)),
                _ => return Err(MountedError::NotDirectory),
            };
            let base = cursor.base.front().cloned();
            let (name, desired) = match (base, change) {
                (None, None) => return Ok(None),
                (Some((name, inode)), None) => {
                    cursor.base.pop_front();
                    let node = self.ensure_canonical_node(inode, cursor.node)?;
                    (name, Some(node))
                }
                (None, Some(change)) => change,
                (Some((base_name, inode)), Some((change_name, desired))) => {
                    match base_name.cmp(&change_name) {
                        Ordering::Less => {
                            cursor.base.pop_front();
                            let node = self.ensure_canonical_node(inode, cursor.node)?;
                            (base_name, Some(node))
                        }
                        Ordering::Equal => {
                            cursor.base.pop_front();
                            (change_name, desired)
                        }
                        Ordering::Greater => (change_name, desired),
                    }
                }
            };
            cursor.scan_after = Some(name.clone());
            let Some(node) = desired else {
                continue;
            };
            let kind = self.nodes.get(&node).ok_or(MountedError::Corrupt)?.kind;
            cursor.cookie = cursor
                .cookie
                .checked_add(1)
                .ok_or(MountedError::ResourceExhausted)?;
            return Ok(Some(MountedDirEntry {
                node,
                name: name.as_bytes().to_vec(),
                kind,
                next_offset: cursor.cookie,
            }));
        }
    }

    fn fill_directory_base(&mut self, cursor: &mut DirectoryCursor) -> Result<(), MountedError> {
        if !cursor.base.is_empty() || cursor.base_done {
            return Ok(());
        }
        let base = match &self
            .nodes
            .get(&cursor.node)
            .ok_or(MountedError::NotFound)?
            .content
        {
            NodeContent::Directory { base, .. } => *base,
            _ => return Err(MountedError::NotDirectory),
        };
        let Some(base) = base else {
            cursor.base_done = true;
            return Ok(());
        };
        let page = directory_page_after(
            &self.working,
            base,
            cursor.base_after.as_ref(),
            DIRECTORY_PAGE_ENTRIES,
            DIRECTORY_PAGE_BYTES,
            &mut NamespaceCounters::default(),
        )?;
        cursor.base.extend(page.entries);
        cursor.base_after = page.continuation;
        cursor.base_done = cursor.base_after.is_none();
        Ok(())
    }
}
