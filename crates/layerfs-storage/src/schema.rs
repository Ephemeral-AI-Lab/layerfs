pub(crate) const LEGACY_SCHEMA_VERSION: i64 = 1;
pub(crate) const SCHEMA_VERSION: i64 = 2;
pub(crate) const TRANSITION_FORMAT_VERSION: i64 = 1;

pub(crate) const BASE_SCHEMAS: [(&str, &str); 7] = [
    (
        "layerfs_store_meta",
        "CREATE TABLE IF NOT EXISTS layerfs_store_meta (
            store_id INTEGER PRIMARY KEY CHECK (store_id = 1),
            format_marker TEXT NOT NULL,
            schema_version INTEGER NOT NULL,
            journal_mode TEXT NOT NULL,
            synchronous INTEGER NOT NULL,
            temp_store INTEGER NOT NULL,
            mmap_size INTEGER NOT NULL,
            visible_root BLOB
        )",
    ),
    (
        "layerfs_objects",
        "CREATE TABLE IF NOT EXISTS layerfs_objects (
            rowid INTEGER PRIMARY KEY,
            object_id BLOB NOT NULL UNIQUE,
            kind INTEGER NOT NULL,
            canonical_length INTEGER NOT NULL,
            canonical_bytes BLOB NOT NULL
        )",
    ),
    (
        "layerfs_roots",
        "CREATE TABLE IF NOT EXISTS layerfs_roots (
            root_id BLOB PRIMARY KEY,
            directory_object BLOB NOT NULL,
            parent_root BLOB
        )",
    ),
    (
        "layerfs_deltas",
        "CREATE TABLE IF NOT EXISTS layerfs_deltas (
            delta_id BLOB PRIMARY KEY,
            format_version INTEGER NOT NULL,
            parent_root BLOB,
            child_root BLOB NOT NULL,
            payload BLOB NOT NULL
        )",
    ),
    (
        "layerfs_authority",
        "CREATE TABLE IF NOT EXISTS layerfs_authority (
            authority_id INTEGER PRIMARY KEY CHECK (authority_id = 1),
            store_id BLOB NOT NULL CHECK (length(store_id) = 32),
            next_inode_serial INTEGER NOT NULL,
            trusted_history INTEGER NOT NULL CHECK (trusted_history IN (0, 1))
        )",
    ),
    (
        "layerfs_refs",
        "CREATE TABLE IF NOT EXISTS layerfs_refs (
            name TEXT PRIMARY KEY,
            generation INTEGER NOT NULL,
            root_id BLOB NOT NULL CHECK (length(root_id) = 32)
        )",
    ),
    (
        "layerfs_retained_roots",
        "CREATE TABLE IF NOT EXISTS layerfs_retained_roots (
            root_id BLOB PRIMARY KEY CHECK (length(root_id) = 32)
        )",
    ),
];

pub(crate) const LEGACY_DELTA_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS layerfs_deltas (
    delta_id BLOB PRIMARY KEY,
    parent_root BLOB,
    child_root BLOB NOT NULL,
    payload BLOB NOT NULL
)";

