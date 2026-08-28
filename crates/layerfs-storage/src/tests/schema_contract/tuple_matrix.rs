use super::*;
use rusqlite::{params, Connection};

fn id(serial: u16) -> [u8; 32] {
    let mut value = [0_u8; 32];
    value[..2].copy_from_slice(&serial.to_be_bytes());
    value
}

fn blob(present: bool, seed: u16) -> Option<Vec<u8>> {
    present.then(|| id(seed).to_vec())
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
            params![id(1), id(2), id(3), id(4), id(5), id(6)],
        )
        .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn insert_working_outbox(
    connection: &Connection,
    serial: u16,
    expected_head: Option<Vec<u8>>,
    expected_generation: Option<i64>,
    expected_root: Option<Vec<u8>>,
    outcome: Option<&str>,
    outcome_present: Option<i64>,
    outcome_head: Option<Vec<u8>>,
    outcome_generation: Option<i64>,
    outcome_root: Option<Vec<u8>>,
) -> bool {
    let terminal = outcome.is_some();
    let state = match outcome {
        Some("durably_accepted") => "accepted",
        Some("conflict") => "conflict",
        Some("indeterminate") => "indeterminate",
        None => "selected",
        Some(_) => unreachable!(),
    };
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
             VALUES (?1, ?2, ?3, ?4, NULL, 0, ?5, ?6, ?7, ?8,
                     ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16,
                     ?17, ?18, ?19, ?20, NULL)",
            params![
                id(serial),
                id(1),
                id(7),
                id(8),
                id(9),
                expected_head,
                expected_generation,
                expected_root,
                terminal.then_some(1_i64),
                terminal.then(|| id(10).to_vec()),
                terminal.then(|| id(11).to_vec()),
                terminal.then_some(0_i64),
                terminal.then_some(0_i64),
                terminal.then_some(0_i64),
                state,
                outcome,
                outcome_present,
                outcome_head,
                outcome_generation,
                outcome_root,
            ],
        )
        .is_ok()
}

fn valid_branch_expected(head: bool, generation: Option<i64>, root: bool) -> bool {
    matches!(
        (head, generation, root),
        (false, None, false) | (false, Some(0), true)
    ) || matches!((head, generation, root), (true, Some(value), true) if value > 0)
}

fn valid_branch_present(head: bool, generation: Option<i64>, root: bool) -> bool {
    matches!((head, generation, root), (false, Some(0), true))
        || matches!((head, generation, root), (true, Some(value), true) if value > 0)
}

#[test]
fn working_push_outbox_enforces_every_expected_and_outcome_branch_tuple() {
    let connection = create_contract(WORKING_SCHEMA);
    insert_working_binding(&connection);
    let mut serial = 100_u16;
    for generation in [None, Some(0), Some(1)] {
        for head in [false, true] {
            for root in [false, true] {
                let accepted = insert_working_outbox(
                    &connection,
                    serial,
                    blob(head, 12),
                    generation,
                    blob(root, 13),
                    None,
                    None,
                    None,
                    None,
                    None,
                );
                assert_eq!(accepted, valid_branch_expected(head, generation, root));
                serial += 1;
            }
        }
    }

    for outcome in [
        None,
        Some("durably_accepted"),
        Some("conflict"),
        Some("indeterminate"),
    ] {
        for present in [None, Some(0), Some(1)] {
            for generation in [None, Some(0), Some(1)] {
                for head in [false, true] {
                    for root in [false, true] {
                        let exact =
                            present == Some(1) && valid_branch_present(head, generation, root);
                        let absent = present == Some(0) && generation.is_none() && !head && !root;
                        let expected = match outcome {
                            None => present.is_none() && generation.is_none() && !head && !root,
                            Some("durably_accepted") => exact,
                            Some("conflict") => exact || absent,
                            Some("indeterminate") => absent,
                            Some(_) => unreachable!(),
                        };
                        let accepted = insert_working_outbox(
                            &connection,
                            serial,
                            None,
                            None,
                            None,
                            outcome,
                            present,
                            blob(head, 14),
                            generation,
                            blob(root, 15),
                        );
                        assert_eq!(
                            accepted, expected,
                            "{outcome:?} {present:?} {generation:?} {head} {root}"
                        );
                        serial += 1;
                    }
                }
            }
        }
    }
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
            params![id(20)],
        )
        .unwrap();
}

