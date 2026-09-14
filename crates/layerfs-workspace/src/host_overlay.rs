//! Host-owned filesystem operations over one atomic overlay root. Canonical
//! namespace acquisition is lazy; snapshots and open handles own their inputs.
//! This core does not claim to capture kernel-dirty writable mappings.
use crate::correspondence::OriginSequence;
use crate::overlay::{InodeRecord, Mutation, Overlay};
use crate::overlay_budget::{Budget, Charge, Operation, Resources};
use crate::overlay_index::{Index, Root};
use crate::overlay_payload::Payload;
use crate::overlay_ranges::{OwnedPiece, Ranges, Tree};
use crate::snapshot::Snapshot;
use layerfs_content::filesystem::{self as logical, LogicalCounters};
use layerfs_content::tree::directory::{
    directory_lookup, directory_page_after, DirectoryStateRoot, NamespaceCounters,
};
use layerfs_content::tree::inode::InodeTableRoot;
use layerfs_content::{CanonicalName, CanonicalPath, ObjectId};
use layerfs_layerstack_store::{CoreReader, Result, SnapshotReader, StoreError};
use layerfs_workspace_core::namespace::AcquiredInode;
use layerfs_workspace_core::{Attr, Data, FileData, Kind, NodeId, ResourcePolicy, ROOT};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const PAGE: usize = 128;

pub(crate) struct DirectoryPage {
    pub(crate) entries: Vec<(Attr, Vec<u8>)>,
    /// Last examined name, including tombstones; an empty visible page can
    /// therefore advance without hiding later entries.
    pub(crate) continuation: Option<Vec<u8>>,
}

enum CapturedNode {
    Live(NodeId),
    Base(AcquiredInode),
}

pub(crate) struct HostOverlay {
    pub(crate) overlay: Overlay,
    pub(crate) ranges: Ranges,
    pub(crate) payload: Payload,
    pub(crate) reader: SnapshotReader,
    pub(crate) index: Index,
    pub(crate) budget: Arc<Budget>,
    pub(crate) policy: ResourcePolicy,
    base_inodes: InodeTableRoot,
    origins: OriginSequence,
    kernel_detached: AtomicBool,
}

impl HostOverlay {
    pub(crate) fn new(
        reader: SnapshotReader,
        base_root: ObjectId,
        workspace: [u8; 16],
        policy: ResourcePolicy,
        resources: Resources,
    ) -> Result<Self> {
        let namespace = logical::namespace(&CoreReader(&reader), base_root)?;
        let resolved = logical::resolve(
            &CoreReader(&reader),
            base_root,
            &CanonicalPath::root(),
            &mut LogicalCounters::default(),
        )?;
        let portable = crate::cow_tree::portable_metadata(
            &CoreReader(&reader),
            resolved.record.metadata_root,
            resolved.record.kind,
        )?;
        if resolved.record.kind != layerfs_content::tree::inode::InodeKind::Directory {
            return Err(StoreError::Integrity("host namespace root kind"));
        }
        // Namespace root has no incoming namespace binding (refcount zero).
        // acquire_inode intentionally validates non-root records instead.
        let acquired = AcquiredInode {
            inode: resolved.inode,
            mode: portable.permission_mode,
            links: 2,
            mtime_seconds: portable.mtime_seconds,
            mtime_nanoseconds: portable.mtime_nanoseconds,
            data: Data::Directory(layerfs_workspace_core::DirectoryData {
                base: Some(DirectoryStateRoot(resolved.record.content_root)),
                changes: Default::default(),
            }),
        };
        let overlay = Overlay::from_index(resources.index.clone(), base_root)?;
        let host = Self {
            overlay,
            ranges: resources.ranges,
            payload: resources.payload,
            reader,
            index: resources.index,
            budget: resources.budget,
            policy,
            base_inodes: InodeTableRoot(namespace.inode_table_root),
            origins: OriginSequence::new(workspace),
            kernel_detached: AtomicBool::new(false),
        };
        let (record, tree) = host.acquired(ROOT, Some(ROOT), &acquired)?;
        host.overlay
            .mutate(|m| m.cache_inode_record(record, tree.root()))?;
        Ok(host)
    }

