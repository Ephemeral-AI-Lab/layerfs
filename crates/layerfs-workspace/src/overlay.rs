//! Private host overlay roots. Installation never performs backing I/O.
//!
//! These roots describe host-installed state; kernel mapping visibility is a
//! separate obligation of the FUSE adapter, not a property of an Arc clone.
use crate::overlay_index::{Index, Limits, Root};
use layerfs_content::tree::inode::InodeId;
use layerfs_content::{CanonicalName, ObjectId};
use layerfs_layerstack_store::{Result, StoreError};
use layerfs_workspace_core::{Attr, Kind, NodeId};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

const INODE: u8 = 1;
const BINDING: u8 = 2;
const CHANGE_KEY: u8 = 3;
pub(crate) const CHANGE_SEQUENCE: u8 = 4;
pub(crate) const CANONICAL_INODE: u8 = 5;
pub(crate) const REVERSE_BINDING: u8 = 6;
const KERNEL_REFERENCE: u8 = 7;
pub(crate) const CHANGE_BATCH: usize = 128;

/// Fixed metadata; file ranges and directory bindings live in their own trees.
/// No growing alias list, directory delta map, or piece vector is embedded here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InodeRecord {
    pub(crate) attr: Attr,
    pub(crate) revision: u64,
    pub(crate) pins: u32,
    pub(crate) canonical: Option<InodeId>,
    pub(crate) base: Option<ObjectId>,
    pub(crate) parent: Option<NodeId>,
}

impl InodeRecord {
    pub(crate) fn encode(self) -> Result<[u8; 128]> {
        if self.attr.node.0 == 0
            || self.parent.is_some_and(|id| id.0 == 0)
            || self.attr.mtime_nanoseconds >= 1_000_000_000
            || self.attr.mode & !0o7777 != 0
        {
            return Err(StoreError::InvalidInput("overlay inode metadata"));
        }
        let mut out = [0; 128];
        out[0] = 1;
        out[1] = match self.attr.kind {
            Kind::File => 1,
            Kind::Directory => 2,
            Kind::Symlink => 3,
        };
        out[2] = u8::from(self.canonical.is_some());
        out[3] = u8::from(self.base.is_some());
        out[8..16].copy_from_slice(&self.revision.to_be_bytes());
        out[16..20].copy_from_slice(&self.attr.mode.to_be_bytes());
        out[20..24].copy_from_slice(&self.attr.links.to_be_bytes());
        out[24..28].copy_from_slice(&self.pins.to_be_bytes());
        out[32..40].copy_from_slice(&self.attr.mtime_seconds.to_be_bytes());
        out[40..44].copy_from_slice(&self.attr.mtime_nanoseconds.to_be_bytes());
        if let Some(id) = self.canonical {
            out[48..80].copy_from_slice(id.as_bytes());
        }
        if let Some(root) = self.base {
            out[80..112].copy_from_slice(root.as_bytes());
        }
        out[112..120].copy_from_slice(&self.parent.map_or(0, |id| id.0).to_be_bytes());
        out[120..128].copy_from_slice(&self.attr.size.to_be_bytes());
        Ok(out)
    }

    pub(crate) fn decode(node: NodeId, bytes: &[u8]) -> Result<Self> {
        if bytes.len() != 128 || bytes[0] != 1 || bytes[2] > 1 || bytes[3] > 1 {
            return Err(StoreError::Integrity("overlay inode record"));
        }
        let u32_at =
            |offset: usize| u32::from_be_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let parent = decode_sequence(&bytes[112..120])?;
        let record = Self {
            attr: Attr {
                node,
                size: decode_sequence(&bytes[120..128])?,
                kind: match bytes[1] {
                    1 => Kind::File,
                    2 => Kind::Directory,
                    3 => Kind::Symlink,
                    _ => return Err(StoreError::Integrity("overlay inode kind")),
                },
                mode: u32_at(16),
                links: u32_at(20),
                mtime_seconds: i64::from_be_bytes(bytes[32..40].try_into().unwrap()),
                mtime_nanoseconds: u32_at(40),
            },
            revision: decode_sequence(&bytes[8..16])?,
            pins: u32_at(24),
            canonical: (bytes[2] == 1).then(|| InodeId(bytes[48..80].try_into().unwrap())),
            base: if bytes[3] == 1 {
                Some(ObjectId::from_bytes(&bytes[80..112])?)
            } else {
                None
            },
            parent: (parent != 0).then_some(NodeId(parent)),
        };
        if record.encode()?.as_slice() != bytes {
            return Err(StoreError::Integrity("noncanonical overlay inode record"));
        }
        Ok(record)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ChangeKey {
    Inode(NodeId),
    Binding(NodeId, Vec<u8>),
}

impl ChangeKey {
    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        let (kind, node, name) = match self {
            Self::Inode(node) => (INODE, node, &[][..]),
            Self::Binding(node, name) => {
                CanonicalName::from_bytes(name)?;
                (BINDING, node, name.as_slice())
            }
        };
        if node.0 == 0 {
            return Err(StoreError::InvalidInput("overlay inode identity"));
        }
        let mut out = Vec::with_capacity(9 + name.len());
        out.push(kind);
        out.extend_from_slice(&node.0.to_be_bytes());
        out.extend_from_slice(name);
        Ok(out)
    }

    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 9 {
            return Err(StoreError::Integrity("overlay change key"));
        }
        let node = NodeId(u64::from_be_bytes(bytes[1..9].try_into().unwrap()));
        let key = match bytes[0] {
            INODE if bytes.len() == 9 => Self::Inode(node),
            BINDING => Self::Binding(node, bytes[9..].to_vec()),
            _ => return Err(StoreError::Integrity("overlay change kind")),
        };
        key.encode()?;
        Ok(key)
    }
}

