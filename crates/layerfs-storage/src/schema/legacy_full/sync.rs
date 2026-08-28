pub(crate) const SCHEMAS: [(&str, &str); 11] = [
    (
        "layerfs_durable_storages",
        "CREATE TABLE IF NOT EXISTS layerfs_durable_storages (
            durable_storage_id BLOB PRIMARY KEY CHECK (length(durable_storage_id) = 32),
            authenticated_at INTEGER NOT NULL
        )",
    ),
    (
        "layerfs_durable_tracking_refs",
        "CREATE TABLE IF NOT EXISTS layerfs_durable_tracking_refs (
            tracking_ref_id BLOB PRIMARY KEY CHECK (length(tracking_ref_id) = 32),
            durable_storage_id BLOB NOT NULL CHECK (length(durable_storage_id) = 32),
            target_kind TEXT NOT NULL CHECK (target_kind IN ('layer', 'operation_version', 'branch')),
            target_id BLOB NOT NULL CHECK (length(target_id) = 32),
            target_version_id BLOB CHECK (target_version_id IS NULL OR length(target_version_id) = 32),
            generation INTEGER NOT NULL CHECK (generation >= 0),
            root_id BLOB NOT NULL CHECK (length(root_id) = 32),
            verification_receipt_id BLOB NOT NULL CHECK (length(verification_receipt_id) = 32),
            status TEXT NOT NULL CHECK (status IN ('verified_complete', 'evicted')),
            CHECK ((target_kind = 'branch'
                       AND ((generation = 0 AND target_version_id IS NULL)
                            OR (generation > 0 AND target_version_id IS NOT NULL)))
                   OR (target_kind != 'branch' AND target_version_id IS NULL)),
            UNIQUE(durable_storage_id, target_kind, target_id, generation),
            FOREIGN KEY(durable_storage_id)
                REFERENCES layerfs_durable_storages(durable_storage_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_push_outbox",
        "CREATE TABLE IF NOT EXISTS layerfs_push_outbox (
            request_id BLOB PRIMARY KEY CHECK (length(request_id) = 32),
            durable_storage_id BLOB NOT NULL CHECK (length(durable_storage_id) = 32),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            operation_version_id BLOB,
            accepted_generation INTEGER NOT NULL CHECK (accepted_generation >= 0),
            accepted_root_id BLOB NOT NULL CHECK (length(accepted_root_id) = 32),
            expected_head_id BLOB CHECK (
                expected_head_id IS NULL OR length(expected_head_id) = 32),
            expected_durable_generation INTEGER,
            expected_root_id BLOB CHECK (
                expected_root_id IS NULL OR length(expected_root_id) = 32),
            identity_version INTEGER,
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
            CHECK ((expected_durable_generation IS NULL
                       AND expected_head_id IS NULL AND expected_root_id IS NULL)
                   OR (expected_durable_generation IS NOT NULL
                       AND expected_durable_generation >= 0
                       AND expected_root_id IS NOT NULL)),
            CHECK ((identity_version IS NULL AND transfer_id IS NULL
                       AND candidate_digest IS NULL AND unique_bytes IS NULL
                       AND resumed_bytes IS NULL AND retransmitted_bytes IS NULL)
                   OR (identity_version = 1 AND transfer_id IS NOT NULL
                       AND candidate_digest IS NOT NULL AND unique_bytes IS NOT NULL
                       AND resumed_bytes IS NOT NULL AND retransmitted_bytes IS NOT NULL)),
            CHECK (state NOT IN ('transferred', 'accepted', 'conflict', 'indeterminate')
                   OR identity_version = 1),
            FOREIGN KEY(durable_storage_id)
                REFERENCES layerfs_durable_storages(durable_storage_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_transfer_state",
        "CREATE TABLE IF NOT EXISTS layerfs_transfer_state (
            owner_request_id BLOB NOT NULL CHECK (length(owner_request_id) = 32),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            batch_sequence INTEGER NOT NULL CHECK (batch_sequence >= 0),
            direction TEXT NOT NULL CHECK (direction IN ('fetch', 'push')),
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
        "CREATE TABLE IF NOT EXISTS layerfs_branch_push_pages (
            page_id BLOB PRIMARY KEY CHECK (length(page_id) = 32),
            transfer_id BLOB NOT NULL CHECK (length(transfer_id) = 32),
            data_request_id BLOB NOT NULL CHECK (length(data_request_id) = 32),
            page_sequence INTEGER NOT NULL CHECK (page_sequence >= 0),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            bundle BLOB NOT NULL CHECK (length(bundle) <= 1048576),
            identity_version INTEGER NOT NULL CHECK (identity_version = 1),
            page_digest BLOB NOT NULL CHECK (length(page_digest) = 32),
            unique_bytes INTEGER NOT NULL CHECK (unique_bytes >= 0),
            resumed_bytes INTEGER NOT NULL CHECK (resumed_bytes >= 0),
            retransmitted_bytes INTEGER NOT NULL CHECK (retransmitted_bytes >= 0),
            created_at INTEGER NOT NULL,
            UNIQUE(transfer_id, page_sequence)
        )",
    ),
    (
        "layerfs_sync_object_pins",
        "CREATE TABLE IF NOT EXISTS layerfs_sync_object_pins (
            owner_request_id BLOB NOT NULL CHECK (length(owner_request_id) = 32),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            direction TEXT NOT NULL CHECK (direction IN ('fetch', 'push')),
            object_id BLOB NOT NULL CHECK (length(object_id) = 32),
            created_at INTEGER NOT NULL,
            PRIMARY KEY(request_id, direction, object_id),
            FOREIGN KEY(object_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_sync_batch_receipts",
        "CREATE TABLE IF NOT EXISTS layerfs_sync_batch_receipts (
            batch_id BLOB PRIMARY KEY CHECK (length(batch_id) = 32),
            owner_request_id BLOB NOT NULL CHECK (length(owner_request_id) = 32),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            direction TEXT NOT NULL CHECK (direction IN ('fetch', 'push')),
            object_count INTEGER NOT NULL CHECK (object_count > 0),
            canonical_bytes INTEGER NOT NULL CHECK (canonical_bytes > 0),
            created_at INTEGER NOT NULL
        )",
    ),
    (
        "layerfs_fetch_closure_items",
        "CREATE TABLE IF NOT EXISTS layerfs_fetch_closure_items (
            closure_id BLOB NOT NULL CHECK (length(closure_id) = 32),
            object_id BLOB NOT NULL CHECK (length(object_id) = 32),
            created_at INTEGER NOT NULL,
            PRIMARY KEY(closure_id, object_id),
            FOREIGN KEY(object_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_fetch_staging_heads",
        "CREATE TABLE IF NOT EXISTS layerfs_fetch_staging_heads (
            target_kind TEXT NOT NULL CHECK (target_kind IN ('branch', 'layer_stack')),
            target_id BLOB NOT NULL CHECK (length(target_id) = 32),
            durable_storage_id BLOB NOT NULL CHECK (length(durable_storage_id) = 32),
            published_generation INTEGER CHECK (published_generation >= 0),
            published_version_id BLOB CHECK (
                published_version_id IS NULL OR length(published_version_id) = 32),
            published_root_id BLOB CHECK (
                published_root_id IS NULL OR length(published_root_id) = 32),
            staged_generation INTEGER NOT NULL CHECK (staged_generation >= 0),
            staged_version_id BLOB CHECK (
                staged_version_id IS NULL OR length(staged_version_id) = 32),
            staged_root_id BLOB NOT NULL CHECK (length(staged_root_id) = 32),
            updated_at INTEGER NOT NULL,
            PRIMARY KEY(target_kind, target_id),
            CHECK ((published_generation IS NULL
                       AND published_version_id IS NULL AND published_root_id IS NULL)
                   OR (published_generation IS NOT NULL AND published_root_id IS NOT NULL))
        )",
    ),
    (
        "layerfs_released_versions",
        "CREATE TABLE IF NOT EXISTS layerfs_released_versions (
            target_kind TEXT NOT NULL CHECK (target_kind IN ('layer', 'operation_version')),
            owner_id BLOB NOT NULL CHECK (length(owner_id) = 32),
            version_id BLOB NOT NULL CHECK (length(version_id) = 32),
            root_id BLOB NOT NULL CHECK (length(root_id) = 32),
            release_generation INTEGER NOT NULL CHECK (release_generation > 0),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            PRIMARY KEY(target_kind, owner_id, version_id)
        )",
    ),
    (
        "layerfs_sync_receipts",
        "CREATE TABLE IF NOT EXISTS layerfs_sync_receipts (
            request_id BLOB PRIMARY KEY CHECK (length(request_id) = 32),
            durable_storage_id BLOB NOT NULL CHECK (length(durable_storage_id) = 32),
            direction TEXT NOT NULL CHECK (direction IN ('fetch', 'push')),
            candidate_kind TEXT NOT NULL CHECK (candidate_kind IN
                ('layer', 'operation_version', 'branch')),
            candidate_id BLOB NOT NULL CHECK (length(candidate_id) = 32),
            identity_version INTEGER,
            transfer_id BLOB CHECK (transfer_id IS NULL OR length(transfer_id) = 32),
            candidate_digest BLOB CHECK (
                candidate_digest IS NULL OR length(candidate_digest) = 32),
            expected_head_id BLOB,
            expected_generation INTEGER,
            expected_root_id BLOB CHECK (
                expected_root_id IS NULL OR length(expected_root_id) = 32),
            accepted_head_id BLOB,
            accepted_generation INTEGER,
            accepted_root_id BLOB,
            result TEXT NOT NULL CHECK (result IN
                ('fetched', 'durably_accepted', 'conflict', 'indeterminate')),
            unique_bytes INTEGER NOT NULL CHECK (unique_bytes >= 0),
            resumed_bytes INTEGER NOT NULL CHECK (resumed_bytes >= 0),
            retransmitted_bytes INTEGER NOT NULL CHECK (retransmitted_bytes >= 0),
            reconciliation_result TEXT,
            CHECK ((direction = 'push' AND identity_version = 1
                       AND transfer_id IS NOT NULL AND candidate_digest IS NOT NULL)
                   OR (direction = 'fetch' AND identity_version IS NULL
                       AND transfer_id IS NULL AND candidate_digest IS NULL)),
            CHECK ((result IN ('fetched', 'durably_accepted')
                       AND accepted_generation IS NOT NULL
                       AND accepted_root_id IS NOT NULL)
                   OR (result NOT IN ('fetched', 'durably_accepted')
                       AND accepted_head_id IS NULL
                       AND accepted_generation IS NULL
                       AND accepted_root_id IS NULL)),
            FOREIGN KEY(durable_storage_id)
                REFERENCES layerfs_durable_storages(durable_storage_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
];