pub(crate) const PRODUCT_SCHEMAS: [(&str, &str); 22] = [
    (
        "layerfs_layer_stacks",
        "CREATE TABLE IF NOT EXISTS layerfs_layer_stacks (
            layer_stack_id BLOB PRIMARY KEY CHECK (length(layer_stack_id) = 32),
            name TEXT NOT NULL UNIQUE,
            generation INTEGER NOT NULL CHECK (generation >= 0),
            head_layer_id BLOB NOT NULL CHECK (length(head_layer_id) = 32),
            UNIQUE(layer_stack_id, head_layer_id),
            FOREIGN KEY(layer_stack_id, head_layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_layers",
        "CREATE TABLE IF NOT EXISTS layerfs_layers (
            layer_id BLOB PRIMARY KEY CHECK (length(layer_id) = 32),
            layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 32),
            parent_layer_id BLOB,
            root_id BLOB NOT NULL CHECK (length(root_id) = 32),
            creation_kind TEXT NOT NULL CHECK (creation_kind IN ('genesis', 'candidate')),
            source_branch_id BLOB,
            source_branch_depth INTEGER,
            source_branch_generation INTEGER,
            source_branch_head_operation_version_id BLOB,
            source_branch_delta_id BLOB,
            state TEXT NOT NULL CHECK (state IN ('candidate', 'accepted', 'dropped')),
            prepared_request_id BLOB,
            accepted_generation INTEGER,
            UNIQUE(layer_stack_id, layer_id),
            UNIQUE(prepared_request_id),
            CHECK (
                (creation_kind = 'genesis' AND parent_layer_id IS NULL
                    AND source_branch_id IS NULL AND source_branch_depth IS NULL
                    AND source_branch_generation IS NULL
                    AND source_branch_head_operation_version_id IS NULL
                    AND source_branch_delta_id IS NULL AND prepared_request_id IS NULL
                    AND state = 'accepted' AND accepted_generation IS NOT NULL)
                OR
                (creation_kind = 'candidate' AND parent_layer_id IS NOT NULL
                    AND source_branch_id IS NOT NULL AND source_branch_depth IS NOT NULL
                    AND source_branch_depth >= 0
                    AND source_branch_generation IS NOT NULL
                    AND source_branch_generation > 0
                    AND source_branch_head_operation_version_id IS NOT NULL
                    AND source_branch_delta_id IS NOT NULL AND prepared_request_id IS NOT NULL
                    AND ((state = 'accepted' AND accepted_generation IS NOT NULL)
                        OR (state IN ('candidate', 'dropped') AND accepted_generation IS NULL)))
            ),
            FOREIGN KEY(layer_stack_id) REFERENCES layerfs_layer_stacks(layer_stack_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(layer_stack_id, parent_layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(source_branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(source_branch_id, source_branch_head_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(source_branch_delta_id)
                REFERENCES layerfs_branch_deltas(branch_delta_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_branches",
        "CREATE TABLE IF NOT EXISTS layerfs_branches (
            branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 32),
            name TEXT,
            immediate_parent_branch_id BLOB,
            fork_operation_id BLOB,
            fork_operation_version_id BLOB,
            fork_root_id BLOB NOT NULL CHECK (length(fork_root_id) = 32),
            origin_layer_stack_id BLOB NOT NULL CHECK (length(origin_layer_stack_id) = 32),
            origin_layer_id BLOB NOT NULL CHECK (length(origin_layer_id) = 32),
            depth INTEGER NOT NULL CHECK (depth >= 0),
            generation INTEGER NOT NULL CHECK (generation >= 0),
            head_operation_version_id BLOB,
            state TEXT NOT NULL CHECK (state IN ('active', 'dropped')),
            UNIQUE(branch_id, head_operation_version_id),
            CHECK (
                (depth = 0 AND immediate_parent_branch_id IS NULL
                    AND fork_operation_id IS NULL AND fork_operation_version_id IS NULL)
                OR
                (depth > 0 AND immediate_parent_branch_id IS NOT NULL
                    AND fork_operation_id IS NOT NULL AND fork_operation_version_id IS NOT NULL)
            ),
            FOREIGN KEY(immediate_parent_branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(immediate_parent_branch_id, fork_operation_id)
                REFERENCES layerfs_operations(branch_id, operation_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(immediate_parent_branch_id, fork_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(origin_layer_stack_id, origin_layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, head_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_operations",
        "CREATE TABLE IF NOT EXISTS layerfs_operations (
            operation_id BLOB PRIMARY KEY CHECK (length(operation_id) = 32),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            expected_branch_generation INTEGER NOT NULL CHECK (expected_branch_generation >= 0),
            base_kind TEXT NOT NULL CHECK (base_kind IN ('layer', 'operation_version')),
            base_layer_stack_id BLOB,
            base_layer_id BLOB,
            base_operation_version_id BLOB,
            base_root_id BLOB NOT NULL CHECK (length(base_root_id) = 32),
            candidate_root_id BLOB,
            result_operation_version_id BLOB,
            state TEXT NOT NULL CHECK (state IN
                ('running', 'candidate', 'working_recorded', 'durably_accepted',
                 'conflicted', 'discarded', 'failed', 'preserved', 'indeterminate')),
            reconciliation_class TEXT,
            UNIQUE(branch_id, sequence),
            UNIQUE(branch_id, operation_id),
            UNIQUE(operation_id, result_operation_version_id),
            CHECK (
                (base_kind = 'layer' AND base_layer_stack_id IS NOT NULL
                    AND base_layer_id IS NOT NULL AND base_operation_version_id IS NULL)
                OR
                (base_kind = 'operation_version' AND base_layer_stack_id IS NULL
                    AND base_layer_id IS NULL AND base_operation_version_id IS NOT NULL)
            ),
            FOREIGN KEY(branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(base_layer_stack_id, base_layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(base_operation_version_id)
                REFERENCES layerfs_operation_versions(operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, result_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_operation_versions",
        "CREATE TABLE IF NOT EXISTS layerfs_operation_versions (
            operation_version_id BLOB PRIMARY KEY CHECK (length(operation_version_id) = 32),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            parent_operation_version_id BLOB,
            root_id BLOB NOT NULL CHECK (length(root_id) = 32),
            created_by_kind TEXT NOT NULL CHECK (created_by_kind IN ('operation', 'child_merge')),
            created_by_operation_id BLOB,
            created_by_child_branch_id BLOB,
            created_by_branch_delta_id BLOB,
            UNIQUE(branch_id, sequence),
            UNIQUE(branch_id, operation_version_id),
            CHECK (
                (created_by_kind = 'operation' AND created_by_operation_id IS NOT NULL
                    AND created_by_child_branch_id IS NULL AND created_by_branch_delta_id IS NULL)
                OR
                (created_by_kind = 'child_merge' AND created_by_operation_id IS NULL
                    AND created_by_child_branch_id IS NOT NULL
                    AND created_by_branch_delta_id IS NOT NULL)
            ),
            FOREIGN KEY(branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, parent_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, created_by_operation_id)
                REFERENCES layerfs_operations(branch_id, operation_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(created_by_child_branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(created_by_branch_delta_id)
                REFERENCES layerfs_branch_deltas(branch_delta_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_operation_deltas",
        "CREATE TABLE IF NOT EXISTS layerfs_operation_deltas (
            operation_delta_id BLOB PRIMARY KEY CHECK (length(operation_delta_id) = 32),
            operation_id BLOB NOT NULL CHECK (length(operation_id) = 32),
            operation_version_id BLOB NOT NULL CHECK (length(operation_version_id) = 32),
            transition_delta_id BLOB NOT NULL CHECK (length(transition_delta_id) = 32),
            base_root BLOB NOT NULL CHECK (length(base_root) = 32),
            result_root BLOB NOT NULL CHECK (length(result_root) = 32),
            FOREIGN KEY(operation_id, operation_version_id)
                REFERENCES layerfs_operations(operation_id, result_operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(transition_delta_id) REFERENCES layerfs_deltas(delta_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_branch_deltas",
        "CREATE TABLE IF NOT EXISTS layerfs_branch_deltas (
            branch_delta_id BLOB PRIMARY KEY CHECK (length(branch_delta_id) = 32),
            purpose TEXT NOT NULL CHECK (purpose IN ('child_merge', 'layer_stack_merge')),
            source_branch_id BLOB NOT NULL CHECK (length(source_branch_id) = 32),
            source_branch_generation INTEGER NOT NULL CHECK (source_branch_generation > 0),
            source_branch_operation_version_id BLOB NOT NULL CHECK (
                length(source_branch_operation_version_id) = 32),
            base_root BLOB NOT NULL CHECK (length(base_root) = 32),
            source_root BLOB NOT NULL CHECK (length(source_root) = 32),
            destination_root BLOB NOT NULL CHECK (length(destination_root) = 32),
            result_root BLOB NOT NULL CHECK (length(result_root) = 32),
            source_delta_id BLOB NOT NULL CHECK (length(source_delta_id) = 32),
            applied_delta_id BLOB NOT NULL CHECK (length(applied_delta_id) = 32),
            FOREIGN KEY(source_branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(source_branch_id, source_branch_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(source_delta_id) REFERENCES layerfs_deltas(delta_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(applied_delta_id) REFERENCES layerfs_deltas(delta_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_layer_deltas",
        "CREATE TABLE IF NOT EXISTS layerfs_layer_deltas (
            layer_delta_id BLOB PRIMARY KEY CHECK (length(layer_delta_id) = 32),
            parent_layer_id BLOB NOT NULL CHECK (length(parent_layer_id) = 32),
            candidate_layer_id BLOB NOT NULL CHECK (length(candidate_layer_id) = 32),
            transition_delta_id BLOB NOT NULL CHECK (length(transition_delta_id) = 32),
            parent_root BLOB NOT NULL CHECK (length(parent_root) = 32),
            result_root BLOB NOT NULL CHECK (length(result_root) = 32),
            FOREIGN KEY(parent_layer_id) REFERENCES layerfs_layers(layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(candidate_layer_id) REFERENCES layerfs_layers(layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(transition_delta_id) REFERENCES layerfs_deltas(delta_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_branch_transitions",
        "CREATE TABLE IF NOT EXISTS layerfs_branch_transitions (
            transition_id BLOB PRIMARY KEY CHECK (length(transition_id) = 32),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            before_generation INTEGER NOT NULL CHECK (before_generation >= 0),
            after_generation INTEGER NOT NULL CHECK (after_generation = before_generation + 1),
            before_operation_version_id BLOB,
            after_operation_version_id BLOB,
            action_kind TEXT NOT NULL CHECK (action_kind IN
                ('operation_commit', 'child_branch_merge', 'branch_rollback')),
            source_record_id BLOB NOT NULL CHECK (length(source_record_id) = 32),
            request_id BLOB NOT NULL UNIQUE CHECK (length(request_id) = 32),
            FOREIGN KEY(branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, before_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, after_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_layer_stack_transitions",
        "CREATE TABLE IF NOT EXISTS layerfs_layer_stack_transitions (
            transition_id BLOB PRIMARY KEY CHECK (length(transition_id) = 32),
            layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 32),
            before_generation INTEGER NOT NULL CHECK (before_generation >= 0),
            after_generation INTEGER NOT NULL CHECK (after_generation = before_generation + 1),
            before_layer_id BLOB NOT NULL CHECK (length(before_layer_id) = 32),
            after_layer_id BLOB NOT NULL CHECK (length(after_layer_id) = 32),
            action_kind TEXT NOT NULL CHECK (action_kind IN
                ('layer_stack_merge', 'layer_stack_rollback')),
            source_record_id BLOB NOT NULL CHECK (length(source_record_id) = 32),
            request_id BLOB NOT NULL UNIQUE CHECK (length(request_id) = 32),
            FOREIGN KEY(layer_stack_id) REFERENCES layerfs_layer_stacks(layer_stack_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(layer_stack_id, before_layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(layer_stack_id, after_layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_version_leases",
        "CREATE TABLE IF NOT EXISTS layerfs_version_leases (
            lease_id BLOB PRIMARY KEY CHECK (length(lease_id) = 32),
            target_kind TEXT NOT NULL CHECK (target_kind IN ('layer', 'operation_version')),
            target_id BLOB NOT NULL CHECK (length(target_id) = 32),
            owner_kind TEXT NOT NULL CHECK (owner_kind IN
                ('branch', 'operation_workspace', 'mount', 'materialization',
                 'layer_candidate', 'child_branch_merge', 'layer_stack_merge',
                 'sync', 'explicit')),
            owner_id BLOB NOT NULL CHECK (length(owner_id) = 32),
            created_at INTEGER NOT NULL,
            expires_at INTEGER,
            UNIQUE(target_kind, target_id, owner_kind, owner_id)
        )",
    ),
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
            expected_durable_generation INTEGER,
            state TEXT NOT NULL CHECK (state IN
                ('selected', 'transferring', 'transferred', 'accepted',
                 'conflict', 'indeterminate')),
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
            expected_head_id BLOB,
            expected_generation INTEGER,
            accepted_head_id BLOB,
            accepted_generation INTEGER,
            accepted_root_id BLOB,
            result TEXT NOT NULL CHECK (result IN
                ('fetched', 'durably_accepted', 'conflict', 'indeterminate')),
            unique_bytes INTEGER NOT NULL CHECK (unique_bytes >= 0),
            resumed_bytes INTEGER NOT NULL CHECK (resumed_bytes >= 0),
            retransmitted_bytes INTEGER NOT NULL CHECK (retransmitted_bytes >= 0),
            reconciliation_result TEXT,
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

pub(crate) const CURRENT_TABLE_NAMES: [&str; 29] = [
    "layerfs_authority",
    "layerfs_branch_deltas",
    "layerfs_branch_push_pages",
    "layerfs_branch_transitions",
    "layerfs_branches",
    "layerfs_deltas",
    "layerfs_durable_storages",
    "layerfs_durable_tracking_refs",
    "layerfs_fetch_closure_items",
    "layerfs_fetch_staging_heads",
    "layerfs_layer_deltas",
    "layerfs_layer_stack_transitions",
    "layerfs_layer_stacks",
    "layerfs_layers",
    "layerfs_objects",
    "layerfs_operation_deltas",
    "layerfs_operation_versions",
    "layerfs_operations",
    "layerfs_push_outbox",
    "layerfs_refs",
    "layerfs_released_versions",
    "layerfs_retained_roots",
    "layerfs_roots",
    "layerfs_store_meta",
    "layerfs_sync_batch_receipts",
    "layerfs_sync_object_pins",
    "layerfs_sync_receipts",
    "layerfs_transfer_state",
    "layerfs_version_leases",
];

pub(crate) const LEGACY_TABLE_NAMES: [&str; 7] = [
    "layerfs_authority",
    "layerfs_deltas",
    "layerfs_objects",
    "layerfs_refs",
    "layerfs_retained_roots",
    "layerfs_roots",
    "layerfs_store_meta",
];
