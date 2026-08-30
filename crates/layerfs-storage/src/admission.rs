use crate::{ObjectSource, Result};
use layerfs_content::filesystem::{self, ReconcileConflict};
use layerfs_content::ObjectId;

pub struct CandidateReconciliation {
    pub root_id: ObjectId,
    pub objects: build::DeferredObjectStore,
    pub conflicts: Vec<ReconcileConflict>,
}

pub fn reconcile_candidate(
    source: &dyn ObjectSource,
    base_root: ObjectId,
    current_root: ObjectId,
    candidate_root: ObjectId,
) -> Result<CandidateReconciliation> {
    reconcile_candidate_with(source, base_root, current_root, candidate_root, |_| None)
}

pub fn reconcile_candidate_with(
    source: &dyn ObjectSource,
    base_root: ObjectId,
    current_root: ObjectId,
    candidate_root: ObjectId,
    choice: impl FnMut(
        &layerfs_content::filesystem::ReconcileConflict,
    ) -> Option<layerfs_content::filesystem::ReconcileChoice>,
) -> Result<CandidateReconciliation> {
    let mut objects = build::ObjectBuffer::new(source)?;
    let reconciled = filesystem::reconcile_with(
        &mut objects,
        base_root,
        current_root,
        candidate_root,
        choice,
    )?;
    let built = objects.finish(reconciled.root_id, 0)?;
    Ok(CandidateReconciliation {
        root_id: reconciled.root_id,
        objects: built.objects,
        conflicts: reconciled.conflicts,
    })
}

pub fn apply_reconcile_choices(
    source: &dyn ObjectSource,
    working_root: ObjectId,
    branch_root: ObjectId,
    layer_root: ObjectId,
    conflicts: &[ReconcileConflict],
    choices: &[filesystem::ReconcileChoice],
) -> Result<BuiltRoot> {
    if conflicts.len() != choices.len() {
        return Err(crate::StorageError::InvalidInput(
            "reconciliation choice count",
        ));
    }
    let mut objects = build::ObjectBuffer::new(source)?;
    let mut root = working_root;
    for (conflict, choice) in conflicts.iter().zip(choices) {
        let selected_root = match choice {
            filesystem::ReconcileChoice::Branch => branch_root,
            filesystem::ReconcileChoice::Layer => layer_root,
            filesystem::ReconcileChoice::WorkingTree => continue,
        };
        root = filesystem::replace_conflict_from_snapshot(
            &mut objects,
            root,
            selected_root,
            conflict,
        )?;
    }
    objects.finish(root, 0)
}

mod build {
    use crate::{CanonicalObject, ObjectSource, Result, StorageError};
    use layerfs_content::file::rope::{ObjectRead, ObjectStore};
    use layerfs_content::filesystem::{self as logical, ContentChange};
    use layerfs_content::object::references::referenced_objects;
    use layerfs_content::{CoreError, CoreResult, ObjectId};
    use rusqlite::OptionalExtension;
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    use std::time::Instant;

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct BuildCounters {
        pub cdc_bytes_scanned: u64,
        pub encode_hash_invocations: u64,
    }

    pub struct BuiltRoot {
        pub root_id: ObjectId,
        pub objects: DeferredObjectStore,
        pub counters: BuildCounters,
    }

