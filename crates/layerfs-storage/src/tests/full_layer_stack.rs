//! Typed Full LayerStack persistence, CAS, replay, rollback, and role tests.

use super::fixture::test_path;
use crate::{
    derive_id, BranchHead, BranchId, EngineError, FullStorage, LayerCandidate, LayerId,
    LayerStackId, LayerStackMergeOutcome, LayerStackRollbackOutcome, OperationId,
    OperationVersionId, RequestId,
};
use layerfs_core::{encode_bytes_object, ObjectId, ObjectKind};
use rusqlite::params;

fn object(storage: &FullStorage, value: &[u8]) -> ObjectId {
    let canonical = encode_bytes_object(value).unwrap();
    let id = ObjectId::for_bytes(&canonical);
    storage
        .lock_connection()
        .unwrap()
        .execute(
            "INSERT INTO layerfs_objects
             (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, ?2, ?3, ?4)",
            params![
                id.as_bytes(),
                ObjectKind::Bytes as u8,
                canonical.len() as i64,
                canonical
            ],
        )
        .unwrap();
    id
}

#[test]
fn full_layer_stack_bootstrap_candidate_finalization_replay_and_rollback() {
    let path = test_path();
    let storage = FullStorage::create_durable(&path).unwrap();
    let root0 = object(&storage, b"root-0");
    let root1 = object(&storage, b"root-1");
    let root2 = object(&storage, b"root-2");
    let stack_id = LayerStackId::from_bytes([0x11; 32]);
    let genesis_id = LayerId::from_bytes([0x12; 32]);
    let stack = storage
        .bootstrap_layer_stack(stack_id, genesis_id, "main", root0)
        .unwrap();
    assert_eq!(
        storage.bootstrap_layer_stack(stack_id, genesis_id, "main", root0),
        Ok(stack)
    );
    assert!(storage
        .bootstrap_layer_stack(stack_id, genesis_id, "changed", root0)
        .is_err());
    assert_eq!(storage.layer_stack_head(stack_id), Ok(Some(stack)));
    assert_eq!(storage.layer_root(stack_id, genesis_id), Ok(Some(root0)));

    let branch_id = BranchId::from_bytes([0x13; 32]);
    let operation_id = OperationId::from_bytes([0x14; 32]);
    let version_id = OperationVersionId::from_bytes([0x15; 32]);
    let delta_id = [0x16_u8; 32];
    let connection = storage.lock_connection().unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_deltas
         (delta_id, format_version, parent_root_id, result_root_id, payload)
         VALUES (?1, 1, ?2, ?3, X'')",
            params![delta_id, root0.as_bytes(), root1.as_bytes()],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_branches
         (branch_id, name, fork_root_id, origin_layer_stack_id, origin_layer_id,
          depth, generation, head_operation_version_id, state)
         VALUES (?1, 'work', ?2, ?3, ?4, 0, 1, ?5, 'active')",
            params![
                branch_id.as_bytes(),
                root0.as_bytes(),
                stack_id.as_bytes(),
                genesis_id.as_bytes(),
                version_id.as_bytes()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_operations
         (operation_id, branch_id, sequence, expected_branch_generation, base_kind,
          base_layer_stack_id, base_layer_id, base_root_id, candidate_root_id,
          result_operation_version_id, state)
         VALUES (?1, ?2, 0, 0, 'layer', ?3, ?4, ?5, ?6, ?7, 'durably_accepted')",
            params![
                operation_id.as_bytes(),
                branch_id.as_bytes(),
                stack_id.as_bytes(),
                genesis_id.as_bytes(),
                root0.as_bytes(),
                root1.as_bytes(),
                version_id.as_bytes()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_operation_versions
         (operation_version_id, branch_id, sequence, created_by_kind, operation_id,
          transition_delta_id, base_root_id, result_root_id)
         VALUES (?1, ?2, 0, 'operation', ?3, ?4, ?5, ?6)",
            params![
                version_id.as_bytes(),
                branch_id.as_bytes(),
                operation_id.as_bytes(),
                delta_id,
                root0.as_bytes(),
                root1.as_bytes()
            ],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_branch_transitions
         (transition_id, branch_id, before_generation, after_generation,
          before_operation_version_id, after_operation_version_id, action_kind,
          source_record_id, request_id)
         VALUES (?1, ?2, 0, 1, NULL, ?3, 'operation_commit', ?4, ?5)",
            params![
                [0x17_u8; 32],
                branch_id.as_bytes(),
                version_id.as_bytes(),
                operation_id.as_bytes(),
                [0x18_u8; 32]
            ],
        )
        .unwrap();
    connection.execute_batch("COMMIT").unwrap();
    drop(connection);

    let prepare = RequestId::from_bytes([0x19; 32]);
    let source = BranchHead {
        branch_id,
        generation: 1,
        operation_version_id: Some(version_id),
        root: root1,
    };
    let layer_id = LayerId::from_bytes(derive_id(
        b"candidate-layer",
        &[stack_id.as_bytes(), prepare.as_bytes(), root2.as_bytes()],
    ));
    let candidate = LayerCandidate {
        layer_stack_id: stack_id,
        layer_id,
        parent_layer_id: genesis_id,
        source,
        source_depth: 0,
        root: root2,
        request_id: prepare,
    };
    assert_eq!(
        storage.import_layer_candidate(candidate, stack),
        Ok(candidate)
    );
    let mut altered = candidate;
    altered.source_depth = 1;
    assert!(matches!(
        storage.import_layer_candidate(altered, stack),
        Err(EngineError::InvalidRecord("Full Layer candidate identity"))
    ));
    assert_eq!(
        storage.import_layer_candidate(candidate, stack),
        Ok(candidate)
    );
    let finalize = RequestId::from_bytes([0x1a; 32]);
    let accepted = match storage
        .finalize_layer_stack_merge(candidate, stack, finalize)
        .unwrap()
    {
        LayerStackMergeOutcome::DurablyAccepted {
            head,
            reconciled: false,
        } => head,
        other => panic!("unexpected finalization: {other:?}"),
    };
    assert_eq!(
        storage.finalize_layer_stack_merge(candidate, stack, finalize),
        Ok(LayerStackMergeOutcome::DurablyAccepted {
            head: accepted,
            reconciled: true
        })
    );
    assert_eq!(
        storage.import_layer_candidate(candidate, stack),
        Ok(candidate)
    );
    assert_eq!(
        storage.finalize_layer_stack_merge(candidate, stack, RequestId::from_bytes([0x1c; 32])),
        Ok(LayerStackMergeOutcome::Conflict { actual: accepted })
    );
    assert_eq!(storage.layer_stack_head(stack_id), Ok(Some(accepted)));
    let rollback = RequestId::from_bytes([0x1b; 32]);
    let rolled = match storage
        .rollback_layer_stack(accepted, genesis_id, rollback)
        .unwrap()
    {
        LayerStackRollbackOutcome::DurablyAccepted {
            head,
            reconciled: false,
        } => head,
        other => panic!("unexpected rollback: {other:?}"),
    };
    assert_eq!(
        storage.rollback_layer_stack(accepted, genesis_id, rollback),
        Ok(LayerStackRollbackOutcome::DurablyAccepted {
            head: rolled,
            reconciled: true
        })
    );
    assert_eq!(
        storage.rollback_layer_stack(accepted, genesis_id, RequestId::from_bytes([0x1d; 32])),
        Ok(LayerStackRollbackOutcome::Conflict { actual: rolled })
    );
    assert_eq!(storage.layer_root(stack_id, layer_id), Ok(None));
    drop(storage);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn cache_role_cannot_execute_authoritative_layer_stack_methods() {
    let path = test_path();
    let cache = FullStorage::create_cache(&path, [0x31; 32]).unwrap();
    assert!(matches!(
        cache.layer_stack_head(LayerStackId::from_bytes([0x32; 32])),
        Err(EngineError::InvalidRecord("FullStorage authority role"))
    ));
    drop(cache);
    std::fs::remove_file(path).unwrap();
}