    pub(crate) fn snapshot(&self) -> Result<Snapshot> {
        self.overlay.snapshot()
    }
    pub(crate) fn origins(&self) -> &OriginSequence {
        &self.origins
    }
    pub(super) fn mutate<T>(
        &self,
        mut apply: impl FnMut(&mut Mutation<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut output = None;
        self.overlay.mutate(|m| {
            let _permit = self.budget.enter(Operation::Prepare, Charge::default())?;
            // Ordinary operations prepare their change-log records together;
            // the deferred batch is flushed by `Overlay::prepare` before the
            // candidate can be installed.
            m.begin_change_batch();
            output = Some(apply(m)?);
            Ok(())
        })?;
        output.ok_or(StoreError::Integrity("host mutation result"))
    }
    fn record(&self, snapshot: &Snapshot, node: NodeId) -> Result<(InodeRecord, Option<Root>)> {
        snapshot
            .inode_record(node)?
            .ok_or(StoreError::NotFound("node"))
    }
    pub(super) fn directory(&self, snapshot: &Snapshot, node: NodeId) -> Result<InodeRecord> {
        let record = self.metadata(snapshot, node)?;
        if record.attr.kind != Kind::Directory {
            return Err(StoreError::InvalidInput("directory"));
        }
        Ok(record)
    }
    fn tree(&self, snapshot: &Snapshot, node: NodeId) -> Result<(InodeRecord, Tree)> {
        let (record, ranges) = self.record(snapshot, node)?;
        if record.attr.kind != Kind::File {
            return Err(StoreError::InvalidInput("file"));
        }
        let tree = self.ranges.restore(ranges)?;
        if tree.len() != record.attr.size {
            return Err(StoreError::Integrity("host file length"));
        }
        Ok((record, tree))
    }
    pub(crate) fn attr(&self, node: NodeId) -> Result<Attr> {
        let _permit = self.budget.enter(Operation::Read, Charge::default())?;
        Ok(self.metadata(&self.snapshot()?, node)?.attr)
    }
    pub(super) fn metadata(&self, snapshot: &Snapshot, node: NodeId) -> Result<InodeRecord> {
        let bytes = snapshot.inode(node)?.ok_or(StoreError::NotFound("node"))?;
        InodeRecord::decode(node, &bytes)
    }
    pub(crate) fn parent_of(&self, node: NodeId) -> Result<NodeId> {
        let _permit = self.budget.enter(Operation::Read, Charge::default())?;
        self.directory(&self.snapshot()?, node)?
            .parent
            .ok_or(StoreError::Integrity("directory parent"))
    }

    fn acquired(
        &self,
        node: NodeId,
        parent: Option<NodeId>,
        acquired: &AcquiredInode,
    ) -> Result<(InodeRecord, Tree)> {
        let (kind, size, base, tree) = match &acquired.data {
            Data::File(FileData::Base { root, len }) => {
                let tree = if *len == 0 {
                    self.ranges.empty()
                } else {
                    self.ranges.append(
                        &self.ranges.empty(),
                        OwnedPiece::base(root.0, 0, *len, self.origins.allocate()?, 0)?,
                    )?
                };
                (Kind::File, *len, Some(root.0), tree)
            }
            Data::Directory(directory) if directory.changes.is_empty() => (
                Kind::Directory,
                0,
                directory.base.map(|root| root.0),
                self.ranges.empty(),
            ),
            Data::Symlink(target) => (
                Kind::Symlink,
                target.len() as u64,
                None,
                self.bytes_tree(target)?,
            ),
            _ => return Err(StoreError::Integrity("host immutable inode data")),
        };
        Ok((
            InodeRecord {
                attr: Attr {
                    node,
                    size,
                    kind,
                    mode: acquired.mode,
                    links: acquired.links,
                    mtime_seconds: acquired.mtime_seconds,
                    mtime_nanoseconds: acquired.mtime_nanoseconds,
                },
                revision: 0,
                pins: 0,
                canonical: Some(acquired.inode),
                base,
                parent: (kind == Kind::Directory)
                    .then_some(parent.ok_or(StoreError::Integrity("host directory parent"))?),
            },
            tree,
        ))
    }

    /// This lookup uses the candidate's current binding and canonical-ID map.
    /// An alias first touched after a write must reuse the edited inode.
    pub(super) fn resolve_name(
        &self,
        m: &mut Mutation<'_>,
        parent: NodeId,
        name: &[u8],
    ) -> Result<Option<NodeId>> {
        let canonical_name = CanonicalName::from_bytes(name)?;
        let view = m.view();
        let directory = self.directory(&view, parent)?;
        if parent != ROOT && directory.attr.links < 2 {
            return Err(StoreError::NotFound("node path"));
        }
        if let Some(binding) = view.binding(parent, name)? {
            return Ok(binding);
        }
        let canonical = directory
            .base
            .map(|base| {
                directory_lookup(
                    &CoreReader(&self.reader),
                    DirectoryStateRoot(base),
                    &canonical_name,
                    &mut NamespaceCounters::default(),
                )
            })
            .transpose()?
            .flatten();
        let node = match canonical {
            None => None,
            Some(canonical) => match view.canonical_inode(canonical)? {
                Some(node) => Some(node),
                None => {
                    let acquired =
                        crate::cow_tree::acquire_inode(&self.reader, self.base_inodes, canonical)?;
                    let node = m.allocate_inode()?;
                    let (record, tree) = self.acquired(node, Some(parent), &acquired)?;
                    m.cache_inode_record(record, tree.root())?;
                    Some(node)
                }
            },
        };
        m.cache_binding(parent, name, node)?;
        Ok(node)
    }

    pub(crate) fn lookup(&self, parent: NodeId, name: &[u8]) -> Result<Attr> {
        let _permit = self.budget.enter(Operation::Read, Charge::default())?;
        CanonicalName::from_bytes(name)?;
        let snapshot = self.snapshot()?;
        self.directory(&snapshot, parent)?;
        if let Some(binding) = snapshot.binding(parent, name)? {
            return binding
                .map(|node| self.record(&snapshot, node).map(|value| value.0.attr))
                .transpose()?
                .ok_or(StoreError::NotFound("name"));
        }
        self.mutate(|m| {
            let node = self
                .resolve_name(m, parent, name)?
                .ok_or(StoreError::NotFound("name"))?;
            Ok(self.record(&m.view(), node)?.0.attr)
        })
    }

    fn bytes_tree(&self, bytes: &[u8]) -> Result<Tree> {
        if bytes.is_empty() {
            return Ok(self.ranges.empty());
        }
        let origin = self.origins.allocate()?;
        // Small bytes stay in fixed, disk-owned leaves. Larger input streams
        // through bounded arena buffers; neither representation is canonical.
        let piece = if bytes.len() <= 64 {
            OwnedPiece::inline(bytes, origin)?
        } else {
            OwnedPiece::inline_payload(
                self.payload
                    .write_inline_from(&mut &bytes[..], bytes.len() as u64)?,
                origin,
            )?
        };
        Ok(self.ranges.append(&self.ranges.empty(), piece)?)
    }
    fn write_tree(&self, bytes: &[u8]) -> Result<Tree> {
        let owner = self
            .payload
            .write_from(&mut &bytes[..], bytes.len() as u64)?;
        let piece = OwnedPiece::payload(owner, self.origins.allocate()?)?;
        Ok(self.ranges.append(&self.ranges.empty(), piece)?)
    }
    fn spool_check(&self, added: u64) -> Result<()> {
        let charge = self
            .payload
            .stats()?
            .chargeable_bytes
            .checked_add(added)
            .ok_or(StoreError::InvalidInput("workspace spool limit"))?;
        self.policy.check(charge).map_err(crate::live_error)?;
        Ok(())
    }
    fn bump(
        &self,
        m: &mut Mutation<'_>,
        mut record: InodeRecord,
        tree: Option<&Root>,
    ) -> Result<()> {
        record.revision = record
            .revision
            .checked_add(1)
            .ok_or(StoreError::Integrity("inode revision"))?;
        m.put_inode_record(record, tree)
    }
    fn touch_parent(&self, m: &mut Mutation<'_>, parent: NodeId) -> Result<()> {
        let record = self.directory(&m.view(), parent)?;
        self.bump(m, record, None)
    }
    fn create(
        &self,
        parent: NodeId,
        name: &[u8],
        kind: Kind,
        mode: u32,
        tree: Tree,
        pinned: bool,
    ) -> Result<Attr> {
        CanonicalName::from_bytes(name)?;
        self.mutate(|m| self.create_in(m, parent, name, kind, mode, &tree, pinned))
    }
    fn create_in(
        &self,
        m: &mut Mutation<'_>,
        parent: NodeId,
        name: &[u8],
        kind: Kind,
        mode: u32,
        tree: &Tree,
        pinned: bool,
    ) -> Result<Attr> {
        if self.resolve_name(m, parent, name)?.is_some() {
            return Err(StoreError::InvalidInput("name exists"));
        }
        let node = m.allocate_inode()?;
        let record = InodeRecord {
            attr: Attr {
                node,
                size: tree.len(),
                kind,
                mode,
                links: if kind == Kind::Directory { 2 } else { 1 },
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            },
            revision: 0,
            pins: u32::from(pinned),
            canonical: None,
            base: None,
            parent: (kind == Kind::Directory).then_some(parent),
        };
        m.put_inode_record(record, tree.root())?;
        m.put_binding(parent, name, Some(node))?;
        self.touch_parent(m, parent)?;
        Ok(record.attr)
    }
    pub(crate) fn create_file(&self, parent: NodeId, name: &[u8], mode: u32) -> Result<Attr> {
        self.create(
            parent,
            name,
            Kind::File,
            mode & 0o777,
            self.ranges.empty(),
            false,
        )
    }
    pub(crate) fn create_file_open(&self, parent: NodeId, name: &[u8], mode: u32) -> Result<Attr> {
        self.create(
            parent,
            name,
            Kind::File,
            mode & 0o777,
            self.ranges.empty(),
            true,
        )
    }
    pub(crate) fn mkdir(&self, parent: NodeId, name: &[u8], mode: u32) -> Result<Attr> {
        self.create(
            parent,
            name,
            Kind::Directory,
            mode & 0o1777,
            self.ranges.empty(),
            false,
        )
    }
    pub(crate) fn symlink(&self, parent: NodeId, name: &[u8], target: &[u8]) -> Result<Attr> {
        CanonicalName::from_bytes(name)?;
        if target.len() > 4096 || target.contains(&0) {
            return Err(StoreError::InvalidInput("symlink"));
        }
        self.create(
            parent,
            name,
            Kind::Symlink,
            0o777,
            self.bytes_tree(target)?,
            false,
        )
    }
    pub(crate) fn readlink(&self, node: NodeId) -> Result<Vec<u8>> {
        let _permit = self.budget.enter(Operation::Read, Charge::default())?;
        self.snapshot()?.readlink(&self.ranges, node)
    }

    pub(crate) fn read_into(&self, node: NodeId, offset: u64, output: &mut [u8]) -> Result<usize> {
        self.read_snapshot(&self.snapshot()?, node, offset, output)
    }
    pub(crate) fn read_snapshot(
        &self,
        snapshot: &Snapshot,
        node: NodeId,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize> {
        let _permit = self.budget.enter(Operation::Read, Charge::default())?;
        // Caller owns/admits its output buffer; internal I/O is independently
        // bounded by payload and canonical readers.
        snapshot.read_at(&self.ranges, &self.reader, node, offset, output)
    }
    /// Cold canonical names need no live import to be readable from a captured
    /// root. Canonical aliases still route through a captured edited inode.
    pub(crate) fn read_snapshot_path(
        &self,
        snapshot: &Snapshot,
        path: &CanonicalPath,
        offset: u64,
        output: &mut [u8],
    ) -> Result<usize> {
        let _permit = self.budget.enter(Operation::Read, Charge::default())?;
        let mut node = CapturedNode::Live(ROOT);
        for name in path.components() {
            let base = match &node {
                CapturedNode::Live(live) => {
                    let directory = self.directory(snapshot, *live)?;
                    match snapshot.binding(*live, name)? {
                        Some(Some(child)) => {
                            node = CapturedNode::Live(child);
                            continue;
                        }
                        Some(None) => return Err(StoreError::NotFound("snapshot name")),
                        None => directory.base,
                    }
                }
                CapturedNode::Base(base) => match &base.data {
                    Data::Directory(directory) => directory.base.map(|root| root.0),
                    _ => return Err(StoreError::InvalidInput("snapshot directory")),
                },
            }
            .ok_or(StoreError::NotFound("snapshot name"))?;
            let canonical = directory_lookup(
                &CoreReader(&self.reader),
                DirectoryStateRoot(base),
                &CanonicalName::from_bytes(name)?,
                &mut NamespaceCounters::default(),
            )?
            .ok_or(StoreError::NotFound("snapshot name"))?;
            node = match snapshot.canonical_inode(canonical)? {
                Some(live) => CapturedNode::Live(live),
                None => CapturedNode::Base(crate::cow_tree::acquire_inode(
                    &self.reader,
                    self.base_inodes,
                    canonical,
                )?),
            };
        }
        match node {
            CapturedNode::Live(node) => {
                snapshot.read_at(&self.ranges, &self.reader, node, offset, output)
            }
            CapturedNode::Base(base) => {
                let Data::File(FileData::Base { root, len }) = base.data else {
                    return Err(StoreError::InvalidInput("snapshot file"));
                };
                let start = offset.min(len);
                let length = (len - start).min(output.len() as u64) as usize;
                let mut target = &mut output[..length];
                let counters = layerfs_content::file::content::read_range(
                    &CoreReader(&self.reader),
                    root,
                    start..start + length as u64,
                    &mut target,
                )?;
                if !target.is_empty() {
                    return Err(StoreError::Integrity("short canonical read"));
                }
                self.reader.note_rope_read(counters)?;
                Ok(length)
            }
        }
    }
    fn replace_tree(
        &self,
        tree: &Tree,
        start: u64,
        delete: u64,
        replacement: &Tree,
    ) -> Result<Tree> {
        if start.checked_add(delete).is_none_or(|end| end > tree.len()) {
            return Err(StoreError::InvalidInput("workspace splice bounds"));
        }
        let (left, tail) = self.ranges.split(tree, start)?;
        let (_, right) = self.ranges.split(&tail, delete)?;
        Ok(self
            .ranges
            .merge(&self.ranges.merge(&left, replacement)?, &right)?)
    }
    pub(crate) fn write(&self, node: NodeId, offset: u64, bytes: &[u8]) -> Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        offset
            .checked_add(bytes.len() as u64)
            .ok_or(StoreError::InvalidInput("write length"))?;
        self.mutate(|m| {
            let _permit = self.budget.enter(Operation::Write, Charge::default())?;
            let (mut record, mut tree) = self.tree(&m.view(), node)?;
            self.spool_check(bytes.len() as u64)?;
            if offset == tree.len() {
                tree = self.ranges.append_from(
                    &tree,
                    &mut &bytes[..],
                    bytes.len() as u64,
                    &self.origins,
                )?;
            } else {
                let replacement = self.write_tree(bytes)?;
                if offset > tree.len() {
                    tree = self.ranges.truncate(&tree, offset)?;
                }
                let delete = tree.len().saturating_sub(offset).min(bytes.len() as u64);
                tree = self.replace_tree(&tree, offset, delete, &replacement)?;
            }
            record.attr.size = tree.len();
            self.spool_check(0)?;
            self.bump(m, record, tree.root())?;
            Ok(bytes.len())
        })
    }
    pub(crate) fn append(&self, node: NodeId, bytes: &[u8]) -> Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        self.mutate(|m| {
            let _permit = self.budget.enter(Operation::Write, Charge::default())?;
            let (mut record, tree) = self.tree(&m.view(), node)?;
            self.spool_check(bytes.len() as u64)?;
            let next = self.ranges.append_from(
                &tree,
                &mut &bytes[..],
                bytes.len() as u64,
                &self.origins,
            )?;
            record.attr.size = next.len();
            self.spool_check(0)?;
            self.bump(m, record, next.root())?;
            Ok(bytes.len())
        })
    }
    pub(crate) fn edit_many(
        &self,
        node: NodeId,
        edits: &[(u64, u64, crate::WorkspaceFileReplacement)],
    ) -> Result<()> {
        self.edit_many_snapshot(node, edits).map(drop)
    }