#[derive(Clone)]
pub(crate) struct OverlayRoot {
    pub(crate) installation_sequence: u64,
    pub(crate) sequence: u64,
    pub(crate) base_root: ObjectId,
    pub(crate) next_inode: u64,
    pub(crate) live_inline_bytes: u64,
    pub(crate) index: Root,
}

pub(crate) struct Overlay {
    index: Index,
    current: Mutex<Arc<OverlayRoot>>,
    admission: Mutex<MutationQueue>,
    available: Condvar,
    preparation_attempts: AtomicU64,
    root_conflicts: AtomicU64,
}

#[derive(Default)]
struct MutationQueue {
    next: u64,
    serving: u64,
}

// Bounded ordinary-mutation admission. Snapshot/read/Commit construction never
// acquires this queue. Payload transfers precede metadata preparation.
const MAX_QUEUED_MUTATIONS: u64 = 64;

struct MutationTurn<'a>(&'a Overlay);
impl Drop for MutationTurn<'_> {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.0.admission.lock() {
            queue.serving += 1;
            self.0.available.notify_all();
        }
    }
}

pub(crate) struct PreparedMutation {
    source: Arc<OverlayRoot>,
    candidate: Arc<OverlayRoot>,
}

/// A private candidate can only escape after its entire operation succeeds.
pub(crate) struct Mutation<'a> {
    pub(super) index: &'a Index,
    pub(super) candidate: OverlayRoot,
    logical_change: bool,
}

impl Overlay {
    pub(crate) fn temporary(directory: &Path, base_root: ObjectId, limits: Limits) -> Result<Self> {
        let index = Index::temporary(directory, limits)?;
        Self::from_index(index, base_root)
    }

    pub(crate) fn from_index(index: Index, base_root: ObjectId) -> Result<Self> {
        let root = index.empty_root()?;
        Ok(Self {
            index,
            admission: Mutex::new(MutationQueue::default()),
            available: Condvar::new(),
            preparation_attempts: AtomicU64::new(0),
            root_conflicts: AtomicU64::new(0),
            current: Mutex::new(Arc::new(OverlayRoot {
                installation_sequence: 0,
                sequence: 0,
                base_root,
                next_inode: 2,
                live_inline_bytes: 0,
                index: root,
            })),
        })
    }

    pub(crate) fn acquire(&self) -> Result<Arc<OverlayRoot>> {
        let current = self
            .current
            .lock()
            .map_err(|_| StoreError::Integrity("overlay installation lock"))?;
        Ok(Arc::clone(&current))
    }

