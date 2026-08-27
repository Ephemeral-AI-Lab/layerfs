use super::*;
use layerfs_core::inode::InodeId;
use layerfs_core::logical::{CandidateRoot, InodeMutation, LogicalCounters};
use layerfs_core::namespace_codec::decode_namespace_root;
use layerfs_core::object::access::ObjectStore;
use layerfs_core::CanonicalPath;
use std::cell::Cell;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_WRITER_ID: AtomicU64 = AtomicU64::new(1);

pub struct CandidateWrite<'a> {
    engine: &'a Engine,
    connection: ConnectionGuard<'a>,
    active: bool,
    writer_id: u64,
}

pub struct TrustedCandidate {
    candidate: CandidateRoot,
    store_id: [u8; 32],
    writer_id: u64,
}

impl TrustedCandidate {
    pub fn root(&self) -> ObjectId {
        self.candidate.root()
    }

    pub fn counters(&self) -> LogicalCounters {
        self.candidate.counters()
    }
}

impl Engine {
    pub fn object_ids_page(
        &self,
        after: Option<ObjectId>,
        limit: usize,
    ) -> EngineResult<Vec<ObjectId>> {
        if limit == 0 || limit > 1024 {
            return Err(EngineError::InvalidRecord("object page limit"));
        }
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT object_id FROM layerfs_objects
                 WHERE ?1 IS NULL OR object_id > ?1
                 ORDER BY object_id LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map(
                params![
                    after.map(|id| id.as_bytes().as_slice().to_vec()),
                    i64::try_from(limit).map_err(|_| EngineError::CounterOverflow)?
                ],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_sqlite_error)?;
        rows.map(|row| {
            ObjectId::from_bytes(&row.map_err(map_sqlite_error)?).map_err(EngineError::Core)
        })
        .collect()
    }

    pub fn load_canonical_authenticated(&self, id: ObjectId) -> EngineResult<Vec<u8>> {
        let connection = self.lock_connection()?;
        with_authenticated_canonical_on_connection(
            self,
            &connection,
            id,
            true,
            true,
            |_, canonical| Ok(canonical.to_vec()),
        )
    }