    pub(crate) fn edit_many_snapshot(
        &self,
        node: NodeId,
        edits: &[(u64, u64, crate::WorkspaceFileReplacement)],
    ) -> Result<Snapshot> {
        self.edit_snapshot(node, || {
            edits
                .iter()
                .map(|(start, delete, replacement)| (*start, *delete, replacement))
        })
    }

    /// Borrow public SDK inputs directly; retries do not copy their payloads.
    pub(crate) fn edit_file_snapshot(
        &self,
        node: NodeId,
        edits: &[crate::WorkspaceFileRangeEdit],
    ) -> Result<Snapshot> {
        self.edit_snapshot(node, || {
            edits
                .iter()
                .map(|edit| (edit.start, edit.delete_len, &edit.replacement))
        })
    }

    fn edit_snapshot<'a, I>(&self, node: NodeId, edits: impl Fn() -> I) -> Result<Snapshot>
    where
        I: Iterator<Item = (u64, u64, &'a crate::WorkspaceFileReplacement)>,
    {
        if edits().next().is_none() {
            return Err(StoreError::InvalidInput("workspace edit batch"));
        }
        for (_, _, replacement) in edits() {
            if matches!(replacement, crate::WorkspaceFileReplacement::Inline(bytes) if bytes.len() > layerfs_workspace_core::file_edit::MAX_INLINE_PER_EDIT)
            {
                return Err(StoreError::InvalidInput("workspace inline edit limit"));
            }
        }
        self.mutate(|m| {
            let _permit = self.budget.enter(Operation::Write, Charge::default())?;
            let (mut record, mut tree) = self.tree(&m.view(), node)?;
            for (start, delete, replacement) in edits() {
                let replacement = match replacement {
                    crate::WorkspaceFileReplacement::Inline(bytes) => self.bytes_tree(bytes)?,
                    crate::WorkspaceFileReplacement::Zero(0) => self.ranges.empty(),
                    crate::WorkspaceFileReplacement::Zero(length) => self
                        .ranges
                        .append(&self.ranges.empty(), OwnedPiece::zero(*length)?)?,
                };
                tree = self.replace_tree(&tree, start, delete, &replacement)?;
            }
            record.attr.size = tree.len();
            self.bump(m, record, tree.root())?;
            // Every accepted nonempty edit batch installs an inode revision,
            // including an empty replacement. Its logical generation therefore
            // matches this candidate; failed retries replace/drop this output.
            Ok(m.view())
        })
    }
    /// Metadata mutations return the exact installed `Attr` (#144 R1a): the
    /// reply carries the state the host authority owns, so a caller never
    /// re-reads the node to build a kernel reply. The value is copied from the
    /// same record that is installed, never read back after the mutation lock
    /// is released.
    pub(crate) fn truncate(&self, node: NodeId, size: u64) -> Result<Attr> {
        self.mutate(|m| {
            let (mut record, tree) = self.tree(&m.view(), node)?;
            if size == tree.len() {
                return Ok(record.attr);
            }
            let next = self.ranges.truncate(&tree, size)?;
            record.attr.size = size;
            let installed = record.attr;
            self.bump(m, record, next.root())?;
            Ok(installed)
        })
    }
    pub(crate) fn chmod(&self, node: NodeId, mode: u32) -> Result<Attr> {
        self.mutate(|m| {
            let (mut record, root) = self.record(&m.view(), node)?;
            record.attr.mode = mode & 0o1777;
            let installed = record.attr;
            self.bump(m, record, root.as_ref())?;
            Ok(installed)
        })
    }
    pub(crate) fn set_mtime(&self, node: NodeId, seconds: i64, nanos: u32) -> Result<Attr> {
        if nanos >= 1_000_000_000 {
            return Err(StoreError::InvalidInput("mtime"));
        }
        self.mutate(|m| {
            let (mut record, root) = self.record(&m.view(), node)?;
            record.attr.mtime_seconds = seconds;
            record.attr.mtime_nanoseconds = nanos;
            let installed = record.attr;
            self.bump(m, record, root.as_ref())?;
            Ok(installed)
        })
    }

    pub(crate) fn link(&self, node: NodeId, parent: NodeId, name: &[u8]) -> Result<Attr> {
        self.mutate(|m| self.link_in(m, node, parent, name))
    }
    fn link_in(
        &self,
        m: &mut Mutation<'_>,
        node: NodeId,
        parent: NodeId,
        name: &[u8],
    ) -> Result<Attr> {
        if self.resolve_name(m, parent, name)?.is_some() {
            return Err(StoreError::InvalidInput("name exists"));
        }
        let (mut record, root) = self.record(&m.view(), node)?;
        if record.attr.kind == Kind::Directory {
            return Err(StoreError::InvalidInput("directory link"));
        }
        record.attr.links = record
            .attr
            .links
            .checked_add(1)
            .ok_or(StoreError::InvalidInput("inode links"))?;
        m.put_binding(parent, name, Some(node))?;
        self.bump(m, record, root.as_ref())?;
        self.touch_parent(m, parent)?;
        Ok(record.attr)
    }
    fn empty_directory(&self, snapshot: &Snapshot, node: NodeId) -> Result<bool> {
        let directory = self.directory(snapshot, node)?;
        let mut after = None;
        loop {
            let page = snapshot.bindings(node, after.as_deref())?;
            if page.iter().any(|(_, child)| child.is_some()) {
                return Ok(false);
            }
            let next = page.last().map(|(name, _)| name.clone());
            if page.len() < PAGE {
                break;
            }
            after = next;
        }
        let Some(base) = directory.base else {
            return Ok(true);
        };
        let mut after = None;
        loop {
            let page = directory_page_after(
                &CoreReader(&self.reader),
                DirectoryStateRoot(base),
                after.as_ref(),
                PAGE,
                256 * 1024,
                &mut NamespaceCounters::default(),
            )?;
            for (name, _) in &page.entries {
                if snapshot.binding(node, name.as_bytes())? != Some(None) {
                    return Ok(false);
                }
            }
            let Some(next) = page.continuation else {
                return Ok(true);
            };
            after = Some(next);
        }
    }
    fn retire_binding(
        &self,
        m: &mut Mutation<'_>,
        parent: NodeId,
        name: &[u8],
        node: NodeId,
    ) -> Result<()> {
        let (mut record, root) = self.record(&m.view(), node)?;
        record.attr.links = record
            .attr
            .links
            .checked_sub(1)
            .ok_or(StoreError::Integrity("inode links"))?;
        m.put_binding(parent, name, None)?;
        if record.pins == 0 && (record.attr.links == 0 || record.attr.kind == Kind::Directory) {
            m.remove_inode_record(node)?;
        } else {
            self.bump(m, record, root.as_ref())?;
        }
        Ok(())
    }
    pub(crate) fn unlink(&self, parent: NodeId, name: &[u8], directory: bool) -> Result<()> {
        self.mutate(|m| {
            let node = self
                .resolve_name(m, parent, name)?
                .ok_or(StoreError::NotFound("name"))?;
            let (record, _) = self.record(&m.view(), node)?;
            if directory != (record.attr.kind == Kind::Directory) {
                return Err(StoreError::InvalidInput("unlink kind"));
            }
            if directory && !self.empty_directory(&m.view(), node)? {
                return Err(StoreError::InvalidInput("directory not empty"));
            }
            self.retire_binding(m, parent, name, node)?;
            self.touch_parent(m, parent)
        })
    }
    pub(crate) fn rename(
        &self,
        parent: NodeId,
        name: &[u8],
        new_parent: NodeId,
        new_name: &[u8],
        no_replace: bool,
    ) -> Result<()> {
        self.mutate(|m| {
            let node = self
                .resolve_name(m, parent, name)?
                .ok_or(StoreError::NotFound("name"))?;
            let target = self.resolve_name(m, new_parent, new_name)?;
            if (parent == new_parent && name == new_name) || target == Some(node) {
                return Ok(());
            }
            let (mut record, root) = self.record(&m.view(), node)?;
            if no_replace && target.is_some() {
                return Err(StoreError::InvalidInput("name exists"));
            }
            if let Some(target) = target {
                let (existing, _) = self.record(&m.view(), target)?;
                if (record.attr.kind == Kind::Directory) != (existing.attr.kind == Kind::Directory)
                {
                    return Err(StoreError::InvalidInput("rename type"));
                }
                if existing.attr.kind == Kind::Directory
                    && !self.empty_directory(&m.view(), target)?
                {
                    return Err(StoreError::InvalidInput("directory not empty"));
                }
            }
            if record.attr.kind == Kind::Directory {
                let mut ancestor = new_parent;
                for depth in 0..=layerfs_content::limits::MAX_PATH_COMPONENTS {
                    if ancestor == node {
                        return Err(StoreError::InvalidInput("rename descendant"));
                    }
                    if ancestor == ROOT {
                        break;
                    }
                    if depth == layerfs_content::limits::MAX_PATH_COMPONENTS {
                        return Err(StoreError::Integrity("directory ancestor depth"));
                    }
                    ancestor = self
                        .directory(&m.view(), ancestor)?
                        .parent
                        .ok_or(StoreError::Integrity("directory parent"))?;
                }
                record.parent = Some(new_parent);
            }
            if let Some(target) = target {
                self.retire_binding(m, new_parent, new_name, target)?;
            }
            m.put_binding(parent, name, None)?;
            m.put_binding(new_parent, new_name, Some(node))?;
            if record.attr.kind == Kind::Directory {
                m.cache_inode_record(record, root.as_ref())?;
            }
            self.touch_parent(m, parent)?;
            if new_parent != parent {
                self.touch_parent(m, new_parent)?;
            }
            Ok(())
        })
    }
    /// Resolve and pin one SDK target in the same root installation. The SDK
    /// coordinator validates the complete canonical path before entering here.
    pub(crate) fn pin_path(&self, path: &str) -> Result<NodeId> {
        self.mutate(|m| {
            let mut node = ROOT;
            for name in path
                .as_bytes()
                .split(|byte| *byte == b'/')
                .filter(|name| !name.is_empty())
            {
                node = self
                    .resolve_name(m, node, name)?
                    .ok_or(StoreError::NotFound("workspace edit path"))?;
            }
            let (mut record, root) = self.record(&m.view(), node)?;
            if record.attr.kind != Kind::File {
                return Err(StoreError::InvalidInput("workspace edit file"));
            }
            record.pins = record
                .pins
                .checked_add(1)
                .ok_or(StoreError::Integrity("node pin"))?;
            m.cache_inode_record(record, root.as_ref())?;
            Ok(node)
        })
    }
    pub(crate) fn pin(&self, node: NodeId, truncate: bool) -> Result<()> {
        self.pin_kind(node, Kind::File, truncate)
    }
    pub(crate) fn pin_directory(&self, node: NodeId) -> Result<()> {
        self.pin_kind(node, Kind::Directory, false)
    }
    fn pin_kind(&self, node: NodeId, kind: Kind, truncate: bool) -> Result<()> {
        self.mutate(|m| {
            let (mut record, root) = self.record(&m.view(), node)?;
            if record.attr.kind != kind {
                return Err(StoreError::InvalidInput("open kind"));
            }
            record.pins = record
                .pins
                .checked_add(1)
                .ok_or(StoreError::Integrity("node pin"))?;
            if truncate && record.attr.size != 0 {
                record.attr.size = 0;
                self.bump(m, record, None)
            } else {
                m.cache_inode_record(record, root.as_ref())
            }
        })
    }
    pub(crate) fn unpin(&self, node: NodeId) -> Result<()> {
        self.mutate(|m| {
            let (mut record, root) = self.record(&m.view(), node)?;
            record.pins = record
                .pins
                .checked_sub(1)
                .ok_or(StoreError::Integrity("node pin"))?;
            let removed = record.attr.links == 0
                || (record.attr.kind == Kind::Directory && record.attr.links < 2);
            if node != ROOT && removed && record.pins == 0 {
                m.remove_inode_record(node)
            } else {
                m.cache_inode_record(record, root.as_ref())
            }
        })
    }

    /// Name-key cursor; merges only two bounded pages. Metadata import happens
    /// in the same leased candidate, so no whole-directory materialization.
    pub(crate) fn directory_page(
        &self,
        node: NodeId,
        after: Option<&[u8]>,
    ) -> Result<DirectoryPage> {
        if let Some(name) = after {
            CanonicalName::from_bytes(name)?;
        }
        self.mutate(|m| self.directory_page_in(m, node, after))
    }
    pub(super) fn directory_page_in(
        &self,
        m: &mut Mutation<'_>,
        node: NodeId,
        after: Option<&[u8]>,
    ) -> Result<DirectoryPage> {
        let view = m.view();
        let directory = self.directory(&view, node)?;
        let overlay = view.bindings(node, after)?;
        let canonical_after = after.map(CanonicalName::from_bytes).transpose()?;
        let base = directory
            .base
            .map(|base| {
                directory_page_after(
                    &CoreReader(&self.reader),
                    DirectoryStateRoot(base),
                    canonical_after.as_ref(),
                    PAGE,
                    256 * 1024,
                    &mut NamespaceCounters::default(),
                )
            })
            .transpose()?
            .map_or_else(Vec::new, |page| page.entries);
        let full = overlay.len() == PAGE || base.len() == PAGE;
        let mut names = overlay
            .into_iter()
            .map(|(name, _)| name)
            .chain(base.into_iter().map(|(name, _)| name.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        let full = full || names.len() > PAGE;
        names.truncate(PAGE);
        let continuation = if full { names.last().cloned() } else { None };
        let mut output = Vec::with_capacity(names.len());
        for name in names {
            if let Some(child) = self.resolve_name(m, node, &name)? {
                output.push((self.record(&m.view(), child)?.0.attr, name));
            }
        }
        Ok(DirectoryPage {
            entries: output,
            continuation,
        })
    }

    pub(super) fn retain_kernel_in(&self, m: &mut Mutation<'_>, node: NodeId) -> Result<()> {
        if self.kernel_detached.load(Ordering::Acquire) {
            return Err(StoreError::InvalidInput("detached kernel session"));
        }
        let before = m.kernel_references(node)?;
        let next = before
            .checked_add(1)
            .ok_or(StoreError::InvalidInput("kernel lookup count"))?;
        if before == 0 {
            let (mut record, root) = self.record(&m.view(), node)?;
            record.pins = record
                .pins
                .checked_add(1)
                .ok_or(StoreError::InvalidInput("inode pins"))?;
            m.cache_inode_record(record, root.as_ref())?;
        }
        m.set_kernel_references(node, next)
    }

    fn forget_kernel_in(&self, m: &mut Mutation<'_>, node: NodeId, count: u64) -> Result<()> {
        if count == 0 {
            return Ok(());
        }
        let before = m.kernel_references(node)?;
        // FUSE seeds one implicit root lookup without an entry reply.
        if node == ROOT && before == 0 && count == 1 {
            return Ok(());
        }
        let remaining = before
            .checked_sub(count)
            .ok_or(StoreError::InvalidInput("kernel forget count"))?;
        m.set_kernel_references(node, remaining)?;
        if remaining == 0 {
            let (mut record, root) = self.record(&m.view(), node)?;
            record.pins = record
                .pins
                .checked_sub(1)
                .ok_or(StoreError::Integrity("kernel inode pin"))?;
            let removed = record.attr.links == 0
                || (record.attr.kind == Kind::Directory && record.attr.links < 2);
            if node != ROOT && removed && record.pins == 0 {
                m.remove_inode_record(node)?;
            } else {
                m.cache_inode_record(record, root.as_ref())?;
            }
        }
        Ok(())
    }

    pub(crate) fn kernel_entry(
        &self,
        operation: layerfs_fuse::host_wire::Operation<'_>,
    ) -> Result<Attr> {
        use layerfs_fuse::host_wire::Operation as Op;
        self.mutate(|m| {
            if self.kernel_detached.load(Ordering::Acquire) {
                return Err(StoreError::InvalidInput("detached kernel session"));
            }
            let attr = match operation {
                Op::Lookup {
                    parent,
                    name,
                    kernel: true,
                } => {
                    let node = self
                        .resolve_name(m, parent, name)?
                        .ok_or(StoreError::NotFound("name"))?;
                    self.metadata(&m.view(), node)?.attr
                }
                Op::Create {
                    parent,
                    name,
                    mode,
                    open,
                    kernel: true,
                } => self.create_in(
                    m,
                    parent,
                    name,
                    Kind::File,
                    mode & 0o777,
                    &self.ranges.empty(),
                    open,
                )?,
                Op::Mkdir {
                    parent,
                    name,
                    mode,
                    kernel: true,
                } => self.create_in(
                    m,
                    parent,
                    name,
                    Kind::Directory,
                    mode & 0o1777,
                    &self.ranges.empty(),
                    false,
                )?,
                Op::Symlink {
                    parent,
                    name,
                    target,
                    kernel: true,
                } => {
                    if target.len() > 4096 || target.contains(&0) {
                        return Err(StoreError::InvalidInput("symlink target"));
                    }
                    let tree = self.bytes_tree(target)?;
                    self.create_in(m, parent, name, Kind::Symlink, 0o777, &tree, false)?
                }
                Op::Link {
                    node,
                    parent,
                    name,
                    kernel: true,
                } => self.link_in(m, node, parent, name)?,
                _ => return Err(StoreError::InvalidInput("kernel entry operation")),
            };
            self.retain_kernel_in(m, attr.node)?;
            Ok(attr)
        })
    }

    pub(crate) fn kernel_directory_page(
        &self,
        node: NodeId,
        after: Option<&[u8]>,
    ) -> Result<DirectoryPage> {
        if let Some(name) = after {
            CanonicalName::from_bytes(name)?;
        }
        self.mutate(|m| {
            if self.kernel_detached.load(Ordering::Acquire) {
                return Err(StoreError::InvalidInput("detached kernel session"));
            }
            let page = self.directory_page_in(m, node, after)?;
            for (attr, _) in &page.entries {
                self.retain_kernel_in(m, attr.node)?;
            }
            Ok(page)
        })
    }

    pub(crate) fn kernel_forget(&self, node: NodeId, count: u64) -> Result<()> {
        self.kernel_forget_batch(&[(node, count)])
    }

    pub(crate) fn kernel_forget_batch(&self, entries: &[(NodeId, u64)]) -> Result<()> {
        if entries.len() > PAGE {
            return Err(StoreError::InvalidInput("kernel forget batch"));
        }
        if self.kernel_detached.load(Ordering::Acquire) {
            return Ok(());
        }
        self.mutate(|m| {
            if self.kernel_detached.load(Ordering::Acquire) {
                return Ok(());
            }
            for (node, count) in entries {
                self.forget_kernel_in(m, *node, *count)?;
            }
            Ok(())
        })
    }

    pub(crate) fn detach_kernel(&self) -> Result<()> {
        self.kernel_detached.store(true, Ordering::Release);
        loop {
            let remaining = self.mutate(|m| {
                let page = m.kernel_reference_page()?;
                let count = page.len();
                for (node, refs) in page {
                    self.forget_kernel_in(m, node, refs)?;
                }
                Ok(count)
            })?;
            if remaining < PAGE {
                return Ok(());
            }
        }
    }

    /// One bounded assist; callers schedule further work without interpreting
    /// a Commit as a live-operation pause or draining the graph at acquisition.
    /// The metadata index is recycled proportionally to what ordinary
    /// operations retired: a fixed handful of pages per step would otherwise
    /// let obsolete page versions outrun reclamation and exhaust the catalog
    /// quota long before any inode, owner or disk bound is reached.
    /// Root preparations observed by this host overlay (#144 R3a counter).
    pub(crate) fn preparation_attempts(&self) -> u64 {
        self.overlay.preparation_attempts()
    }

    pub(crate) fn maintain(&self) -> Result<bool> {
        if self.overlay.directory_cleanup_pending()? {
            self.mutate(|m| m.cleanup_deleted_directory())?;
        }
        const RECLAIM_STEPS: usize = 16;
        const RECLAIM_BATCH: usize = 256;
        for _ in 0..RECLAIM_STEPS {
            if !self.index.reclaim_pending() {
                break;
            }
            self.index.reclaim(RECLAIM_BATCH)?;
        }
        // #144 R3a: the payload release queue is per-write work that advances
        // one job per `reclaim_step`. A drain step calls this once, so the
        // queue, not the pending correspondence batch, bounded how much real
        // work a step could retire. A bounded multi-job duty keeps every call
        // finite while letting one batched step retire one batch of jobs.
        const RELEASE_STEPS: usize = 8;
        for _ in 0..RELEASE_STEPS {
            if !self.payload.reclaim_step()? {
                break;
            }
        }
        let index = self.index.stats()?;
        Ok(self.overlay.directory_cleanup_pending()?
            || index.reclamation_pending
            || self.payload.stats()?.cleanup_pending)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::overlay_budget::HostAdmission;
    use crate::WorkspaceFileReplacement as Replacement;
    use layerfs_layerstack_store::{
        EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
    };
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    pub(crate) struct Fixture {
        pub(crate) directory: PathBuf,
        pub(crate) host: Arc<HostOverlay>,
        pub(crate) _store: LayerStackStore,
    }
    impl Fixture {
        pub(crate) fn new(build: impl FnOnce(&Path), policy: ResourcePolicy) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "layerfs-host-ops-{}-{}",
                std::process::id(),
                crate::WorkspaceId::new()
            ));
            let source = directory.join("source");
            fs::create_dir_all(&source).unwrap();
            build(&source);
            let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
            let genesis = store
                .initialize_layerstack(
                    EntityName::new("host-operations").unwrap(),
                    LayerStackInitialization::Directory(source.clone()),
                )
                .unwrap()
                .genesis_layer_id;
            let branch = store
                .fork_branch(
                    EntityName::new("main").unwrap(),
                    LocalForkSource::Layer { layer_id: genesis },
                )
                .unwrap();
            let pinned = store.pin_branch(branch).unwrap();
            let runtime = directory.join("runtime");
            fs::create_dir(&runtime).unwrap();
            let resources =
                Budget::open(&runtime, policy, HostAdmission::shared().unwrap()).unwrap();
            let host = Arc::new(
                HostOverlay::new(
                    pinned.reader,
                    pinned.root,
                    crate::WorkspaceId::new().bytes(),
                    policy,
                    resources,
                )
                .unwrap(),
            );
            // Every subsequent cold read must use authenticated canonical Store
            // backing; it cannot accidentally read the initialization tree.
            fs::remove_dir_all(source).unwrap();
            Self {
                directory,
                host,
                _store: store,
            }
        }
        fn bytes(&self, node: NodeId) -> Vec<u8> {
            let mut bytes = vec![0; self.host.attr(node).unwrap().size as usize];
            assert_eq!(
                self.host.read_into(node, 0, &mut bytes).unwrap(),
                bytes.len()
            );
            bytes
        }
        fn snapshot_bytes(&self, snapshot: &Snapshot, path: &str) -> Vec<u8> {
            let mut bytes = vec![0; 4096];
            let count = self
                .host
                .read_snapshot_path(snapshot, &CanonicalPath::new(path).unwrap(), 0, &mut bytes)
                .unwrap();
            bytes.truncate(count);
            bytes
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.directory).unwrap();
        }
    }

