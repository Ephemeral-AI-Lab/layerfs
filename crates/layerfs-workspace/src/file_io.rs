use crate::cow_tree::{Data, FileData, Node, NodeId, Workspace};
use crate::file_edit::{
    Piece, PieceTree, MAX_EDITS_PER_FILE, MAX_INLINE_PER_EDIT, MAX_INLINE_PER_WORKSPACE,
    MAX_PIECE_ALLOCATION,
};
use layerfs_content::file::rope::read_range;
use layerfs_layerstack_store::{CoreReader, Result, SnapshotReader, StoreError};
use std::fs::{File, OpenOptions};
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::Path;
use std::sync::Arc;

pub(crate) struct EditCheckpoint {
    node: NodeId,
    value: crate::cow_tree::Node,
    dirty: bool,
    spool_bytes: u64,
    inline_bytes: u64,
    piece_allocation_bytes: u64,
    spool_write_metrics: crate::cow_tree::SpoolWriteMetrics,
    mutation_generation: u64,
    mutation_paths: std::collections::BTreeMap<String, u64>,
}

pub struct ReadPlan {
    reader: SnapshotReader,
    requested: u64,
    source: ReadSource,
}
enum ReadSource {
    Base(layerfs_content::file::rope::FileStateRoot, u64, u64),
    Edited(std::path::PathBuf, Vec<Piece>),
}

impl ReadPlan {
    pub fn read(self) -> Result<Vec<u8>> {
        let started = std::time::Instant::now();
        let reader = self.reader.clone();
        let output = match self.source {
            ReadSource::Base(root, start, end) => read_base(&self.reader, root, start, end),
            ReadSource::Edited(spool, pieces) => {
                let mut output = Vec::with_capacity(as_usize(self.requested)?);
                let spool = File::open(spool)?;
                for piece in pieces {
                    match piece {
                        Piece::Base { root, offset, len } => {
                            output.extend(read_base(&self.reader, root, offset, offset + len)?)
                        }
                        Piece::Inline { bytes, offset, len } => output
                            .extend_from_slice(&bytes[as_usize(offset)?..as_usize(offset + len)?]),
                        Piece::Zero { len } => output.resize(output.len() + as_usize(len)?, 0),
                        Piece::Spool { offset, len } => {
                            let start = output.len();
                            output.resize(start + as_usize(len)?, 0);
                            read_exact_at(&spool, &mut output[start..], offset)?;
                        }
                    }
                }
                Ok(output)
            }
        }?;
        reader.note_workspace_read(self.requested, output.len() as u64, elapsed_ns(started))?;
        Ok(output)
    }
}

impl Workspace {
    pub(crate) fn note_commit_edit_state(&self) -> Result<()> {
        let mut edits = 0_u64;
        let mut pieces = 0_u64;
        let mut height = 0_u64;
        let mut charge = 0_u64;
        let mut spool_live = 0_u64;
        for node in self.dirty.iter().filter_map(|node| self.nodes.get(node)) {
            if let Data::File(FileData::Edited {
                pieces: tree,
                edits: file_edits,
                ..
            }) = &node.data
            {
                edits = edits.saturating_add(u64::from(*file_edits));
                pieces = pieces.saturating_add(tree.count() as u64);
                height = height.max(tree.height() as u64);
                charge = charge.saturating_add(tree.logical_allocation_charge()?);
                spool_live = spool_live.saturating_add(tree.spool_len());
            }
        }
        layerfs_layerstack_store::note_workspace_commit_edit_state(
            edits,
            pieces,
            height,
            charge,
            self.spool_bytes,
            spool_live,
            self.spool_bytes.saturating_sub(spool_live),
        );
        Ok(())
    }

    pub(crate) fn edit_checkpoint(&self, node: NodeId) -> Result<EditCheckpoint> {
        Ok(EditCheckpoint {
            node,
            value: self
                .nodes
                .get(&node)
                .ok_or(StoreError::NotFound("node"))?
                .clone(),
            dirty: self.dirty.contains(&node),
            spool_bytes: self.spool_bytes,
            inline_bytes: self.inline_bytes,
            piece_allocation_bytes: self.piece_allocation_bytes,
            spool_write_metrics: self.spool_write_metrics,
            mutation_generation: self.mutation_generation,
            mutation_paths: self.mutation_paths.clone(),
        })
    }

