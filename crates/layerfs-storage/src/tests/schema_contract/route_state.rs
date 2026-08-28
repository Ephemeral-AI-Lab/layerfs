use super::*;
use rusqlite::{params, Connection};

fn id(serial: u16) -> [u8; 32] {
    let mut value = [0_u8; 32];
    value[..2].copy_from_slice(&serial.to_be_bytes());
    value
}

fn insert_full_meta(connection: &Connection) {
    connection
        .execute(
            "INSERT INTO layerfs_store_meta
             (store_id, format_marker, schema_version, store_role, storage_id,
              durable_storage_id, next_inode_serial, trusted_history,
              journal_mode, synchronous, temp_store, mmap_size)
             VALUES (1, 'layerfs-full-sqlite', 1, 'durable', ?1, ?1, 0, 0,
                     'DELETE', 2, 1, 0)",
            params![id(1)],
        )
        .unwrap();
}

fn insert_full_route(
    connection: &Connection,
    serial: u16,
    direction: &str,
    result: &str,
    reconciliation: Option<&str>,
) -> bool {
    let push = direction == "push";
    let decided = matches!(result, "fetched" | "durably_accepted" | "verified_complete");
    connection
        .execute(
            "INSERT INTO layerfs_sync_receipts
             (request_id, authority_storage_id, direction, candidate_kind, candidate_id,
              identity_version, transfer_id, candidate_digest,
              expected_head_id, expected_generation, expected_root_id,
              decided_head_present, decided_head_id, decided_generation, decided_root_id,
              result, unique_bytes, resumed_bytes, retransmitted_bytes,
              reconciliation_result)
             VALUES (?1, ?2, ?3, 'branch', ?4, ?5, ?6, ?7,
                     NULL, NULL, NULL, ?8, NULL, ?9, ?10, ?11, 0, 0, 0, ?12)",
            params![
                id(serial),
                id(1),
                direction,
                id(2),
                push.then_some(1_i64),
                push.then(|| id(3).to_vec()),
                push.then(|| id(4).to_vec()),
                i64::from(decided),
                decided.then_some(0_i64),
                decided.then(|| id(5).to_vec()),
                result,
                reconciliation,
            ],
        )
        .is_ok()
}

fn valid_full_route(direction: &str, result: &str, reconciliation: Option<&str>) -> bool {
    matches!(
        (direction, result, reconciliation),
        (
            "push",
            "durably_accepted" | "conflict",
            Some("exact" | "ordered_replay")
        ) | ("fetch", "fetched", Some("verified_complete"))
            | ("prepare", "verified_complete", Some("verified_complete"))
            | ("push" | "fetch" | "prepare", "indeterminate", None)
    )
}

#[test]
fn full_receipts_reject_every_direction_result_and_reconciliation_mismatch() {
    let connection = create_contract(FULL_SCHEMA);
    insert_full_meta(&connection);
    let mut serial = 100_u16;
    for direction in ["push", "fetch", "prepare"] {
        for result in [
            "fetched",
            "durably_accepted",
            "conflict",
            "indeterminate",
            "verified_complete",
        ] {
            for reconciliation in [
                None,
                Some("exact"),
                Some("ordered_replay"),
                Some("verified_complete"),
                Some("unexpected"),
            ] {
                let accepted =
                    insert_full_route(&connection, serial, direction, result, reconciliation);
                assert_eq!(
                    accepted,
                    valid_full_route(direction, result, reconciliation),
                    "{direction} {result} {reconciliation:?}"
                );
                serial += 1;
            }
        }
    }
}

fn insert_working_binding(connection: &Connection) {
    connection
        .execute(
            "INSERT INTO layerfs_working_base_bindings
             (binding_id, durable_storage_id, target_kind, target_id,
              target_version_id, generation, root_id, verification_receipt_id,
              authority_pin_id, pin_expires_at, status)
             VALUES (?1, ?2, 'branch', ?3, NULL, 0, ?4, ?5, ?6, NULL,
                     'external_pinned')",
            params![id(10), id(11), id(12), id(13), id(14), id(15)],
        )
        .unwrap();
}

