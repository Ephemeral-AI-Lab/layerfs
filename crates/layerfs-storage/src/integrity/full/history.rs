//! Accepted Full-store history-root verification.

use super::closure::{drain, enqueue, validate_namespace_graph_disk, Role};
use super::object::{merge_failed_observation, ConnectionStore, VerificationObservation};
use crate::refs::validate_ref_name;
use crate::scratch::{DiskNamespace, DiskTable};
use crate::{map_sqlite_error, EngineError, EngineResult};
use layerfs_core::ObjectId;
use rusqlite::{params, Connection, OptionalExtension};
use std::cell::Cell;
use std::path::Path;

pub(crate) struct RetainedUnionObservation {
    pub(crate) verification: VerificationObservation,
    pub(crate) peak_bytes: u64,
}

pub(crate) fn authenticated_closure_for_each(
    connection: &Connection,
    store: &Path,
    store_id: [u8; 32],
    roots: impl IntoIterator<Item = ObjectId>,
    mut visitor: impl FnMut(ObjectId) -> EngineResult<()>,
) -> EngineResult<()> {
    let work = DiskTable::create_near_with_store_id(store, "fetch-closure", store_id)?;
    let ids = DiskTable::create_near_with_store_id(store, "fetch-object-ids", store_id)?;
    let payloads = ids.namespace(b"payload-lengths")?;
    let statements = Cell::new(0);
    let failed = Cell::new(VerificationObservation::default());
    let result = (|| {
        for root in roots {
            enqueue(&work, root, Role::Namespace, true)?;
        }
        drain(connection, &work, &payloads, &statements, &failed)?;
        work.for_each_key(|key| {
            if key.len() != 34 {
                return Err(EngineError::InvalidRecord("closure key"));
            }
            ids.put(&key[..32], &[])
        })?;
        ids.for_each_key(|key| {
            if key.len() == 32 {
                visitor(ObjectId::from_bytes(key)?)?;
            }
            Ok(())
        })
    })();
    match (result, work.finish(), ids.finish()) {
        (Ok(()), Ok(_), Ok(_)) => Ok(()),
        (Err(error), _, _) => Err(error),
        (_, Err(error), _) | (_, _, Err(error)) => Err(error),
    }
}

pub(crate) fn verify_full_accepted_state(
    connection: &Connection,
    store: &Path,
    store_id: [u8; 32],
) -> EngineResult<()> {
    if crate::full::compaction::verify::verify_full_product_integrity(connection)? {
        return Ok(());
    }
    let mut after: Option<Vec<u8>> = None;
    loop {
        let roots = connection
            .prepare(
                "SELECT root_id FROM (
                     SELECT root_id FROM layerfs_retained_roots
                     UNION SELECT result_root_id AS root_id FROM layerfs_layers l
                       WHERE l.state != 'dropped' AND NOT EXISTS(
                         SELECT 1 FROM layerfs_released_versions r
                         WHERE r.target_kind = 'layer'
                           AND r.layer_stack_id = l.layer_stack_id AND r.layer_id = l.layer_id)
                     UNION SELECT fork_root_id AS root_id FROM layerfs_branches
                       WHERE state = 'active'
                     UNION SELECT result_root_id AS root_id FROM layerfs_operation_versions v
                       WHERE NOT EXISTS(SELECT 1 FROM layerfs_released_versions r
                         WHERE r.target_kind = 'operation_version'
                           AND r.branch_id = v.branch_id
                           AND r.operation_version_id = v.operation_version_id)
                     UNION SELECT root_id FROM layerfs_durable_tracking_refs
                       WHERE status = 'verified_complete')
                 WHERE ?1 IS NULL OR root_id > ?1
                 GROUP BY root_id ORDER BY root_id LIMIT 64",
            )
            .map_err(map_sqlite_error)?
            .query_map(params![after.as_deref()], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sqlite_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(map_sqlite_error)?;
        if roots.is_empty() {
            break;
        }
        after = roots.last().cloned();
        authenticated_closure_for_each(
            connection,
            store,
            store_id,
            roots
                .iter()
                .map(|root| ObjectId::from_bytes(root).map_err(EngineError::Core))
                .collect::<EngineResult<Vec<_>>>()?,
            |_| Ok(()),
        )?;
    }
    verify_full_tracking_membership(connection, store, store_id)
}

