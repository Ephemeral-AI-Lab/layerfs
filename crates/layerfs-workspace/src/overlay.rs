use crate::model::{Attr, Data, DirectoryData, FileData, Node, NodeId, Workspace};
use layerfs_storage_core::internal::StagedChange;
use layerfs_storage_core::{Change, Result, StorageError};
use std::collections::{BTreeMap, BTreeSet};

impl Workspace {
    pub fn create_file(&mut self, parent: NodeId, name: &[u8], mode: u32) -> Result<Attr> {
        self.ensure_active()?;
        let path = self.child_path(parent, name)?;
        let node = self.new_spool_node(mode & 0o777, path.clone())?;
        self.insert_name(parent, name, node)?;
        self.mutations.push(StagedChange::Inline(Change::Write {
            path,
            bytes: Vec::new(),
            mode: mode & 0o777,
        }));
        self.attr(node)
    }

    pub fn mkdir(&mut self, parent: NodeId, name: &[u8], mode: u32) -> Result<Attr> {
        self.ensure_active()?;
        let path = self.child_path(parent, name)?;
        let node = self.allocate(Node {
            canonical: None,
            paths: BTreeSet::from([path.clone()]),
            mode: mode & 0o1777,
            links: 2,
            pins: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            data: Data::Directory(DirectoryData {
                base: None,
                changes: BTreeMap::new(),
            }),
        });
        self.insert_name(parent, name, node)?;
        self.mutations.push(StagedChange::Inline(Change::Mkdir {
            path,
            mode: mode & 0o1777,
        }));
        self.attr(node)
    }

    pub fn symlink(&mut self, parent: NodeId, name: &[u8], target: Vec<u8>) -> Result<Attr> {
        self.ensure_active()?;
        if target.len() > 4096 || target.contains(&0) {
            return Err(StorageError::InvalidInput("symlink"));
        }
        let path = self.child_path(parent, name)?;
        let node = self.allocate(Node {
            canonical: None,
            paths: BTreeSet::from([path.clone()]),
            mode: 0o777,
            links: 1,
            pins: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            data: Data::Symlink(target.clone()),
        });
        self.insert_name(parent, name, node)?;
        self.mutations
            .push(StagedChange::Inline(Change::Symlink { path, target }));
        self.attr(node)
    }

    pub fn link(&mut self, node: NodeId, parent: NodeId, name: &[u8]) -> Result<Attr> {
        self.ensure_active()?;
        if matches!(
            self.nodes
                .get(&node)
                .ok_or(StorageError::NotFound("node"))?
                .data,
            Data::Directory(_)
        ) {
            return Err(StorageError::InvalidInput("directory link"));
        }
        let source = self.path_of(node)?;
        let target = self.child_path(parent, name)?;
        self.insert_name(parent, name, node)?;
        let value = self.nodes.get_mut(&node).unwrap();
        value.links += 1;
        value.paths.insert(target.clone());
        self.mutations
            .push(StagedChange::Inline(Change::HardLink { source, target }));
        self.attr(node)
    }

    pub fn unlink(&mut self, parent: NodeId, name: &[u8], directory: bool) -> Result<()> {
        self.ensure_active()?;
        let node = self.lookup_node(parent, name)?;
        let value = self.nodes.get(&node).unwrap();
        if directory != matches!(value.data, Data::Directory(_)) {
            return Err(StorageError::InvalidInput("unlink kind"));
        }
        if directory && !self.directory_entries(node)?.is_empty() {
            return Err(StorageError::InvalidInput("directory not empty"));
        }
        let path = self.child_path(parent, name)?;
        self.directory_mut(parent)?
            .changes
            .insert(name.to_vec(), None);
        let value = self.nodes.get_mut(&node).unwrap();
        value.links = value.links.saturating_sub(1);
        value.paths.remove(&path);
        self.mutations
            .push(StagedChange::Inline(Change::Remove { path }));
        self.reclaim(node);
        Ok(())
    }

