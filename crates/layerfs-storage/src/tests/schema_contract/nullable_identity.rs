use super::*;
use rusqlite::params;

fn id(seed: u8) -> [u8; 32] {
    [seed; 32]
}

fn assert_full_tracking_duplicate_rejected(
    generation: i64,
    first_version: Option<[u8; 32]>,
    second_version: Option<[u8; 32]>,
) {
    let connection = create_contract(FULL_SCHEMA);
    connection.execute_batch("BEGIN").unwrap();
    let insert = |tracking_ref_id, target_version_id| {
        connection.execute(
            "INSERT INTO layerfs_durable_tracking_refs
             (tracking_ref_id, target_kind, target_id, target_version_id,
              generation, root_id, verification_receipt_id, status)
             VALUES (?1, 'branch', ?2, ?3, ?4, ?5, ?6, 'verified_complete')",
            params![
                tracking_ref_id,
                id(0x11),
                target_version_id,
                generation,
                id(0x12),
                id(0x13),
            ],
        )
    };
    assert_eq!(insert(id(0x14), first_version).unwrap(), 1);
    assert!(insert(id(0x15), second_version).is_err());
    connection.execute_batch("ROLLBACK").unwrap();
}

fn assert_working_binding_duplicate_rejected(
    generation: i64,
    first_version: Option<[u8; 32]>,
    second_version: Option<[u8; 32]>,
) {
    let connection = create_contract(WORKING_SCHEMA);
    let insert = |binding_id, target_version_id| {
        connection.execute(
            "INSERT INTO layerfs_working_base_bindings
             (binding_id, durable_storage_id, target_kind, target_id,
              target_version_id, generation, root_id, verification_receipt_id,
              authority_pin_id, pin_expires_at, status)
             VALUES (?1, ?2, 'branch', ?3, ?4, ?5, ?6, ?7, ?8, NULL,
                     'external_pinned')",
            params![
                binding_id,
                id(0x21),
                id(0x22),
                target_version_id,
                generation,
                id(0x23),
                id(0x24),
                id(0x25),
            ],
        )
    };
    assert_eq!(insert(id(0x26), first_version).unwrap(), 1);
    assert!(insert(id(0x27), second_version).is_err());
}

#[test]
fn nullable_target_versions_do_not_bypass_logical_tracking_uniqueness() {
    assert_full_tracking_duplicate_rejected(0, None, None);
    assert_full_tracking_duplicate_rejected(1, Some(id(0x31)), Some(id(0x32)));
    assert_working_binding_duplicate_rejected(0, None, None);
    assert_working_binding_duplicate_rejected(1, Some(id(0x33)), Some(id(0x34)));
}