    pub(crate) fn prepare(
        &self,
        source: Arc<OverlayRoot>,
        apply: impl FnOnce(&mut Mutation<'_>) -> Result<()>,
    ) -> Result<PreparedMutation> {
        self.preparation_attempts.fetch_add(1, Ordering::Relaxed);
        let mut mutation = Mutation {
            index: &self.index,
            logical_change: false,
            candidate: OverlayRoot {
                installation_sequence: source
                    .installation_sequence
                    .checked_add(1)
                    .ok_or(StoreError::InvalidInput("overlay installation overflow"))?,
                sequence: source
                    .sequence
                    .checked_add(1)
                    .ok_or(StoreError::InvalidInput("overlay sequence overflow"))?,
                base_root: source.base_root,
                next_inode: source.next_inode,
                live_inline_bytes: source.live_inline_bytes,
                index: source.index.clone(),
            },
        };
        apply(&mut mutation)?;
        if !mutation.logical_change {
            // Cache, handle ownership, equivalent backing substitution and
            // covered-entry retirement change root identity, not user generation.
            mutation.candidate.sequence = source.sequence;
        }
        Ok(PreparedMutation {
            source,
            candidate: Arc::new(mutation.candidate),
        })
    }

    /// A stale root is returned to the caller for release/repreparation outside
    /// this lock. Per-inode revision equality cannot authorize a root replacement.
    pub(crate) fn install(&self, prepared: PreparedMutation) -> Result<bool> {
        let previous = {
            let mut current = self
                .current
                .lock()
                .map_err(|_| StoreError::Integrity("overlay installation lock"))?;
            if !Arc::ptr_eq(&current, &prepared.source) {
                self.root_conflicts.fetch_add(1, Ordering::Relaxed);
                return Ok(false);
            }
            std::mem::replace(&mut *current, prepared.candidate)
        };
        drop(previous);
        Ok(true)
    }

    /// FIFO turns bound preparation and prevent disjoint writers starving one
    /// another through speculative whole-root retries. A turn owns no root lock
    /// while it reads/writes index pages; readers can use their retained roots.
    pub(crate) fn mutate(
        &self,
        mut apply: impl FnMut(&mut Mutation<'_>) -> Result<()>,
    ) -> Result<()> {
        let mut queue = self
            .admission
            .lock()
            .map_err(|_| StoreError::Integrity("overlay mutation admission"))?;
        if queue.next - queue.serving >= MAX_QUEUED_MUTATIONS {
            return Err(StoreError::StoreBusy);
        }
        let ticket = queue.next;
        queue.next = ticket
            .checked_add(1)
            .ok_or(StoreError::InvalidInput("overlay mutation ticket overflow"))?;
        while ticket != queue.serving {
            queue = self
                .available
                .wait(queue)
                .map_err(|_| StoreError::Integrity("overlay mutation admission"))?;
        }
        drop(queue);
        let _turn = MutationTurn(self);
        // Direct prepared callers may have raced the ticket. They must rebase;
        // no immutable candidate is patched under installation synchronization.
        for _ in 0..3 {
            let prepared = self.prepare(self.acquire()?, &mut apply)?;
            if self.install(prepared)? {
                return Ok(());
            }
        }
        Err(StoreError::StoreBusy)
    }

    pub(crate) fn snapshot(&self) -> Result<crate::snapshot::Snapshot> {
        Ok(crate::snapshot::Snapshot::new(
            self.index.clone(),
            self.acquire()?,
        ))
    }

    #[cfg(test)]
    fn inode(&self, root: &Arc<OverlayRoot>, inode: NodeId) -> Result<Option<Vec<u8>>> {
        crate::snapshot::Snapshot::new(self.index.clone(), root.clone()).inode(inode)
    }

    #[cfg(test)]
    fn binding(
        &self,
        root: &Arc<OverlayRoot>,
        parent: NodeId,
        name: &[u8],
    ) -> Result<Option<Option<NodeId>>> {
        crate::snapshot::Snapshot::new(self.index.clone(), root.clone()).binding(parent, name)
    }

    #[cfg(test)]
    fn changes(
        &self,
        root: &Arc<OverlayRoot>,
        covered: u64,
        after: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, u64, ChangeKey)>> {
        crate::snapshot::Snapshot::new(self.index.clone(), root.clone()).changes(covered, after)
    }
}

impl Mutation<'_> {
    pub(crate) fn kernel_references(&self, inode: NodeId) -> Result<u64> {
        let key = [vec![KERNEL_REFERENCE], inode.0.to_be_bytes().to_vec()].concat();
        self.index
            .get(&self.candidate.index, &key)?
            .map_or(Ok(0), |bytes| decode_sequence(&bytes))
    }

    pub(crate) fn set_kernel_references(&mut self, inode: NodeId, count: u64) -> Result<()> {
        if inode.0 == 0 {
            return Err(StoreError::InvalidInput("kernel inode"));
        }
        let key = [vec![KERNEL_REFERENCE], inode.0.to_be_bytes().to_vec()].concat();
        self.candidate.index = if count == 0 {
            self.index.remove(&self.candidate.index, &key)?
        } else {
            self.index
                .set(&self.candidate.index, &key, &count.to_be_bytes())?
        };
        Ok(())
    }

    pub(crate) fn kernel_reference_page(&self) -> Result<Vec<(NodeId, u64)>> {
        use std::ops::Bound::Included;
        self.index
            .scan(
                &self.candidate.index,
                Included(&[KERNEL_REFERENCE]),
                Included(&[KERNEL_REFERENCE + 1]),
                CHANGE_BATCH,
            )?
            .into_iter()
            .map(|(key, bytes)| {
                if key.len() != 9 || key[0] != KERNEL_REFERENCE {
                    return Err(StoreError::Integrity("kernel reference index"));
                }
                Ok((
                    NodeId(decode_sequence(&key[1..])?),
                    decode_sequence(&bytes)?,
                ))
            })
            .collect()
    }

    pub(crate) fn view(&self) -> crate::snapshot::Snapshot {
        crate::snapshot::Snapshot::new(self.index.clone(), Arc::new(self.candidate.clone()))
    }

    pub(crate) fn allocate_inode(&mut self) -> Result<NodeId> {
        let id = self.candidate.next_inode;
        self.candidate.next_inode = id
            .checked_add(1)
            .ok_or(StoreError::InvalidInput("overlay inode overflow"))?;
        Ok(NodeId(id))
    }

    pub(crate) fn put_inode(&mut self, inode: NodeId, record: &[u8]) -> Result<()> {
        let key = ChangeKey::Inode(inode).encode()?;
        if inode.0 >= self.candidate.next_inode {
            self.candidate.next_inode = inode
                .0
                .checked_add(1)
                .ok_or(StoreError::InvalidInput("overlay inode overflow"))?;
        }
        self.candidate.index = self.index.set(&self.candidate.index, &key, record)?;
        self.changed(&key)
    }

    pub(crate) fn put_inode_record(
        &mut self,
        record: InodeRecord,
        ranges: Option<&Root>,
    ) -> Result<()> {
        self.store_inode_record(record, ranges, true)
    }

    pub(crate) fn cache_inode_record(
        &mut self,
        record: InodeRecord,
        ranges: Option<&Root>,
    ) -> Result<()> {
        self.store_inode_record(record, ranges, false)
    }

    pub(crate) fn store_inode_record(
        &mut self,
        record: InodeRecord,
        ranges: Option<&Root>,
        changed: bool,
    ) -> Result<()> {
        let bytes = record.encode()?;
        let key = ChangeKey::Inode(record.attr.node).encode()?;
        let before = match self.index.get_linked(&self.candidate.index, &key)? {
            Some((bytes, root))
                if InodeRecord::decode(record.attr.node, &bytes)?.attr.kind == Kind::File =>
            {
                crate::overlay_ranges::Ranges::root_inline_bytes(self.index, root.as_ref())?
            }
            _ => 0,
        };
        let after = if record.attr.kind == Kind::File {
            crate::overlay_ranges::Ranges::root_inline_bytes(self.index, ranges)?
        } else {
            0
        };
        let inline = self
            .candidate
            .live_inline_bytes
            .checked_sub(before)
            .and_then(|value| value.checked_add(after))
            .filter(|value| *value <= layerfs_workspace_core::file_edit::MAX_INLINE_PER_WORKSPACE)
            .ok_or(StoreError::InvalidInput("workspace inline limit"))?;
        if record.attr.node.0 >= self.candidate.next_inode {
            self.candidate.next_inode = record
                .attr
                .node
                .0
                .checked_add(1)
                .ok_or(StoreError::InvalidInput("overlay inode overflow"))?;
        }
        self.candidate.index =
            self.index
                .set_linked(&self.candidate.index, &key, &bytes, ranges)?;
        if let Some(canonical) = record.canonical {
            let key = [vec![CANONICAL_INODE], canonical.as_bytes().to_vec()].concat();
            let bytes = record.attr.node.0.to_be_bytes();
            match self.index.get(&self.candidate.index, &key)? {
                Some(old) if old != bytes => {
                    return Err(StoreError::Integrity(
                        "canonical overlay inode already assigned",
                    ))
                }
                Some(_) => {}
                None => {
                    self.candidate.index = self.index.set(&self.candidate.index, &key, &bytes)?
                }
            }
        }
        if changed {
            self.changed(&key)?;
        }
        self.candidate.live_inline_bytes = inline;
        Ok(())
    }

    pub(crate) fn remove_inode_record(&mut self, inode: NodeId) -> Result<()> {
        let key = ChangeKey::Inode(inode).encode()?;
        let (bytes, ranges) = self
            .index
            .get_linked(&self.candidate.index, &key)?
            .ok_or(StoreError::NotFound("overlay inode"))?;
        let record = InodeRecord::decode(inode, &bytes)?;
        let inline = if record.attr.kind == Kind::File {
            crate::overlay_ranges::Ranges::root_inline_bytes(self.index, ranges.as_ref())?
        } else {
            0
        };
        let retained_inline = self
            .candidate
            .live_inline_bytes
            .checked_sub(inline)
            .ok_or(StoreError::Integrity("workspace inline charge"))?;
        if let Some(canonical) = record.canonical {
            let canonical = [vec![CANONICAL_INODE], canonical.as_bytes().to_vec()].concat();
            self.candidate.index = self.index.remove(&self.candidate.index, &canonical)?;
        }
        self.candidate.index = self.index.remove(&self.candidate.index, &key)?;
        if record.attr.kind == Kind::Directory {
            self.remove_cookie_directory(inode)?;
        }
        self.changed(&key)?;
        self.candidate.live_inline_bytes = retained_inline;
        Ok(())
    }

    pub(crate) fn put_binding(
        &mut self,
        parent: NodeId,
        name: &[u8],
        inode: Option<NodeId>,
    ) -> Result<()> {
        self.store_binding(parent, name, inode, true)
    }

    pub(crate) fn cache_binding(
        &mut self,
        parent: NodeId,
        name: &[u8],
        inode: Option<NodeId>,
    ) -> Result<()> {
        self.store_binding(parent, name, inode, false)
    }

    fn store_binding(
        &mut self,
        parent: NodeId,
        name: &[u8],
        inode: Option<NodeId>,
        changed: bool,
    ) -> Result<()> {
        if inode.is_some_and(|id| id.0 == 0) {
            return Err(StoreError::InvalidInput("overlay inode identity"));
        }
        self.store_cookie_binding(parent, name, inode)?;
        let key = ChangeKey::Binding(parent, name.to_vec()).encode()?;
        if let Some(old) = self.index.get(&self.candidate.index, &key)? {
            let old = decode_sequence(&old)?;
            if old != 0 {
                let reverse = reverse_binding_key(NodeId(old), parent, name);
                self.candidate.index = self.index.remove(&self.candidate.index, &reverse)?;
            }
        }
        self.candidate.index = self.index.set(
            &self.candidate.index,
            &key,
            &inode.map_or(0, |id| id.0).to_be_bytes(),
        )?;
        if let Some(inode) = inode {
            self.candidate.index = self.index.set(
                &self.candidate.index,
                &reverse_binding_key(inode, parent, name),
                &[],
            )?;
        }
        if changed {
            self.changed(&key)?;
        }
        Ok(())
    }

    fn changed(&mut self, key: &[u8]) -> Result<()> {
        self.logical_change = true;
        let by_key = [vec![CHANGE_KEY], key.to_vec()].concat();
        if let Some(old) = self.index.get(&self.candidate.index, &by_key)? {
            let old_sequence = decode_sequence(&old)?;
            let old_key = sequence_key(old_sequence, key);
            self.candidate.index = self.index.remove(&self.candidate.index, &old_key)?;
        }
        self.candidate.index = self.index.set(
            &self.candidate.index,
            &by_key,
            &self.candidate.sequence.to_be_bytes(),
        )?;
        self.candidate.index = self.index.set(
            &self.candidate.index,
            &sequence_key(self.candidate.sequence, key),
            &[],
        )?;
        Ok(())
    }

    /// Drop only exactly covered bookkeeping; binding masks and newer changes
    /// survive. The caller supplies one bounded batch from a captured root.
    pub(crate) fn forget_covered(&mut self, key: &ChangeKey, sequence: u64) -> Result<()> {
        let key = key.encode()?;
        let by_key = [vec![CHANGE_KEY], key.clone()].concat();
        if self.index.get(&self.candidate.index, &by_key)?.as_deref()
            == Some(sequence.to_be_bytes().as_slice())
        {
            self.candidate.index = self.index.remove(&self.candidate.index, &by_key)?;
            self.candidate.index = self
                .index
                .remove(&self.candidate.index, &sequence_key(sequence, &key))?;
        }
        Ok(())
    }
}

pub(crate) fn reverse_binding_key(inode: NodeId, parent: NodeId, name: &[u8]) -> Vec<u8> {
    let mut key = Vec::with_capacity(17 + name.len());
    key.push(REVERSE_BINDING);
    key.extend_from_slice(&inode.0.to_be_bytes());
    key.extend_from_slice(&parent.0.to_be_bytes());
    key.extend_from_slice(name);
    key
}

fn sequence_key(sequence: u64, key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + key.len());
    out.push(CHANGE_SEQUENCE);
    out.extend_from_slice(&sequence.to_be_bytes());
    out.extend_from_slice(key);
    out
}

