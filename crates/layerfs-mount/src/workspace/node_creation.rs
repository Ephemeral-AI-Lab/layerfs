impl MountedWorkspace {
    fn create_node(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        kind: MountedFileType,
        mode: u32,
        target: Vec<u8>,
    ) -> Result<MountedNodeId, MountedError> {
        self.require_live()?;
        if self.nodes.len() == MAX_MOUNTED_NODES {
            return Err(MountedError::ResourceExhausted);
        }
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        if self.nodes.get(&parent).ok_or(MountedError::NotFound)?.kind != MountedFileType::Directory
        {
            return Err(MountedError::NotDirectory);
        }
        if self.directory_changes == MAX_DIRECTORY_CHANGES {
            let NodeContent::Directory { changes, .. } = &self
                .nodes
                .get(&parent)
                .ok_or(MountedError::NotFound)?
                .content
            else {
                return Err(MountedError::NotDirectory);
            };
            match changes.get(&name) {
                Some(None) => {}
                Some(Some(_)) => return Err(MountedError::AlreadyExists),
                None => return Err(MountedError::ResourceExhausted),
            }
        }
        if self.find_child(parent, &name)?.is_some() {
            return Err(MountedError::AlreadyExists);
        }
        self.preflight_dirty(&[parent], 1)?;
        let id = MountedNodeId(self.next_node);
        let (mtime_seconds, mtime_nanoseconds) = now_timestamp()?;
        let content = match kind {
            MountedFileType::RegularFile => NodeContent::File {
                base: None,
                base_visible_len: 0,
                logical_len: 0,
                ranges: BTreeMap::new(),
                plan: None,
            },
            MountedFileType::Directory => NodeContent::Directory {
                base: None,
                changes: BTreeMap::new(),
            },
            MountedFileType::Symlink => NodeContent::Symlink { target },
        };
        let mutation = self.prepare_directory_entry(parent, name, Some(id))?;
        self.preflight_directory_mutations(std::slice::from_ref(&mutation))?;
        let allocated = self.allocate_node()?;
        debug_assert_eq!(allocated, id);
        self.nodes.insert(
            id,
            MountedNode {
                canonical: None,
                record: None,
                kind,
                mode: mode
                    & if kind == MountedFileType::Directory {
                        0o1777
                    } else {
                        0o777
                    },
                mtime_seconds,
                mtime_nanoseconds,
                namespace_refs: 1,
                parent,
                lookup_refs: 1,
                open_refs: 0,
                deleted: false,
                dirty_content: true,
                dirty_metadata: true,
                dirty_links: false,
                directory_mtime_before: None,
                content,
            },
        );
        self.lookup_refs = self.lookup_refs.saturating_add(1);
        self.sync_node_state(id);
        self.apply_directory_mutations([mutation])?;
        self.observe_resources()?;
        Ok(id)
    }

    fn find_child(
        &mut self,
        parent: MountedNodeId,
        name: &CanonicalName,
    ) -> Result<Option<MountedNodeId>, MountedError> {
        let (change, base) = match &self
            .nodes
            .get(&parent)
            .ok_or(MountedError::NotFound)?
            .content
        {
            NodeContent::Directory { base, changes } => (changes.get(name).copied(), *base),
            _ => return Err(MountedError::NotDirectory),
        };
        if let Some(change) = change {
            return Ok(change);
        }
        let Some(base) = base else {
            return Ok(None);
        };
        let inode = directory_lookup(&self.working, base, name, &mut NamespaceCounters::default())?;
        inode
            .map(|inode| self.ensure_canonical_node(inode, parent))
            .transpose()
    }

    fn prepare_directory_entry(
        &self,
        parent: MountedNodeId,
        name: CanonicalName,
        desired: Option<MountedNodeId>,
    ) -> Result<DirectoryMutation, MountedError> {
        let (base, change_exists) = match &self
            .nodes
            .get(&parent)
            .ok_or(MountedError::NotFound)?
            .content
        {
            NodeContent::Directory { base, changes } => (*base, changes.contains_key(&name)),
            _ => return Err(MountedError::NotDirectory),
        };
        let base_inode = base
            .map(|root| {
                directory_lookup(
                    &self.working,
                    root,
                    &name,
                    &mut NamespaceCounters::default(),
                )
            })
            .transpose()?
            .flatten();
        let desired_inode =
            desired.and_then(|id| self.nodes.get(&id).and_then(|node| node.canonical));
        let normalized = match desired {
            None if base_inode.is_none() => None,
            Some(_) if base_inode.is_some() && desired_inode == base_inode => None,
            value => Some(value),
        };
        let change_delta = match (change_exists, normalized.is_some()) {
            (false, true) => 1,
            (true, false) => -1,
            _ => 0,
        };
        Ok(DirectoryMutation {
            parent,
            name,
            normalized,
            change_delta,
            timestamp: now_timestamp()?,
        })
    }

    fn apply_directory_mutations(
        &mut self,
        mutations: impl IntoIterator<Item = DirectoryMutation>,
    ) -> Result<(), MountedError> {
        let mutations = mutations.into_iter().collect::<Vec<_>>();
        self.preflight_directory_mutations(&mutations)?;
        for mutation in mutations {
            let result = (|| {
                let parent_node = self
                    .nodes
                    .get_mut(&mutation.parent)
                    .ok_or(MountedError::Corrupt)?;
                let NodeContent::Directory { changes, .. } = &mut parent_node.content else {
                    return Err(MountedError::Corrupt);
                };
                match mutation.normalized {
                    Some(value) => {
                        changes.insert(mutation.name.clone(), value);
                    }
                    None => {
                        changes.remove(&mutation.name);
                    }
                }
                self.directory_changes = usize::try_from(
                    i64::try_from(self.directory_changes)
                        .map_err(|_| MountedError::Indeterminate)?
                        .checked_add(i64::from(mutation.change_delta))
                        .ok_or(MountedError::Indeterminate)?,
                )
                .map_err(|_| MountedError::Indeterminate)?;
                if parent_node.directory_mtime_before.is_none() {
                    parent_node.directory_mtime_before = Some((
                        parent_node.mtime_seconds,
                        parent_node.mtime_nanoseconds,
                        parent_node.dirty_metadata,
                    ));
                }
                parent_node.dirty_content = parent_node.canonical.is_none() || !changes.is_empty();
                if changes.is_empty() {
                    if let Some((seconds, nanos, dirty)) = parent_node.directory_mtime_before.take()
                    {
                        parent_node.mtime_seconds = seconds;
                        parent_node.mtime_nanoseconds = nanos;
                        parent_node.dirty_metadata = dirty;
                    }
                } else {
                    parent_node.mtime_seconds = mutation.timestamp.0;
                    parent_node.mtime_nanoseconds = mutation.timestamp.1;
                    parent_node.dirty_metadata = true;
                }
                Ok(())
            })();
            if let Err(error) = result {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(error);
            }
            self.sync_node_state(mutation.parent);
        }
        Ok(())
    }

    fn preflight_directory_mutations(
        &self,
        mutations: &[DirectoryMutation],
    ) -> Result<(), MountedError> {
        let projected = mutations.iter().try_fold(
            i64::try_from(self.directory_changes).map_err(|_| MountedError::ResourceExhausted)?,
            |current, mutation| {
                current
                    .checked_add(i64::from(mutation.change_delta))
                    .ok_or(MountedError::ResourceExhausted)
            },
        )?;
        if !(0..=MAX_DIRECTORY_CHANGES as i64).contains(&projected) {
            return Err(MountedError::ResourceExhausted);
        }
        Ok(())
    }
}