    /// Diagnostic (not a gate): measured metadata and payload-catalog page
    /// writes for the ordinary tiny-file create and first-write path, split by
    /// phase so the next storage change is selected from counters rather than
    /// arithmetic. It asserts only that the workload's bytes are readable.
    #[test]
    fn tiny_create_metadata_page_cost_is_measured() {
        const FILES: usize = 100;
        let fixture = Fixture::new(|_| {}, ResourcePolicy::default());
        let host = &fixture.host;
        let payload_start = host.payload.stats().unwrap();
        let mut nodes = Vec::with_capacity(FILES);
        let before = host.index.stats().unwrap();
        for index in 0..FILES {
            let name = format!("tiny-{index:03}");
            nodes.push(
                host.create_file(ROOT, name.as_bytes(), 0o600)
                    .unwrap_or_else(|error| panic!("create {index} failed: {error:?}"))
                    .node,
            );
        }
        let created = host.index.stats().unwrap();
        for (index, node) in nodes.iter().enumerate() {
            assert_eq!(
                host.write(*node, 0, b"x").unwrap(),
                1,
                "first write {index} must store one byte"
            );
        }
        let written = host.index.stats().unwrap();
        let payload_end = host.payload.stats().unwrap();
        let mut byte = [0_u8; 1];
        let node = host.lookup(ROOT, b"tiny-099").unwrap().node;
        assert_eq!(host.read_into(node, 0, &mut byte).unwrap(), 1);
        assert_eq!(byte, [b'x']);
        println!(
            "tinycreate files={FILES} create_writes={} create_reads={} write_writes={} \
             write_reads={} total_writes={} total_reads={} metadata_live={} metadata_allocated={} \
             metadata_physical={} payload_catalog_allocated={} payload_live_pages={} \
             payload_physical={} payload_catalog_physical={}",
            created.page_writes - before.page_writes,
            created.page_reads - before.page_reads,
            written.page_writes - created.page_writes,
            written.page_reads - created.page_reads,
            written.page_writes - before.page_writes,
            written.page_reads - before.page_reads,
            written.live_pages,
            written.allocated_pages,
            written.physical_bytes,
            payload_end.index_allocated_pages - payload_start.index_allocated_pages,
            payload_end.index_live_pages,
            payload_end.physical_bytes,
            payload_end.index_physical_bytes
        );
    }