pub(crate) fn decode_sequence(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_be_bytes(
        bytes
            .try_into()
            .map_err(|_| StoreError::Integrity("overlay integer"))?,
    ))
}

/// One authenticated transport session. Reservations precede mutation installation;
/// even a lost/oversized result leaves an occupied slot and cannot replay a write.
pub(crate) struct ReplayWindow {
    session: [u8; 16],
    watermark: u64,
    max_requests: usize,
    max_bytes: usize,
    reserved_bytes: usize,
    slots: BTreeMap<u64, ReplaySlot>,
}

struct ReplaySlot {
    reserved: usize,
    request_digest: [u8; 32],
    result: Option<Arc<[u8]>>,
}

pub(crate) enum ReplayAdmission {
    New,
    Completed(Arc<[u8]>),
    Unknown,
}

impl ReplayWindow {
    pub(crate) fn new(session: [u8; 16], max_requests: usize, max_bytes: usize) -> Result<Self> {
        if max_requests == 0 || max_bytes == 0 {
            return Err(StoreError::InvalidInput("overlay replay limits"));
        }
        Ok(Self {
            session,
            watermark: 0,
            max_requests,
            max_bytes,
            reserved_bytes: 0,
            slots: BTreeMap::new(),
        })
    }

    pub(crate) fn admit(
        &mut self,
        session: [u8; 16],
        sequence: u64,
        request_digest: [u8; 32],
        response_bytes: usize,
    ) -> Result<ReplayAdmission> {
        if session != self.session {
            return Ok(ReplayAdmission::Unknown);
        }
        if sequence <= self.watermark {
            return Err(StoreError::InvalidInput("stale overlay request"));
        }
        if let Some(slot) = self.slots.get(&sequence) {
            if slot.request_digest != request_digest {
                return Err(StoreError::InvalidInput("overlay replay request changed"));
            }
            return Ok(match &slot.result {
                Some(result) => ReplayAdmission::Completed(result.clone()),
                None => ReplayAdmission::Unknown,
            });
        }
        if sequence - self.watermark > self.max_requests as u64
            || self.slots.len() >= self.max_requests
            || response_bytes > self.max_bytes.saturating_sub(self.reserved_bytes)
        {
            return Err(StoreError::InvalidInput("overlay replay capacity"));
        }
        self.slots.insert(
            sequence,
            ReplaySlot {
                reserved: response_bytes,
                request_digest,
                result: None,
            },
        );
        self.reserved_bytes += response_bytes;
        Ok(ReplayAdmission::New)
    }

