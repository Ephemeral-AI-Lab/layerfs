pub(crate) const SCHEMAS: [(&str, &str); 11] = [
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
];