    pub fn load_canonical_authenticated_bounded(
        &self,
        id: ObjectId,
        maximum: usize,
    ) -> EngineResult<Vec<u8>> {
        let connection = self.lock_connection()?;
        let length = connection
            .query_row(
                "SELECT canonical_length FROM layerfs_objects WHERE object_id = ?1",
                params![id.as_bytes()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or(EngineError::MissingObject(id))?;
        if usize::try_from(length)
            .ok()
            .is_none_or(|length| length > maximum)
        {
            return Err(EngineError::InvalidRecord("object transfer bound"));
        }
        with_authenticated_canonical_on_connection(
            self,
            &connection,
            id,
            true,
            true,
            |_, canonical| Ok(canonical.to_vec()),
        )
    }

    pub fn begin_candidate_write(&self) -> EngineResult<CandidateWrite<'_>> {
        let mut connection = self.lock_write_connection()?;
        if !connection.transaction {
            connection
                .execute_batch("BEGIN IMMEDIATE")
                .map_err(map_sqlite_error)?;
            connection.transaction = true;
            self.bump(|counters| checked_add(&mut counters.transactions_started, 1))?;
        }
        Ok(CandidateWrite {
            engine: self,
            connection,
            active: true,
            writer_id: NEXT_WRITER_ID
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .map_err(|_| EngineError::CounterOverflow)?,
        })
    }
}

impl CandidateWrite<'_> {
    pub fn trusted_replace_file<R>(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
        input: R,
        initialize: (InodeId, ObjectId),
    ) -> EngineResult<TrustedCandidate>
    where
        R: Read,
    {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::replace_file(self, root, path, input, |_| {
                Ok(initialize)
            })?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_replace_range<R: Read>(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        replacement: R,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::replace_range(
                self,
                root,
                path,
                start,
                delete_len,
                replacement,
            )?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_create_directory(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
        inode: InodeId,
        metadata_root: ObjectId,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::create_directory(
                self,
                root,
                path,
                inode,
                metadata_root,
            )?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_create_symlink(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
        inode: InodeId,
        target: Vec<u8>,
        metadata_root: ObjectId,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::create_symlink(
                self,
                root,
                path,
                inode,
                target,
                metadata_root,
            )?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_hard_link(
        &mut self,
        root: ObjectId,
        source: &CanonicalPath,
        target: &CanonicalPath,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::hard_link(self, root, source, target)?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_rename(
        &mut self,
        root: ObjectId,
        from: &CanonicalPath,
        to: &CanonicalPath,
        source_parent_metadata_root: ObjectId,
        target_parent_metadata_root: ObjectId,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::rename(
                self,
                root,
                from,
                to,
                source_parent_metadata_root,
                target_parent_metadata_root,
            )?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_remove_path(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::remove_path(self, root, path)?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_apply_inode_mutations(
        &mut self,
        root: ObjectId,
        mutations: impl IntoIterator<Item = InodeMutation>,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::apply_inode_mutations(self, root, mutations)?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn allocate_inode_id(&mut self) -> EngineResult<InodeId> {
        let serial = self
            .connection
            .query_row(
                "SELECT next_inode_serial FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        let serial =
            u64::try_from(serial).map_err(|_| EngineError::InvalidRecord("inode serial"))?;
        let next = serial.checked_add(1).ok_or(EngineError::CounterOverflow)?;
        self.connection
            .execute(
                "UPDATE layerfs_authority SET next_inode_serial = ?1 WHERE authority_id = 1",
                params![i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?],
            )
            .map_err(map_sqlite_error)?;
        Ok(InodeId::allocate(self.engine.store_id, serial))
    }

    pub fn commit_candidate(mut self, root: ObjectId) -> EngineResult<ObjectId> {
        self.prepare_candidate_root(root)?;
        product::commit_product_state(
            self.engine,
            &mut self.connection,
            "SELECT EXISTS(SELECT 1 FROM layerfs_retained_roots WHERE root_id = ?1)",
            root.as_bytes(),
        )?;
        self.active = false;
        Ok(root)
    }

    pub fn commit_operation_candidate(
        self,
        operation_id: product::OperationId,
        root: ObjectId,
    ) -> EngineResult<ObjectId> {
        self.commit_operation_candidate_inner(operation_id, root, None)
    }

    pub fn commit_trusted_operation_candidate(
        self,
        operation_id: product::OperationId,
        candidate: TrustedCandidate,
    ) -> EngineResult<ObjectId> {
        if candidate.store_id != self.engine.store_id || candidate.writer_id != self.writer_id {
            return Err(EngineError::InvalidRecord("Core candidate writer"));
        }
        self.commit_operation_candidate_inner(
            operation_id,
            candidate.candidate.root(),
            Some(candidate.candidate.parent_root()),
        )
    }

    fn commit_operation_candidate_inner(
        mut self,
        operation_id: product::OperationId,
        root: ObjectId,
        trusted_parent: Option<ObjectId>,
    ) -> EngineResult<ObjectId> {
        let incumbent = self
            .connection
            .query_row(
                "SELECT candidate_root_id, state, base_root_id FROM layerfs_operations
                 WHERE operation_id = ?1",
                params![operation_id.as_bytes()],
                |row| {
                    Ok((
                        row.get::<_, Option<Vec<u8>>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Vec<u8>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or(EngineError::InvalidRecord("Operation"))?;
        if !matches!(incumbent.1.as_str(), "running" | "candidate") {
            return Err(EngineError::InvalidRecord("Operation candidate state"));
        }
        if let Some(trusted_parent) = trusted_parent {
            let expected_parent = incumbent
                .0
                .as_deref()
                .map(ObjectId::from_bytes)
                .transpose()
                .map_err(EngineError::Core)?
                .unwrap_or(ObjectId::from_bytes(&incumbent.2).map_err(EngineError::Core)?);
            if trusted_parent != expected_parent {
                return Err(EngineError::InvalidRecord("Core candidate parent"));
            }
            self.prepare_trusted_candidate_root(root)?;
        } else {
            self.prepare_candidate_root(root)?;
        }
        self.connection
            .execute(
                "UPDATE layerfs_operations SET candidate_root_id = ?1, state = 'candidate'
                 WHERE operation_id = ?2 AND state IN ('running', 'candidate')",
                params![root.as_bytes(), operation_id.as_bytes()],
            )
            .map_err(map_sqlite_error)?;
        if let Some(previous) = incumbent
            .0
            .filter(|previous| previous.as_slice() != root.as_bytes())
        {
            product::release_retained_root_if_unreferenced(&self.connection, &previous)?;
        }
        product::commit_product_state_pair(
            self.engine,
            &mut self.connection,
            "SELECT EXISTS(SELECT 1 FROM layerfs_operations
             WHERE operation_id = ?1 AND candidate_root_id = ?2 AND state = 'candidate')",
            operation_id.as_bytes(),
            root.as_bytes(),
        )?;
        self.active = false;
        Ok(root)
    }

    fn prepare_candidate_root(&self, root: ObjectId) -> EngineResult<()> {
        self.authenticate_and_retain_root(root)?;
        let statements = Cell::new(0);
        let failed = Cell::new(integrity::VerificationObservation::default());
        let observation = integrity::verify_root(
            &self.connection,
            &self.engine.path,
            self.engine.store_id,
            root,
            &statements,
            &failed,
        )?;
        self.engine.bump(|counters| {
            checked_add(&mut counters.candidate_full_scans, 1)?;
            checked_add(&mut counters.root_verifications, 1)?;
            checked_add(&mut counters.root_verification_objects, observation.objects)?;
            checked_add(&mut counters.root_verification_bytes, observation.bytes)?;
            super::add_verification_progress_counters(counters, observation)
        })
    }

    fn prepare_trusted_candidate_root(&self, root: ObjectId) -> EngineResult<()> {
        self.authenticate_and_retain_root(root)?;
        self.engine
            .bump(|counters| checked_add(&mut counters.candidate_shallow_bindings, 1))
    }

    fn authenticate_and_retain_root(&self, root: ObjectId) -> EngineResult<()> {
        ObjectStore::with_authenticated_canonical(self, root, |canonical| {
            decode_namespace_root(canonical).map(drop)
        })
        .map_err(EngineError::Core)?;
        self.connection
            .execute(
                "INSERT INTO layerfs_retained_roots (root_id) VALUES (?1)
                 ON CONFLICT(root_id) DO NOTHING",
                params![root.as_bytes()],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    pub fn commit_objects(mut self) -> EngineResult<()> {
        self.engine
            .commit_dispatch
            .commit(&self.connection)
            .map_err(map_sqlite_error)?;
        self.connection.transaction = false;
        self.active = false;
        self.engine
            .bump(|counters| checked_add(&mut counters.transactions_committed, 1))
    }

    fn commit_sync_pins(mut self, batch_id: [u8; 32]) -> EngineResult<()> {
        product::commit_product_state(
            self.engine,
            &mut self.connection,
            "SELECT EXISTS(SELECT 1 FROM layerfs_sync_batch_receipts WHERE batch_id = ?1)",
            &batch_id,
        )?;
        self.active = false;
        Ok(())
    }
}

impl Engine {
    pub fn contains_authenticated_object(&self, id: ObjectId) -> EngineResult<bool> {
        let connection = self.lock_connection()?;
        let present = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_objects WHERE object_id = ?1)",
                params![id.as_bytes()],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)?;
        if present {
            with_authenticated_canonical_on_connection(
                self,
                &connection,
                id,
                true,
                true,
                |_, _| Ok(()),
            )?;
        }
        Ok(present)
    }

    pub fn accept_canonical_batch(&self, objects: &[(ObjectId, Vec<u8>)]) -> EngineResult<()> {
        let mut writer = self.begin_candidate_write()?;
        for (expected, canonical) in objects {
            let actual = writer.put(canonical).map_err(EngineError::Core)?;
            if actual != *expected {
                return Err(EngineError::IdentityMismatch {
                    expected: *expected,
                    actual,
                });
            }
        }
        writer.commit_objects()
    }

    pub fn accept_canonical_batch_pinned(
        &self,
        owner_request_id: crate::product::RequestId,
        request_id: crate::product::RequestId,
        direction: &str,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> EngineResult<()> {
        if !matches!(direction, "fetch" | "push") {
            return Err(EngineError::InvalidRecord("sync pin direction"));
        }
        if objects.is_empty() {
            return Err(EngineError::InvalidRecord("empty sync batch"));
        }
        let mut writer = self.begin_candidate_write()?;
        let mut batch_hasher = blake3::Hasher::new();
        batch_hasher.update(b"layerfs.sync.batch.v1\0");
        batch_hasher.update(owner_request_id.as_bytes());
        batch_hasher.update(request_id.as_bytes());
        batch_hasher.update(direction.as_bytes());
        let mut canonical_bytes = 0_u64;
        for (expected, canonical) in objects {
            let length =
                u64::try_from(canonical.len()).map_err(|_| EngineError::CounterOverflow)?;
            batch_hasher.update(expected.as_bytes());
            batch_hasher.update(&length.to_be_bytes());
            canonical_bytes = canonical_bytes
                .checked_add(length)
                .ok_or(EngineError::CounterOverflow)?;
            let actual = writer.put(canonical).map_err(EngineError::Core)?;
            if actual != *expected {
                return Err(EngineError::IdentityMismatch {
                    expected: *expected,
                    actual,
                });
            }
            writer
                .connection
                .execute(
                    "INSERT INTO layerfs_sync_object_pins
                     (owner_request_id, request_id, direction, object_id, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(request_id, direction, object_id) DO NOTHING",
                    params![
                        owner_request_id.as_bytes(),
                        request_id.as_bytes(),
                        direction,
                        expected.as_bytes(),
                        product::unix_seconds()?,
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
        let wrong_owner = writer
            .connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM layerfs_sync_object_pins
                 WHERE request_id = ?1 AND direction = ?2 AND owner_request_id != ?3)",
                params![
                    request_id.as_bytes(),
                    direction,
                    owner_request_id.as_bytes()
                ],
                |row| row.get::<_, bool>(0),
            )
            .map_err(map_sqlite_error)?;
        if wrong_owner {
            return Err(EngineError::InvalidRecord("sync pin owner"));
        }
        let batch_id = *batch_hasher.finalize().as_bytes();
        writer
            .connection
            .execute(
                "INSERT INTO layerfs_sync_batch_receipts
                 (batch_id, owner_request_id, request_id, direction,
                  object_count, canonical_bytes, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(batch_id) DO NOTHING",
                params![
                    batch_id.as_slice(),
                    owner_request_id.as_bytes(),
                    request_id.as_bytes(),
                    direction,
                    i64::try_from(objects.len()).map_err(|_| EngineError::CounterOverflow)?,
                    i64::try_from(canonical_bytes).map_err(|_| EngineError::CounterOverflow)?,
                    product::unix_seconds()?,
                ],
            )
            .map_err(map_sqlite_error)?;
        writer.commit_sync_pins(batch_id)
    }
}

impl ObjectStore for CandidateWrite<'_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        with_read_canonical_on_connection(
            self.engine,
            &self.connection,
            id,
            true,
            true,
            |_, bytes| Ok(bytes.to_vec()),
        )
        .map_err(core_store_error)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> Result<T, CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, CoreError>,
    {
        with_authenticated_canonical_on_connection(
            self.engine,
            &self.connection,
            id,
            true,
            true,
            |_, bytes| callback(bytes).map_err(EngineError::Core),
        )
        .map_err(core_store_error)
    }

    fn put(&mut self, canonical: &[u8]) -> Result<ObjectId, CoreError> {
        put_canonical_object_on_connection(self.engine, &self.connection, canonical)
            .map(|(id, _)| id)
            .map_err(core_store_error)
    }
}

impl Drop for CandidateWrite<'_> {
    fn drop(&mut self) {
        if self.active && self.connection.transaction {
            if self
                .engine
                .commit_dispatch
                .rollback(&self.connection)
                .is_ok()
            {
                let _ = self
                    .engine
                    .bump(|counters| checked_add(&mut counters.transactions_rolled_back, 1));
            }
            self.connection.transaction = false;
            self.active = false;
        }
    }
}
