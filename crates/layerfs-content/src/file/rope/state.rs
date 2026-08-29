use super::validate::child_summaries;
use crate::error::{CoreError, CoreResult};
use crate::file::extent::{ExtentNodeV3, ExtentSliceV3, FileStateV3};
use crate::file::extent_codec::decode_node_with_context;
use crate::object::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

pub use crate::object::access::{ObjectRead, ObjectStore};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RopeCounters {
    pub payload_bytes_read: u64,
    pub payload_bytes_written: u64,
    pub cdc_bytes_scanned: u64,
    pub chunks_created: u64,
    pub nodes_read: u64,
    pub nodes_created: u64,
    pub deferred_peak_bytes: u64,
    pub deferred_prunes: u64,
    pub tree_level_before: Option<u8>,
    pub logical_len_before: Option<u64>,
    pub logical_len_after: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileStateRoot(pub ObjectId);

#[derive(Clone, Debug)]
pub struct ReadPlan {
    pub(super) state: FileStateV3,
    pub(super) mapping: ExtentNodeV3,
}

impl ReadPlan {
    pub fn logical_len(&self) -> u64 {
        self.state.logical_len
    }
}

#[derive(Clone, Copy)]
pub(super) struct Summary {
    pub(super) id: ObjectId,
    pub(super) bytes: u64,
    pub(super) extents: u64,
    pub(super) level: u8,
}

pub(super) enum Pending {
    Extents(Vec<ExtentSliceV3>),
    Children(Vec<Summary>),
}

pub(super) struct ReplacementScan {
    pub(super) levels: Vec<Pending>,
    pub(super) counters: RopeCounters,
    pub(super) bytes_scanned: u64,
    pub(super) pending: BTreeMap<ObjectId, Vec<u8>>,
    pub(super) persisted_nodes: u64,
}

pub(super) struct DeferredNodes<'a, S> {
    pub(super) store: &'a mut S,
    nodes: BTreeMap<ObjectId, Vec<u8>>,
}

impl<'a, S: ObjectStore> DeferredNodes<'a, S> {
    pub(super) fn new(store: &'a mut S) -> Self {
        Self {
            store,
            nodes: BTreeMap::new(),
        }
    }

    pub(super) fn with_nodes(store: &'a mut S, nodes: BTreeMap<ObjectId, Vec<u8>>) -> Self {
        Self { store, nodes }
    }

    pub(super) fn into_nodes(self) -> BTreeMap<ObjectId, Vec<u8>> {
        self.nodes
    }

    pub(super) fn flush_sealed_with<F>(
        &mut self,
        levels: &[Pending],
        put: &mut F,
    ) -> CoreResult<u64>
    where
        F: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
    {
        let mut protected = BTreeSet::new();
        for pending in levels {
            if let Pending::Children(children) = pending {
                if let Some(first) = children.first() {
                    self.protect_boundary(*first, &mut protected)?;
                }
                if let Some(last) = children.last() {
                    self.protect_boundary(*last, &mut protected)?;
                }
            }
        }
        let sealed = self
            .nodes
            .keys()
            .filter(|id| !protected.contains(id))
            .copied()
            .collect::<Vec<_>>();
        let mut flushed = BTreeSet::new();
        for id in sealed {
            self.flush_node_with(id, &mut flushed, put)?;
        }
        u64::try_from(flushed.len()).map_err(|_| CoreError::LengthOverflow)
    }

    fn protect_boundary(
        &self,
        expected: Summary,
        protected: &mut BTreeSet<ObjectId>,
    ) -> CoreResult<()> {
        if !protected.insert(expected.id) {
            return Ok(());
        }
        let Some(canonical) = self.nodes.get(&expected.id) else {
            return Ok(());
        };
        if let ExtentNodeV3::Branch {
            level, children, ..
        } = decode_node_with_context(canonical, true)?
        {
            let summaries = child_summaries(&children, level - 1);
            if let Some(first) = summaries.first() {
                self.protect_boundary(*first, protected)?;
            }
            if let Some(last) = summaries.last() {
                self.protect_boundary(*last, protected)?;
            }
        }
        Ok(())
    }

    fn flush_node_with<F>(
        &mut self,
        id: ObjectId,
        flushed: &mut BTreeSet<ObjectId>,
        put: &mut F,
    ) -> CoreResult<()>
    where
        F: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
    {
        if !flushed.insert(id) {
            return Ok(());
        }
        let Some(canonical) = self.nodes.get(&id).cloned() else {
            flushed.remove(&id);
            return Ok(());
        };
        if let ExtentNodeV3::Branch {
            level, children, ..
        } = decode_node_with_context(&canonical, true)?
        {
            for child in child_summaries(&children, level - 1) {
                self.flush_node_with(child.id, flushed, put)?;
            }
        }
        if put(self.store, &canonical)? != id {
            return Err(CoreError::IdentityMismatch);
        }
        self.nodes.remove(&id);
        Ok(())
    }

    pub(super) fn commit(&mut self, root: Summary) -> CoreResult<u64> {
        let mut visited = BTreeSet::new();
        self.commit_node(root, true, &mut visited)?;
        u64::try_from(visited.len()).map_err(|_| CoreError::LengthOverflow)
    }

    fn commit_node(
        &mut self,
        expected: Summary,
        root: bool,
        visited: &mut BTreeSet<ObjectId>,
    ) -> CoreResult<()> {
        if !visited.insert(expected.id) {
            return Ok(());
        }
        let Some(canonical) = self.nodes.get(&expected.id).cloned() else {
            visited.remove(&expected.id);
            return Ok(());
        };
        let node = decode_node_with_context(&canonical, root)?;
        if node.level() != expected.level
            || node.logical_len() != expected.bytes
            || node.extent_count() != expected.extents
        {
            return Err(CoreError::InvalidRecord("deferred extent summary"));
        }
        if let ExtentNodeV3::Branch {
            level, children, ..
        } = node
        {
            for child in child_summaries(&children, level - 1) {
                self.commit_node(child, false, visited)?;
            }
        }
        if self.store.put(&canonical)? != expected.id {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(())
    }
}

impl<S: ObjectStore> ObjectStore for DeferredNodes<'_, S> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.nodes
            .get(&id)
            .cloned()
            .map_or_else(|| self.store.get(id), Ok)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        if self
            .nodes
            .insert(id, canonical.to_vec())
            .is_some_and(|prior| prior != canonical)
        {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(id)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        match self.nodes.get(&id) {
            Some(bytes) if ObjectId::for_bytes(bytes) == id => callback(bytes),
            Some(_) => Err(CoreError::IdentityMismatch),
            None => self.store.with_authenticated_canonical(id, callback),
        }
    }
}