    #[test]
    fn sdk_candidate_snapshot_has_installed_generation_and_survives_later_write() {
        let fixture = Fixture::new(|_| {}, ResourcePolicy::default());
        let host = &fixture.host;
        let node = host.create_file(ROOT, b"candidate", 0o600).unwrap().node;
        host.write(node, 0, b"old").unwrap();
        let installed = host
            .edit_many_snapshot(node, &[(0, 3, Replacement::Inline(b"SDK".to_vec()))])
            .unwrap();
        assert_eq!(
            installed.root.sequence,
            host.overlay.acquire().unwrap().sequence
        );
        assert_eq!(
            installed.root.installation_sequence,
            host.overlay.acquire().unwrap().installation_sequence
        );
        host.write(node, 0, b"new").unwrap();
        let mut bytes = [0; 3];
        host.read_snapshot(&installed, node, 0, &mut bytes).unwrap();
        assert_eq!(&bytes, b"SDK");
        let empty_replacement = host
            .edit_many_snapshot(node, &[(0, 0, Replacement::Inline(Vec::new()))])
            .unwrap();
        assert_eq!(
            empty_replacement.root.sequence,
            host.overlay.acquire().unwrap().sequence
        );
        assert!(host.edit_many_snapshot(node, &[]).is_err());
    }