fn insert_working_state(
    connection: &Connection,
    serial: u16,
    state: &str,
    outcome: Option<&str>,
) -> bool {
    let accepted = outcome == Some("durably_accepted");
    connection
        .execute(
            "INSERT INTO layerfs_push_outbox
             (request_id, origin_base_binding_id, branch_id, durable_branch_id,
              candidate_operation_version_id, candidate_generation, candidate_root_id,
              expected_head_id, expected_generation, expected_root_id,
              identity_version, transfer_id, candidate_digest,
              unique_bytes, resumed_bytes, retransmitted_bytes, state, outcome,
              outcome_head_present, outcome_head_id, outcome_generation, outcome_root_id,
              reconciliation_result)
             VALUES (?1, ?2, ?3, ?4, NULL, 0, ?5, NULL, NULL, NULL,
                     1, ?6, ?7, 0, 0, 0, ?8, ?9, ?10, NULL, ?11, ?12, NULL)",
            params![
                id(serial),
                id(10),
                id(16),
                id(17),
                id(18),
                id(19),
                id(20),
                state,
                outcome,
                outcome.map(|_| i64::from(accepted)),
                accepted.then_some(0_i64),
                accepted.then(|| id(21).to_vec()),
            ],
        )
        .is_ok()
}

fn valid_working_state(state: &str, outcome: Option<&str>) -> bool {
    matches!(
        (state, outcome),
        ("selected" | "transferring" | "transferred", None)
            | ("accepted", Some("durably_accepted"))
            | ("conflict", Some("conflict"))
            | ("indeterminate", Some("indeterminate"))
    )
}

#[test]
fn working_outbox_rejects_every_state_outcome_mismatch() {
    let connection = create_contract(WORKING_SCHEMA);
    insert_working_binding(&connection);
    let mut serial = 300_u16;
    for state in [
        "selected",
        "transferring",
        "transferred",
        "accepted",
        "conflict",
        "indeterminate",
    ] {
        for outcome in [
            None,
            Some("durably_accepted"),
            Some("conflict"),
            Some("indeterminate"),
        ] {
            assert_eq!(
                insert_working_state(&connection, serial, state, outcome),
                valid_working_state(state, outcome),
                "{state} {outcome:?}"
            );
            serial += 1;
        }
    }
}

#[test]
fn evicted_cache_tracking_releases_membership_payload_and_receipt_before_header_delete() {
    let connection = create_contract(FULL_SCHEMA);
    insert_full_meta(&connection);
    connection.execute_batch("BEGIN").unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_objects
             (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, 1, 1, x'00')",
            params![id(400)],
        )
        .unwrap();
    assert!(insert_full_route(
        &connection,
        401,
        "prepare",
        "verified_complete",
        Some("verified_complete")
    ));
    connection
        .execute(
            "INSERT INTO layerfs_durable_tracking_refs
             (tracking_ref_id, target_kind, target_id, target_version_id, generation,
              root_id, verification_receipt_id, status)
             VALUES (?1, 'branch', ?2, NULL, 0, ?3, ?4, 'verified_complete')",
            params![id(402), id(403), id(400), id(401)],
        )
        .unwrap();
    connection
        .execute(
            "INSERT INTO layerfs_fetch_closure_items
             (tracking_ref_id, object_id, created_at) VALUES (?1, ?2, 0)",
            params![id(402), id(400)],
        )
        .unwrap();

    assert!(connection
        .execute(
            "UPDATE layerfs_durable_tracking_refs SET status = 'evicted'
             WHERE tracking_ref_id = ?1",
            params![id(402)],
        )
        .is_err());
    connection
        .execute(
            "DELETE FROM layerfs_fetch_closure_items WHERE tracking_ref_id = ?1",
            params![id(402)],
        )
        .unwrap();
    connection
        .execute(
            "UPDATE layerfs_durable_tracking_refs
             SET status = 'evicted', root_id = NULL, verification_receipt_id = NULL
             WHERE tracking_ref_id = ?1",
            params![id(402)],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM layerfs_sync_receipts WHERE request_id = ?1",
            params![id(401)],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM layerfs_objects WHERE object_id = ?1",
            params![id(400)],
        )
        .unwrap();
    connection
        .execute(
            "DELETE FROM layerfs_durable_tracking_refs WHERE tracking_ref_id = ?1",
            params![id(402)],
        )
        .unwrap();
    connection.execute_batch("COMMIT").unwrap();
}