fn verify_full_tracking_membership(
    connection: &Connection,
    store: &Path,
    store_id: [u8; 32],
) -> EngineResult<()> {
    let mut after: Option<Vec<u8>> = None;
    loop {
        let tracking = connection
            .query_row(
                "SELECT tracking_ref_id, root_id FROM layerfs_durable_tracking_refs
                 WHERE status = 'verified_complete'
                   AND (?1 IS NULL OR tracking_ref_id > ?1)
                 ORDER BY tracking_ref_id LIMIT 1",
                params![after.as_deref()],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some((tracking_ref, root)) = tracking else {
            return Ok(());
        };
        let root = ObjectId::from_bytes(&root).map_err(EngineError::Core)?;
        let mut expected = 0_i64;
        authenticated_closure_for_each(connection, store, store_id, [root], |object| {
            let present = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_fetch_closure_items
                     WHERE tracking_ref_id = ?1 AND object_id = ?2)",
                    params![tracking_ref.as_slice(), object.as_bytes()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if !present {
                return Err(EngineError::InvalidRecord("Full tracking membership"));
            }
            expected = expected
                .checked_add(1)
                .ok_or(EngineError::CounterOverflow)?;
            Ok(())
        })?;
        let actual = connection
            .query_row(
                "SELECT count(*) FROM layerfs_fetch_closure_items
                 WHERE tracking_ref_id = ?1",
                params![tracking_ref.as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        if actual != expected {
            return Err(EngineError::InvalidRecord("Full tracking membership"));
        }
        after = Some(tracking_ref);
    }
}

pub(crate) fn verify_retained_union_observed_counted(
    connection: &Connection,
    store: &Path,
    store_id: [u8; 32],
    statements: &Cell<u64>,
    failed: &Cell<VerificationObservation>,
) -> EngineResult<RetainedUnionObservation> {
    let mut retained = retained_union(connection, store, store_id, statements, failed)?;
    retained.observation.add_scratch(retained.work.finish()?)?;
    Ok(RetainedUnionObservation {
        verification: retained.observation,
        peak_bytes: retained.peak_bytes,
    })
}

pub(crate) struct RetainedUnion {
    pub(crate) work: DiskTable,
    pub(crate) peak_bytes: u64,
    pub(crate) observation: VerificationObservation,
}

fn note_statement_attempt(statements: &Cell<u64>) -> EngineResult<()> {
    statements.set(
        statements
            .get()
            .checked_add(1)
            .ok_or(EngineError::CounterOverflow)?,
    );
    Ok(())
}

pub(crate) fn retained_union(
    connection: &Connection,
    store: &Path,
    store_id: [u8; 32],
    statements: &Cell<u64>,
    failed: &Cell<VerificationObservation>,
) -> EngineResult<RetainedUnion> {
    let work = DiskTable::create_near_with_store_id(store, "closure", store_id)?;
    let graph = match DiskTable::create_near_with_store_id(store, "namespace-graph", store_id) {
        Ok(graph) => graph,
        Err(error) => {
            let mut observation = VerificationObservation::default();
            if let Ok(scratch) = work.finish() {
                let _ = observation.add_scratch(scratch);
            }
            merge_failed_observation(failed, observation);
            return Err(error);
        }
    };
    let result = (|| {
        let records = graph.namespace(b"records")?;
        let state = graph.namespace(b"state")?;
        let payload_lengths = graph.namespace(b"payload-lengths")?;
        let validated_roots = graph.namespace(b"validated-roots")?;
        let mut observation = VerificationObservation::default();
        let mut peak_bytes = work.storage_bytes()?.saturating_add(graph.storage_bytes()?);
        note_statement_attempt(statements)?;
        let mut statement = connection
            .prepare("SELECT name, generation, root_id FROM layerfs_refs ORDER BY name")
            .map_err(map_sqlite_error)?;
        let refs = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(map_sqlite_error)?;
        for row in refs {
            let (name, generation, root) = row.map_err(map_sqlite_error)?;
            validate_ref_name(&name)?;
            if generation < 0 {
                return Err(EngineError::InvalidRecord("ref generation"));
            }
            let root = ObjectId::from_bytes(&root)?;
            note_statement_attempt(statements)?;
            let retained = connection
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM layerfs_retained_roots WHERE root_id = ?1)",
                    params![root.as_bytes().as_slice()],
                    |row| row.get::<_, bool>(0),
                )
                .map_err(map_sqlite_error)?;
            if !retained {
                return Err(EngineError::MissingRoot(root));
            }
            if claim_root(&validated_roots, root)? {
                observation.retained_roots_validated = observation
                    .retained_roots_validated
                    .checked_add(1)
                    .ok_or(EngineError::CounterOverflow)?;
                enqueue(&work, root, Role::Namespace, true)?;
                observation.merge(drain(
                    connection,
                    &work,
                    &payload_lengths,
                    statements,
                    failed,
                )?)?;
                records.clear()?;
                state.clear()?;
                observation.merge(validate_namespace_graph_disk(
                    connection,
                    &records,
                    &state,
                    &payload_lengths,
                    root,
                    statements,
                    failed,
                )?)?;
                peak_bytes =
                    peak_bytes.max(work.storage_bytes()?.saturating_add(graph.storage_bytes()?));
            }
        }
        drop(statement);
        note_statement_attempt(statements)?;
        let mut statement = connection
            .prepare("SELECT root_id FROM layerfs_retained_roots ORDER BY root_id")
            .map_err(map_sqlite_error)?;
        let roots = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sqlite_error)?;
        for root in roots {
            let root = ObjectId::from_bytes(&root.map_err(map_sqlite_error)?)?;
            if claim_root(&validated_roots, root)? {
                observation.retained_roots_validated = observation
                    .retained_roots_validated
                    .checked_add(1)
                    .ok_or(EngineError::CounterOverflow)?;
                enqueue(&work, root, Role::Namespace, true)?;
                observation.merge(drain(
                    connection,
                    &work,
                    &payload_lengths,
                    statements,
                    failed,
                )?)?;
                records.clear()?;
                state.clear()?;
                observation.merge(validate_namespace_graph_disk(
                    connection,
                    &records,
                    &state,
                    &payload_lengths,
                    root,
                    statements,
                    failed,
                )?)?;
                peak_bytes =
                    peak_bytes.max(work.storage_bytes()?.saturating_add(graph.storage_bytes()?));
            }
        }
        drop(statement);
        note_statement_attempt(statements)?;
        let mut statement = connection
            .prepare(
                "SELECT l.root_id FROM layerfs_layers l
                    WHERE l.state != 'dropped' AND NOT EXISTS(
                        SELECT 1 FROM layerfs_released_versions r
                        WHERE r.target_kind = 'layer'
                          AND r.owner_id = l.layer_stack_id
                          AND r.version_id = l.layer_id)
                 UNION ALL SELECT fork_root_id FROM layerfs_branches WHERE state = 'active'
                 UNION ALL SELECT v.root_id FROM layerfs_operation_versions v
                    WHERE NOT EXISTS(
                        SELECT 1 FROM layerfs_released_versions r
                        WHERE r.target_kind = 'operation_version'
                          AND r.owner_id = v.branch_id
                          AND r.version_id = v.operation_version_id)
                 UNION ALL SELECT candidate_root_id FROM layerfs_operations
                    WHERE candidate_root_id IS NOT NULL
                      AND state IN ('running', 'candidate', 'preserved', 'indeterminate')
                 UNION ALL SELECT published_root_id FROM layerfs_fetch_staging_heads
                    WHERE published_root_id IS NOT NULL
                 UNION ALL SELECT root_id FROM layerfs_durable_tracking_refs",
            )
            .map_err(map_sqlite_error)?;
        let roots = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sqlite_error)?;
        for root in roots {
            let root = ObjectId::from_bytes(&root.map_err(map_sqlite_error)?)?;
            if claim_root(&validated_roots, root)? {
                observation.retained_roots_validated = observation
                    .retained_roots_validated
                    .checked_add(1)
                    .ok_or(EngineError::CounterOverflow)?;
                enqueue(&work, root, Role::Namespace, true)?;
                observation.merge(drain(
                    connection,
                    &work,
                    &payload_lengths,
                    statements,
                    failed,
                )?)?;
                records.clear()?;
                state.clear()?;
                observation.merge(validate_namespace_graph_disk(
                    connection,
                    &records,
                    &state,
                    &payload_lengths,
                    root,
                    statements,
                    failed,
                )?)?;
                peak_bytes =
                    peak_bytes.max(work.storage_bytes()?.saturating_add(graph.storage_bytes()?));
            }
        }
        drop(statement);
        note_statement_attempt(statements)?;
        let mut statement = connection
            .prepare(
                "SELECT object_id FROM layerfs_sync_object_pins
                 UNION SELECT object_id FROM layerfs_fetch_closure_items
                 ORDER BY object_id",
            )
            .map_err(map_sqlite_error)?;
        let pins = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sqlite_error)?;
        let object_store = ConnectionStore::new(connection, statements, failed);
        for pin in pins {
            let id = ObjectId::from_bytes(&pin.map_err(map_sqlite_error)?)?;
            object_store
                .with_record(id, false, |_, _| Ok(()))
                .map_err(|cause| EngineError::MalformedObject { id, cause })?;
            enqueue(&work, id, Role::Payload, false)?;
        }
        observation.merge(object_store.observation())?;
        Ok((peak_bytes, observation))
    })();
    match result {
        Ok((peak_bytes, mut observation)) => {
            observation.add_scratch(graph.finish()?)?;
            observation.statements = statements.get();
            Ok(RetainedUnion {
                work,
                peak_bytes,
                observation,
            })
        }
        Err(error) => {
            let mut observation = VerificationObservation::default();
            if let Ok(scratch) = work.finish() {
                let _ = observation.add_scratch(scratch);
            }
            if let Ok(scratch) = graph.finish() {
                let _ = observation.add_scratch(scratch);
            }
            merge_failed_observation(failed, observation);
            Err(error)
        }
    }
}

fn claim_root(validated_roots: &DiskNamespace<'_>, root: ObjectId) -> EngineResult<bool> {
    if validated_roots.get(root.as_bytes())?.is_some() {
        return Ok(false);
    }
    validated_roots.put(root.as_bytes(), &[])?;
    Ok(true)
}