    #[test]
    fn authenticated_client_and_sdk_share_one_host_root_and_owned_snapshots() {
        use crate::host_operations::HostOperations;
        use layerfs_fuse::host_client::HostClient;
        use layerfs_fuse::live_runtime::LiveRuntime;
        use layerfs_fuse::live_transport::BackingServer;
        use layerfs_fuse::{FilesystemPort, KernelEntry, PortError};

        let fixture = Fixture::new(
            |source| {
                fs::write(source.join("canonical"), b"base").unwrap();
                fs::hard_link(source.join("canonical"), source.join("alias")).unwrap();
            },
            ResourcePolicy::default(),
        );
        let host = &fixture.host;
        let operations = Arc::new(HostOperations::new(host.clone()));
        let dispatch = operations.clone();
        let server = BackingServer::start(move |request| dispatch.request(request)).unwrap();
        let runtime = LiveRuntime::shared().unwrap();
        let client = runtime
            .block_on(HostClient::connect(
                format!("127.0.0.1:{}", server.port()),
                server.capability(),
                [41; 16],
                runtime.scheduler(),
            ))
            .unwrap();
        let file = client.lookup(ROOT, b"canonical").unwrap();
        assert_eq!(client.read(file.node, 0, 4).unwrap(), b"base");
        let before = host.snapshot().unwrap();
        assert_eq!(client.write(file.node, 0, b"host").unwrap(), 4);
        assert_eq!(client.lookup(ROOT, b"alias").unwrap().node, file.node);
        assert_eq!(fixture.bytes(file.node), b"host");
        host.edit_many(file.node, &[(1, 2, Replacement::Inline(b"SDK".to_vec()))])
            .unwrap();
        assert_eq!(client.read(file.node, 0, 8).unwrap(), b"hSDKt");
        let mut captured = [0; 4];
        host.read_snapshot(&before, file.node, 0, &mut captured)
            .unwrap();
        assert_eq!(&captured, b"base");
        client.fsync(Some(file.node)).unwrap();

        let opened = client.create_file_open(ROOT, b"opened", 0o600).unwrap();
        client.write(opened.node, 0, b"orphan").unwrap();
        client.unlink(ROOT, b"opened", false).unwrap();
        assert_eq!(client.read(opened.node, 0, 6).unwrap(), b"orphan");
        client.unpin(opened.node, true).unwrap();
        assert_eq!(client.attr(opened.node), Err(PortError::NotFound));

        runtime.block_on(async {
            let (attr, references) = client
                .kernel_entry_async(ROOT, b"canonical", KernelEntry::Lookup)
                .await
                .unwrap();
            assert_eq!(attr.node, file.node);
            references.submitted();
            let (page, mut references) = client.kernel_directory_page_async(ROOT, 0).await.unwrap();
            assert_eq!(&page[0].2, b".");
            assert_eq!(&page[1].2, b"..");
            assert_eq!(page.iter().filter(|(cookie, _, _)| *cookie > 2).count(), 2);
            references.release_unemitted(1).unwrap();
            references.submitted();
            client.detach().await.unwrap();
        });
        assert!(host
            .mutate(|m| m.kernel_reference_page())
            .unwrap()
            .is_empty());
        assert!(server.healthy());
        drop(client);
        drop(server);
        drop(operations);
        assert_eq!(fixture.bytes(file.node), b"hSDKt");
        host.read_snapshot(&before, file.node, 0, &mut captured)
            .unwrap();
        assert_eq!(&captured, b"base");
        println!("authenticated TCP HostClient/HostOperations PASS: sharedSDK/FUSEoperationauthority,snapshots,canonicalalias,open-unlinked,actualfsync,numericcookies,partialkernelreplyanddetach");
    }

    #[test]
    fn logical_spool_and_inline_limits_remain_distinct_and_failure_atomic() {
        let mut policy = ResourcePolicy::default();
        policy.max_spool_bytes = 4;
        let fixture = Fixture::new(|_| {}, policy);
        let host = &fixture.host;
        let file = host.create_file(ROOT, b"spool", 0o600).unwrap();
        assert_eq!(host.write(file.node, 0, b"four").unwrap(), 4);
        assert_eq!(host.payload.stats().unwrap().chargeable_bytes, 4);
        let captured = host.snapshot().unwrap();
        host.truncate(file.node, 0).unwrap();
        for _ in 0..128 {
            if !host.maintain().unwrap() {
                break;
            }
        }
        assert_eq!(host.payload.stats().unwrap().chargeable_bytes, 4);
        let before = host.overlay.acquire().unwrap();
        assert!(host.append(file.node, b"x").is_err());
        assert_eq!(host.overlay.acquire().unwrap().sequence, before.sequence);
        assert_eq!(
            host.payload.stats().unwrap().chargeable_bytes,
            4,
            "rejected append must not advance charged source highwater"
        );
        drop(before);
        drop(captured);
        for _ in 0..256 {
            if !host.maintain().unwrap() {
                break;
            }
        }
        assert_eq!(host.payload.stats().unwrap().chargeable_bytes, 0);
        assert_eq!(host.append(file.node, b"x").unwrap(), 1);
        assert_eq!(fixture.bytes(file.node), b"x");

        // SDK Inline has always had its own quota. Host-private storage must
        // preserve that distinction even above one fixed metadata leaf.
        let input = vec![b'i'; layerfs_workspace_core::file_edit::MAX_INLINE_PER_EDIT];
        let mut nodes = Vec::new();
        for i in 0..8 {
            let node = host
                .create_file(ROOT, format!("inline-{i}").as_bytes(), 0o600)
                .unwrap()
                .node;
            host.edit_many(node, &[(0, 0, Replacement::Inline(input.clone()))])
                .unwrap();
            nodes.push(node);
        }
        assert_eq!(
            host.overlay.acquire().unwrap().live_inline_bytes,
            8 * 1024 * 1024
        );
        assert_eq!(host.payload.stats().unwrap().chargeable_bytes, 1);
        let excess = host
            .create_file(ROOT, b"inline-excess", 0o600)
            .unwrap()
            .node;
        let before = host.overlay.acquire().unwrap();
        assert!(host
            .edit_many(excess, &[(0, 0, Replacement::Inline(vec![1]))])
            .is_err());
        assert_eq!(host.overlay.acquire().unwrap().sequence, before.sequence);
        assert_eq!(host.attr(excess).unwrap().size, 0);
        let inline_snapshot = host.snapshot().unwrap();
        host.truncate(nodes[0], 0).unwrap();
        host.edit_many(excess, &[(0, 0, Replacement::Inline(input.clone()))])
            .unwrap();
        assert_eq!(
            host.overlay.acquire().unwrap().live_inline_bytes,
            8 * 1024 * 1024
        );
        let mut retained = [0; 1];
        host.read_snapshot(&inline_snapshot, nodes[0], 0, &mut retained)
            .unwrap();
        assert_eq!(&retained, b"i");
        drop(inline_snapshot);
        drop(before);
        drop(fixture);

        let mut zero_spool = ResourcePolicy::default();
        zero_spool.max_spool_bytes = 0;
        let fixture = Fixture::new(|_| {}, zero_spool);
        let host = &fixture.host;
        let node = host.create_file(ROOT, b"inline-only", 0o600).unwrap().node;
        host.edit_many(node, &[(0, 0, Replacement::Inline(input))])
            .unwrap();
        assert_eq!(host.payload.stats().unwrap().chargeable_bytes, 0);
        assert!(host.write(node, 0, b"x").is_err());
        assert_eq!(host.attr(node).unwrap().size, 1024 * 1024);
        println!("host logical quota PASS: exact4-byte spool/retained owner/recovery;8MiB inline/+1 atomic rejection;zero-spool SDK1MiB permitted");
    }

