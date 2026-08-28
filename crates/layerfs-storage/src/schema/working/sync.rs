//! Working leases, outbound Push custody, and transfer-state DDL.

pub(super) const SCHEMAS: [(&str, &str); 3] = [
    (
        "layerfs_version_leases",
        "CREATE TABLE layerfs_version_leases (
            lease_id BLOB PRIMARY KEY CHECK (length(lease_id) = 32),
            target_kind TEXT NOT NULL CHECK (target_kind IN
                ('external_base', 'operation_version')),
            binding_id BLOB CHECK (binding_id IS NULL OR length(binding_id) = 32),
            branch_id BLOB CHECK (branch_id IS NULL OR length(branch_id) = 32),
            operation_version_id BLOB CHECK (
                operation_version_id IS NULL OR length(operation_version_id) = 32),
            owner_kind TEXT NOT NULL CHECK (owner_kind IN
                ('operation_workspace', 'mount', 'materialization',
                 'layer_candidate', 'sync', 'explicit')),
            owner_id BLOB NOT NULL CHECK (length(owner_id) = 32),
            created_at INTEGER NOT NULL,
            expires_at INTEGER CHECK (expires_at IS NULL OR expires_at >= created_at),
            UNIQUE(binding_id, owner_kind, owner_id),
            UNIQUE(branch_id, operation_version_id, owner_kind, owner_id),
            CHECK ((target_kind = 'external_base' AND binding_id IS NOT NULL
                       AND branch_id IS NULL AND operation_version_id IS NULL)
                   OR (target_kind = 'operation_version' AND binding_id IS NULL
                       AND branch_id IS NOT NULL AND operation_version_id IS NOT NULL)),
            FOREIGN KEY(binding_id) REFERENCES layerfs_working_base_bindings(binding_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_push_outbox",
        "CREATE TABLE layerfs_push_outbox (
            request_id BLOB PRIMARY KEY CHECK (length(request_id) = 32),
            origin_base_binding_id BLOB NOT NULL CHECK (length(origin_base_binding_id) = 32),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            durable_branch_id BLOB NOT NULL CHECK (length(durable_branch_id) = 32),
            candidate_operation_version_id BLOB CHECK (
                candidate_operation_version_id IS NULL
                OR length(candidate_operation_version_id) = 32),
            candidate_generation INTEGER NOT NULL CHECK (candidate_generation >= 0),
            candidate_root_id BLOB NOT NULL CHECK (length(candidate_root_id) = 32),
            expected_head_id BLOB CHECK (
                expected_head_id IS NULL OR length(expected_head_id) = 32),
            expected_generation INTEGER CHECK (
                expected_generation IS NULL OR expected_generation >= 0),
            expected_root_id BLOB CHECK (
                expected_root_id IS NULL OR length(expected_root_id) = 32),
            identity_version INTEGER CHECK (identity_version IS NULL OR identity_version = 1),
            transfer_id BLOB CHECK (transfer_id IS NULL OR length(transfer_id) = 32),
            candidate_digest BLOB CHECK (
                candidate_digest IS NULL OR length(candidate_digest) = 32),
            unique_bytes INTEGER CHECK (unique_bytes IS NULL OR unique_bytes >= 0),
            resumed_bytes INTEGER CHECK (resumed_bytes IS NULL OR resumed_bytes >= 0),
            retransmitted_bytes INTEGER CHECK (
                retransmitted_bytes IS NULL OR retransmitted_bytes >= 0),
            state TEXT NOT NULL CHECK (state IN
                ('selected', 'transferring', 'transferred', 'accepted',
                 'conflict', 'indeterminate')),
            outcome TEXT CHECK (outcome IS NULL OR outcome IN
                ('durably_accepted', 'conflict', 'indeterminate')),
            outcome_head_present INTEGER CHECK (
                outcome_head_present IS NULL OR outcome_head_present IN (0, 1)),
            outcome_head_id BLOB CHECK (
                outcome_head_id IS NULL OR length(outcome_head_id) = 32),
            outcome_generation INTEGER CHECK (
                outcome_generation IS NULL OR outcome_generation >= 0),
            outcome_root_id BLOB CHECK (
                outcome_root_id IS NULL OR length(outcome_root_id) = 32),
            reconciliation_result TEXT,
            CHECK ((state IN ('selected', 'transferring', 'transferred')
                       AND outcome IS NULL)
                   OR (state = 'accepted' AND outcome IS NOT NULL
                       AND outcome = 'durably_accepted')
                   OR (state = 'conflict' AND outcome IS NOT NULL
                       AND outcome = 'conflict')
                   OR (state = 'indeterminate' AND outcome IS NOT NULL
                       AND outcome = 'indeterminate')),
            CHECK ((candidate_generation = 0 AND candidate_operation_version_id IS NULL)
                   OR (candidate_generation > 0 AND candidate_operation_version_id IS NOT NULL)),
            CHECK ((expected_generation IS NULL AND expected_head_id IS NULL
                       AND expected_root_id IS NULL)
                   OR (expected_generation IS NOT NULL
                       AND expected_generation = 0 AND expected_head_id IS NULL
                       AND expected_root_id IS NOT NULL)
                   OR (expected_generation IS NOT NULL
                       AND expected_generation > 0 AND expected_head_id IS NOT NULL
                       AND expected_root_id IS NOT NULL)),
            CHECK ((identity_version IS NULL AND transfer_id IS NULL
                       AND candidate_digest IS NULL AND unique_bytes IS NULL
                       AND resumed_bytes IS NULL AND retransmitted_bytes IS NULL)
                   OR (identity_version = 1 AND transfer_id IS NOT NULL
                       AND candidate_digest IS NOT NULL AND unique_bytes IS NOT NULL
                       AND resumed_bytes IS NOT NULL AND retransmitted_bytes IS NOT NULL)),
            CHECK (state NOT IN ('transferred', 'accepted', 'conflict', 'indeterminate')
                   OR identity_version = 1),
            CHECK ((outcome IS NULL AND outcome_head_present IS NULL
                       AND outcome_head_id IS NULL AND outcome_generation IS NULL
                       AND outcome_root_id IS NULL)
                   OR (outcome IS NOT NULL AND outcome_head_present IS NOT NULL)),
            CHECK ((outcome IS NULL AND outcome_head_present IS NULL
                       AND outcome_head_id IS NULL AND outcome_generation IS NULL
                       AND outcome_root_id IS NULL)
                   OR (outcome IS NOT NULL AND outcome = 'durably_accepted'
                       AND outcome_head_present = 1
                       AND outcome_generation IS NOT NULL AND outcome_root_id IS NOT NULL
                       AND ((outcome_generation = 0 AND outcome_head_id IS NULL)
                         OR (outcome_generation > 0 AND outcome_head_id IS NOT NULL)))
                   OR (outcome IS NOT NULL AND outcome = 'conflict' AND
                       ((outcome_head_present = 0 AND outcome_head_id IS NULL
                         AND outcome_generation IS NULL AND outcome_root_id IS NULL)
                        OR (outcome_head_present = 1 AND outcome_generation IS NOT NULL
                            AND outcome_root_id IS NOT NULL
                            AND ((outcome_generation = 0 AND outcome_head_id IS NULL)
                              OR (outcome_generation > 0
                                  AND outcome_head_id IS NOT NULL)))))
                   OR (outcome IS NOT NULL AND outcome = 'indeterminate'
                       AND outcome_head_present = 0
                       AND outcome_head_id IS NULL AND outcome_generation IS NULL
                       AND outcome_root_id IS NULL)),
            FOREIGN KEY(origin_base_binding_id)
                REFERENCES layerfs_working_base_bindings(binding_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, candidate_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_transfer_state",
        "CREATE TABLE layerfs_transfer_state (
            owner_request_id BLOB NOT NULL CHECK (length(owner_request_id) = 32),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            batch_sequence INTEGER NOT NULL CHECK (batch_sequence >= 0),
            direction TEXT NOT NULL CHECK (direction = 'push'),
            cursor BLOB,
            state TEXT NOT NULL CHECK (state IN ('negotiating', 'transferring', 'complete')),
            unique_bytes INTEGER NOT NULL CHECK (unique_bytes >= 0),
            resumed_bytes INTEGER NOT NULL CHECK (resumed_bytes >= 0),
            retransmitted_bytes INTEGER NOT NULL CHECK (retransmitted_bytes >= 0),
            PRIMARY KEY(request_id, batch_sequence)
        )",
    ),
];