    pub struct CoreReader<'a>(pub &'a dyn ObjectSource);

    impl ObjectRead for CoreReader<'_> {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.0.read_object(id).map_err(core_read_error)
        }

        fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
        where
            F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
        {
            let objects = self.0.read_objects(ids).map_err(core_read_error)?;
            if objects.len() != ids.len() {
                return Err(CoreError::MissingObject);
            }
            for (expected, object) in ids.iter().zip(objects) {
                if object.id != *expected || ObjectId::for_bytes(&object.bytes) != object.id {
                    return Err(CoreError::IdentityMismatch);
                }
                callback(
                    object.id,
                    layerfs_content::decode_bytes_object(&object.bytes)?,
                )?;
            }
            Ok(())
        }
    }

    fn core_read_error(error: StorageError) -> CoreError {
        match error {
            StorageError::MissingObject(_) => CoreError::MissingObject,
            StorageError::Integrity(message) | StorageError::InvalidInput(message) => {
                CoreError::InvalidRecord(message)
            }
            StorageError::Unavailable | StorageError::StoreMissing => {
                CoreError::ValidationAuthorityUnavailable
            }
            StorageError::Core(error) => error,
            StorageError::WrongStoreRole | StorageError::WrongParent => CoreError::WrongLogicalRole,
            StorageError::WrongStoreSchema => CoreError::SchemaMismatch,
            StorageError::CommitHeadMoved { .. }
            | StorageError::LayerHeadMoved { .. }
            | StorageError::LayerStackNameConflict { .. }
            | StorageError::BranchNameConflict { .. }
            | StorageError::ReadOnlyBranch(_) => CoreError::PublicationConflict,
            StorageError::StoreBusy
            | StorageError::StoreAlreadyExists
            | StorageError::NotFound(_)
            | StorageError::Database(_)
            | StorageError::Io(_) => CoreError::Io,
        }
    }

    #[doc(hidden)]
    pub struct DeferredObjectStore {
        storage: DeferredObjects,
        count: u64,
        encoded_bytes: u64,
    }

    enum DeferredObjects {
        Memory {
            order: VecDeque<ObjectId>,
            rows: BTreeMap<ObjectId, Vec<u8>>,
            bytes: usize,
        },
        Spill {
            connection: rusqlite::Connection,
            pending: Vec<(ObjectId, Vec<u8>)>,
            pending_bytes: usize,
            #[allow(dead_code)]
            cursor: i64,
        },
    }

    const DEFERRED_MEMORY_BYTES: usize = 8 * 1024 * 1024;

    pub(crate) enum SeenIds {
        Memory(BTreeSet<ObjectId>),
        Spill(rusqlite::Connection),
    }

    #[doc(hidden)]
    pub struct SpillableObjectSet(SeenIds);

    impl SpillableObjectSet {
        pub fn empty() -> Result<Self> {
            Ok(Self(SeenIds::empty()?))
        }

        pub fn contains(&self, id: ObjectId) -> Result<bool> {
            match &self.0 {
                SeenIds::Memory(seen) => Ok(seen.contains(&id)),
                SeenIds::Spill(connection) => connection
                    .query_row(
                        "SELECT 1 FROM seen WHERE object_id=?1",
                        [id.as_bytes().as_slice()],
                        |_| Ok(()),
                    )
                    .optional()
                    .map(|row| row.is_some())
                    .map_err(Into::into),
            }
        }

        pub fn insert_page(&mut self, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
            self.0.insert_page(ids)
        }
    }

    pub fn collect_dependency_set(
        source: &(impl ObjectSource + ?Sized),
        roots: impl IntoIterator<Item = ObjectId>,
        output: &mut SpillableObjectSet,
    ) -> Result<()> {
        let mut active = BTreeSet::new();
        for root in roots {
            collect_dependency(source, root, output, &mut active)?;
        }
        Ok(())
    }

    fn collect_dependency(
        source: &(impl ObjectSource + ?Sized),
        id: ObjectId,
        output: &mut SpillableObjectSet,
        active: &mut BTreeSet<ObjectId>,
    ) -> Result<()> {
        if active.contains(&id) {
            return Err(StorageError::Integrity("object cycle"));
        }
        if output.insert_page(&[id])?.is_empty() {
            return Ok(());
        }
        active.insert(id);
        let canonical = source.read_object(id)?;
        layerfs_content::authenticate_identity(&canonical, id)?;
        let children = referenced_objects(&canonical)?
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        drop(canonical);
        for page in children.chunks(crate::ID_BATCH_COUNT) {
            for child in page {
                collect_dependency(source, *child, output, active)?;
            }
        }
        active.remove(&id);
        Ok(())
    }

    impl SeenIds {
        pub(crate) fn empty() -> Result<Self> {
            Ok(Self::Memory(BTreeSet::new()))
        }

        pub(crate) fn new(root: ObjectId) -> Result<Self> {
            Ok(Self::Memory(BTreeSet::from([root])))
        }

        pub(crate) fn insert_page(&mut self, ids: &[ObjectId]) -> Result<Vec<ObjectId>> {
            if ids.len() > crate::ID_BATCH_COUNT || ids.windows(2).any(|pair| pair[0] >= pair[1]) {
                return Err(StorageError::Integrity("seen-ID page"));
            }
            if let Self::Memory(seen) = self {
                let inserted = ids
                    .iter()
                    .copied()
                    .filter(|id| seen.insert(*id))
                    .collect::<Vec<_>>();
                if seen.len() <= DEFERRED_MEMORY_BYTES / 64 {
                    return Ok(inserted);
                }
                let mut connection = scratch_seen()?;
                let mut page = Vec::with_capacity(crate::ID_BATCH_COUNT);
                for id in seen.iter().copied() {
                    page.push(id);
                    if page.len() == crate::ID_BATCH_COUNT {
                        insert_seen_rows(&mut connection, &page)?;
                        page.clear();
                    }
                }
                if !page.is_empty() {
                    insert_seen_rows(&mut connection, &page)?;
                }
                *self = Self::Spill(connection);
                return Ok(inserted);
            }
            let Self::Spill(connection) = self else {
                unreachable!()
            };
            insert_seen_rows(connection, ids)
        }
    }

    fn scratch_seen() -> Result<rusqlite::Connection> {
        let connection = rusqlite::Connection::open("")?;
        connection.pragma_update(None, "journal_mode", "OFF")?;
        connection.pragma_update(None, "synchronous", "OFF")?;
        connection.pragma_update(None, "temp_store", "FILE")?;
        connection.pragma_update(None, "cache_size", -8192_i64)?;
        connection.execute_batch(
            "CREATE TABLE seen(object_id BLOB PRIMARY KEY NOT NULL) WITHOUT ROWID",
        )?;
        Ok(connection)
    }

    fn insert_seen_rows(
        connection: &mut rusqlite::Connection,
        ids: &[ObjectId],
    ) -> Result<Vec<ObjectId>> {
        static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        let sql = SQL.get_or_init(|| {
            let values = (1..=crate::ID_BATCH_COUNT)
                .map(|index| format!("(?{index})"))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "INSERT INTO seen(object_id) SELECT column1 FROM (VALUES {values})
             WHERE column1 IS NOT NULL
             ON CONFLICT DO NOTHING RETURNING object_id"
            )
        });
        let mut parameters = ids
            .iter()
            .map(|id| rusqlite::types::Value::Blob(id.as_bytes().to_vec()))
            .collect::<Vec<_>>();
        parameters.resize(crate::ID_BATCH_COUNT, rusqlite::types::Value::Null);
        let transaction = connection.transaction()?;
        let mut statement = transaction.prepare_cached(sql)?;
        let mut inserted = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                row.get::<_, Vec<u8>>(0)
            })?
            .map(|row| ObjectId::from_bytes(&row?).map_err(StorageError::Core))
            .collect::<Result<Vec<_>>>()?;
        drop(statement);
        transaction.commit()?;
        inserted.sort();
        Ok(inserted)
    }

    pub(crate) fn transfer_root_union<S, I>(
        source: &S,
        roots: I,
        transfer: &mut crate::TransferPipeline<'_>,
    ) -> Result<()>
    where
        S: ObjectSource + ?Sized,
        I: IntoIterator<Item = (ObjectId, bool)>,
    {
        let mut walk = ObjectTransfer {
            source,
            seen: SeenIds::empty()?,
            active: BTreeSet::new(),
            active_bytes: 0,
            transfer,
        };
        for (root_id, known_complete) in roots {
            let inserted = walk.seen.insert_page(&[root_id])?;
            if known_complete {
                walk.transfer.prune_complete_root();
                continue;
            }
            if inserted.is_empty() {
                continue;
            }
            let missing = walk.transfer.announce_objects(&[root_id])?;
            let started = Instant::now();
            let bytes = source.read_object(root_id)?;
            crate::note_push_phase(crate::PushPhase::SourceReadAuth, elapsed_ns(started));
            walk.active.insert(root_id);
            walk.visit(
                CanonicalObject { id: root_id, bytes },
                missing.is_missing(0)?,
            )?;
        }
        Ok(())
    }

    pub fn transfer_root_transition(
        source: &(impl ObjectSource + ?Sized),
        target: &dyn crate::TransferTarget,
        old: ObjectId,
        new: ObjectId,
    ) -> Result<crate::TransferReceipt> {
        let mut frontier = DeferredObjectStore::new()?;
        let mut seen = SeenIds::empty()?;
        let mut active = BTreeSet::new();
        let mut pruned = 0_u64;
        collect_transition_frontier(
            source,
            Some(old),
            new,
            &mut seen,
            &mut active,
            &mut frontier,
            &mut pruned,
        )?;
        let mut pipeline = crate::TransferPipeline::new(target)?;
        for _ in 0..pruned {
            pipeline.prune_complete_root();
        }
        frontier.visit_batches(&mut |objects, _| {
            let mut ids = objects.iter().map(|object| object.id).collect::<Vec<_>>();
            ids.sort();
            let missing = pipeline.announce_objects(&ids)?;
            let missing = ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| missing.is_missing(index).ok()?.then_some(*id))
                .collect::<BTreeSet<_>>();
            for object in objects {
                if missing.contains(&object.id) {
                    pipeline.stage_object(object.clone())?;
                }
            }
            Ok(())
        })?;
        pipeline.finish()
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_transition_frontier(
        source: &(impl ObjectSource + ?Sized),
        old: Option<ObjectId>,
        new: ObjectId,
        seen: &mut SeenIds,
        active: &mut BTreeSet<ObjectId>,
        frontier: &mut DeferredObjectStore,
        pruned: &mut u64,
    ) -> Result<()> {
        if old == Some(new) {
            *pruned = pruned.saturating_add(1);
            return Ok(());
        }
        if seen.insert_page(&[new])?.is_empty() {
            return Ok(());
        }
        if !active.insert(new) {
            return Err(StorageError::Integrity("object cycle"));
        }
        let started = Instant::now();
        let canonical = source.read_object(new)?;
        crate::note_push_phase(crate::PushPhase::SourceReadAuth, elapsed_ns(started));
        crate::note_traversal_authentication();
        layerfs_content::authenticate_identity(&canonical, new)?;
        let mut new_children = referenced_objects(&canonical)?;
        let mut unique = BTreeSet::new();
        new_children.retain(|child| unique.insert(*child));
        let mut old_children = if new_children.is_empty() {
            Vec::new()
        } else {
            match old {
                Some(old) => {
                    let started = Instant::now();
                    let canonical = source.read_object(old)?;
                    crate::note_push_phase(crate::PushPhase::SourceReadAuth, elapsed_ns(started));
                    referenced_objects(&canonical)?
                }
                None => Vec::new(),
            }
        };
        unique.clear();
        old_children.retain(|child| unique.insert(*child));
        let old_set = old_children.iter().copied().collect::<BTreeSet<_>>();
        let new_set = new_children.iter().copied().collect::<BTreeSet<_>>();
        *pruned = pruned.saturating_add(
            new_children
                .iter()
                .filter(|child| old_set.contains(child))
                .count() as u64,
        );
        new_children.retain(|child| !old_set.contains(child));
        old_children.retain(|child| !new_set.contains(child));
        for (index, child) in new_children.into_iter().enumerate() {
            collect_transition_frontier(
                source,
                old_children.get(index).copied(),
                child,
                seen,
                active,
                frontier,
                pruned,
            )?;
        }
        active.remove(&new);
        frontier.put(new, &canonical)
    }

    struct ObjectTransfer<'source, 'borrow, 'destination, S: ObjectSource + ?Sized> {
        source: &'source S,
        seen: SeenIds,
        active: BTreeSet<ObjectId>,
        active_bytes: usize,
        transfer: &'borrow mut crate::TransferPipeline<'destination>,
    }

    impl<S: ObjectSource + ?Sized> ObjectTransfer<'_, '_, '_, S> {
        fn visit(&mut self, object: CanonicalObject, send: bool) -> Result<()> {
            let object_id = object.id;
            let object_bytes = object.bytes.len();
            self.active_bytes = self
                .active_bytes
                .checked_add(object_bytes)
                .ok_or(StorageError::Integrity("transfer buffer ceiling"))?;
            self.transfer.observe_external_buffer(self.active_bytes)?;
            let started = Instant::now();
            crate::note_traversal_authentication();
            layerfs_content::authenticate_identity(&object.bytes, object.id)?;
            let mut children = BTreeSet::new();
            for child in referenced_objects(&object.bytes)? {
                if self.active.contains(&child) {
                    return Err(StorageError::Integrity("object cycle"));
                }
                children.insert(child);
            }
            crate::note_push_phase(crate::PushPhase::SourceReadAuth, elapsed_ns(started));
            let children = children.into_iter().collect::<Vec<_>>();
            for page in children.chunks(crate::ID_BATCH_COUNT) {
                let ids = self.seen.insert_page(page)?;
                if ids.is_empty() {
                    continue;
                }
                let missing = self.transfer.announce_objects(&ids)?;
                let send = ids
                    .iter()
                    .enumerate()
                    .map(|(index, id)| Ok((*id, missing.is_missing(index)?)))
                    .collect::<Result<BTreeMap<_, _>>>()?;
                let mut traverse = Vec::with_capacity(ids.len());
                for id in &ids {
                    let started = Instant::now();
                    let traverse_id = send.get(id).copied().unwrap_or(false)
                        || !self.source.prune_existing_subtree(*id)?;
                    crate::note_push_phase(crate::PushPhase::Frontier, elapsed_ns(started));
                    if traverse_id {
                        traverse.push(*id);
                    }
                }
                for child_id in traverse {
                    let child_send = *send
                        .get(&child_id)
                        .ok_or(StorageError::Integrity("transfer child"))?;
                    let started = Instant::now();
                    let bytes = self.source.read_object(child_id)?;
                    crate::note_push_phase(crate::PushPhase::SourceReadAuth, elapsed_ns(started));
                    let child = CanonicalObject {
                        id: child_id,
                        bytes,
                    };
                    self.active.insert(child_id);
                    self.visit(child, child_send)?;
                }
            }
            self.active.remove(&object_id);
            self.active_bytes -= object_bytes;
            if send {
                self.transfer.observe_external_buffer(object_bytes)?;
                self.transfer.stage_object(object)?;
            }
            Ok(())
        }
    }

    pub(crate) struct RootVerifier<'a> {
        source: &'a dyn ObjectSource,
        seen: SeenIds,
        active: BTreeSet<ObjectId>,
    }

    impl<'a> RootVerifier<'a> {
        pub(crate) fn new(source: &'a dyn ObjectSource) -> Result<Self> {
            Ok(Self {
                source,
                seen: SeenIds::empty()?,
                active: BTreeSet::new(),
            })
        }

        pub(crate) fn verify(&mut self, root: ObjectId) -> Result<()> {
            if self.seen.insert_page(&[root])?.is_empty() {
                return Ok(());
            }
            self.active.insert(root);
            self.visit(root)
        }

        fn visit(&mut self, id: ObjectId) -> Result<()> {
            let canonical = self.source.read_object(id)?;
            layerfs_content::authenticate_identity(&canonical, id)?;
            let children = referenced_objects(&canonical)?
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            for page in children.chunks(crate::ID_BATCH_COUNT) {
                if page.iter().any(|child| self.active.contains(child)) {
                    return Err(StorageError::Integrity("object cycle"));
                }
                for child in self.seen.insert_page(page)? {
                    self.active.insert(child);
                    self.visit(child)?;
                }
            }
            self.active.remove(&id);
            Ok(())
        }
    }

    fn elapsed_ns(started: Instant) -> u64 {
        started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
    }

    impl DeferredObjectStore {
        pub(crate) fn new() -> Result<Self> {
            Ok(Self {
                storage: DeferredObjects::Memory {
                    order: VecDeque::new(),
                    rows: BTreeMap::new(),
                    bytes: 0,
                },
                count: 0,
                encoded_bytes: 0,
            })
        }

        pub fn len(&self) -> u64 {
            self.count
        }

        pub fn is_empty(&self) -> bool {
            self.count == 0
        }

        pub fn encoded_bytes(&self) -> u64 {
            self.encoded_bytes
        }

        pub fn ids_in_order(&self, limit: usize) -> Result<Option<Vec<ObjectId>>> {
            if self.count > limit as u64 {
                return Ok(None);
            }
            let mut ids = Vec::with_capacity(self.count as usize);
            match &self.storage {
                DeferredObjects::Memory { order, .. } => ids.extend(order.iter().copied()),
                DeferredObjects::Spill {
                    connection,
                    pending,
                    ..
                } => {
                    let mut statement =
                        connection.prepare("SELECT object_id FROM objects ORDER BY sequence")?;
                    let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
                    for id in rows {
                        ids.push(ObjectId::from_bytes(&id?)?);
                    }
                    ids.extend(pending.iter().map(|(id, _)| *id));
                }
            }
            if ids.len() != self.count as usize {
                return Err(StorageError::Integrity("deferred object ID count"));
            }
            Ok(Some(ids))
        }

        fn reachable_from(self, root: ObjectId) -> Result<Self> {
            let mut reachable = Self::new()?;
            let mut seen = SeenIds::new(root)?;
            let mut stack = vec![(root, false)];
            while let Some((id, expanded)) = stack.pop() {
                let Some(canonical) = self.get(id)? else {
                    continue;
                };
                if expanded {
                    reachable.put(id, &canonical)?;
                    continue;
                }
                stack.push((id, true));
                let mut children = referenced_objects(&canonical)?;
                children.sort();
                children.dedup();
                let mut inserted = Vec::with_capacity(children.len());
                for page in children.chunks(crate::ID_BATCH_COUNT) {
                    inserted.extend(seen.insert_page(page)?);
                }
                stack.extend(inserted.into_iter().rev().map(|child| (child, false)));
            }
            Ok(reachable)
        }

        fn get(&self, id: ObjectId) -> Result<Option<Vec<u8>>> {
            match &self.storage {
                DeferredObjects::Memory { rows, .. } => Ok(rows.get(&id).cloned()),
                DeferredObjects::Spill {
                    connection,
                    pending,
                    ..
                } => {
                    if let Some((_, bytes)) =
                        pending.iter().rev().find(|(pending, _)| *pending == id)
                    {
                        return Ok(Some(bytes.clone()));
                    }
                    Ok(connection
                        .query_row(
                            "SELECT bytes FROM objects WHERE object_id=?1",
                            [id.as_bytes().as_slice()],
                            |row| row.get(0),
                        )
                        .optional()?)
                }
            }
        }

        fn put(&mut self, id: ObjectId, canonical: &[u8]) -> Result<()> {
            let charge = canonical.len() + 64;
            if matches!(
                &self.storage,
                DeferredObjects::Memory { bytes, .. } if *bytes + charge > DEFERRED_MEMORY_BYTES
            ) {
                self.spill()?;
            }
            match &mut self.storage {
                DeferredObjects::Memory { order, rows, bytes } => {
                    order.push_back(id);
                    rows.insert(id, canonical.to_vec());
                    *bytes += charge;
                }
                DeferredObjects::Spill {
                    connection,
                    pending,
                    pending_bytes,
                    ..
                } => {
                    if !pending.is_empty()
                        && (pending.len() == crate::OBJECT_BATCH_COUNT
                            || *pending_bytes + canonical.len() > crate::OBJECT_BATCH_BYTES)
                    {
                        flush_objects(connection, pending, pending_bytes)?;
                    }
                    pending.push((id, canonical.to_vec()));
                    *pending_bytes += canonical.len();
                    if pending.len() == crate::OBJECT_BATCH_COUNT
                        || *pending_bytes >= crate::OBJECT_BATCH_BYTES
                    {
                        flush_objects(connection, pending, pending_bytes)?;
                    }
                }
            }
            self.count += 1;
            self.encoded_bytes = self
                .encoded_bytes
                .checked_add(canonical.len() as u64)
                .ok_or(StorageError::Integrity("candidate bytes"))?;
            Ok(())
        }

        fn spill(&mut self) -> Result<()> {
            let DeferredObjects::Memory { order, rows, .. } = std::mem::replace(
                &mut self.storage,
                DeferredObjects::Memory {
                    order: VecDeque::new(),
                    rows: BTreeMap::new(),
                    bytes: 0,
                },
            ) else {
                return Ok(());
            };
            let mut connection = rusqlite::Connection::open("")?;
            connection.pragma_update(None, "journal_mode", "OFF")?;
            connection.pragma_update(None, "synchronous", "OFF")?;
            connection.pragma_update(None, "temp_store", "FILE")?;
            connection.pragma_update(None, "cache_size", -8192_i64)?;
            connection.execute_batch(
                "CREATE TABLE objects(
                sequence INTEGER PRIMARY KEY,
                object_id BLOB NOT NULL UNIQUE,
                bytes BLOB NOT NULL
             ) STRICT",
            )?;
            let transaction = connection.transaction()?;
            {
                let mut insert =
                    transaction.prepare("INSERT INTO objects(object_id,bytes) VALUES(?1,?2)")?;
                for id in order {
                    insert.execute(rusqlite::params![
                        id.as_bytes().as_slice(),
                        rows.get(&id)
                            .ok_or(StorageError::Integrity("deferred object"))?
                    ])?;
                }
            }
            transaction.commit()?;
            self.storage = DeferredObjects::Spill {
                connection,
                pending: Vec::new(),
                pending_bytes: 0,
                cursor: 0,
            };
            Ok(())
        }

        #[allow(dead_code)]
        pub(crate) fn stage(&mut self, object: CanonicalObject) -> Result<()> {
            if let Some(known) = self.get(object.id)? {
                return if known == object.bytes {
                    Ok(())
                } else {
                    Err(StorageError::Integrity("deferred object collision"))
                };
            }
            self.put(object.id, &object.bytes)
        }

        #[allow(dead_code)]
        pub(crate) fn pop_first(&mut self) -> Result<Option<CanonicalObject>> {
            let object = match &mut self.storage {
                DeferredObjects::Memory { order, rows, .. } => order
                    .pop_front()
                    .and_then(|id| rows.remove(&id).map(|bytes| CanonicalObject { id, bytes })),
                DeferredObjects::Spill {
                    connection,
                    pending,
                    pending_bytes,
                    cursor,
                } => {
                    flush_objects(connection, pending, pending_bytes)?;
                    let row = connection
                        .query_row(
                            "SELECT sequence,object_id,bytes FROM objects
                             WHERE sequence>?1 ORDER BY sequence LIMIT 1",
                            [*cursor],
                            |row| {
                                Ok((
                                    row.get::<_, i64>(0)?,
                                    row.get::<_, Vec<u8>>(1)?,
                                    row.get::<_, Vec<u8>>(2)?,
                                ))
                            },
                        )
                        .optional()?;
                    row.map(|(sequence, id, bytes)| {
                        *cursor = sequence;
                        ObjectId::from_bytes(&id).map(|id| CanonicalObject { id, bytes })
                    })
                    .transpose()?
                }
            };
            if let Some(object) = &object {
                self.count -= 1;
                self.encoded_bytes -= object.bytes.len() as u64;
            }
            Ok(object)
        }

        pub fn visit_batches(
            &self,
            visitor: &mut dyn FnMut(&[CanonicalObject], bool) -> Result<()>,
        ) -> Result<()> {
            let mut batch = Vec::with_capacity(crate::OBJECT_BATCH_COUNT);
            let mut bytes = 0_usize;
            let mut emit = |object: CanonicalObject| -> Result<()> {
                if !batch.is_empty()
                    && (batch.len() == crate::OBJECT_BATCH_COUNT
                        || bytes + object.bytes.len() > crate::OBJECT_BATCH_BYTES)
                {
                    visitor(&batch, false)?;
                    batch.clear();
                    bytes = 0;
                }
                bytes += object.bytes.len();
                batch.push(object);
                Ok(())
            };
            match &self.storage {
                DeferredObjects::Memory { order, rows, .. } => {
                    for id in order {
                        emit(CanonicalObject {
                            id: *id,
                            bytes: rows
                                .get(id)
                                .ok_or(StorageError::Integrity("deferred object"))?
                                .clone(),
                        })?;
                    }
                }
                DeferredObjects::Spill {
                    connection,
                    pending,
                    ..
                } => {
                    let mut statement = connection
                        .prepare("SELECT object_id,bytes FROM objects ORDER BY sequence")?;
                    let rows = statement.query_map([], |row| {
                        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?))
                    })?;
                    for row in rows {
                        let (id, bytes) = row?;
                        emit(CanonicalObject {
                            id: ObjectId::from_bytes(&id)?,
                            bytes,
                        })?;
                    }
                    for (id, bytes) in pending {
                        emit(CanonicalObject {
                            id: *id,
                            bytes: bytes.clone(),
                        })?;
                    }
                }
            }
            if !batch.is_empty() {
                visitor(&batch, true)?;
            }
            Ok(())
        }

        #[cfg(test)]
        fn spilled(&self) -> bool {
            matches!(self.storage, DeferredObjects::Spill { .. })
        }
    }

    fn flush_objects(
        connection: &mut rusqlite::Connection,
        pending: &mut Vec<(ObjectId, Vec<u8>)>,
        pending_bytes: &mut usize,
    ) -> Result<()> {
        if pending.is_empty() {
            return Ok(());
        }
        let transaction = connection.transaction()?;
        {
            let mut insert =
                transaction.prepare("INSERT INTO objects(object_id,bytes) VALUES(?1,?2)")?;
            for (id, bytes) in pending.drain(..) {
                insert.execute(rusqlite::params![id.as_bytes().as_slice(), bytes])?;
            }
        }
        transaction.commit()?;
        *pending_bytes = 0;
        Ok(())
    }

    pub struct ObjectBuffer<'a> {
        source: Option<&'a dyn ObjectSource>,
        objects: DeferredObjectStore,
    }

    impl<'a> ObjectBuffer<'a> {
        pub fn new(source: &'a dyn ObjectSource) -> Result<Self> {
            Ok(Self {
                source: Some(source),
                objects: DeferredObjectStore::new()?,
            })
        }

        pub fn empty() -> Result<Self> {
            Ok(Self {
                source: None,
                objects: DeferredObjectStore::new()?,
            })
        }

        pub fn finish(self, root_id: ObjectId, cdc_bytes_scanned: u64) -> Result<BuiltRoot> {
            let encode_hash_invocations = self.objects.len();
            Ok(BuiltRoot {
                root_id,
                counters: BuildCounters {
                    cdc_bytes_scanned,
                    encode_hash_invocations,
                },
                objects: self.objects.reachable_from(root_id)?,
            })
        }
    }

    impl ObjectStore for ObjectBuffer<'_> {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            if let Some(bytes) = self.objects.get(id).map_err(|_| CoreError::MissingObject)? {
                return Ok(bytes);
            }
            self.source
                .ok_or(CoreError::MissingObject)?
                .read_object(id)
                .map_err(core_read_error)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            if let Some(bytes) = self.objects.get(id).map_err(|_| CoreError::MissingObject)? {
                if bytes != canonical {
                    return Err(CoreError::IdentityMismatch);
                }
                return Ok(id);
            }
            self.objects
                .put(id, canonical)
                .map_err(|_| CoreError::MissingObject)?;
            Ok(id)
        }
    }

    impl ObjectSource for ObjectBuffer<'_> {
        fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
            ObjectStore::get(self, id).map_err(StorageError::from)
        }
    }

    pub fn empty_root(seed: [u8; 32]) -> Result<BuiltRoot> {
        let mut store = ObjectBuffer::empty()?;
        let root_id = logical::empty_root(&mut store, seed)?;
        store.finish(root_id, 0)
    }

    pub fn apply_changes(
        source: &dyn ObjectSource,
        base_root: ObjectId,
        changes: &[ContentChange],
        seed: [u8; 32],
    ) -> Result<BuiltRoot> {
        let mut store = ObjectBuffer::new(source)?;
        let applied = logical::apply_changes(&mut store, base_root, changes, seed)?;
        store.finish(applied.root_id, applied.counters.cdc_bytes_scanned)
    }

    pub fn dependency_order(
        source: &(impl ObjectSource + ?Sized),
        root: ObjectId,
    ) -> Result<Vec<ObjectId>> {
        let mut ordered = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![(root, false)];
        while let Some((id, expanded)) = stack.pop() {
            if expanded {
                ordered.push(id);
                continue;
            }
            if !seen.insert(id) {
                continue;
            }
            let canonical = source.read_object(id)?;
            layerfs_content::authenticate_identity(&canonical, id)?;
            stack.push((id, true));
            let children = referenced_objects(&canonical)?;
            stack.extend(children.into_iter().rev().map(|child| (child, false)));
        }
        Ok(ordered)
    }

    #[cfg(test)]
    mod scratch_tests {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/merkle_unit.rs"
        ));
    }
}

pub use build::*;
