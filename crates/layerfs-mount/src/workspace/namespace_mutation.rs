impl MountedWorkspace {
    pub fn create_file(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        mode: u32,
    ) -> Result<(MountedAttr, MountedHandleId), MountedError> {
        self.preflight_handle(false)?;
        let node =
            self.create_node(parent, name, MountedFileType::RegularFile, mode, Vec::new())?;
        let handle = self.open_file(node, false)?;
        self.counters.creates += 1;
        Ok((self.getattr(node)?, handle))
    }

    pub fn mknod_file(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        mode: u32,
    ) -> Result<MountedAttr, MountedError> {
        let node =
            self.create_node(parent, name, MountedFileType::RegularFile, mode, Vec::new())?;
        self.counters.creates += 1;
        self.getattr(node)
    }

    pub fn mkdir(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        mode: u32,
    ) -> Result<MountedAttr, MountedError> {
        let node = self.create_node(parent, name, MountedFileType::Directory, mode, Vec::new())?;
        self.counters.mkdirs += 1;
        self.getattr(node)
    }

    pub fn symlink(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        target: Vec<u8>,
    ) -> Result<MountedAttr, MountedError> {
        SymlinkStateV1::new(target.clone())?;
        let node = self.create_node(parent, name, MountedFileType::Symlink, 0o777, target)?;
        self.counters.symlinks += 1;
        self.getattr(node)
    }

    pub fn readlink(&self, node: MountedNodeId) -> Result<Vec<u8>, MountedError> {
        match &self.nodes.get(&node).ok_or(MountedError::NotFound)?.content {
            NodeContent::Symlink { target } => Ok(target.clone()),
            _ => Err(MountedError::InvalidRange),
        }
    }

    pub fn link(
        &mut self,
        node: MountedNodeId,
        parent: MountedNodeId,
        name: &[u8],
    ) -> Result<MountedAttr, MountedError> {
        self.require_live()?;
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        if self.find_child(parent, &name)?.is_some() {
            return Err(MountedError::AlreadyExists);
        }
        let entry = self.nodes.get(&node).ok_or(MountedError::NotFound)?;
        if entry.kind != MountedFileType::RegularFile || entry.deleted {
            return Err(MountedError::Unsupported);
        }
        let namespace_refs = entry
            .namespace_refs
            .checked_add(1)
            .ok_or(MountedError::ResourceExhausted)?;
        self.preflight_dirty(&[node, parent], 0)?;
        let mutation = self.prepare_directory_entry(parent, name, Some(node))?;
        self.apply_directory_mutations([mutation])?;
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        entry.namespace_refs = namespace_refs;
        entry.dirty_links = entry.canonical.is_some();
        self.sync_node_state(node);
        self.counters.links += 1;
        self.getattr(node)
    }

    pub fn unlink(&mut self, parent: MountedNodeId, name: &[u8]) -> Result<(), MountedError> {
        self.unlink_inner(parent, name, false)
    }

    pub fn rmdir(&mut self, parent: MountedNodeId, name: &[u8]) -> Result<(), MountedError> {
        self.unlink_inner(parent, name, true)
    }

    fn unlink_inner(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        directory: bool,
    ) -> Result<(), MountedError> {
        self.require_live()?;
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        let node = self
            .find_child(parent, &name)?
            .ok_or(MountedError::NotFound)?;
        let kind = self.nodes.get(&node).ok_or(MountedError::Corrupt)?.kind;
        if directory != (kind == MountedFileType::Directory) {
            return Err(if directory {
                MountedError::NotDirectory
            } else {
                MountedError::IsDirectory
            });
        }
        if directory && !self.directory_is_empty(node)? {
            return Err(MountedError::NotEmpty);
        }
        self.preflight_dirty(&[parent, node], 0)?;
        let mutation = self.prepare_directory_entry(parent, name, None)?;
        self.apply_directory_mutations([mutation])?;
        self.detach_node(node)?;
        self.counters.unlinks += 1;
        if let Err(error) = self.normalize_spool() {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.observe_resources()?;
        Ok(())
    }

    pub fn rename(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
        new_parent: MountedNodeId,
        new_name: &[u8],
        no_replace: bool,
    ) -> Result<(), MountedError> {
        self.require_live()?;
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        let new_name =
            CanonicalName::from_bytes(new_name).map_err(|_| MountedError::InvalidName)?;
        let source = self
            .find_child(parent, &name)?
            .ok_or(MountedError::NotFound)?;
        if parent == new_parent && name == new_name {
            return Ok(());
        }
        let source_kind = self.nodes.get(&source).ok_or(MountedError::Corrupt)?.kind;
        if source_kind == MountedFileType::Directory {
            let mut ancestor = new_parent;
            loop {
                if ancestor == source {
                    return Err(MountedError::InvalidRange);
                }
                let next = self
                    .nodes
                    .get(&ancestor)
                    .ok_or(MountedError::NotFound)?
                    .parent;
                if next == ancestor {
                    break;
                }
                ancestor = next;
            }
        }
        let target = self.find_child(new_parent, &new_name)?;
        if let Some(target) = target {
            if no_replace {
                return Err(MountedError::AlreadyExists);
            }
            if target == source {
                return Ok(());
            }
            let target_kind = self.nodes.get(&target).ok_or(MountedError::Corrupt)?.kind;
            if (source_kind == MountedFileType::Directory)
                != (target_kind == MountedFileType::Directory)
            {
                return Err(if source_kind == MountedFileType::Directory {
                    MountedError::NotDirectory
                } else {
                    MountedError::IsDirectory
                });
            }
            if target_kind == MountedFileType::Directory && !self.directory_is_empty(target)? {
                return Err(MountedError::NotEmpty);
            }
        }
        let mut affected = vec![parent, new_parent];
        if let Some(target) = target {
            affected.push(target);
        }
        self.preflight_dirty(&affected, 0)?;
        let mutations = vec![
            self.prepare_directory_entry(parent, name, None)?,
            self.prepare_directory_entry(new_parent, new_name, Some(source))?,
        ];
        self.apply_directory_mutations(mutations)?;
        if let Some(target) = target {
            self.detach_node(target)?;
        }
        if source_kind == MountedFileType::Directory {
            self.nodes
                .get_mut(&source)
                .ok_or(MountedError::Corrupt)?
                .parent = new_parent;
        }
        self.counters.renames += 1;
        if let Err(error) = self.normalize_spool() {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.observe_resources()?;
        Ok(())
    }

    pub fn truncate(&mut self, node: MountedNodeId, length: u64) -> Result<(), MountedError> {
        self.require_live()?;
        let entry = self.nodes.get(&node).ok_or(MountedError::NotFound)?;
        let deleted = entry.deleted;
        let old_len = match &entry.content {
            NodeContent::File { logical_len, .. } => *logical_len,
            _ => return Err(MountedError::IsDirectory),
        };
        let projected_logical = if deleted {
            if length > MAX_LOGICAL_FILE_BYTES {
                return Err(MountedError::NoSpace);
            }
            None
        } else {
            self.preflight_dirty(&[node], 0)?;
            Some(self.preflight_logical_file(old_len, length)?)
        };
        let timestamp = now_timestamp()?;
        let (removed, old_count, new_count) = {
            let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
            let NodeContent::File {
                base_visible_len,
                logical_len,
                ranges,
                plan,
                ..
            } = &mut entry.content
            else {
                return Err(MountedError::IsDirectory);
            };
            let old_count = ranges.len();
            let removed = truncate_dirty_ranges(ranges, length);
            *base_visible_len = (*base_visible_len).min(length);
            *logical_len = length;
            *plan = None;
            if !entry.deleted {
                entry.dirty_content = true;
                entry.dirty_metadata = true;
            }
            entry.mtime_seconds = timestamp.0;
            entry.mtime_nanoseconds = timestamp.1;
            (removed, old_count, ranges.len())
        };
        self.live_ranges = self.live_ranges - old_count + new_count;
        self.spool.live = self.spool.live.saturating_sub(removed);
        if let Some(projected) = projected_logical {
            self.logical_workspace_bytes = projected;
        }
        self.sync_node_state(node);
        if let Err(error) = self.normalize_spool() {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.observe_resources()?;
        Ok(())
    }

    pub fn chmod(&mut self, node: MountedNodeId, mode: u32) -> Result<MountedAttr, MountedError> {
        self.require_live()?;
        if !self.nodes.get(&node).ok_or(MountedError::NotFound)?.deleted {
            self.preflight_dirty(&[node], 0)?;
        }
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
        entry.mode = mode
            & if entry.kind == MountedFileType::Directory {
                0o1777
            } else {
                0o777
            };
        if !entry.deleted {
            entry.dirty_metadata = true;
        }
        self.sync_node_state(node);
        self.getattr(node)
    }

    pub fn set_mtime(
        &mut self,
        node: MountedNodeId,
        seconds: i64,
        nanoseconds: u32,
    ) -> Result<MountedAttr, MountedError> {
        self.require_live()?;
        if nanoseconds > 999_999_999 {
            return Err(MountedError::InvalidRange);
        }
        if !self.nodes.get(&node).ok_or(MountedError::NotFound)?.deleted {
            self.preflight_dirty(&[node], 0)?;
        }
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
        entry.mtime_seconds = seconds;
        entry.mtime_nanoseconds = nanoseconds;
        if !entry.deleted {
            entry.dirty_metadata = true;
        }
        self.sync_node_state(node);
        self.getattr(node)
    }
}