    pub fn rename(
        &mut self,
        parent: NodeId,
        name: &[u8],
        target_parent: NodeId,
        target: &[u8],
        no_replace: bool,
    ) -> Result<()> {
        self.ensure_active()?;
        let node = self.lookup_node(parent, name)?;
        let source = self.child_path(parent, name)?;
        let destination = self.child_path(target_parent, target)?;
        if source == destination {
            return Ok(());
        }
        let source_directory = matches!(self.nodes[&node].data, Data::Directory(_));
        let existing = match self.lookup_node(target_parent, target) {
            Ok(existing) if existing == node => return Ok(()),
            Ok(existing) => Some(existing),
            Err(StorageError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        if no_replace && existing.is_some() {
            return Err(StorageError::InvalidInput("rename target"));
        }
        if let Some(existing) = existing {
            let target_directory = matches!(self.nodes[&existing].data, Data::Directory(_));
            if source_directory != target_directory {
                return Err(StorageError::InvalidInput("rename type"));
            }
        }
        if source_directory {
            let target_parent_path = self.path_of(target_parent)?;
            if target_parent_path == source
                || (target_parent_path.starts_with(&source)
                    && target_parent_path.as_bytes().get(source.len()) == Some(&b'/'))
            {
                return Err(StorageError::InvalidInput("rename descendant"));
            }
        }
        if let Some(existing) = existing {
            if source_directory && !self.directory_is_empty(existing)? {
                return Err(StorageError::InvalidInput("directory not empty"));
            }
        }
        if existing.is_some() {
            self.unlink(target_parent, target, source_directory)?;
        }
        self.directory_mut(parent)?
            .changes
            .insert(name.to_vec(), None);
        self.directory_mut(target_parent)?
            .changes
            .insert(target.to_vec(), Some(node));
        self.replace_path_prefix(&source, &destination);
        self.mutations.push(StagedChange::Inline(Change::Rename {
            source,
            target: destination,
        }));
        Ok(())
    }

    pub fn pin(&mut self, node: NodeId, truncate: bool) -> Result<()> {
        if !matches!(
            self.nodes
                .get(&node)
                .ok_or(StorageError::NotFound("node"))?
                .data,
            Data::File(_)
        ) {
            return Err(StorageError::InvalidInput("open"));
        }
        if truncate {
            self.truncate(node, 0)?;
        }
        self.nodes.get_mut(&node).unwrap().pins += 1;
        Ok(())
    }

    pub fn unpin(&mut self, node: NodeId) -> Result<()> {
        let value = self
            .nodes
            .get_mut(&node)
            .ok_or(StorageError::NotFound("node"))?;
        value.pins = value
            .pins
            .checked_sub(1)
            .ok_or(StorageError::Integrity("node pin"))?;
        self.reclaim(node);
        Ok(())
    }

    pub fn chmod(&mut self, node: NodeId, mode: u32) -> Result<()> {
        self.ensure_active()?;
        let path = self.path_of(node)?;
        self.nodes
            .get_mut(&node)
            .ok_or(StorageError::NotFound("node"))?
            .mode = mode & 0o1777;
        self.mutations
            .push(StagedChange::Inline(Change::SetMode { path, mode }));
        Ok(())
    }

    pub fn set_mtime(&mut self, node: NodeId, seconds: i64, nanos: u32) -> Result<()> {
        self.ensure_active()?;
        if nanos > 999_999_999 {
            return Err(StorageError::InvalidInput("mtime"));
        }
        let path = self.path_of(node)?;
        let value = self
            .nodes
            .get_mut(&node)
            .ok_or(StorageError::NotFound("node"))?;
        value.mtime_seconds = seconds;
        value.mtime_nanoseconds = nanos;
        self.mutations.push(StagedChange::Inline(Change::SetMtime {
            path,
            seconds,
            nanoseconds: nanos,
        }));
        Ok(())
    }

    fn insert_name(&mut self, parent: NodeId, name: &[u8], node: NodeId) -> Result<()> {
        match self.lookup_node(parent, name) {
            Ok(_) => return Err(StorageError::InvalidInput("name exists")),
            Err(StorageError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
        self.directory_mut(parent)?
            .changes
            .insert(name.to_vec(), Some(node));
        Ok(())
    }

    fn replace_path_prefix(&mut self, source: &str, target: &str) {
        for node in self.nodes.values_mut() {
            node.paths = node
                .paths
                .iter()
                .map(|path| {
                    if path == source {
                        target.to_owned()
                    } else if path.starts_with(source)
                        && path.as_bytes().get(source.len()) == Some(&b'/')
                    {
                        format!("{target}{}", &path[source.len()..])
                    } else {
                        path.clone()
                    }
                })
                .collect();
        }
    }

    fn reclaim(&mut self, node: NodeId) {
        if self
            .nodes
            .get(&node)
            .is_some_and(|value| value.paths.is_empty() && value.pins == 0)
        {
            self.dirty.remove(&node);
            if let Some(value) = self.nodes.remove(&node) {
                if let Some(inode) = value.canonical {
                    self.canonical_nodes.remove(&inode);
                }
                if let Data::File(FileData::Overlay { spool, charged, .. }) = value.data {
                    self.spool_bytes = self
                        .spool_bytes
                        .saturating_sub(charged.iter().map(|(start, end)| end - start).sum());
                    let _ = std::fs::remove_file(spool);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ROOT;
    use layerfs_branch_store::BranchStore;
    use layerfs_layer_store::LayerStore;
    use std::sync::Arc;

    #[derive(Debug, Eq, PartialEq)]
    struct Snapshot {
        nodes: std::collections::HashMap<NodeId, Node>,
        canonical_nodes: std::collections::HashMap<layerfs_core::inode::InodeId, NodeId>,
        dirty: BTreeSet<NodeId>,
        mutations: Vec<StagedChange>,
        next_node: u64,
        spool_bytes: u64,
    }

    fn snapshot(workspace: &Workspace) -> Snapshot {
        Snapshot {
            nodes: workspace.nodes.clone(),
            canonical_nodes: workspace.canonical_nodes.clone(),
            dirty: workspace.dirty.clone(),
            mutations: workspace.mutations.clone(),
            next_node: workspace.next_node,
            spool_bytes: workspace.spool_bytes,
        }
    }

    fn fixture(label: &str) -> (std::path::PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-rename-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let layer = Arc::new(LayerStore::open(root.join("layer.sqlite")).unwrap());
        let (history, genesis) = layer.provision().unwrap();
        let branch = BranchStore::open(root.join("branch.sqlite"), layer).unwrap();
        let record = branch
            .create_branch_from_layer(history.id, genesis.id)
            .unwrap();
        let workspace = Workspace::open(branch, record.id, root.join("spool")).unwrap();
        (root, workspace)
    }

    #[test]
    fn rename_validates_every_noop_and_rejection_before_mutation() {
        let (root, mut workspace) = fixture("same-path");
        workspace.create_file(ROOT, b"a", 0o600).unwrap();
        let before = snapshot(&workspace);
        workspace.rename(ROOT, b"a", ROOT, b"a", false).unwrap();
        assert_eq!(snapshot(&workspace), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("same-inode");
        let file = workspace.create_file(ROOT, b"a", 0o600).unwrap();
        workspace.link(file.node, ROOT, b"b").unwrap();
        let before = snapshot(&workspace);
        workspace.rename(ROOT, b"a", ROOT, b"b", false).unwrap();
        assert_eq!(snapshot(&workspace), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("file-over-directory");
        workspace.create_file(ROOT, b"file", 0o600).unwrap();
        workspace.mkdir(ROOT, b"directory", 0o700).unwrap();
        assert_rejected(&mut workspace, |workspace| {
            workspace.rename(ROOT, b"file", ROOT, b"directory", false)
        });
        assert_rejected(&mut workspace, |workspace| {
            workspace.rename(ROOT, b"directory", ROOT, b"file", false)
        });
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("nonempty-directory");
        workspace.mkdir(ROOT, b"source", 0o700).unwrap();
        let target = workspace.mkdir(ROOT, b"target", 0o700).unwrap();
        workspace.create_file(target.node, b"child", 0o600).unwrap();
        assert_rejected(&mut workspace, |workspace| {
            workspace.rename(ROOT, b"source", ROOT, b"target", false)
        });
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("descendant");
        let source = workspace.mkdir(ROOT, b"source", 0o700).unwrap();
        let child = workspace.mkdir(source.node, b"child", 0o700).unwrap();
        workspace.create_file(child.node, b"target", 0o600).unwrap();
        assert_rejected(&mut workspace, |workspace| {
            workspace.rename(ROOT, b"source", child.node, b"target", false)
        });
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_spool_io_does_not_advance_the_overlay() {
        let (root, mut workspace) = fixture("failed-write");
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap();
        workspace.write(file.node, 0, b"base").unwrap();
        let Data::File(FileData::Overlay { spool, .. }) = &workspace.nodes[&file.node].data else {
            panic!("expected overlay")
        };
        std::fs::remove_file(spool).unwrap();
        let before = snapshot(&workspace);
        assert!(workspace.write(file.node, 4, b"lost").is_err());
        assert_eq!(snapshot(&workspace), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();

        let (root, mut workspace) = fixture("failed-truncate");
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap();
        workspace.write(file.node, 0, b"base").unwrap();
        let Data::File(FileData::Overlay { spool, .. }) = &workspace.nodes[&file.node].data else {
            panic!("expected overlay")
        };
        std::fs::remove_file(spool).unwrap();
        let before = snapshot(&workspace);
        assert!(workspace.truncate(file.node, 2).is_err());
        assert_eq!(snapshot(&workspace), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn assert_rejected(
        workspace: &mut Workspace,
        operation: impl FnOnce(&mut Workspace) -> Result<()>,
    ) {
        let before = snapshot(workspace);
        assert!(operation(workspace).is_err());
        assert_eq!(snapshot(workspace), before);
    }
}
