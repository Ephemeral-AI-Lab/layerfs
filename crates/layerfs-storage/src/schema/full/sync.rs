//! Full tracking, transfer custody, closure membership, and receipt DDL.

pub(super) const SCHEMAS: [(&str, &str); 7] = [
    (
        "layerfs_durable_tracking_refs",
        "CREATE TABLE layerfs_durable_tracking_refs (
            tracking_ref_id BLOB PRIMARY KEY CHECK (length(tracking_ref_id) = 32),
            store_id INTEGER NOT NULL DEFAULT 1 CHECK (store_id = 1),
            target_kind TEXT NOT NULL CHECK (target_kind IN
                ('layer', 'operation_version', 'branch')),
            target_id BLOB NOT NULL CHECK (length(target_id) = 32),
            target_version_id BLOB CHECK (
                target_version_id IS NULL OR length(target_version_id) = 32),
            generation INTEGER NOT NULL CHECK (generation >= 0),
            root_id BLOB CHECK (root_id IS NULL OR length(root_id) = 32),
            verification_receipt_id BLOB CHECK (
                verification_receipt_id IS NULL OR length(verification_receipt_id) = 32),
            status TEXT NOT NULL CHECK (status IN ('verified_complete', 'evicted', 'invalid')),
            CHECK ((status = 'verified_complete' AND root_id IS NOT NULL
                       AND verification_receipt_id IS NOT NULL)
                   OR (status IN ('evicted', 'invalid') AND root_id IS NULL
                       AND verification_receipt_id IS NULL)),
            CHECK ((target_kind = 'branch'
                       AND ((generation = 0 AND target_version_id IS NULL)
                         OR (generation > 0 AND target_version_id IS NOT NULL)))
                   OR (target_kind != 'branch' AND target_version_id IS NULL)),
            UNIQUE(target_kind, target_id, generation),
            FOREIGN KEY(store_id) REFERENCES layerfs_store_meta(store_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(verification_receipt_id) REFERENCES layerfs_sync_receipts(request_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_transfer_state",
        "CREATE TABLE layerfs_transfer_state (
            owner_request_id BLOB NOT NULL CHECK (length(owner_request_id) = 32),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            batch_sequence INTEGER NOT NULL CHECK (batch_sequence >= 0),
            direction TEXT NOT NULL CHECK (direction IN ('fetch', 'push', 'prepare')),
            cursor BLOB,
            state TEXT NOT NULL CHECK (state IN ('negotiating', 'transferring', 'complete')),
            unique_bytes INTEGER NOT NULL CHECK (unique_bytes >= 0),
            resumed_bytes INTEGER NOT NULL CHECK (resumed_bytes >= 0),
            retransmitted_bytes INTEGER NOT NULL CHECK (retransmitted_bytes >= 0),
            PRIMARY KEY(request_id, batch_sequence)
        )",
    ),
    (
        "layerfs_branch_push_pages",
        "CREATE TABLE layerfs_branch_push_pages (
            page_id BLOB PRIMARY KEY CHECK (length(page_id) = 32),
            transfer_id BLOB NOT NULL CHECK (length(transfer_id) = 32),
            data_request_id BLOB NOT NULL CHECK (length(data_request_id) = 32),
            page_sequence INTEGER NOT NULL CHECK (page_sequence >= 0),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            origin_authority_storage_id BLOB NOT NULL CHECK (
                length(origin_authority_storage_id) = 32),
            origin_target_kind TEXT NOT NULL CHECK (
                origin_target_kind IN ('branch', 'layer_stack')),
            origin_target_id BLOB NOT NULL CHECK (length(origin_target_id) = 32),
            origin_version_id BLOB CHECK (
                origin_version_id IS NULL OR length(origin_version_id) = 32),
            origin_generation INTEGER NOT NULL CHECK (origin_generation >= 0),
            origin_root_id BLOB NOT NULL CHECK (length(origin_root_id) = 32),
            origin_verification_receipt_id BLOB NOT NULL CHECK (
                length(origin_verification_receipt_id) = 32),
            origin_pin_id BLOB NOT NULL CHECK (length(origin_pin_id) = 32),
            bundle BLOB NOT NULL CHECK (length(bundle) <= 1048576),
            identity_version INTEGER NOT NULL CHECK (identity_version = 1),
            page_digest BLOB NOT NULL CHECK (length(page_digest) = 32),
            unique_bytes INTEGER NOT NULL CHECK (unique_bytes >= 0),
            resumed_bytes INTEGER NOT NULL CHECK (resumed_bytes >= 0),
            retransmitted_bytes INTEGER NOT NULL CHECK (retransmitted_bytes >= 0),
            created_at INTEGER NOT NULL,
            UNIQUE(transfer_id, page_sequence),
            CHECK ((origin_target_kind = 'branch'
                       AND ((origin_generation = 0 AND origin_version_id IS NULL)
                         OR (origin_generation > 0 AND origin_version_id IS NOT NULL)))
                   OR (origin_target_kind = 'layer_stack' AND origin_version_id IS NOT NULL)),
            FOREIGN KEY(origin_authority_storage_id)
                REFERENCES layerfs_store_meta(durable_storage_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(origin_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(origin_verification_receipt_id)
                REFERENCES layerfs_sync_receipts(request_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(origin_pin_id) REFERENCES layerfs_version_leases(lease_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_sync_object_pins",
        "CREATE TABLE layerfs_sync_object_pins (
            owner_request_id BLOB NOT NULL CHECK (length(owner_request_id) = 32),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            direction TEXT NOT NULL CHECK (direction IN ('fetch', 'push', 'prepare')),
            object_id BLOB NOT NULL CHECK (length(object_id) = 32),
            created_at INTEGER NOT NULL,
            PRIMARY KEY(request_id, direction, object_id),
            FOREIGN KEY(object_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_sync_batch_receipts",
        "CREATE TABLE layerfs_sync_batch_receipts (
            batch_id BLOB PRIMARY KEY CHECK (length(batch_id) = 32),
            owner_request_id BLOB NOT NULL CHECK (length(owner_request_id) = 32),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            direction TEXT NOT NULL CHECK (direction IN ('fetch', 'push', 'prepare')),
            object_count INTEGER NOT NULL CHECK (object_count > 0),
            canonical_bytes INTEGER NOT NULL CHECK (canonical_bytes > 0),
            created_at INTEGER NOT NULL
        )",
    ),
    (
        "layerfs_fetch_closure_items",
        "CREATE TABLE layerfs_fetch_closure_items (
            tracking_ref_id BLOB NOT NULL CHECK (length(tracking_ref_id) = 32),
            object_id BLOB NOT NULL CHECK (length(object_id) = 32),
            created_at INTEGER NOT NULL,
            PRIMARY KEY(tracking_ref_id, object_id),
            FOREIGN KEY(tracking_ref_id)
                REFERENCES layerfs_durable_tracking_refs(tracking_ref_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(object_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_sync_receipts",
        "CREATE TABLE layerfs_sync_receipts (
            request_id BLOB PRIMARY KEY CHECK (length(request_id) = 32),
            authority_storage_id BLOB NOT NULL CHECK (length(authority_storage_id) = 32),
            direction TEXT NOT NULL CHECK (direction IN ('fetch', 'push', 'prepare')),
            candidate_kind TEXT NOT NULL CHECK (candidate_kind IN
                ('layer', 'operation_version', 'branch')),
            candidate_id BLOB NOT NULL CHECK (length(candidate_id) = 32),
            identity_version INTEGER,
            transfer_id BLOB CHECK (transfer_id IS NULL OR length(transfer_id) = 32),
            candidate_digest BLOB CHECK (
                candidate_digest IS NULL OR length(candidate_digest) = 32),
            expected_head_id BLOB CHECK (
                expected_head_id IS NULL OR length(expected_head_id) = 32),
            expected_generation INTEGER CHECK (
                expected_generation IS NULL OR expected_generation >= 0),
            expected_root_id BLOB CHECK (
                expected_root_id IS NULL OR length(expected_root_id) = 32),
            decided_head_present INTEGER NOT NULL CHECK (decided_head_present IN (0, 1)),
            decided_head_id BLOB CHECK (
                decided_head_id IS NULL OR length(decided_head_id) = 32),
            decided_generation INTEGER CHECK (
                decided_generation IS NULL OR decided_generation >= 0),
            decided_root_id BLOB CHECK (
                decided_root_id IS NULL OR length(decided_root_id) = 32),
            result TEXT NOT NULL CHECK (result IN
                ('fetched', 'durably_accepted', 'conflict', 'indeterminate',
                 'verified_complete')),
            unique_bytes INTEGER NOT NULL CHECK (unique_bytes >= 0),
            resumed_bytes INTEGER NOT NULL CHECK (resumed_bytes >= 0),
            retransmitted_bytes INTEGER NOT NULL CHECK (retransmitted_bytes >= 0),
            reconciliation_result TEXT,
            CHECK ((direction = 'push'
                       AND result IN ('durably_accepted', 'conflict', 'indeterminate'))
                   OR (direction = 'fetch' AND result IN ('fetched', 'indeterminate'))
                   OR (direction = 'prepare'
                       AND result IN ('verified_complete', 'indeterminate'))),
            CHECK ((result = 'indeterminate' AND reconciliation_result IS NULL)
                   OR (direction = 'push' AND result != 'indeterminate'
                       AND reconciliation_result IS NOT NULL
                       AND reconciliation_result IN ('exact', 'ordered_replay'))
                   OR (direction IN ('fetch', 'prepare') AND result != 'indeterminate'
                       AND reconciliation_result IS NOT NULL
                       AND reconciliation_result = 'verified_complete')),
            CHECK ((direction = 'push' AND identity_version = 1
                       AND transfer_id IS NOT NULL AND candidate_digest IS NOT NULL)
                   OR (direction != 'push' AND identity_version IS NULL
                       AND transfer_id IS NULL AND candidate_digest IS NULL)),
            CHECK ((candidate_kind = 'branch' AND
                       ((expected_generation IS NULL AND expected_head_id IS NULL
                         AND expected_root_id IS NULL)
                        OR (expected_generation IS NOT NULL
                            AND expected_generation = 0 AND expected_head_id IS NULL
                            AND expected_root_id IS NOT NULL)
                        OR (expected_generation IS NOT NULL
                            AND expected_generation > 0 AND expected_head_id IS NOT NULL
                            AND expected_root_id IS NOT NULL)))
                   OR (candidate_kind IN ('layer', 'operation_version')
                       AND expected_head_id IS NOT NULL
                       AND expected_generation IS NOT NULL
                       AND expected_root_id IS NOT NULL)),
            CHECK ((result IN ('fetched', 'durably_accepted', 'verified_complete')
                       AND decided_head_present = 1
                       AND decided_generation IS NOT NULL AND decided_root_id IS NOT NULL
                       AND ((candidate_kind = 'branch'
                               AND ((decided_generation = 0 AND decided_head_id IS NULL)
                                 OR (decided_generation > 0
                                     AND decided_head_id IS NOT NULL)))
                            OR (candidate_kind IN ('layer', 'operation_version')
                                AND decided_head_id IS NOT NULL)))
                   OR (result = 'conflict' AND
                       ((candidate_kind = 'branch' AND decided_head_present = 0
                         AND decided_head_id IS NULL AND decided_generation IS NULL
                         AND decided_root_id IS NULL)
                        OR (decided_head_present = 1
                            AND decided_generation IS NOT NULL
                            AND decided_root_id IS NOT NULL
                            AND ((candidate_kind = 'branch'
                                    AND ((decided_generation = 0
                                          AND decided_head_id IS NULL)
                                      OR (decided_generation > 0
                                          AND decided_head_id IS NOT NULL)))
                                 OR (candidate_kind IN ('layer', 'operation_version')
                                     AND decided_head_id IS NOT NULL)))))
                   OR (result = 'indeterminate' AND decided_head_present = 0
                       AND decided_head_id IS NULL AND decided_generation IS NULL
                       AND decided_root_id IS NULL)),
            FOREIGN KEY(authority_storage_id)
                REFERENCES layerfs_store_meta(durable_storage_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
];