    #[test]
    fn kernel_lookup_forget_and_detach_own_open_unlinked_inodes_atomically() {
        use layerfs_fuse::host_wire::Operation as Op;
        let fixture = Fixture::new(
            |source| fs::write(source.join("cold"), b"base").unwrap(),
            ResourcePolicy::default(),
        );
        let host = &fixture.host;
        let lookup = Op::Lookup {
            parent: ROOT,
            name: b"cold",
            kernel: true,
        };
        let file = host.kernel_entry(lookup).unwrap();
        host.kernel_entry(lookup).unwrap();
        assert_eq!(host.mutate(|m| m.kernel_references(file.node)).unwrap(), 2);
        host.unlink(ROOT, b"cold", false).unwrap();
        assert_eq!(fixture.bytes(file.node), b"base");
        host.kernel_forget(file.node, 1).unwrap();
        assert!(host.kernel_forget(file.node, 2).is_err());
        assert_eq!(host.mutate(|m| m.kernel_references(file.node)).unwrap(), 1);
        host.kernel_forget(file.node, 1).unwrap();
        assert!(host.attr(file.node).is_err());

        let created = host
            .kernel_entry(Op::Create {
                parent: ROOT,
                name: b"open",
                mode: 0o600,
                open: true,
                kernel: true,
            })
            .unwrap();
        host.write(created.node, 0, b"open").unwrap();
        host.unlink(ROOT, b"open", false).unwrap();
        host.kernel_forget(created.node, 1).unwrap();
        assert_eq!(fixture.bytes(created.node), b"open");
        host.unpin(created.node).unwrap();
        assert!(host.attr(created.node).is_err());

        let linked = host.create_file(ROOT, b"linked", 0o600).unwrap();
        host.kernel_entry(Op::Lookup {
            parent: ROOT,
            name: b"linked",
            kernel: true,
        })
        .unwrap();
        host.mutate(|m| m.set_kernel_references(linked.node, u64::MAX))
            .unwrap();
        let before = host.overlay.acquire().unwrap();
        assert!(host
            .kernel_entry(Op::Link {
                node: linked.node,
                parent: ROOT,
                name: b"must-not-appear",
                kernel: true
            })
            .is_err());
        assert_eq!(host.overlay.acquire().unwrap().sequence, before.sequence);
        assert!(host.lookup(ROOT, b"must-not-appear").is_err());
        assert_eq!(host.attr(linked.node).unwrap().links, 1);
        host.kernel_forget(linked.node, u64::MAX).unwrap();

        for i in 0..130 {
            host.create_file(ROOT, format!("page-{i:03}").as_bytes(), 0o600)
                .unwrap();
        }
        let mut after = None;
        let mut retained = Vec::new();
        loop {
            let page = host.kernel_directory_page(ROOT, after.as_deref()).unwrap();
            assert!(page.entries.len() <= PAGE);
            retained.extend(page.entries);
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(retained.len(), 131);
        for (attr, _) in &retained {
            assert_eq!(host.mutate(|m| m.kernel_references(attr.node)).unwrap(), 1);
        }
        host.detach_kernel().unwrap();
        assert!(host
            .mutate(|m| m.kernel_reference_page())
            .unwrap()
            .is_empty());
        host.kernel_forget(retained[0].0.node, 1).unwrap();
        assert!(host
            .kernel_entry(Op::Lookup {
                parent: ROOT,
                name: b"linked",
                kernel: true
            })
            .is_err());
        assert!(host.lookup(ROOT, b"linked").is_ok());
        println!("host kernel ownership PASS:2lookups/counted FORGET,independentopenpin,overflow rollback,131entriesinboundedpages,2-batchdetachandlateFORGET");
    }

    #[test]
    fn canonical_cold_snapshot_alias_write_splice_and_failed_batch_are_isolated() {
        let fixture = Fixture::new(
            |source| {
                fs::write(source.join("cold"), b"canonical bytes").unwrap();
                fs::hard_link(source.join("cold"), source.join("alias")).unwrap();
            },
            ResourcePolicy::default(),
        );
        let host = &fixture.host;
        let cold_snapshot = host.snapshot().unwrap();
        assert!(cold_snapshot.changes(0, None).unwrap().is_empty());
        assert_eq!(
            fixture.snapshot_bytes(&cold_snapshot, "cold"),
            b"canonical bytes"
        );
        let file = host.lookup(ROOT, b"cold").unwrap();
        assert_eq!(file.links, 2);
        assert!(
            host.snapshot()
                .unwrap()
                .changes(0, None)
                .unwrap()
                .is_empty(),
            "lazy import must not manufacture a dirty inode"
        );
        let before_write = host.snapshot().unwrap();
        host.write(file.node, 0, b"HOST").unwrap();
        let alias = host.lookup(ROOT, b"alias").unwrap();
        assert_eq!(alias.node, file.node);
        assert_eq!(fixture.bytes(alias.node), b"HOSTnical bytes");
        assert_eq!(
            fixture.snapshot_bytes(&cold_snapshot, "alias"),
            b"canonical bytes"
        );
        assert_eq!(
            fixture.snapshot_bytes(&before_write, "cold"),
            b"canonical bytes"
        );
        assert_eq!(
            fixture.snapshot_bytes(&host.snapshot().unwrap(), "alias"),
            b"HOSTnical bytes"
        );
        host.edit_many(
            file.node,
            &[(4, 5, Replacement::Inline(b" different ".to_vec()))],
        )
        .unwrap();
        let mut expected = b"HOSTnical bytes".to_vec();
        expected.splice(4..9, b" different ".iter().copied());
        assert_eq!(fixture.bytes(file.node), expected);
        let stable = host.snapshot().unwrap();
        assert!(host
            .edit_many(
                file.node,
                &[
                    (0, 1, Replacement::Inline(vec![b'X'])),
                    (9999, 1, Replacement::Zero(0))
                ]
            )
            .is_err());
        assert_eq!(host.snapshot().unwrap().root.sequence, stable.root.sequence);
        assert_eq!(fixture.bytes(file.node), expected);
        host.truncate(file.node, 7).unwrap();
        expected.truncate(7);
        host.truncate(file.node, 11).unwrap();
        expected.resize(11, 0);
        assert_eq!(fixture.bytes(file.node), expected);
        host.chmod(file.node, 0o2777).unwrap();
        assert_eq!(host.attr(file.node).unwrap().mode, 0o777);
        host.set_mtime(file.node, 123, 456).unwrap();
        assert!(host.set_mtime(file.node, 123, 1_000_000_000).is_err());
        assert_eq!(host.attr(file.node).unwrap().mtime_nanoseconds, 456);
        assert_eq!(
            fixture.snapshot_bytes(&stable, "cold"),
            b"HOST different  bytes"
        );
        println!("host canonical/cold snapshot, hardlink lazy import, unequal splice and failed batch: PASS");
    }

    #[test]
    fn namespace_rename_snapshots_and_open_unlinked_handles_keep_identity() {
        let fixture = Fixture::new(
            |source| {
                fs::create_dir(source.join("base-dir")).unwrap();
            },
            ResourcePolicy::default(),
        );
        let host = &fixture.host;
        let file = host.create_file_open(ROOT, b"live", 0o664).unwrap();
        host.write(file.node, 0, b"open bytes").unwrap();
        assert_eq!(host.link(file.node, ROOT, b"alias").unwrap().links, 2);
        let captured = host.snapshot().unwrap();
        host.unlink(ROOT, b"live", false).unwrap();
        host.rename(ROOT, b"alias", ROOT, b"moved", true).unwrap();
        assert_eq!(host.lookup(ROOT, b"moved").unwrap().node, file.node);
        assert_eq!(fixture.bytes(file.node), b"open bytes");
        assert_eq!(
            captured.binding(ROOT, b"alias").unwrap(),
            Some(Some(file.node))
        );
        assert_eq!(captured.binding(ROOT, b"moved").unwrap(), None);
        host.unlink(ROOT, b"moved", false).unwrap();
        assert_eq!(host.attr(file.node).unwrap().links, 0);
        let replacement = host.create_file(ROOT, b"moved", 0o600).unwrap();
        assert_ne!(replacement.node, file.node);
        assert_eq!(fixture.bytes(file.node), b"open bytes");
        host.unpin(file.node).unwrap();
        assert!(host.attr(file.node).is_err());
        let mut held = [0; 10];
        assert_eq!(
            host.read_snapshot(&captured, file.node, 0, &mut held)
                .unwrap(),
            10
        );
        assert_eq!(&held, b"open bytes");

        let dir = host.mkdir(ROOT, b"directory", 0o1750).unwrap();
        let child = host.mkdir(dir.node, b"child", 0o700).unwrap();
        assert!(host
            .rename(ROOT, b"directory", child.node, b"cycle", false)
            .is_err());
        assert_eq!(host.lookup(ROOT, b"directory").unwrap().node, dir.node);
        let parent = host.lookup(ROOT, b"base-dir").unwrap();
        host.rename(ROOT, b"directory", parent.node, b"renamed", false)
            .unwrap();
        assert_eq!(host.parent_of(dir.node).unwrap(), parent.node);
        assert_eq!(host.parent_of(child.node).unwrap(), dir.node);
        assert!(host.unlink(parent.node, b"renamed", true).is_err());
        host.pin_directory(child.node).unwrap();
        host.unlink(dir.node, b"child", true).unwrap();
        assert_eq!(host.parent_of(child.node).unwrap(), dir.node);
        assert!(host.create_file(child.node, b"orphan", 0o600).is_err());
        host.unpin(child.node).unwrap();
        assert!(host.attr(child.node).is_err());
        let symlink = host
            .symlink(ROOT, b"symbolic", b"base-dir/renamed")
            .unwrap();
        assert_eq!(host.readlink(symlink.node).unwrap(), b"base-dir/renamed");
        assert!(host.symlink(ROOT, b"bad", b"nul\0target").is_err());

        let done = Arc::new(AtomicBool::new(false));
        let worker = {
            let host = host.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                for _ in 0..32 {
                    host.rename(ROOT, b"moved", ROOT, b"moving", true).unwrap();
                    host.rename(ROOT, b"moving", ROOT, b"moved", true).unwrap();
                }
                done.store(true, Ordering::Release);
            })
        };
        let mut observations = 0;
        while !done.load(Ordering::Acquire) {
            let view = host.snapshot().unwrap();
            let a = view.binding(ROOT, b"moved").unwrap().flatten() == Some(replacement.node);
            let b = view.binding(ROOT, b"moving").unwrap().flatten() == Some(replacement.node);
            assert_ne!(
                a, b,
                "an atomic rename must not expose half of its bindings"
            );
            observations += 1;
        }
        worker.join().unwrap();
        assert!(observations > 0);
        println!("host namespace/open-unlinked identity PASS; concurrent rename snapshot observations={observations}");
    }

    /// Focused ownership-accounting reproduction, explicitly selected by name.
    /// This is **not** the #123 million-changed-file qualification: it uses
    /// 8,193 distinct regular files, one small ordinary write per file, one
    /// bounded existing maintenance step per completed write, no Commit and no
    /// retained snapshot. Ignored in the default suite because of its scale.
    #[test]
    #[ignore = "scale reproduction; select explicitly: cargo test -p layerfs-workspace many_ordinary_files"]
    fn many_ordinary_files_do_not_exhaust_payload_owner_admission() {
        const FILES: usize = 8193;
        let fixture = Fixture::new(|_| {}, ResourcePolicy::default());
        let host = &fixture.host;
        let mut peak_owners: usize = 0;
        let mut peak_pending: usize = 0;
        let mut peak_persisted: usize = 0;
        let mut failure: Option<(usize, String)> = None;
        for index in 0..FILES {
            let name = format!("file-{index:05}");
            let node = host
                .create_file(ROOT, name.as_bytes(), 0o600)
                .unwrap_or_else(|error| panic!("ordinary create {index} failed: {error:?}"))
                .node;
            match host.write(node, 0, b"x") {
                Ok(written) => assert_eq!(written, 1),
                Err(error) => {
                    let stats = host.payload.stats().unwrap();
                    println!(
                        "REPRODUCER first failure index={index} error={error:?} owners={} \
                         persisted_tokens={} pending_releases={} physical={} index_physical={} \
                         chargeable={}",
                        stats.owners,
                        stats.persisted_tokens,
                        stats.pending_releases,
                        stats.physical_bytes,
                        stats.index_physical_bytes,
                        stats.chargeable_bytes
                    );
                    failure = Some((index, format!("{error:?}")));
                    break;
                }
            }
            host.maintain().unwrap();
            let stats = host.payload.stats().unwrap();
            peak_owners = peak_owners.max(stats.owners);
            peak_pending = peak_pending.max(stats.pending_releases);
            peak_persisted = peak_persisted.max(stats.persisted_tokens);
            if index % 1000 == 999 || index == FILES - 1 {
                println!(
                    "REPRODUCER census file={index} keys={:?}",
                    host.payload.catalog_census().unwrap()
                );
                println!(
                    "REPRODUCER tree file={index} {:?}",
                    host.payload.catalog_tree_census().unwrap()
                );
                println!(
                    "REPRODUCER headers file={index} {:?}",
                    host.payload.catalog_header_census().unwrap()
                );
                let metadata = host.index.stats().unwrap();
                println!(
                    "REPRODUCER progress file={index} payload_index_live={} \
                     payload_index_allocated={} payload_index_reclaimed={} \
                     payload_index_roots={} payload_index_pending={} payload_index_physical={} \
                     metadata_live={} owners={} persisted={} pending={} physical={}",
                    stats.index_live_pages,
                    stats.index_allocated_pages,
                    stats.index_reclaimed_pages,
                    stats.index_pending_roots,
                    stats.index_reclamation_pending,
                    stats.index_physical_bytes,
                    metadata.live_pages,
                    stats.owners,
                    stats.persisted_tokens,
                    stats.pending_releases,
                    stats.physical_bytes
                );
            }
        }
        let stats = host.payload.stats().unwrap();
        println!(
            "REPRODUCER files={FILES} peak_resident_owners={peak_owners} \
             peak_pending_releases={peak_pending} peak_persisted_tokens={peak_persisted} \
             owners={} persisted_tokens={} pending_releases={} chargeable={} physical={} \
             index_physical={}",
            stats.owners,
            stats.persisted_tokens,
            stats.pending_releases,
            stats.chargeable_bytes,
            stats.physical_bytes,
            stats.index_physical_bytes
        );
        // Whichever file the failure landed on stays empty, and every earlier
        // acknowledged byte is still readable: a rejected admission may not
        // corrupt retained ownership.
        let acknowledged = failure.as_ref().map_or(FILES, |(index, _)| *index);
        for index in 0..acknowledged {
            let attr = host
                .lookup(ROOT, format!("file-{index:05}").as_bytes())
                .unwrap();
            let mut byte = [0];
            assert_eq!(
                host.read_into(attr.node, 0, &mut byte).unwrap(),
                1,
                "prior acknowledged byte lost at {index}"
            );
            assert_eq!(byte, [b'x'], "prior acknowledged byte changed at {index}");
        }
        if let Some((index, error)) = failure {
            let attr = host
                .lookup(ROOT, format!("file-{index:05}").as_bytes())
                .unwrap();
            assert_eq!(attr.size, 0, "rejected ordinary write was not atomic");
            panic!(
                "ordinary write {index}/{FILES} rejected: {error}; earlier acknowledged bytes and \
                 the rejected file's emptiness verified"
            );
        }
        let settled = host.payload.stats().unwrap();
        assert_eq!(
            settled.persisted_tokens, FILES,
            "every live file retains exactly one persisted payload token"
        );
        assert!(
            peak_owners <= ResourcePolicy::default().overlay.max_payload_owners,
            "resident owners exceeded the configured cap: {peak_owners}"
        );
        println!(
            "REPRODUCER PASS files={FILES} verified_bytes={acknowledged} \
             peak_resident_owners={peak_owners} peak_pending_releases={peak_pending} \
             resident_owners={} persisted_tokens={}",
            settled.owners, settled.persisted_tokens
        );
    }

    #[test]
    fn directory_pages_advance_over_tombstones_without_materializing_namespace() {
        let fixture = Fixture::new(
            |source| {
                for i in 0..130 {
                    fs::write(source.join(format!("n{i:03}")), [i as u8]).unwrap();
                }
            },
            ResourcePolicy::default(),
        );
        let host = &fixture.host;
        for i in 0..128 {
            host.unlink(ROOT, format!("n{i:03}").as_bytes(), false)
                .unwrap();
        }
        let first = host.directory_page(ROOT, None).unwrap();
        assert!(first.entries.is_empty());
        let cursor = first
            .continuation
            .expect("tombstone-only page still has a next cursor");
        let final_page = host.directory_page(ROOT, Some(&cursor)).unwrap();
        assert_eq!(
            final_page
                .entries
                .iter()
                .map(|(_, name)| name.as_slice())
                .collect::<Vec<_>>(),
            [b"n128".as_slice(), b"n129".as_slice()]
        );
        assert!(final_page.continuation.is_none());
        println!(
            "host bounded directory pagination PASS:128 tombstones followed by2 visible entries"
        );
    }
}
