use super::*;
use crate::sqlite::admission::index::validate_index_schemas;
use rusqlite::params;

mod constraints;
mod fixture;
mod nullable_identity;
mod route_state;
mod tuple_matrix;

use fixture::*;

#[test]
fn frozen_identities_roles_and_sorted_manifests_are_exact() {
    assert_eq!(LEGACY_FULL_SCHEMA.identity, SchemaIdentity::LegacyFull);
    assert_eq!(
        LEGACY_FULL_SCHEMA.format_marker,
        "layerfs-phase4a-sqlite-blob"
    );
    assert_eq!(LEGACY_FULL_SCHEMA.schema_version, 2);
    assert_eq!(LEGACY_FULL_SCHEMA.table_names.len(), 29);
    assert!(LEGACY_FULL_SCHEMA.index_schemas.is_empty());
    assert!(SchemaIdentity::LegacyFull.is_explicit_migration_source());
    for role in [
        StoreRole::Working,
        StoreRole::Durable,
        StoreRole::DurableCache,
    ] {
        assert!(!LEGACY_FULL_SCHEMA.admits_role(role));
    }

    assert_eq!(FULL_SCHEMA.identity, SchemaIdentity::Full);
    assert_eq!(FULL_SCHEMA.format_marker, "layerfs-full-sqlite");
    assert_eq!(FULL_SCHEMA.schema_version, 1);
    assert_eq!(FULL_SCHEMA.table_names.len(), 21);
    assert_eq!(FULL_SCHEMA.index_schemas.len(), 4);
    assert!(FULL_SCHEMA.admits_role(StoreRole::Durable));
    assert!(FULL_SCHEMA.admits_role(StoreRole::DurableCache));
    assert!(!FULL_SCHEMA.admits_role(StoreRole::Working));
    assert!(FULL_SCHEMA.admits_exact_role(StoreRole::Durable, StoreRole::Durable));
    assert!(!FULL_SCHEMA.admits_exact_role(StoreRole::Durable, StoreRole::DurableCache));
    assert!(!StoreRole::DurableCache.is_authority());
    assert!(StoreRole::Durable.is_authority());

    assert_eq!(WORKING_SCHEMA.identity, SchemaIdentity::Working);
    assert_eq!(WORKING_SCHEMA.format_marker, "layerfs-working-sqlite");
    assert_eq!(WORKING_SCHEMA.schema_version, 1);
    assert_eq!(WORKING_SCHEMA.table_names.len(), 14);
    assert_eq!(WORKING_SCHEMA.index_schemas.len(), 2);
    assert!(WORKING_SCHEMA.admits_role(StoreRole::Working));
    assert!(!WORKING_SCHEMA.admits_role(StoreRole::Durable));
    assert!(!WORKING_SCHEMA.admits_role(StoreRole::DurableCache));
    assert_eq!(
        StoreRole::Working.schema_identity(),
        SchemaIdentity::Working
    );

    for contract in [LEGACY_FULL_SCHEMA, FULL_SCHEMA, WORKING_SCHEMA] {
        assert_eq!(contract.identity.format_marker(), contract.format_marker);
        assert_eq!(contract.identity.schema_version(), contract.schema_version);
        assert!(contract
            .table_names
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
    }
}

#[test]
fn full_ddl_executes_with_exact_tables_columns_fks_and_indexes() {
    let connection = create_contract(FULL_SCHEMA);
    assert_sql_shapes(&connection, FULL_SCHEMA);
    assert!(assert_fk_targets_are_local(&connection, FULL_SCHEMA) >= 30);
    assert_eq!(
        columns(&connection, "layerfs_store_meta"),
        [
            "store_id",
            "format_marker",
            "schema_version",
            "store_role",
            "storage_id",
            "durable_storage_id",
            "next_inode_serial",
            "trusted_history",
            "journal_mode",
            "synchronous",
            "temp_store",
            "mmap_size",
        ]
    );
    assert_eq!(
        columns(&connection, "layerfs_operation_versions"),
        [
            "operation_version_id",
            "branch_id",
            "sequence",
            "parent_operation_version_id",
            "created_by_kind",
            "operation_id",
            "child_branch_id",
            "branch_delta_id",
            "transition_delta_id",
            "base_root_id",
            "result_root_id",
        ]
    );
    assert!(columns(&connection, "layerfs_layers").contains(&"transition_delta_id".into()));
    assert!(!FULL_SCHEMA
        .table_names
        .contains(&"layerfs_operation_deltas"));
    assert!(!FULL_SCHEMA.table_names.contains(&"layerfs_layer_deltas"));
    assert!(!FULL_SCHEMA.table_names.contains(&"layerfs_push_outbox"));
}

#[test]
fn working_ddl_executes_with_exact_tables_columns_fks_and_no_upstream_placeholders() {
    let connection = create_contract(WORKING_SCHEMA);
    assert_sql_shapes(&connection, WORKING_SCHEMA);
    assert!(assert_fk_targets_are_local(&connection, WORKING_SCHEMA) >= 15);
    assert_eq!(
        columns(&connection, "layerfs_store_meta"),
        [
            "store_id",
            "format_marker",
            "schema_version",
            "store_role",
            "storage_id",
            "next_inode_serial",
            "trusted_history",
            "journal_mode",
            "synchronous",
            "temp_store",
            "mmap_size",
        ]
    );
    assert_eq!(
        columns(&connection, "layerfs_working_base_bindings"),
        [
            "binding_id",
            "durable_storage_id",
            "target_kind",
            "target_id",
            "target_version_id",
            "generation",
            "root_id",
            "verification_receipt_id",
            "authority_pin_id",
            "pin_expires_at",
            "status",
        ]
    );
    assert_eq!(
        columns(&connection, "layerfs_operations")[4..7],
        ["base_kind", "base_binding_id", "base_operation_version_id"]
    );
    let version_columns = columns(&connection, "layerfs_operation_versions");
    for folded in [
        "transition_delta_id",
        "base_root_id",
        "result_root_id",
        "release_state",
        "release_generation",
        "release_request_id",
    ] {
        assert!(version_columns.contains(&folded.to_owned()));
    }
    assert!(columns(&connection, "layerfs_working_layer_candidates")
        .contains(&"expected_layer_stack_id".into()));
    for forbidden in [
        "layerfs_layer_stacks",
        "layerfs_layers",
        "layerfs_durable_tracking_refs",
        "layerfs_fetch_closure_items",
        "layerfs_sync_object_pins",
        "layerfs_sync_receipts",
    ] {
        assert!(!WORKING_SCHEMA.table_names.contains(&forbidden));
    }
    let binding_fks = connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_list('layerfs_working_base_bindings')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(binding_fks, 0);
}

#[test]
fn immutable_meta_roles_reject_wrong_family_and_authority_binding() {
    let full = create_contract(FULL_SCHEMA);
    let insert_full = |role: &str, storage: &[u8; 32], authority: &[u8; 32]| {
        full.execute(
            "INSERT INTO layerfs_store_meta
             (store_id, format_marker, schema_version, store_role, storage_id,
              durable_storage_id, next_inode_serial, trusted_history,
              journal_mode, synchronous, temp_store, mmap_size)
             VALUES (1, 'layerfs-full-sqlite', 1, ?1, ?2, ?3, 0, 0,
                     'DELETE', 2, 1, 0)",
            params![role, storage, authority],
        )
    };
    assert!(insert_full("working", &[1; 32], &[1; 32]).is_err());
    assert!(insert_full("durable", &[1; 32], &[2; 32]).is_err());
    assert_eq!(insert_full("durable", &[1; 32], &[1; 32]).unwrap(), 1);

    let cache = create_contract(FULL_SCHEMA);
    assert!(cache
        .execute(
            "INSERT INTO layerfs_store_meta
             (store_id, format_marker, schema_version, store_role, storage_id,
              durable_storage_id, next_inode_serial, trusted_history,
              journal_mode, synchronous, temp_store, mmap_size)
             VALUES (1, 'layerfs-full-sqlite', 1, 'durable_cache', ?1, ?1,
                     0, 0, 'DELETE', 2, 1, 0)",
            params![[3_u8; 32]],
        )
        .is_err());
    cache
        .execute(
            "INSERT INTO layerfs_store_meta
             (store_id, format_marker, schema_version, store_role, storage_id,
              durable_storage_id, next_inode_serial, trusted_history,
              journal_mode, synchronous, temp_store, mmap_size)
             VALUES (1, 'layerfs-full-sqlite', 1, 'durable_cache', ?1, ?2,
                     0, 0, 'DELETE', 2, 1, 0)",
            params![[3_u8; 32], [4_u8; 32]],
        )
        .unwrap();

    let working = create_contract(WORKING_SCHEMA);
    assert!(working
        .execute(
            "INSERT INTO layerfs_store_meta
             (store_id, format_marker, schema_version, store_role, storage_id,
              next_inode_serial, trusted_history, journal_mode, synchronous,
              temp_store, mmap_size)
             VALUES (1, 'layerfs-working-sqlite', 1, 'durable', ?1,
                     0, 0, 'DELETE', 2, 1, 0)",
            params![[5_u8; 32]],
        )
        .is_err());
}

#[test]
fn exact_index_manifest_rejects_missing_wrong_or_extra_named_indexes() {
    let missing = create_contract(FULL_SCHEMA);
    missing
        .execute_batch("DROP INDEX layerfs_full_transfer_state_owner_idx")
        .unwrap();
    assert!(validate_index_schemas(&missing, FULL_SCHEMA.index_schemas).is_err());

    let wrong = create_contract(WORKING_SCHEMA);
    wrong
        .execute_batch(
            "DROP INDEX layerfs_working_version_leases_owner_idx;
             CREATE INDEX layerfs_working_version_leases_owner_idx
             ON layerfs_version_leases (owner_id, owner_kind);",
        )
        .unwrap();
    assert!(validate_index_schemas(&wrong, WORKING_SCHEMA.index_schemas).is_err());

    let extra = create_contract(FULL_SCHEMA);
    extra
        .execute_batch(
            "CREATE INDEX unregistered_index
             ON layerfs_branch_push_pages (branch_id)",
        )
        .unwrap();
    assert!(validate_index_schemas(&extra, FULL_SCHEMA.index_schemas).is_err());
}

#[test]
fn hot_owner_cleanup_and_object_lookup_queries_choose_declared_indexes() {
    let full = create_contract(FULL_SCHEMA);
    for (sql, index) in [
        (
            "EXPLAIN QUERY PLAN DELETE FROM layerfs_sync_object_pins
             WHERE owner_request_id = x'00' AND direction = 'push'",
            "layerfs_full_sync_object_pins_owner_idx",
        ),
        (
            "EXPLAIN QUERY PLAN DELETE FROM layerfs_sync_batch_receipts
             WHERE owner_request_id = x'00' AND direction = 'push'",
            "layerfs_full_sync_batch_receipts_owner_idx",
        ),
        (
            "EXPLAIN QUERY PLAN DELETE FROM layerfs_transfer_state
             WHERE owner_request_id = x'00' AND direction = 'push'",
            "layerfs_full_transfer_state_owner_idx",
        ),
        (
            "EXPLAIN QUERY PLAN DELETE FROM layerfs_version_leases
             WHERE owner_kind = 'sync' AND owner_id = x'00'",
            "layerfs_full_version_leases_owner_idx",
        ),
    ] {
        assert!(query_plan(&full, sql).contains(index), "{sql}");
    }
    assert!(query_plan(
        &full,
        "EXPLAIN QUERY PLAN SELECT canonical_bytes FROM layerfs_objects
         WHERE object_id = x'00'"
    )
    .contains("sqlite_autoindex_layerfs_objects_1"));

    let working = create_contract(WORKING_SCHEMA);
    assert!(query_plan(
        &working,
        "EXPLAIN QUERY PLAN DELETE FROM layerfs_transfer_state
         WHERE owner_request_id = x'00' AND direction = 'push'"
    )
    .contains("layerfs_working_transfer_state_owner_idx"));
    assert!(query_plan(
        &working,
        "EXPLAIN QUERY PLAN DELETE FROM layerfs_version_leases
         WHERE owner_kind = 'sync' AND owner_id = x'00'"
    )
    .contains("layerfs_working_version_leases_owner_idx"));
    assert!(query_plan(
        &working,
        "EXPLAIN QUERY PLAN SELECT canonical_bytes FROM layerfs_objects
         WHERE object_id = x'00'"
    )
    .contains("sqlite_autoindex_layerfs_objects_1"));
}