    pub(crate) fn restore_edit(&mut self, checkpoint: EditCheckpoint) -> Result<()> {
        if matches!(checkpoint.value.data, Data::File(FileData::Base { .. })) {
            self.open_spools.remove(&checkpoint.node);
            if let Data::File(FileData::Edited { spool, .. }) = &self.nodes[&checkpoint.node].data {
                if spool.exists() {
                    std::fs::remove_file(spool)?;
                }
            }
        }
        self.nodes.insert(checkpoint.node, checkpoint.value);
        if checkpoint.dirty {
            self.dirty.insert(checkpoint.node);
        } else {
            self.dirty.remove(&checkpoint.node);
        }
        self.spool_bytes = checkpoint.spool_bytes;
        self.inline_bytes = checkpoint.inline_bytes;
        self.piece_allocation_bytes = checkpoint.piece_allocation_bytes;
        self.spool_write_metrics = checkpoint.spool_write_metrics;
        self.mutation_generation = checkpoint.mutation_generation;
        self.mutation_paths = checkpoint.mutation_paths;
        Ok(())
    }

    pub fn read(&self, node: NodeId, offset: u64, size: usize) -> Result<Vec<u8>> {
        self.read_plan(node, offset, size)?.read()
    }
    pub fn read_plan(&self, node: NodeId, offset: u64, size: usize) -> Result<ReadPlan> {
        let end = self
            .attr(node)?
            .size
            .min(offset.saturating_add(size as u64));
        let source = match &self
            .nodes
            .get(&node)
            .ok_or(StoreError::NotFound("node"))?
            .data
        {
            Data::File(FileData::Base { root, .. }) => ReadSource::Base(*root, offset, end),
            Data::File(FileData::Edited { spool, pieces, .. }) => {
                ReadSource::Edited(spool.clone(), pieces.range(offset, end)?)
            }
            _ => return Err(StoreError::InvalidInput("read")),
        };
        Ok(ReadPlan {
            reader: self.reader.clone(),
            requested: end.saturating_sub(offset),
            source,
        })
    }

    pub fn write(&mut self, node: NodeId, offset: u64, bytes: &[u8]) -> Result<usize> {
        self.write_inner(node, offset, bytes.len(), Some(bytes))
    }
    pub(crate) fn write_zero(&mut self, node: NodeId, offset: u64, len: usize) -> Result<usize> {
        self.invalidate_capture();
        self.write_inner(node, offset, len, None)
    }
    fn write_inner(
        &mut self,
        node: NodeId,
        offset: u64,
        byte_len: usize,
        bytes: Option<&[u8]>,
    ) -> Result<usize> {
        self.ensure_active()?;
        if byte_len == 0 {
            return Ok(0);
        }
        let old_len = self.attr(node)?.size;
        let end = offset
            .checked_add(byte_len as u64)
            .ok_or(StoreError::InvalidInput("write length"))?;
        self.ensure_edited(node)?;
        let (spool, high_water, old, edits) = self.edited_state(node)?;
        self.spool_file(node, &spool)?;
        let start = offset.min(old_len);
        let delete_len = if offset < old_len {
            (old_len - offset).min(byte_len as u64)
        } else {
            0
        };
        let mut replacement = Vec::with_capacity(2);
        if offset > old_len {
            replacement.push(Piece::Zero {
                len: offset - old_len,
            });
        }
        replacement.push(if bytes.is_some() {
            Piece::Spool {
                offset: high_water,
                len: byte_len as u64,
            }
        } else {
            Piece::Zero {
                len: byte_len as u64,
            }
        });
        let next = old.replace(start, delete_len, replacement)?;
        let next_edits = next_edit(edits)?;
        let generation = self.next_generation()?;
        let paths = self.nodes[&node].paths.iter().cloned().collect();
        let appended = bytes.map_or(0, |bytes| bytes.len() as u64);
        self.policy.check(
            self.spool_bytes
                .checked_add(appended)
                .ok_or(StoreError::InvalidInput("workspace spool limit"))?,
        )?;
        self.check_piece_resources(&old, &next)?;
        if let Some(bytes) = bytes {
            let started = std::time::Instant::now();
            let file = self.spool_file(node, &spool)?;
            if file.metadata()?.len() != high_water {
                return Err(StoreError::Integrity("spool high-water"));
            }
            if let Err(error) = append_spool(file, bytes, high_water) {
                file.set_len(high_water)
                    .map_err(|_| StoreError::Integrity("spool append cleanup failure"))?;
                return Err(error.into());
            }
            self.spool_write_metrics.write_bytes = self
                .spool_write_metrics
                .write_bytes
                .saturating_add(appended);
            self.spool_write_metrics.write_ns = self
                .spool_write_metrics
                .write_ns
                .saturating_add(elapsed_ns(started));
        }
        self.install_edit(
            node,
            old,
            next,
            next_edits,
            high_water + appended,
            appended,
            generation,
            paths,
        )?;
        if let Some(bytes) = bytes {
            self.capture_write(node, offset, old_len, bytes);
        }
        debug_assert_eq!(self.attr(node)?.size, old_len.max(end));
        Ok(byte_len)
    }

