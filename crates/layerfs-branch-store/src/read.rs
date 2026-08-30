use crate::BranchStore;
use layerfs_content::ObjectId;
use layerfs_storage::{
    collect_dependency_set, BranchId, BranchRecord, BranchScope, BranchScopeRecord,
    CanonicalObject, LayerId, LayerRecord, LayerStackEndpoint, LayerStackScopeRecord, ObjectSource,
    RemotePlacement, Result, SpillableObjectSet, StorageError, StoreDb,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SnapshotReader {
    db: StoreDb,
    parent: Option<Arc<dyn LayerStackEndpoint>>,
    root: ObjectId,
    complete: bool,
    protected: Option<Arc<Mutex<SpillableObjectSet>>>,
}

pub struct PinnedSnapshot {
    pub branch: BranchRecord,
    pub scope: BranchScopeRecord,
    pub root: ObjectId,
    pub reader: SnapshotReader,
}

impl BranchStore {
    pub(crate) fn scope_requires_local(&self, scope: BranchScope, root: ObjectId) -> Result<bool> {
        match scope {
            BranchScope::Local => self.db.complete_root(root),
            BranchScope::Remote {
                serving_mode: RemotePlacement::Reference,
                ..
            } => self.db.complete_root(root),
            BranchScope::Remote {
                serving_mode: RemotePlacement::Replica,
                ..
            } => Ok(true),
        }
    }

    pub(crate) fn layer_requires_local(&self, layer_id: LayerId, root: ObjectId) -> Result<bool> {
        let (layer, scope) = self.visible_layer_scope(layer_id)?;
        if layer.root_id != root {
            return Err(StorageError::Integrity("Layer root"));
        }
        Ok(scope.serving_mode == RemotePlacement::Replica || self.db.complete_root(root)?)
    }

    pub(crate) fn visible_layer_scope(
        &self,
        layer_id: LayerId,
    ) -> Result<(LayerRecord, LayerStackScopeRecord)> {
        let layer = self
            .db
            .layer(layer_id)?
            .ok_or(StorageError::NotFound("pulled Layer"))?;
        let scope = self
            .db
            .layer_stack_scope(layer.layer_stack_id)?
            .ok_or(StorageError::NotFound("pulled LayerStack scope"))?;
        if !self.local_layer_ancestor(layer.layer_stack_id, layer.id, scope.through_layer_id)? {
            return Err(StorageError::NotFound(
                "Layer in acquired LayerStack prefix",
            ));
        }
        Ok((layer, scope))
    }

    pub fn snapshot_reader(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        root: ObjectId,
    ) -> Result<SnapshotReader> {
        let complete = self.db.complete_root(root)?;
        let parent = if complete {
            None
        } else {
            if parent.store_id()? != self.parent_store_id() {
                return Err(StorageError::WrongParent);
            }
            Some(parent)
        };
        Ok(SnapshotReader {
            db: self.db.clone(),
            parent,
            root,
            complete,
            protected: None,
        })
    }

    fn scoped_reader(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        root: ObjectId,
        scope: BranchScope,
    ) -> Result<SnapshotReader> {
        let require_local = self.scope_requires_local(scope, root)?;
        self.reader_with_policy(parent, root, require_local)
    }

    pub(crate) fn reader_with_policy(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        root: ObjectId,
        require_local: bool,
    ) -> Result<SnapshotReader> {
        let parent = if require_local {
            None
        } else {
            if parent.store_id()? != self.parent_store_id() {
                return Err(StorageError::WrongParent);
            }
            Some(parent)
        };
        Ok(SnapshotReader {
            db: self.db.clone(),
            parent,
            root,
            complete: require_local,
            protected: None,
        })
    }

    pub fn pin_branch(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        branch_id: BranchId,
    ) -> Result<PinnedSnapshot> {
        let pinned = self
            .db
            .pin_branch(branch_id)?
            .ok_or(StorageError::NotFound("Branch"))?;
        let reader = self.scoped_reader(parent, pinned.root_id, pinned.scope.scope)?;
        Ok(PinnedSnapshot {
            branch: pinned.branch,
            scope: pinned.scope,
            root: pinned.root_id,
            reader,
        })
    }

    pub(crate) fn pair_reader_with_policy(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        left: ObjectId,
        left_local: bool,
        right: ObjectId,
        right_local: bool,
    ) -> Result<SnapshotReader> {
        self.roots_reader_with_policy(parent, &[(left, left_local), (right, right_local)])
    }

    pub(crate) fn roots_reader_with_policy(
        &self,
        parent: Arc<dyn LayerStackEndpoint>,
        roots: &[(ObjectId, bool)],
    ) -> Result<SnapshotReader> {
        let root = roots
            .first()
            .ok_or(StorageError::InvalidInput("snapshot roots"))?
            .0;
        let complete = roots.iter().all(|(_, local)| *local);
        if complete {
            return self.reader_with_policy(parent, root, true);
        }
        if parent.store_id()? != self.parent_store_id() {
            return Err(StorageError::WrongParent);
        }
        let local = roots
            .iter()
            .filter_map(|(root, local)| local.then_some(*root))
            .collect::<Vec<_>>();
        let protected = if local.is_empty() {
            None
        } else {
            Some(Arc::new(Mutex::new(protected_ids(self, local)?)))
        };
        Ok(SnapshotReader {
            db: self.db.clone(),
            parent: Some(parent),
            root,
            complete: false,
            protected,
        })
    }
}

impl SnapshotReader {
    pub fn root(&self) -> ObjectId {
        self.root
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    fn requires_local(&self, id: ObjectId) -> Result<bool> {
        if self.complete {
            return Ok(true);
        }
        self.protected.as_ref().map_or(Ok(false), |protected| {
            protected
                .lock()
                .map_err(|_| StorageError::Integrity("protected root membership"))?
                .contains(id)
        })
    }

    fn parent(&self) -> Result<&dyn LayerStackEndpoint> {
        self.parent
            .as_deref()
            .ok_or(StorageError::Integrity("SnapshotReader parent route"))
    }
}

impl ObjectSource for SnapshotReader {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        match self.db.read_object_row(id) {
            Ok(bytes) => Ok(bytes),
            Err(StorageError::MissingObject(_)) if !self.requires_local(id)? => {
                let bytes = self
                    .parent()?
                    .read_object(id)
                    .map_err(|error| match error {
                        StorageError::MissingObject(_) | StorageError::NotFound(_) => {
                            StorageError::Integrity("visible authority closure")
                        }
                        error => error,
                    })?;
                layerfs_content::authenticate_identity(&bytes, id)?;
                Ok(bytes)
            }
            Err(StorageError::MissingObject(_)) => {
                Err(StorageError::Integrity("complete local closure"))
            }
            Err(error) => Err(error),
        }
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        let mut rows = self.db.existing_object_rows(ids)?;
        let missing = ids
            .iter()
            .filter(|id| !rows.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        for id in &missing {
            if self.requires_local(*id)? {
                return Err(StorageError::Integrity("complete local closure"));
            }
        }
        if !missing.is_empty() {
            let remote = self
                .parent()?
                .read_objects(&missing)
                .map_err(|error| match error {
                    StorageError::MissingObject(_) | StorageError::NotFound(_) => {
                        StorageError::Integrity("visible authority closure")
                    }
                    error => error,
                })?;
            for object in remote {
                layerfs_content::authenticate_identity(&object.bytes, object.id)?;
                rows.insert(object.id, object.bytes);
            }
        }
        ordered(ids, rows)
    }

    fn prune_existing_subtree(&self, id: ObjectId) -> Result<bool> {
        Ok(!self.requires_local(id)? && !self.db.has_object(id)?)
    }
}

impl ObjectSource for BranchStore {
    fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
        self.db.read_object_row(id)
    }

    fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
        self.db.read_object_rows(ids)
    }
}

