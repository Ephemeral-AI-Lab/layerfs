use super::*;
use rusqlite::params;

#[test]
fn one_external_binding_serves_multiple_branches_and_operation_bases_are_xor() {
    let connection = create_contract(WORKING_SCHEMA);
    let binding = [0x11_u8; 32];
    connection
        .execute(
            "INSERT INTO layerfs_working_base_bindings
             (binding_id, durable_storage_id, target_kind, target_id,
              target_version_id, generation, root_id, verification_receipt_id,
              authority_pin_id, pin_expires_at, status)
             VALUES (?1, ?2, 'branch', ?3, NULL, 0, ?4, ?5, ?6, NULL,
                     'external_pinned')",
            params![
                binding,
                [0x12_u8; 32],
                [0x13_u8; 32],
                [0x14_u8; 32],
                [0x15_u8; 32],
                [0x16_u8; 32],
            ],
        )
        .unwrap();
    for branch in [[0x21_u8; 32], [0x22_u8; 32]] {
        connection
            .execute(
                "INSERT INTO layerfs_branches
                 (branch_id, name, immediate_parent_branch_id, fork_operation_id,
                  fork_operation_version_id, fork_root_id, origin_base_binding_id,
                  depth, generation, head_operation_version_id, state)
                 VALUES (?1, NULL, NULL, NULL, NULL, ?2, ?3, 0, 0, NULL, 'active')",
                params![branch, [0x14_u8; 32], binding],
            )
            .unwrap();
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM layerfs_branches
                 WHERE origin_base_binding_id = ?1",
                params![binding],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        2
    );
    connection
        .execute(
            "INSERT INTO layerfs_operations
             (operation_id, branch_id, sequence, expected_branch_generation,
              base_kind, base_binding_id, base_operation_version_id, base_root_id,
              candidate_root_id, result_operation_version_id, state,
              reconciliation_class)
             VALUES (?1, ?2, 0, 0, 'external_base', ?3, NULL, ?4,
                     NULL, NULL, 'running', NULL)",
            params![[0x31_u8; 32], [0x21_u8; 32], binding, [0x14_u8; 32]],
        )
        .unwrap();
    assert!(connection
        .execute(
            "INSERT INTO layerfs_operations
             (operation_id, branch_id, sequence, expected_branch_generation,
              base_kind, base_binding_id, base_operation_version_id, base_root_id,
              candidate_root_id, result_operation_version_id, state,
              reconciliation_class)
             VALUES (?1, ?2, 1, 0, 'external_base', ?3, ?4, ?5,
                     NULL, NULL, 'running', NULL)",
            params![
                [0x32_u8; 32],
                [0x21_u8; 32],
                binding,
                [0x33_u8; 32],
                [0x14_u8; 32],
            ],
        )
        .is_err());
}

#[test]
fn slim_origin_and_folded_owners_replace_legacy_compatibility_tables() {
    let full = create_contract(FULL_SCHEMA);
    let push_columns = columns(&full, "layerfs_branch_push_pages");
    for field in [
        "origin_authority_storage_id",
        "origin_target_kind",
        "origin_target_id",
        "origin_version_id",
        "origin_generation",
        "origin_root_id",
        "origin_verification_receipt_id",
        "origin_pin_id",
    ] {
        assert!(push_columns.contains(&field.to_owned()));
    }
    for forbidden in [
        "layerfs_authority",
        "layerfs_durable_storages",
        "layerfs_fetch_staging_heads",
        "layerfs_operation_deltas",
        "layerfs_layer_deltas",
        "layerfs_push_outbox",
    ] {
        assert!(!FULL_SCHEMA.table_names.contains(&forbidden));
    }
}

#[test]
fn full_release_request_covers_multiple_versions_and_operation_base_is_global() {
    let connection = create_contract(FULL_SCHEMA);
    connection.execute_batch("BEGIN").unwrap();
    let request = [0x70_u8; 32];
    for (serial, version) in [(0x71_u8, 0x72_u8), (0x73_u8, 0x74_u8)] {
        connection
            .execute(
                "INSERT INTO layerfs_released_versions
                 (release_id, target_kind, layer_stack_id, layer_id, branch_id,
                  operation_version_id, root_id, release_generation, request_id)
                 VALUES (?1, 'operation_version', NULL, NULL, ?2, ?3, ?4, 1, ?5)",
                params![
                    [serial; 32],
                    [0x75_u8; 32],
                    [version; 32],
                    [0x76_u8; 32],
                    request,
                ],
            )
            .unwrap();
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM layerfs_released_versions WHERE request_id = ?1",
                params![request],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        2
    );
    connection.execute_batch("ROLLBACK").unwrap();

    let base_fk = connection
        .prepare("PRAGMA foreign_key_list(layerfs_operations)")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let base_fk = base_fk
        .iter()
        .filter(|(_, _, from, _)| from == "base_operation_version_id")
        .collect::<Vec<_>>();
    assert_eq!(base_fk.len(), 1);
    assert_eq!(base_fk[0].1, "layerfs_operation_versions");
    assert_eq!(base_fk[0].3, "operation_version_id");
    assert_eq!(
        connection
            .prepare("PRAGMA foreign_key_list(layerfs_operations)")
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
            .into_iter()
            .filter(|id| *id == base_fk[0].0)
            .count(),
        1
    );
}
