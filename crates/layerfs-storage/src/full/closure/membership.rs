//! Accepted closure authentication and membership traversal.

use crate::error::{EngineError, EngineResult};
use crate::full::legacy_store::{checked_add, Engine};
use crate::full::record_id::BranchId;
use crate::full::transfer::batch::{BranchPushBundle, MAX_PUSH_OPERATION_RECORDS};
use crate::integrity;
use crate::object::with_authenticated_canonical_on_connection;
use layerfs_core::{namespace_codec::decode_namespace_root, ObjectId};
use rusqlite::Connection;
use std::cell::Cell;

pub(crate) fn authenticate_root(
    engine: &Engine,
    connection: &Connection,
    root: ObjectId,
) -> EngineResult<()> {
    authenticate_root_object(engine, connection, root)?;
    let statements = Cell::new(0);
    let failed = Cell::new(integrity::VerificationObservation::default());
    let observation = integrity::verify_root(
        connection,
        &engine.path,
        engine.store_id,
        root,
        &statements,
        &failed,
    )?;
    engine.bump(|counters| {
        checked_add(&mut counters.candidate_full_scans, 1)?;
        checked_add(&mut counters.root_verifications, 1)?;
        checked_add(&mut counters.root_verification_objects, observation.objects)?;
        checked_add(&mut counters.root_verification_bytes, observation.bytes)?;
        crate::sqlite::connection::add_verification_progress_counters(counters, observation)
    })
}

pub(crate) fn authenticate_root_shallow(
    engine: &Engine,
    connection: &Connection,
    root: ObjectId,
) -> EngineResult<()> {
    authenticate_root_object(engine, connection, root)?;
    engine.bump(|counters| checked_add(&mut counters.candidate_shallow_bindings, 1))
}

pub(crate) fn authenticate_root_object(
    engine: &Engine,
    connection: &Connection,
    root: ObjectId,
) -> EngineResult<()> {
    with_authenticated_canonical_on_connection(
        engine,
        connection,
        root,
        true,
        true,
        |_, canonical| {
            decode_namespace_root(canonical)
                .map(drop)
                .map_err(EngineError::Core)
        },
    )
}

pub(crate) fn collect_fetch_branch_roots(
    bundle: &BranchPushBundle,
    roots: &mut std::collections::BTreeSet<(BranchId, ObjectId)>,
) -> EngineResult<()> {
    fn collect(
        bundle: &BranchPushBundle,
        roots: &mut std::collections::BTreeSet<(BranchId, ObjectId)>,
        branches: &mut std::collections::BTreeSet<BranchId>,
        remaining: &mut usize,
    ) -> EngineResult<()> {
        if !branches.insert(bundle.head.branch_id) {
            return Err(EngineError::InvalidRecord("Fetch duplicate Branch bundle"));
        }
        let records = 1_usize
            .checked_add(bundle.operations.len())
            .and_then(|count| count.checked_add(bundle.child_merges.len()))
            .and_then(|count| count.checked_add(bundle.rollbacks.len()))
            .and_then(|count| count.checked_add(bundle.origin_stack.layers.len()))
            .and_then(|count| count.checked_add(bundle.origin_stack.transitions.len()))
            .ok_or(EngineError::CounterOverflow)?;
        *remaining = remaining
            .checked_sub(records)
            .ok_or(EngineError::InvalidRecord("Fetch history page required"))?;
        roots.insert((bundle.head.branch_id, bundle.ancestry.fork_root));
        let head_released = bundle.head.operation_version_id.is_some_and(|head| {
            bundle.operations.iter().any(|operation| {
                operation.operation_version_id == head && operation.release.is_some()
            }) || bundle
                .child_merges
                .iter()
                .any(|merge| merge.operation_version_id == head && merge.release.is_some())
        });
        if !head_released {
            roots.insert((bundle.head.branch_id, bundle.head.root));
        }
        roots.extend(
            bundle
                .operations
                .iter()
                .filter(|operation| operation.release.is_none())
                .map(|operation| (bundle.head.branch_id, operation.root)),
        );
        roots.extend(
            bundle
                .child_merges
                .iter()
                .filter(|merge| merge.release.is_none())
                .map(|merge| (bundle.head.branch_id, merge.root)),
        );
        for dependency in &bundle.dependencies {
            collect(dependency, roots, branches, remaining)?;
        }
        Ok(())
    }

    let mut remaining = MAX_PUSH_OPERATION_RECORDS;
    collect(
        bundle,
        roots,
        &mut std::collections::BTreeSet::new(),
        &mut remaining,
    )
}