#[allow(clippy::too_many_arguments)]
fn insert_full_receipt(
    connection: &Connection,
    serial: u16,
    candidate_kind: &str,
    expected_head: Option<Vec<u8>>,
    expected_generation: Option<i64>,
    expected_root: Option<Vec<u8>>,
    result: &str,
    decided_present: i64,
    decided_head: Option<Vec<u8>>,
    decided_generation: Option<i64>,
    decided_root: Option<Vec<u8>>,
) -> bool {
    let direction = match result {
        "fetched" => "fetch",
        "verified_complete" => "prepare",
        _ => "push",
    };
    let push = direction == "push";
    let reconciliation_result = match (direction, result) {
        (_, "indeterminate") => None,
        ("push", _) => Some("exact"),
        ("fetch" | "prepare", _) => Some("verified_complete"),
        _ => unreachable!(),
    };
    connection
        .execute(
            "INSERT INTO layerfs_sync_receipts
             (request_id, authority_storage_id, direction, candidate_kind, candidate_id,
              identity_version, transfer_id, candidate_digest,
              expected_head_id, expected_generation, expected_root_id,
              decided_head_present, decided_head_id, decided_generation, decided_root_id,
              result, unique_bytes, resumed_bytes, retransmitted_bytes,
              reconciliation_result)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                     ?11, ?12, ?13, ?14, ?15, ?16, 0, 0, 0, ?17)",
            params![
                id(serial),
                id(20),
                direction,
                candidate_kind,
                id(21),
                push.then_some(1_i64),
                push.then(|| id(22).to_vec()),
                push.then(|| id(23).to_vec()),
                expected_head,
                expected_generation,
                expected_root,
                decided_present,
                decided_head,
                decided_generation,
                decided_root,
                result,
                reconciliation_result,
            ],
        )
        .is_ok()
}

fn valid_full_expected(kind: &str, head: bool, generation: Option<i64>, root: bool) -> bool {
    if kind == "branch" {
        valid_branch_expected(head, generation, root)
    } else {
        head && generation.is_some() && root
    }
}

fn valid_full_present(kind: &str, head: bool, generation: Option<i64>, root: bool) -> bool {
    if kind == "branch" {
        valid_branch_present(head, generation, root)
    } else {
        head && generation.is_some() && root
    }
}

#[test]
fn full_sync_receipts_enforce_candidate_specific_expected_and_decided_tuples() {
    let connection = create_contract(FULL_SCHEMA);
    insert_full_meta(&connection);
    let mut serial = 1_000_u16;
    for kind in ["branch", "layer", "operation_version"] {
        for generation in [None, Some(0), Some(1)] {
            for head in [false, true] {
                for root in [false, true] {
                    let (result, decided_head) = if kind == "branch" {
                        ("fetched", None)
                    } else {
                        ("verified_complete", Some(id(24).to_vec()))
                    };
                    let accepted = insert_full_receipt(
                        &connection,
                        serial,
                        kind,
                        blob(head, 25),
                        generation,
                        blob(root, 26),
                        result,
                        1,
                        decided_head,
                        Some(0),
                        Some(id(27).to_vec()),
                    );
                    assert_eq!(accepted, valid_full_expected(kind, head, generation, root));
                    serial += 1;
                }
            }
        }
    }

    for kind in ["branch", "layer", "operation_version"] {
        for result in [
            "fetched",
            "durably_accepted",
            "conflict",
            "indeterminate",
            "verified_complete",
        ] {
            for present in [0, 1] {
                for generation in [None, Some(0), Some(1)] {
                    for head in [false, true] {
                        for root in [false, true] {
                            let exact =
                                present == 1 && valid_full_present(kind, head, generation, root);
                            let absent = present == 0 && generation.is_none() && !head && !root;
                            let expected = match result {
                                "fetched" | "durably_accepted" | "verified_complete" => exact,
                                "conflict" => exact || kind == "branch" && absent,
                                "indeterminate" => absent,
                                _ => unreachable!(),
                            };
                            let (expected_head, expected_generation, expected_root) =
                                if kind == "branch" {
                                    (None, None, None)
                                } else {
                                    (Some(id(28).to_vec()), Some(0), Some(id(29).to_vec()))
                                };
                            let accepted = insert_full_receipt(
                                &connection,
                                serial,
                                kind,
                                expected_head,
                                expected_generation,
                                expected_root,
                                result,
                                present,
                                blob(head, 30),
                                generation,
                                blob(root, 31),
                            );
                            assert_eq!(
                                accepted, expected,
                                "{kind} {result} {present} {generation:?} {head} {root}"
                            );
                            serial += 1;
                        }
                    }
                }
            }
        }
    }
}
