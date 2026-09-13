//! Private host overlay roots. Installation never performs backing I/O.
//!
//! These roots describe host-installed state; kernel mapping visibility is a
//! separate obligation of the FUSE adapter, not a property of an Arc clone.
use crate::overlay_index::{Index, Limits, Root};
use layerfs_content::{CanonicalName, ObjectId};
use layerfs_layerstack_store::{Result, StoreError};
use layerfs_workspace_core::NodeId;
use std::collections::BTreeMap;
use std::ops::Bound;
use std::path::Path;
use std::sync::{Arc, Mutex};

const INODE: u8 = 1;
const BINDING: u8 = 2;
const CHANGE_KEY: u8 = 3;
const CHANGE_SEQUENCE: u8 = 4;
const CHANGE_BATCH: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ChangeKey {
    Inode(NodeId),
    Binding(NodeId, Vec<u8>),
}

impl ChangeKey {
    fn encode(&self) -> Result<Vec<u8>> {
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

    fn decode(bytes: &[u8]) -> Result<Self> {
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

pub(crate) struct OverlayRoot {
    pub(crate) sequence: u64,
    pub(crate) base_root: ObjectId,
    pub(crate) next_inode: u64,
    index: Root,
}

pub(crate) struct Overlay {
    index: Index,
    current: Mutex<Arc<OverlayRoot>>,
}

pub(crate) struct PreparedMutation {
    source: Arc<OverlayRoot>,
    candidate: Arc<OverlayRoot>,
}

/// A private candidate can only escape after its entire operation succeeds.
pub(crate) struct Mutation<'a> {
    index: &'a Index,
    candidate: OverlayRoot,
}

impl Overlay {
    pub(crate) fn temporary(directory: &Path, base_root: ObjectId, limits: Limits) -> Result<Self> {
        let index = Index::temporary(directory, limits)?;
        let root = index.empty_root()?;
        Ok(Self {
            index,
            current: Mutex::new(Arc::new(OverlayRoot {
                sequence: 0,
                base_root,
                next_inode: 2,
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
        let mut mutation = Mutation {
            index: &self.index,
            candidate: OverlayRoot {
                sequence: source
                    .sequence
                    .checked_add(1)
                    .ok_or(StoreError::InvalidInput("overlay sequence overflow"))?,
                base_root: source.base_root,
                next_inode: source.next_inode,
                index: source.index.clone(),
            },
        };
        apply(&mut mutation)?;
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
                return Ok(false);
            }
            std::mem::replace(&mut *current, prepared.candidate)
        };
        drop(previous);
        Ok(true)
    }

    pub(crate) fn inode(&self, root: &OverlayRoot, inode: NodeId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .index
            .get(&root.index, &ChangeKey::Inode(inode).encode()?)?)
    }

    /// None means consult immutable backing; Some(None) is a retained mask.
    pub(crate) fn binding(
        &self,
        root: &OverlayRoot,
        parent: NodeId,
        name: &[u8],
    ) -> Result<Option<Option<NodeId>>> {
        self.index
            .get(
                &root.index,
                &ChangeKey::Binding(parent, name.to_vec()).encode()?,
            )?
            .map(|bytes| {
                let value = decode_sequence(&bytes)?;
                Ok((value != 0).then_some(NodeId(value)))
            })
            .transpose()
    }

    /// Each batch seeks past the caller's last key, so covered prefixes and
    /// previously returned pages are not scanned again.
    pub(crate) fn changes(
        &self,
        root: &OverlayRoot,
        covered: u64,
        after: Option<&[u8]>,
    ) -> Result<Vec<(Vec<u8>, u64, ChangeKey)>> {
        if covered >= root.sequence {
            return Ok(Vec::new());
        }
        let mut first = vec![CHANGE_SEQUENCE];
        first.extend_from_slice(&(covered + 1).to_be_bytes());
        let start = match after {
            Some(after) if after >= first.as_slice() && after.first() == Some(&CHANGE_SEQUENCE) => {
                Bound::Excluded(after)
            }
            Some(_) => return Err(StoreError::InvalidInput("overlay change cursor")),
            None => Bound::Included(first.as_slice()),
        };
        self.index
            .scan(
                &root.index,
                start,
                Bound::Excluded(&[CHANGE_SEQUENCE + 1]),
                CHANGE_BATCH,
            )?
            .into_iter()
            .map(|(key, value)| {
                if key.len() < 18 || !value.is_empty() {
                    return Err(StoreError::Integrity("overlay sequence index"));
                }
                let sequence = decode_sequence(&key[1..9])?;
                if sequence > root.sequence {
                    return Err(StoreError::Integrity("overlay future change"));
                }
                let change = ChangeKey::decode(&key[9..])?;
                Ok((key, sequence, change))
            })
            .collect()
    }
}

impl Mutation<'_> {
    pub(crate) fn allocate_inode(&mut self) -> Result<NodeId> {
        let id = self.candidate.next_inode;
        self.candidate.next_inode = id
            .checked_add(1)
            .ok_or(StoreError::InvalidInput("overlay inode overflow"))?;
        Ok(NodeId(id))
    }

    pub(crate) fn put_inode(&mut self, inode: NodeId, record: &[u8]) -> Result<()> {
        let key = ChangeKey::Inode(inode).encode()?;
        self.candidate.index = self.index.set(&self.candidate.index, &key, record)?;
        self.changed(&key)
    }

    pub(crate) fn put_binding(
        &mut self,
        parent: NodeId,
        name: &[u8],
        inode: Option<NodeId>,
    ) -> Result<()> {
        if inode.is_some_and(|id| id.0 == 0) {
            return Err(StoreError::InvalidInput("overlay inode identity"));
        }
        let key = ChangeKey::Binding(parent, name.to_vec()).encode()?;
        self.candidate.index = self.index.set(
            &self.candidate.index,
            &key,
            &inode.map_or(0, |id| id.0).to_be_bytes(),
        )?;
        self.changed(&key)
    }

    fn changed(&mut self, key: &[u8]) -> Result<()> {
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

fn sequence_key(sequence: u64, key: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + key.len());
    out.push(CHANGE_SEQUENCE);
    out.extend_from_slice(&sequence.to_be_bytes());
    out.extend_from_slice(key);
    out
}

fn decode_sequence(bytes: &[u8]) -> Result<u64> {
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
        response_bytes: usize,
    ) -> Result<ReplayAdmission> {
        if session != self.session {
            return Ok(ReplayAdmission::Unknown);
        }
        if sequence <= self.watermark {
            return Err(StoreError::InvalidInput("stale overlay request"));
        }
        if let Some(slot) = self.slots.get(&sequence) {
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
    fn replay_retains_lost_reply_and_never_reapplies_stale_or_unknown_request() {
        let session = [1; 16];
        let mut replay = ReplayWindow::new(session, 2, 8).unwrap();
        assert!(matches!(
            replay.admit(session, 1, 8).unwrap(),
            ReplayAdmission::New
        ));
        assert!(matches!(
            replay.admit(session, 1, 8).unwrap(),
            ReplayAdmission::Unknown
        ));
        assert!(replay.acknowledge(session, 1).is_err());
        assert!(replay.admit(session, 2, 1).is_err());
        replay.finish(1, Arc::from(&b"appended"[..])).unwrap();
        match replay.admit(session, 1, 8).unwrap() {
            ReplayAdmission::Completed(bytes) => assert_eq!(&*bytes, b"appended"),
            _ => panic!("lost reply must return the installed result"),
        }
        assert!(matches!(
            replay.admit([2; 16], 1, 8).unwrap(),
            ReplayAdmission::Unknown
        ));
        replay.acknowledge(session, 1).unwrap();
        assert!(replay.admit(session, 1, 8).is_err());
        assert_eq!(replay.reserved_bytes, 0);
        assert!(replay.admit(session, 4, 1).is_err());
        assert!(matches!(
            replay.admit(session, 3, 1).unwrap(),
            ReplayAdmission::New
        ));
        replay.finish(3, Arc::from(&b"x"[..])).unwrap();
        assert!(replay.acknowledge(session, 3).is_err());
        assert!(matches!(
            replay.admit(session, 2, 1).unwrap(),
            ReplayAdmission::New
        ));
        replay.finish(2, Arc::from(&b"y"[..])).unwrap();
        replay.acknowledge(session, 3).unwrap();
        assert!(replay.slots.is_empty());
    }
}