    pub(crate) fn edit_many(
        &mut self,
        node: NodeId,
        edits: Vec<(u64, u64, crate::WorkspaceFileReplacement)>,
    ) -> Result<()> {
        if edits.is_empty() {
            return Err(StoreError::InvalidInput("workspace edit batch"));
        }
        self.ensure_active()?;
        let (old, prior_edits, was_base) = match &self.nodes[&node].data {
            Data::File(FileData::Base { root, len }) => (PieceTree::base(*root, *len)?, 0, true),
            Data::File(FileData::Edited {
                spool,
                pieces,
                edits,
                ..
            }) => {
                self.spool_file(node, spool)?;
                (pieces.clone(), *edits, false)
            }
            _ => return Err(StoreError::InvalidInput("file")),
        };
        let total_edits = prior_edits
            .checked_add(
                u32::try_from(edits.len())
                    .map_err(|_| StoreError::InvalidInput("workspace edit limit"))?,
            )
            .filter(|value| *value <= MAX_EDITS_PER_FILE)
            .ok_or(StoreError::InvalidInput("workspace edit limit"))?;
        let generation = self
            .mutation_generation
            .checked_add(edits.len() as u64)
            .ok_or(StoreError::Integrity("Workspace mutation generation"))?;
        let mut next = old.clone();
        for (start, delete_len, replacement) in edits {
            let piece = match replacement {
                crate::WorkspaceFileReplacement::Inline(bytes) => {
                    if bytes.len() > MAX_INLINE_PER_EDIT {
                        return Err(StoreError::InvalidInput("workspace inline edit limit"));
                    }
                    (!bytes.is_empty()).then(|| Piece::Inline {
                        len: bytes.len() as u64,
                        bytes: Arc::from(bytes),
                        offset: 0,
                    })
                }
                crate::WorkspaceFileReplacement::Zero(len) => {
                    (len != 0).then_some(Piece::Zero { len })
                }
            };
            next = next.replace(start, delete_len, piece)?;
            if was_base {
                self.inline_bytes
                    .checked_add(next.inline_len())
                    .filter(|value| *value <= MAX_INLINE_PER_WORKSPACE)
                    .ok_or(StoreError::InvalidInput("workspace inline limit"))?;
                self.piece_allocation_bytes
                    .checked_add(next.logical_allocation_charge()?)
                    .filter(|value| *value <= MAX_PIECE_ALLOCATION)
                    .ok_or(StoreError::InvalidInput("workspace piece allocation limit"))?;
            } else {
                self.check_piece_resources(&old, &next)?;
            }
        }
        let paths = self.nodes[&node].paths.iter().cloned().collect();
        self.invalidate_capture();
        self.ensure_edited(node)?;
        let (_, high_water, installed, _) = self.edited_state(node)?;
        self.install_edit(
            node,
            installed,
            next,
            total_edits,
            high_water,
            0,
            generation,
            paths,
        )
    }

