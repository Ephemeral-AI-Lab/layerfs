use super::{CandidateWrite, TrustedCandidate};
use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::full::legacy_store::{
    checked_add, commit_product_state, commit_product_state_pair, Engine,
};
use crate::full::record_id::{OperationId, RequestId};
use crate::integrity;
use crate::object::with_authenticated_canonical_on_connection;
use crate::sqlite::connection::add_verification_progress_counters;
use crate::working::compaction::reachability::release_retained_root_if_unreferenced;
use crate::working::lease::unix_seconds;
use layerfs_core::namespace_codec::decode_namespace_root;
use layerfs_core::object::access::ObjectStore;
use layerfs_core::ObjectId;
use rusqlite::{params, OptionalExtension};
use std::cell::Cell;

impl CandidateWrite<'_> {
    pub fn commit_candidate(mut self, root: ObjectId) -> EngineResult<ObjectId> {
        self.prepare_candidate_root(root)?;
        commit_product_state(
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
        operation_id: OperationId,
        root: ObjectId,
    ) -> EngineResult<ObjectId> {
        self.commit_operation_candidate_inner(operation_id, root, None)
    }

    pub fn commit_trusted_operation_candidate(
        self,
        operation_id: OperationId,
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
        operation_id: OperationId,
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
            release_retained_root_if_unreferenced(&self.connection, &previous)?;
        }
        commit_product_state_pair(
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
            add_verification_progress_counters(counters, observation)
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
        commit_product_state(
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
        owner_request_id: RequestId,
        request_id: RequestId,
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
                        unix_seconds()?,
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
                    unix_seconds()?,
                ],
            )
            .map_err(map_sqlite_error)?;
        writer.commit_sync_pins(batch_id)
    }
}
