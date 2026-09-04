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
    pub payload_ids_read: u64,
    pub payload_batches_read: u64,
    pub max_payload_batch: u64,
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
    pub(super) private_metrics: DeferredNodeMetrics,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct DeferredNodeMetrics {
    pub(super) peak_bytes: usize,
    pub(super) flush_objects: u64,
    pub(super) flush_bytes: u64,
}

pub(super) struct DeferredNodes<'a, S> {
    pub(super) store: &'a mut S,
    nodes: BTreeMap<ObjectId, Vec<u8>>,
    private_limit: Option<usize>,
    charged_bytes: usize,
    metrics: DeferredNodeMetrics,
}

impl<'a, S: ObjectStore> DeferredNodes<'a, S> {
    pub(super) fn new(store: &'a mut S) -> Self {
        Self {
            store,
            nodes: BTreeMap::new(),
            private_limit: None,
            charged_bytes: 0,
            metrics: DeferredNodeMetrics::default(),
        }
    }

    pub(super) fn with_nodes(store: &'a mut S, nodes: BTreeMap<ObjectId, Vec<u8>>) -> Self {
        Self { store, nodes, private_limit: None, charged_bytes: 0, metrics: DeferredNodeMetrics::default() }
    }

    /// Only for the explicitly private file preparation route. The map shares
    /// the caller's file-deferred allowance with its outer collector.
    pub(super) fn with_private_budget(store: &'a mut S, nodes: BTreeMap<ObjectId, Vec<u8>>, maximum: usize) -> CoreResult<Self> {
        let charged_bytes = nodes.values().try_fold(std::mem::size_of::<Self>(), |total, bytes| {
            total.checked_add(bytes.capacity()).and_then(|n| n.checked_add(1024)).ok_or(CoreError::LengthOverflow)
        })?;
        if charged_bytes > maximum { return Err(CoreError::ObjectLimitExceeded); }
        Ok(Self { store, nodes, private_limit: Some(maximum), charged_bytes,
            metrics: DeferredNodeMetrics { peak_bytes: charged_bytes, ..DeferredNodeMetrics::default() } })
    }

    pub(super) fn private_metrics(&self) -> DeferredNodeMetrics { self.metrics }

    fn flush_private(&mut self) -> CoreResult<u64> {
        let count = self.nodes.len() as u64;
        // Never drain a subset: outer preparation owns any successfully copied
        // prefix on failure, while local descendants remain a closed set.
        for (id, canonical) in &self.nodes {
            if self.store.put(canonical)? != *id { return Err(CoreError::IdentityMismatch); }
            self.metrics.flush_objects = self.metrics.flush_objects.checked_add(1).ok_or(CoreError::LengthOverflow)?;
            self.metrics.flush_bytes = self.metrics.flush_bytes.checked_add(canonical.len() as u64).ok_or(CoreError::LengthOverflow)?;
        }
        self.nodes.clear();
        self.charged_bytes = std::mem::size_of::<Self>();
        Ok(count)
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
        // Private preparation flushes only under resource pressure and needs
        // neither recursive boundary protection nor a sealed/visited set.
        if self.private_limit.is_some() { return Ok(0); }
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
        if self.private_limit.is_some() {
            ObjectStore::with_authenticated_canonical(self, root.id, |canonical| {
                let node = decode_node_with_context(canonical, true)?;
                if node.level() != root.level || node.logical_len() != root.bytes || node.extent_count() != root.extents {
                    return Err(CoreError::InvalidRecord("deferred extent summary"));
                }
                Ok(())
            })?;
            return self.flush_private();
        }
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
        if let Some(maximum) = self.private_limit {
            if let Some(prior) = self.nodes.get(&id) {
                return if prior == canonical { Ok(id) } else { Err(CoreError::IdentityMismatch) };
            }
            let charge = canonical.len().checked_add(1024).ok_or(CoreError::LengthOverflow)?;
            if self.charged_bytes.checked_add(charge).ok_or(CoreError::LengthOverflow)? > maximum {
                self.flush_private()?;
            }
            let next = self.charged_bytes.checked_add(charge).ok_or(CoreError::LengthOverflow)?;
            if next > maximum {
                if self.store.put(canonical)? != id { return Err(CoreError::IdentityMismatch); }
                self.metrics.flush_objects = self.metrics.flush_objects.checked_add(1).ok_or(CoreError::LengthOverflow)?;
                self.metrics.flush_bytes = self.metrics.flush_bytes.checked_add(canonical.len() as u64).ok_or(CoreError::LengthOverflow)?;
                return Ok(id);
            }
            // Reserve before either BTreeMap growth or canonical ownership.
            // Charge a full 1-KiB tree-node allowance per entry, including the
            // initially sparse node, rather than amortizing allocator capacity.
            self.charged_bytes = next;
            self.metrics.peak_bytes = self.metrics.peak_bytes.max(next);
            self.nodes.insert(id, canonical.to_vec());
            return Ok(id);
        }
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


#[cfg(test)]
mod private_preparation_tests {
    use super::*;
    struct PrivateStore { rows: BTreeMap<ObjectId, Vec<u8>>, remaining: usize }
    impl ObjectStore for PrivateStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.rows.get(&id).cloned().ok_or(CoreError::MissingObject)
        }
        fn put(&mut self, bytes: &[u8]) -> CoreResult<ObjectId> {
            if self.remaining == 0 { return Err(CoreError::ObjectLimitExceeded); }
            self.remaining -= 1;
            let id = ObjectId::for_bytes(bytes);
            self.rows.insert(id, bytes.to_vec()); Ok(id)
        }
    }
    #[test]
    fn private_inner_nodes_bound_growth_and_preserve_closed_set_on_failure() {
        let mut store = PrivateStore { rows: BTreeMap::new(), remaining: 1 };
        assert!(matches!(DeferredNodes::with_private_budget(&mut store, BTreeMap::new(), 0), Err(CoreError::ObjectLimitExceeded)));
        let mut nodes = DeferredNodes::with_private_budget(&mut store, BTreeMap::new(), 4096).unwrap();
        let first = ObjectStore::put(&mut nodes, &crate::encode_bytes_object(b"one").unwrap()).unwrap();
        let second = ObjectStore::put(&mut nodes, &crate::encode_bytes_object(b"two").unwrap()).unwrap();
        let before = nodes.charged_bytes;
        assert!(matches!(ObjectStore::put(&mut nodes, &crate::encode_bytes_object(&[7; 4096]).unwrap()), Err(CoreError::ObjectLimitExceeded)));
        assert_eq!(nodes.nodes.len(), 2);
        assert!(nodes.nodes.contains_key(&first) && nodes.nodes.contains_key(&second));
        assert_eq!(nodes.charged_bytes, before);
        assert!(nodes.metrics.peak_bytes <= 4096);
        assert_eq!(nodes.metrics.flush_objects, 1);
        assert_eq!(nodes.store.rows.len(), 1, "successful prefix remains private");
        // The complete local set permits retry against the same private owner.
        nodes.store.remaining = usize::MAX;
        nodes.flush_private().unwrap();
        assert!(nodes.nodes.is_empty());
        assert!(ObjectStore::get(&nodes, first).is_ok());
        assert!(ObjectStore::get(&nodes, second).is_ok());
        drop(nodes);
        store.rows.clear();
        assert!(store.rows.is_empty());
    }
}