    pub fn truncate(&mut self, node: NodeId, size: u64) -> Result<()> {
        self.invalidate_capture();
        self.ensure_active()?;
        let old_len = self.attr(node)?.size;
        if size == old_len {
            return Ok(());
        }
        self.ensure_edited(node)?;
        let (spool, high_water, old, edits) = self.edited_state(node)?;
        self.spool_file(node, &spool)?;
        let (start, delete_len, replacement) = if size < old_len {
            (size, old_len - size, None)
        } else {
            (
                old_len,
                0,
                Some(Piece::Zero {
                    len: size - old_len,
                }),
            )
        };
        let next = old.replace(start, delete_len, replacement)?;
        let generation = self.next_generation()?;
        let paths = self.nodes[&node].paths.iter().cloned().collect();
        self.check_piece_resources(&old, &next)?;
        self.install_edit(
            node,
            old,
            next,
            next_edit(edits)?,
            high_water,
            0,
            generation,
            paths,
        )
    }

    fn next_generation(&self) -> Result<u64> {
        self.mutation_generation
            .checked_add(1)
            .ok_or(StoreError::Integrity("Workspace mutation generation"))
    }
    fn edited_state(&self, node: NodeId) -> Result<(std::path::PathBuf, u64, PieceTree, u32)> {
        match &self.nodes[&node].data {
            Data::File(FileData::Edited {
                spool,
                spool_high_water,
                pieces,
                edits,
                ..
            }) => Ok((spool.clone(), *spool_high_water, pieces.clone(), *edits)),
            _ => Err(StoreError::InvalidInput("file")),
        }
    }
    fn check_piece_resources(&self, old: &PieceTree, next: &PieceTree) -> Result<()> {
        self.inline_bytes
            .checked_sub(old.inline_len())
            .and_then(|v| v.checked_add(next.inline_len()))
            .filter(|v| *v <= MAX_INLINE_PER_WORKSPACE)
            .ok_or(StoreError::InvalidInput("workspace inline limit"))?;
        self.piece_allocation_bytes
            .checked_sub(old.logical_allocation_charge()?)
            .and_then(|v| v.checked_add(next.logical_allocation_charge().ok()?))
            .filter(|v| *v <= MAX_PIECE_ALLOCATION)
            .ok_or(StoreError::InvalidInput("workspace piece allocation limit"))?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn install_edit(
        &mut self,
        node: NodeId,
        old: PieceTree,
        next: PieceTree,
        edits: u32,
        high_water: u64,
        appended: u64,
        generation: u64,
        paths: Vec<String>,
    ) -> Result<()> {
        self.inline_bytes = self.inline_bytes - old.inline_len() + next.inline_len();
        self.piece_allocation_bytes = self.piece_allocation_bytes
            - old.logical_allocation_charge()?
            + next.logical_allocation_charge()?;
        self.spool_bytes += appended;
        let Data::File(FileData::Edited {
            pieces,
            edits: current_edits,
            spool_high_water,
            ..
        }) = &mut self.nodes.get_mut(&node).unwrap().data
        else {
            return Err(StoreError::Integrity("edited file"));
        };
        *pieces = next;
        *current_edits = edits;
        *spool_high_water = high_water;
        self.dirty.insert(node);
        self.mutation_generation = generation;
        for path in paths {
            self.mutation_paths.insert(path, generation);
        }
        Ok(())
    }

    pub fn fsync(&mut self, node: Option<NodeId>) -> Result<()> {
        let spools = if let Some(node) = node {
            match &self
                .nodes
                .get(&node)
                .ok_or(StoreError::NotFound("node"))?
                .data
            {
                Data::File(FileData::Edited { spool, .. }) => vec![(node, spool.clone())],
                _ => Vec::new(),
            }
        } else {
            self.nodes
                .iter()
                .filter_map(|(node, value)| match &value.data {
                    Data::File(FileData::Edited { spool, .. }) => Some((*node, spool.clone())),
                    _ => None,
                })
                .collect()
        };
        let started = std::time::Instant::now();
        for (node, spool) in spools {
            self.spool_file(node, &spool)?;
        }
        self.finish_capture(node);
        self.spool_write_metrics.fence_count =
            self.spool_write_metrics.fence_count.saturating_add(1);
        self.spool_write_metrics.fence_ns = self
            .spool_write_metrics
            .fence_ns
            .saturating_add(elapsed_ns(started));
        Ok(())
    }
    pub(crate) fn take_spool_write_metrics(&mut self) -> crate::cow_tree::SpoolWriteMetrics {
        std::mem::take(&mut self.spool_write_metrics)
    }
    pub(crate) fn clear_spool(&mut self) -> Result<()> {
        self.invalidate_capture();
        self.open_spools.clear();
        for value in self.nodes.values() {
            if let Data::File(FileData::Edited { spool, .. }) = &value.data {
                if spool.exists() {
                    std::fs::remove_file(spool)?;
                }
            }
        }
        self.spool_bytes = 0;
        self.inline_bytes = 0;
        self.piece_allocation_bytes = 0;
        Ok(())
    }
    pub(crate) fn new_spool_node(&mut self, mode: u32, path: String) -> Result<NodeId> {
        let node = NodeId(self.next_node);
        self.new_spool_node_reserved_inner(node, mode, path, false)?;
        Ok(node)
    }
    pub(crate) fn new_spool_node_reserved(
        &mut self,
        node: NodeId,
        mode: u32,
        path: String,
    ) -> Result<()> {
        self.new_spool_node_reserved_inner(node, mode, path, true)
    }
    fn new_spool_node_reserved_inner(
        &mut self,
        node: NodeId,
        mode: u32,
        path: String,
        reserved: bool,
    ) -> Result<()> {
        let spool = self.spool.join(node.0.to_string());
        let started = std::time::Instant::now();
        let file = create_spool(&spool)?;
        self.note_spool_open(elapsed_ns(started));
        let data = Data::File(FileData::Edited {
            base: None,
            spool,
            spool_high_water: 0,
            pieces: PieceTree::empty(),
            edits: 0,
        });
        let value = Node {
            canonical: None,
            paths: [path].into(),
            mode,
            links: 1,
            pins: 0,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            data,
        };
        if reserved {
            if self.nodes.insert(node, value).is_some() {
                return Err(StoreError::Integrity("reserved node"));
            }
        } else {
            let allocated = self.allocate(value);
            debug_assert_eq!(allocated, node);
        }
        if self.open_spools.insert(node, file).is_some() {
            return Err(StoreError::Integrity("spool descriptor"));
        }
        Ok(())
    }
    fn ensure_edited(&mut self, node: NodeId) -> Result<()> {
        if let Data::File(FileData::Base { root, len }) = self.nodes[&node].data {
            let path = self.spool.join(node.0.to_string());
            let pieces = PieceTree::base(root, len)?;
            let next_allocation = self
                .piece_allocation_bytes
                .checked_add(pieces.logical_allocation_charge()?)
                .filter(|v| *v <= MAX_PIECE_ALLOCATION)
                .ok_or(StoreError::InvalidInput("workspace piece allocation limit"))?;
            let started = std::time::Instant::now();
            let file = create_spool(&path)?;
            let open_ns = elapsed_ns(started);
            if self.open_spools.contains_key(&node) {
                let _ = std::fs::remove_file(path);
                return Err(StoreError::Integrity("spool descriptor"));
            }
            self.nodes.get_mut(&node).unwrap().data = Data::File(FileData::Edited {
                base: Some((root, len)),
                spool: path,
                spool_high_water: 0,
                pieces,
                edits: 0,
            });
            self.piece_allocation_bytes = next_allocation;
            self.open_spools.insert(node, file);
            self.note_spool_open(open_ns);
        }
        matches!(self.nodes[&node].data, Data::File(FileData::Edited { .. }))
            .then_some(())
            .ok_or(StoreError::InvalidInput("file"))
    }
    pub(crate) fn spool_file(&self, node: NodeId, path: &Path) -> Result<&File> {
        let file = self
            .open_spools
            .get(&node)
            .ok_or(StoreError::Integrity("spool descriptor"))?;
        let open = file.metadata()?;
        let linked = std::fs::metadata(path)?;
        if open.dev() != linked.dev() || open.ino() != linked.ino() {
            return Err(StoreError::Integrity("spool descriptor identity"));
        }
        Ok(file)
    }
    fn note_spool_open(&mut self, ns: u64) {
        self.spool_write_metrics.write_open_count =
            self.spool_write_metrics.write_open_count.saturating_add(1);
        self.spool_write_metrics.write_ns = self.spool_write_metrics.write_ns.saturating_add(ns);
    }
}

fn next_edit(edits: u32) -> Result<u32> {
    edits
        .checked_add(1)
        .filter(|v| *v <= MAX_EDITS_PER_FILE)
        .ok_or(StoreError::InvalidInput("workspace edit limit"))
}
fn elapsed_ns(started: std::time::Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
fn create_spool(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
}

fn append_spool(file: &File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    #[cfg(test)]
    if INJECT_SHORT_APPEND.with(|inject| inject.replace(false)) {
        file.write_all_at(&bytes[..bytes.len() / 2], offset)?;
        return Err(std::io::Error::other("injected short spool append"));
    }
    file.write_all_at(bytes, offset)
}

#[cfg(test)]
thread_local! {
    static INJECT_SHORT_APPEND: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
fn read_base(
    reader: &SnapshotReader,
    root: layerfs_content::file::rope::FileStateRoot,
    start: u64,
    end: u64,
) -> Result<Vec<u8>> {
    if start >= end {
        return Ok(Vec::new());
    }
    let mut bytes = Vec::with_capacity(as_usize(end - start)?);
    let counters = read_range(&CoreReader(reader), root, start..end, &mut bytes)?;
    reader.note_rope_read(counters)?;
    Ok(bytes)
}
fn read_exact_at(file: &File, mut output: &mut [u8], mut offset: u64) -> Result<()> {
    while !output.is_empty() {
        let read = file.read_at(output, offset)?;
        if read == 0 {
            return Err(StoreError::Integrity("spool eof"));
        }
        offset += read as u64;
        output = &mut output[read..];
    }
    Ok(())
}

fn as_usize(value: u64) -> Result<usize> {
    usize::try_from(value).map_err(|_| StoreError::InvalidInput("file range"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ROOT;
    use layerfs_layerstack_store::{
        EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };

    fn workspace(label: &str) -> (std::path::PathBuf, Workspace) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-file-edit-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspace = Workspace::open(store, branch, root.join("spool")).unwrap();
        (root, workspace)
    }

    fn workspace_with_file(
        label: &str,
    ) -> (
        std::path::PathBuf,
        Workspace,
        layerfs_layerstack_store::BranchId,
    ) {
        let root = std::env::temp_dir().join(format!(
            "layerfs-file-edit-source-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("file"), b"abcdefghij").unwrap();
        let store = LayerStackStore::create(root.join("store.sqlite")).unwrap();
        let layer = store
            .initialize_layerstack(
                EntityName::new("project").unwrap(),
                LayerStackInitialization::Directory(source),
            )
            .unwrap()
            .genesis_layer_id;
        let branch = store
            .fork_branch(
                EntityName::new("main").unwrap(),
                LocalForkSource::Layer { layer_id: layer },
            )
            .unwrap();
        let workspace = Workspace::open(store, branch, root.join("spool")).unwrap();
        (root, workspace, branch)
    }

    #[test]
    fn edit_limit_and_sparse_physical_charge_are_exact() {
        let (root, mut workspace) = workspace("limits");
        let sparse = workspace.create_file(ROOT, b"sparse", 0o600).unwrap().node;
        workspace.write(sparse, 60 * 1024, b"x").unwrap();
        assert_eq!(workspace.attr(sparse).unwrap().size, 60 * 1024 + 1);
        assert_eq!(workspace.spool_bytes, 1);
        let Data::File(FileData::Edited { spool, .. }) = &workspace.nodes[&sparse].data else {
            panic!("edited file")
        };
        assert_eq!(std::fs::metadata(spool).unwrap().len(), 1);

        let file = workspace.create_file(ROOT, b"limit", 0o600).unwrap().node;
        for value in 0..MAX_EDITS_PER_FILE {
            workspace.write(file, 0, &[(value & 0xff) as u8]).unwrap();
        }
        let before = workspace.read(file, 0, 1).unwrap();
        assert!(workspace.write(file, 0, b"x").is_err());
        assert_eq!(workspace.read(file, 0, 1).unwrap(), before);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn short_spool_append_restores_high_water_and_piece_root() {
        let (root, mut workspace) = workspace("short-append");
        let file = workspace.create_file(ROOT, b"file", 0o600).unwrap().node;
        workspace.write(file, 0, b"base").unwrap();
        let before = workspace.nodes[&file].clone();
        let before_charge = workspace.spool_bytes;
        INJECT_SHORT_APPEND.with(|inject| inject.set(true));
        assert!(workspace.write(file, 4, b"failure").is_err());
        assert_eq!(workspace.nodes[&file], before);
        assert_eq!(workspace.spool_bytes, before_charge);
        let Data::File(FileData::Edited {
            spool,
            spool_high_water,
            ..
        }) = &workspace.nodes[&file].data
        else {
            panic!("edited file")
        };
        assert_eq!(std::fs::metadata(spool).unwrap().len(), *spool_high_water);
        assert_eq!(workspace.read(file, 0, 16).unwrap(), b"base");
        workspace.policy.max_spool_bytes = before_charge;
        assert!(workspace.write(file, 4, b"x").is_err());
        assert_eq!(workspace.spool_bytes, before_charge);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_inline_limit_accepts_eight_mib_and_rejects_the_next_byte() {
        let (root, mut workspace) = workspace("inline-limit");
        for index in 0..8 {
            let name = format!("file-{index}");
            let file = workspace
                .create_file(ROOT, name.as_bytes(), 0o600)
                .unwrap()
                .node;
            workspace
                .edit_many(
                    file,
                    vec![(
                        0,
                        0,
                        crate::WorkspaceFileReplacement::Inline(vec![index; 1024 * 1024]),
                    )],
                )
                .unwrap();
        }
        assert_eq!(workspace.inline_bytes, MAX_INLINE_PER_WORKSPACE);
        let extra = workspace.create_file(ROOT, b"extra", 0o600).unwrap().node;
        assert!(workspace
            .edit_many(
                extra,
                vec![(0, 0, crate::WorkspaceFileReplacement::Inline(vec![0]),)],
            )
            .is_err());
        assert_eq!(workspace.inline_bytes, MAX_INLINE_PER_WORKSPACE);
        assert_eq!(workspace.attr(extra).unwrap().size, 0);
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn generation_overflow_rejects_without_logical_or_physical_change() {
        let (root, mut workspace, _) = workspace_with_file("generation-overflow");
        let file = workspace.lookup(ROOT, b"file").unwrap().node;
        workspace.mutation_generation = u64::MAX;
        let before = workspace.nodes[&file].clone();
        let charges = (
            workspace.spool_bytes,
            workspace.inline_bytes,
            workspace.piece_allocation_bytes,
        );
        assert!(workspace
            .edit_many(
                file,
                vec![(0, 0, crate::WorkspaceFileReplacement::Inline(b"P".to_vec()),)],
            )
            .is_err());
        assert_eq!(workspace.nodes[&file], before);
        assert_eq!(
            (
                workspace.spool_bytes,
                workspace.inline_bytes,
                workspace.piece_allocation_bytes,
            ),
            charges
        );
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn discard_reclaims_one_live_base_inline_zero_spool_composition() {
        let (root, mut workspace, branch) = workspace_with_file("mixed-discard");
        let branch_root = workspace.store.pin_branch(branch).unwrap().root;
        let file = workspace.lookup(ROOT, b"file").unwrap().node;
        workspace
            .edit_many(
                file,
                vec![
                    (1, 2, crate::WorkspaceFileReplacement::Inline(b"X".to_vec())),
                    (2, 0, crate::WorkspaceFileReplacement::Zero(2)),
                ],
            )
            .unwrap();
        let end = workspace.attr(file).unwrap().size;
        workspace.write(file, end, b"S").unwrap();
        let Data::File(FileData::Edited { spool, pieces, .. }) = &workspace.nodes[&file].data
        else {
            panic!("edited file")
        };
        let spool = spool.clone();
        let variants = pieces.pieces();
        assert!(variants
            .iter()
            .any(|piece| matches!(piece, Piece::Base { .. })));
        assert!(variants
            .iter()
            .any(|piece| matches!(piece, Piece::Inline { .. })));
        assert!(variants
            .iter()
            .any(|piece| matches!(piece, Piece::Zero { .. })));
        assert!(variants
            .iter()
            .any(|piece| matches!(piece, Piece::Spool { .. })));
        assert!(spool.exists());
        workspace.discard().unwrap();
        assert_eq!(
            workspace.store.pin_branch(branch).unwrap().root,
            branch_root
        );
        assert_eq!(workspace.spool_bytes, 0);
        assert_eq!(workspace.inline_bytes, 0);
        assert_eq!(workspace.piece_allocation_bytes, 0);
        assert!(!spool.exists());
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }
}