fn ordered(ids: &[ObjectId], rows: BTreeMap<ObjectId, Vec<u8>>) -> Result<Vec<CanonicalObject>> {
    ids.iter()
        .map(|id| {
            let bytes = rows
                .get(id)
                .cloned()
                .ok_or(StorageError::Integrity("object batch"))?;
            Ok(CanonicalObject { id: *id, bytes })
        })
        .collect()
}

fn protected_ids(
    store: &BranchStore,
    roots: impl IntoIterator<Item = ObjectId>,
) -> Result<SpillableObjectSet> {
    let mut protected = SpillableObjectSet::empty()?;
    collect_dependency_set(store, roots, &mut protected).map_err(|error| match error {
        StorageError::MissingObject(_) => StorageError::Integrity("complete local closure"),
        error => error,
    })?;
    Ok(protected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_layerstack_store::LayerStackStore;
    use layerfs_storage::{
        AdmissionSetReceipt, BranchFact, BranchId, CommitHistoryPage, CommitId, CommitRecord,
        EntityName, Fact, LayerId, LayerPrefixPage, LayerRecord, LayerStackFact, LayerStackId,
        LayerStackInitialization, LayerStackRecord, MissingBitmap, PullLayerResult,
        RemotePlacement, StoreId,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingEndpoint {
        inner: Arc<LayerStackStore>,
        calls: Arc<AtomicUsize>,
        unavailable: bool,
    }

    impl CountingEndpoint {
        fn enter(&self) -> Result<()> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            if self.unavailable {
                Err(StorageError::Unavailable)
            } else {
                Ok(())
            }
        }
    }

    impl ObjectSource for CountingEndpoint {
        fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
            self.enter()?;
            self.inner.read_object(id)
        }

        fn read_objects(&self, ids: &[ObjectId]) -> Result<Vec<CanonicalObject>> {
            self.enter()?;
            self.inner.read_objects(ids)
        }
    }

    impl LayerStackEndpoint for CountingEndpoint {
        fn store_id(&self) -> Result<StoreId> {
            self.enter()?;
            Ok(self.inner.store_id())
        }

        fn layer_stack(&self, id: LayerStackId) -> Result<Option<LayerStackRecord>> {
            self.enter()?;
            self.inner.layer_stack(id)
        }

        fn layer_stack_fact(&self, id: LayerStackId) -> Result<Option<LayerStackFact>> {
            self.enter()?;
            self.inner.layer_stack_fact(id)
        }

        fn layer(&self, id: LayerId) -> Result<Option<LayerRecord>> {
            self.enter()?;
            self.inner.layer(id)
        }

        fn branch(&self, id: BranchId) -> Result<Option<BranchRecord>> {
            self.enter()?;
            self.inner.branch(id)
        }

        fn branch_fact(&self, id: BranchId) -> Result<Option<BranchFact>> {
            self.enter()?;
            self.inner.branch_fact(id)
        }

        fn commit(&self, id: CommitId) -> Result<Option<CommitRecord>> {
            self.enter()?;
            self.inner.commit(id)
        }

        fn layer_prefix_page(
            &self,
            through: LayerId,
            cursor: Option<LayerId>,
            limit: u16,
        ) -> Result<LayerPrefixPage> {
            self.enter()?;
            self.inner.layer_prefix_page(through, cursor, limit)
        }

        fn layer_ancestry_page(
            &self,
            through: LayerId,
            stop: Option<LayerId>,
            cursor: Option<LayerId>,
            limit: u16,
        ) -> Result<LayerPrefixPage> {
            self.enter()?;
            self.inner.layer_ancestry_page(through, stop, cursor, limit)
        }

        fn commit_history_page(
            &self,
            branch: BranchId,
            through: CommitId,
            cursor: Option<CommitId>,
            limit: u16,
        ) -> Result<CommitHistoryPage> {
            self.enter()?;
            self.inner
                .commit_history_page(branch, through, cursor, limit)
        }

        fn commit_ancestry_page(
            &self,
            through: CommitId,
            stop: Option<CommitId>,
            cursor: Option<CommitId>,
            limit: u16,
        ) -> Result<CommitHistoryPage> {
            self.enter()?;
            self.inner
                .commit_ancestry_page(through, stop, cursor, limit)
        }

        fn owned_commit_page(
            &self,
            branch: BranchId,
            through: CommitId,
            cursor: Option<CommitId>,
            limit: u16,
        ) -> Result<CommitHistoryPage> {
            self.enter()?;
            self.inner.owned_commit_page(branch, through, cursor, limit)
        }

        fn missing_objects(&self, ids: &[ObjectId]) -> Result<MissingBitmap> {
            self.enter()?;
            LayerStackEndpoint::missing_objects(self.inner.as_ref(), ids)
        }

        fn missing_facts(&self, facts: &[Fact]) -> Result<MissingBitmap> {
            self.enter()?;
            LayerStackEndpoint::missing_facts(self.inner.as_ref(), facts)
        }

        fn admit_objects(&self, objects: &[CanonicalObject]) -> Result<AdmissionSetReceipt> {
            self.enter()?;
            LayerStackEndpoint::admit_objects(self.inner.as_ref(), objects)
        }

        fn admit_facts(&self, facts: &[Fact]) -> Result<AdmissionSetReceipt> {
            self.enter()?;
            LayerStackEndpoint::admit_facts(self.inner.as_ref(), facts)
        }

        fn publish_branch(
            &self,
            branch: &BranchRecord,
            observed: Option<CommitId>,
        ) -> Result<layerfs_storage::PushResult> {
            self.enter()?;
            LayerStackEndpoint::publish_branch(self.inner.as_ref(), branch, observed)
        }

        fn add_layer(&self, branch: BranchId) -> Result<layerfs_storage::AuthorityAddResult> {
            self.enter()?;
            LayerStackEndpoint::add_layer(self.inner.as_ref(), branch)
        }
    }

    #[test]
    fn complete_reader_retains_no_parent_route() {
        let root = temp("complete-reader");
        std::fs::create_dir_all(&root).unwrap();
        let authority = Arc::new(
            LayerStackStore::create(root.join("authority.sqlite")).expect("authority Store"),
        );
        let layer_id = authority
            .initialize_layerstack(
                EntityName::new("complete").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap()
            .genesis_layer_id;
        let layer = authority.layer(layer_id).unwrap().unwrap();
        let branches =
            BranchStore::create(root.join("branch.sqlite"), authority.store_id()).unwrap();
        assert!(matches!(
            branches
                .pull_layer(authority.clone(), layer_id, RemotePlacement::Replica)
                .unwrap(),
            PullLayerResult::Created { .. }
        ));
        let calls = Arc::new(AtomicUsize::new(0));
        let unavailable = Arc::new(CountingEndpoint {
            inner: authority.clone(),
            calls: calls.clone(),
            unavailable: true,
        });

        let reader = branches
            .snapshot_reader(unavailable, layer.root_id)
            .expect("complete reader must not contact its parent");
        assert_eq!(
            reader.read_object(layer.root_id).unwrap(),
            authority.read_object(layer.root_id).unwrap()
        );
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        drop(reader);
        drop(branches);
        drop(authority);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mixed_reader_never_falls_back_for_a_receipted_root() {
        let root = temp("mixed-reader");
        std::fs::create_dir_all(&root).unwrap();
        let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
        let complete_layer = authority
            .initialize_layerstack(
                EntityName::new("complete").unwrap(),
                LayerStackInitialization::Empty,
            )
            .unwrap()
            .genesis_layer_id;
        let reference_source = root.join("reference-source");
        std::fs::create_dir_all(&reference_source).unwrap();
        std::fs::write(reference_source.join("file"), b"reference").unwrap();
        let reference_layer = authority
            .initialize_layerstack(
                EntityName::new("reference").unwrap(),
                LayerStackInitialization::Directory(reference_source),
            )
            .unwrap()
            .genesis_layer_id;
        let complete_root = authority.layer(complete_layer).unwrap().unwrap().root_id;
        let reference_root = authority.layer(reference_layer).unwrap().unwrap().root_id;
        let branch_path = root.join("branch.sqlite");
        let branches = BranchStore::create(&branch_path, authority.store_id()).unwrap();
        branches
            .pull_layer(authority.clone(), complete_layer, RemotePlacement::Replica)
            .unwrap();
        branches
            .pull_layer(
                authority.clone(),
                reference_layer,
                RemotePlacement::Reference,
            )
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let endpoint = Arc::new(CountingEndpoint {
            inner: authority.clone(),
            calls: calls.clone(),
            unavailable: false,
        });
        let reader = branches
            .pair_reader_with_policy(endpoint, complete_root, true, reference_root, false)
            .unwrap();
        let calls_before_damage = calls.load(Ordering::Relaxed);
        rusqlite::Connection::open(&branch_path)
            .unwrap()
            .execute(
                "DELETE FROM objects WHERE object_id=?1",
                [complete_root.as_bytes().as_slice()],
            )
            .unwrap();

        assert!(matches!(
            reader.read_object(complete_root),
            Err(StorageError::Integrity("complete local closure"))
        ));
        assert_eq!(calls.load(Ordering::Relaxed), calls_before_damage);

        drop(reader);
        drop(branches);
        drop(authority);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn three_root_reader_protects_every_receipted_union_member() {
        let root = temp("three-root-reader");
        std::fs::create_dir_all(&root).unwrap();
        let authority = Arc::new(LayerStackStore::create(root.join("authority.sqlite")).unwrap());
        let mut layers = Vec::new();
        for name in ["first", "second", "remote"] {
            let source = root.join(name);
            std::fs::create_dir_all(&source).unwrap();
            std::fs::write(source.join("file"), name).unwrap();
            layers.push(
                authority
                    .initialize_layerstack(
                        EntityName::new(name).unwrap(),
                        LayerStackInitialization::Directory(source),
                    )
                    .unwrap()
                    .genesis_layer_id,
            );
        }
        let roots = layers
            .iter()
            .map(|layer| authority.layer(*layer).unwrap().unwrap().root_id)
            .collect::<Vec<_>>();
        let branch_path = root.join("branch.sqlite");
        let branches = BranchStore::create(&branch_path, authority.store_id()).unwrap();
        for layer in &layers[..2] {
            branches
                .pull_layer(authority.clone(), *layer, RemotePlacement::Replica)
                .unwrap();
        }
        branches
            .pull_layer(authority.clone(), layers[2], RemotePlacement::Reference)
            .unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let endpoint = Arc::new(CountingEndpoint {
            inner: authority.clone(),
            calls: calls.clone(),
            unavailable: false,
        });
        let reader = branches
            .roots_reader_with_policy(
                endpoint,
                &[(roots[0], true), (roots[1], true), (roots[2], false)],
            )
            .unwrap();
        let calls_before_damage = calls.load(Ordering::Relaxed);
        rusqlite::Connection::open(&branch_path)
            .unwrap()
            .execute(
                "DELETE FROM objects WHERE object_id=?1",
                [roots[1].as_bytes().as_slice()],
            )
            .unwrap();

        assert!(matches!(
            reader.read_object(roots[1]),
            Err(StorageError::Integrity("complete local closure"))
        ));
        assert_eq!(calls.load(Ordering::Relaxed), calls_before_damage);

        drop(reader);
        drop(branches);
        drop(authority);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn temp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "layerfs-v2-read-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