    pub(crate) fn finish(&mut self, sequence: u64, result: Arc<[u8]>) -> Result<()> {
        let slot = self
            .slots
            .get_mut(&sequence)
            .ok_or(StoreError::Integrity("unadmitted overlay result"))?;
        if slot.result.is_some() || result.len() > slot.reserved {
            return Err(StoreError::Integrity("overlay result reservation"));
        }
        slot.result = Some(result);
        Ok(())
    }

    pub(crate) fn acknowledge(&mut self, session: [u8; 16], through: u64) -> Result<()> {
        if session != self.session {
            return Err(StoreError::InvalidInput("overlay replay session"));
        }
        if through <= self.watermark {
            return Ok(());
        }
        if through - self.watermark > self.max_requests as u64 {
            return Err(StoreError::InvalidInput("overlay replay watermark"));
        }
        for sequence in self.watermark + 1..=through {
            if self
                .slots
                .get(&sequence)
                .is_none_or(|slot| slot.result.is_none())
            {
                return Err(StoreError::InvalidInput(
                    "unresolved overlay acknowledgment",
                ));
            }
        }
        for sequence in self.watermark + 1..=through {
            self.reserved_bytes -= self.slots.remove(&sequence).unwrap().reserved;
        }
        self.watermark = through;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlay() -> Overlay {
        Overlay::temporary(
            &std::env::temp_dir(),
            ObjectId::for_bytes(b"base"),
            Limits {
                max_pages: 4096,
                max_roots: 128,
                max_key_bytes: 384,
                max_value_bytes: 1024,
            },
        )
        .unwrap()
    }

    #[test]
    fn stale_disjoint_writer_reprepares_without_losing_updates_or_retained_roots() {
        let overlay = overlay();
        let before = overlay.acquire().unwrap();
        let a = overlay
            .prepare(before.clone(), |m| m.put_inode(NodeId(2), b"A"))
            .unwrap();
        let b = overlay
            .prepare(before.clone(), |m| m.put_inode(NodeId(3), b"B"))
            .unwrap();
        assert!(overlay.install(a).unwrap());
        let captured = overlay.acquire().unwrap();
        assert!(!overlay.install(b).unwrap());
        let b = overlay
            .prepare(overlay.acquire().unwrap(), |m| m.put_inode(NodeId(3), b"B"))
            .unwrap();
        assert!(overlay.install(b).unwrap());
        let current = overlay.acquire().unwrap();
        assert_eq!(
            overlay.inode(&current, NodeId(2)).unwrap().as_deref(),
            Some(&b"A"[..])
        );
        assert_eq!(
            overlay.inode(&current, NodeId(3)).unwrap().as_deref(),
            Some(&b"B"[..])
        );
        assert_eq!(overlay.inode(&captured, NodeId(3)).unwrap(), None);
        assert_eq!(overlay.inode(&before, NodeId(2)).unwrap(), None);
        assert_eq!(
            overlay.changes(&current, captured.sequence, None).unwrap()[0].2,
            ChangeKey::Inode(NodeId(3))
        );
    }

    #[test]
    fn tracking_replaces_old_sequence_and_cleanup_keeps_masks_and_newer_changes() {
        let overlay = overlay();
        let first = overlay
            .prepare(overlay.acquire().unwrap(), |m| {
                m.put_inode(NodeId(2), b"A")?;
                m.put_binding(NodeId(1), b"removed", None)
            })
            .unwrap();
        assert!(overlay.install(first).unwrap());
        let snapshot = overlay.acquire().unwrap();
        let second = overlay
            .prepare(snapshot.clone(), |m| m.put_inode(NodeId(2), b"B"))
            .unwrap();
        assert!(overlay.install(second).unwrap());
        let cleanup = overlay
            .prepare(overlay.acquire().unwrap(), |m| {
                for (_, sequence, key) in overlay.changes(&snapshot, 0, None)? {
                    m.forget_covered(&key, sequence)?;
                }
                Ok(())
            })
            .unwrap();
        assert!(overlay.install(cleanup).unwrap());
        let current = overlay.acquire().unwrap();
        assert_eq!(
            overlay.binding(&current, NodeId(1), b"removed").unwrap(),
            Some(None)
        );
        let changes = overlay.changes(&current, 0, None).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(
            (changes[0].1, &changes[0].2),
            (2, &ChangeKey::Inode(NodeId(2)))
        );
        assert_eq!(overlay.changes(&snapshot, 0, None).unwrap().len(), 2);
    }

    #[test]
    fn failed_preparation_never_installs_partial_state() {
        let overlay = overlay();
        let before = overlay.acquire().unwrap();
        assert!(overlay
            .prepare(before.clone(), |m| {
                m.put_inode(NodeId(2), b"must not escape")?;
                m.put_binding(NodeId(1), b"..", None)
            })
            .is_err());
        assert!(Arc::ptr_eq(&before, &overlay.acquire().unwrap()));
        assert_eq!(overlay.inode(&before, NodeId(2)).unwrap(), None);
    }

    #[test]
    fn owned_snapshot_outlives_live_owner_and_acquires_without_index_io() {
        let overlay = overlay();
        let initial = overlay
            .prepare(overlay.acquire().unwrap(), |m| {
                m.put_inode(NodeId(2), b"captured")?;
                m.put_binding(NodeId(1), b"name", Some(NodeId(2)))
            })
            .unwrap();
        assert!(overlay.install(initial).unwrap());
        let before = overlay.index.stats().unwrap();
        let snapshot = overlay.snapshot().unwrap();
        let after = overlay.index.stats().unwrap();
        assert_eq!(
            (before.page_reads, before.page_writes, before.root_leases),
            (after.page_reads, after.page_writes, after.root_leases)
        );
        let later = overlay
            .prepare(overlay.acquire().unwrap(), |m| {
                m.put_inode(NodeId(2), b"newer")?;
                m.put_binding(NodeId(1), b"name", None)
            })
            .unwrap();
        assert!(overlay.install(later).unwrap());
        drop(overlay);
        assert_eq!(
            snapshot.inode(NodeId(2)).unwrap().as_deref(),
            Some(&b"captured"[..])
        );
        assert_eq!(
            snapshot.binding(NodeId(1), b"name").unwrap(),
            Some(Some(NodeId(2)))
        );
        assert_eq!(snapshot.changes(0, None).unwrap().len(), 2);
    }

    #[test]
    fn bounded_fifo_preparation_keeps_snapshot_acquisition_independent() {
        let overlay = overlay();
        std::thread::scope(|scope| {
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let owner = &overlay;
            let writer = scope.spawn(move || {
                owner.mutate(|m| {
                    m.put_inode(NodeId(2), b"prepared")?;
                    ready_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                })
            });
            ready_rx.recv().unwrap();
            // A pending ordinary preparation owns its source graph, not the
            // installation lock. Capture sees the old complete state promptly.
            let snapshot = overlay.snapshot().unwrap();
            assert_eq!(snapshot.root.sequence, 0);
            assert_eq!(snapshot.inode(NodeId(2)).unwrap(), None);
            release_tx.send(()).unwrap();
            writer.join().unwrap().unwrap();
            let writers = (3..19)
                .map(|id| {
                    scope.spawn(move || {
                        owner.mutate(|m| {
                            let allocated = m.allocate_inode()?;
                            m.put_inode(allocated, &(id as u64).to_be_bytes())
                        })
                    })
                })
                .collect::<Vec<_>>();
            for writer in writers {
                writer.join().unwrap().unwrap();
            }
            assert_eq!(snapshot.inode(NodeId(2)).unwrap(), None);
        });
        let current = overlay.snapshot().unwrap();
        for id in 2..19 {
            assert!(current.inode(NodeId(id)).unwrap().is_some());
        }
        assert_eq!(current.root.sequence, 17);
        assert_eq!(overlay.root_conflicts.load(Ordering::Relaxed), 0);
        assert_eq!(overlay.preparation_attempts.load(Ordering::Relaxed), 17);
    }

    #[test]
    fn fixed_inode_metadata_and_linked_ranges_remain_owned_after_live_drop() {
        let overlay = overlay();
        let empty = overlay.index.empty_root().unwrap();
        let ranges = overlay
            .index
            .set(&empty, b"range", b"captured bytes")
            .unwrap();
        let record = InodeRecord {
            attr: Attr {
                node: NodeId(42),
                size: 14,
                kind: Kind::File,
                mode: 0o640,
                links: 2,
                mtime_seconds: -1,
                mtime_nanoseconds: 123,
            },
            revision: 7,
            pins: 1,
            canonical: Some(InodeId([3; 32])),
            base: Some(ObjectId::for_bytes(b"base content")),
            parent: None,
        };
        overlay
            .mutate(|m| {
                m.put_inode_record(record, Some(&ranges))?;
                m.put_binding(NodeId(1), b"file", Some(NodeId(42)))
            })
            .unwrap();
        let snapshot = overlay.snapshot().unwrap();
        let (decoded, retained) = snapshot.inode_record(NodeId(42)).unwrap().unwrap();
        assert_eq!(decoded, record);
        assert_eq!(snapshot.root.next_inode, 43);
        let index = overlay.index.clone();
        drop(ranges);
        drop(empty);
        drop(snapshot);
        drop(overlay);
        assert_eq!(
            index.get(&retained.unwrap(), b"range").unwrap().as_deref(),
            Some(&b"captured bytes"[..])
        );
        let mut malformed = record.encode().unwrap();
        malformed[4] = 1;
        assert!(InodeRecord::decode(NodeId(42), &malformed).is_err());
        assert!(InodeRecord::decode(NodeId(0), &record.encode().unwrap()).is_err());
        let mut malformed = record;
        malformed.attr.mtime_nanoseconds = 1_000_000_000;
        assert!(malformed.encode().is_err());
    }

    #[test]
    fn cached_base_identity_and_alias_indexes_do_not_create_dirty_history() {
        let overlay = overlay();
        let record = InodeRecord {
            attr: Attr {
                node: NodeId(2),
                size: 0,
                kind: Kind::File,
                mode: 0o640,
                links: 2,
                mtime_seconds: 0,
                mtime_nanoseconds: 0,
            },
            revision: 0,
            pins: 0,
            canonical: Some(InodeId([7; 32])),
            base: None,
            parent: None,
        };
        overlay
            .mutate(|m| {
                m.cache_inode_record(record, None)?;
                m.cache_binding(NodeId(1), b"a", Some(NodeId(2)))?;
                m.cache_binding(NodeId(1), b"b", Some(NodeId(2)))
            })
            .unwrap();
        let before = overlay.snapshot().unwrap();
        assert_eq!(before.root.sequence, 0);
        assert!(before.changes(0, None).unwrap().is_empty());
        assert!(before.changes(before.root.sequence + 1, None).is_err());
        assert_eq!(
            before.canonical_inode(InodeId([7; 32])).unwrap(),
            Some(NodeId(2))
        );
        assert_eq!(before.aliases(NodeId(2), None).unwrap().len(), 2);
        overlay
            .mutate(|m| {
                m.put_binding(NodeId(1), b"a", None)?;
                m.put_binding(NodeId(1), b"c", Some(NodeId(2)))
            })
            .unwrap();
        let after = overlay.snapshot().unwrap();
        assert_eq!(
            after
                .aliases(NodeId(2), None)
                .unwrap()
                .into_iter()
                .map(|(_, _, name)| name)
                .collect::<Vec<_>>(),
            vec![b"b".to_vec(), b"c".to_vec()]
        );
        assert_eq!(
            after.bindings(NodeId(1), Some(b"a")).unwrap(),
            vec![
                (b"b".to_vec(), Some(NodeId(2))),
                (b"c".to_vec(), Some(NodeId(2)))
            ]
        );
        assert_eq!(before.bindings(NodeId(1), None).unwrap().len(), 2);
        let mut wrong = record;
        wrong.attr.node = NodeId(99);
        assert!(overlay
            .mutate(|m| m.cache_inode_record(wrong, None))
            .is_err());
        assert_eq!(
            overlay
                .snapshot()
                .unwrap()
                .canonical_inode(InodeId([7; 32]))
                .unwrap(),
            Some(NodeId(2))
        );
    }

    #[test]
    fn ownership_only_install_keeps_generation_but_still_rejects_stale_root() {
        let overlay = overlay();
        overlay.mutate(|m| m.put_inode(NodeId(2), b"A")).unwrap();
        let original = overlay.acquire().unwrap();
        let stale = overlay
            .prepare(original.clone(), |m| m.put_inode(NodeId(3), b"B"))
            .unwrap();
        overlay
            .mutate(|m| m.forget_covered(&ChangeKey::Inode(NodeId(2)), original.sequence))
            .unwrap();
        let cleaned = overlay.acquire().unwrap();
        assert_eq!(cleaned.sequence, original.sequence);
        assert_eq!(
            cleaned.installation_sequence,
            original.installation_sequence + 1
        );
        assert!(!Arc::ptr_eq(&original, &cleaned));
        assert!(!overlay.install(stale).unwrap());
        assert!(overlay
            .snapshot()
            .unwrap()
            .changes(0, None)
            .unwrap()
            .is_empty());
        overlay.mutate(|m| m.put_inode(NodeId(3), b"B")).unwrap();
        assert_eq!(
            overlay
                .snapshot()
                .unwrap()
                .changes(original.sequence, None)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn snapshot_reads_exact_owned_payload_after_live_replacement_and_owner_drop() {
        use crate::correspondence::OriginSequence;
        use crate::overlay_payload::{Limits as PayloadLimits, Payload};
        use crate::overlay_ranges::{Limits as RangeLimits, OwnedPiece, Ranges};
        use layerfs_layerstack_store::{
            EntityName, LayerStackInitialization, LayerStackStore, LocalForkSource,
        };
        let directory = std::env::temp_dir().join(format!(
            "layerfs-owned-snapshot-{}",
            crate::WorkspaceId::new()
        ));
        std::fs::create_dir(&directory).unwrap();
        {
            let store = LayerStackStore::create(directory.join("store.sqlite")).unwrap();
            let init = store
                .initialize_layerstack(
                    EntityName::new("snapshot").unwrap(),
                    LayerStackInitialization::Empty,
                )
                .unwrap();
            let branch = store
                .fork_branch(
                    EntityName::new("main").unwrap(),
                    LocalForkSource::Layer {
                        layer_id: init.genesis_layer_id,
                    },
                )
                .unwrap();
            let pinned = store.pin_branch(branch).unwrap();
            let policy = Limits {
                max_pages: 16384,
                ..Limits::default()
            };
            let payload = Payload::temporary(
                &directory,
                PayloadLimits {
                    physical_bytes: 1024 * 1024,
                    owners: 4096,
                    readers: 16,
                    writes: 4,
                    index: policy,
                },
            )
            .unwrap();
            let index =
                Index::temporary_with_owner(&directory, policy, Arc::new(payload.clone())).unwrap();
            let ranges =
                Ranges::new(index.clone(), payload.clone(), RangeLimits::default()).unwrap();
            let origins = OriginSequence::new([3; 16]);
            let bytes = payload.write_from(&mut &b"captured"[..], 8).unwrap();
            let initial = ranges
                .append(
                    &ranges.empty(),
                    OwnedPiece::payload(bytes, origins.allocate().unwrap()).unwrap(),
                )
                .unwrap();
            let overlay = Overlay::from_index(index, pinned.root).unwrap();
            let mut record = InodeRecord {
                attr: Attr {
                    node: NodeId(2),
                    size: 8,
                    kind: Kind::File,
                    mode: 0o640,
                    links: 1,
                    mtime_seconds: 0,
                    mtime_nanoseconds: 0,
                },
                revision: 0,
                pins: 0,
                canonical: None,
                base: None,
                parent: None,
            };
            overlay
                .mutate(|m| {
                    let mut root = record;
                    root.attr.node = NodeId(1);
                    root.attr.size = 0;
                    root.attr.kind = Kind::Directory;
                    root.attr.links = 2;
                    root.parent = Some(NodeId(1));
                    m.cache_inode_record(root, None)?;
                    m.put_inode_record(record, initial.root())?;
                    m.put_binding(NodeId(1), b"file", Some(NodeId(2)))
                })
                .unwrap();
            let captured = overlay.snapshot().unwrap();
            let changed = ranges
                .replace(
                    &initial,
                    0,
                    8,
                    [OwnedPiece::inline(b"live", origins.allocate().unwrap())],
                )
                .unwrap();
            record.attr.size = 4;
            record.revision = 1;
            overlay
                .mutate(|m| m.put_inode_record(record, changed.root()))
                .unwrap();
            let live = overlay.snapshot().unwrap();
            drop(initial);
            drop(changed);
            drop(overlay);
            let mut bytes = [0; 8];
            assert_eq!(
                captured
                    .read_at(&ranges, &pinned.reader, NodeId(2), 0, &mut bytes)
                    .unwrap(),
                8
            );
            assert_eq!(&bytes, b"captured");
            let mut bytes = [0; 4];
            assert_eq!(
                live.read_at(&ranges, &pinned.reader, NodeId(2), 0, &mut bytes)
                    .unwrap(),
                4
            );
            assert_eq!(&bytes, b"live");
            assert_eq!(
                captured
                    .read_at(&ranges, &pinned.reader, NodeId(2), 2, &mut bytes)
                    .unwrap(),
                4
            );
            assert_eq!(&bytes, b"ptur");
            assert_eq!(
                captured
                    .read_at(&ranges, &pinned.reader, NodeId(2), u64::MAX, &mut bytes)
                    .unwrap(),
                0
            );
            assert_eq!(&bytes, b"ptur");
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn replay_retains_lost_reply_and_never_reapplies_stale_or_unknown_request() {
        let session = [1; 16];
        let mut replay = ReplayWindow::new(session, 2, 8).unwrap();
        assert!(matches!(
            replay.admit(session, 1, [7; 32], 8).unwrap(),
            ReplayAdmission::New
        ));
        assert!(matches!(
            replay.admit(session, 1, [7; 32], 8).unwrap(),
            ReplayAdmission::Unknown
        ));
        assert!(replay.acknowledge(session, 1).is_err());
        assert!(replay.admit(session, 2, [7; 32], 1).is_err());
        replay.finish(1, Arc::from(&b"appended"[..])).unwrap();
        assert!(replay.admit(session, 1, [8; 32], 8).is_err());
        match replay.admit(session, 1, [7; 32], 8).unwrap() {
            ReplayAdmission::Completed(bytes) => assert_eq!(&*bytes, b"appended"),
            _ => panic!("lost reply must return the installed result"),
        }
        assert!(matches!(
            replay.admit([2; 16], 1, [7; 32], 8).unwrap(),
            ReplayAdmission::Unknown
        ));
        replay.acknowledge(session, 1).unwrap();
        assert!(replay.admit(session, 1, [7; 32], 8).is_err());
        assert_eq!(replay.reserved_bytes, 0);
        assert!(replay.admit(session, 4, [7; 32], 1).is_err());
        assert!(matches!(
            replay.admit(session, 3, [7; 32], 1).unwrap(),
            ReplayAdmission::New
        ));
        replay.finish(3, Arc::from(&b"x"[..])).unwrap();
        assert!(replay.acknowledge(session, 3).is_err());
        assert!(matches!(
            replay.admit(session, 2, [7; 32], 1).unwrap(),
            ReplayAdmission::New
        ));
        replay.finish(2, Arc::from(&b"y"[..])).unwrap();
        replay.acknowledge(session, 3).unwrap();
        assert!(replay.slots.is_empty());
    }
}
