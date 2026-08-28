//! Private Working Branch, merge-delta, and transition DDL.

pub(super) const SCHEMAS: [(&str, &str); 3] = [
    (
        "layerfs_branches",
        "CREATE TABLE layerfs_branches (
            branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 32),
            name TEXT CHECK (name IS NULL OR length(name) BETWEEN 1 AND 255),
            immediate_parent_branch_id BLOB CHECK (
                immediate_parent_branch_id IS NULL OR length(immediate_parent_branch_id) = 32),
            fork_operation_id BLOB CHECK (
                fork_operation_id IS NULL OR length(fork_operation_id) = 32),
            fork_operation_version_id BLOB CHECK (
                fork_operation_version_id IS NULL OR length(fork_operation_version_id) = 32),
            fork_root_id BLOB NOT NULL CHECK (length(fork_root_id) = 32),
            origin_base_binding_id BLOB NOT NULL CHECK (length(origin_base_binding_id) = 32),
            depth INTEGER NOT NULL CHECK (depth >= 0),
            generation INTEGER NOT NULL CHECK (generation >= 0),
            head_operation_version_id BLOB CHECK (
                head_operation_version_id IS NULL OR length(head_operation_version_id) = 32),
            state TEXT NOT NULL CHECK (state IN ('active', 'dropped')),
            UNIQUE(branch_id, head_operation_version_id),
            CHECK ((depth = 0 AND immediate_parent_branch_id IS NULL
                       AND fork_operation_id IS NULL AND fork_operation_version_id IS NULL)
                   OR (depth > 0 AND immediate_parent_branch_id IS NOT NULL
                       AND fork_operation_id IS NOT NULL
                       AND fork_operation_version_id IS NOT NULL)),
            FOREIGN KEY(origin_base_binding_id)
                REFERENCES layerfs_working_base_bindings(binding_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(immediate_parent_branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(immediate_parent_branch_id, fork_operation_id)
                REFERENCES layerfs_operations(branch_id, operation_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(immediate_parent_branch_id, fork_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, head_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_branch_deltas",
        "CREATE TABLE layerfs_branch_deltas (
            branch_delta_id BLOB PRIMARY KEY CHECK (length(branch_delta_id) = 32),
            purpose TEXT NOT NULL CHECK (purpose IN ('child_merge', 'layer_stack_merge')),
            source_branch_id BLOB NOT NULL CHECK (length(source_branch_id) = 32),
            source_branch_generation INTEGER NOT NULL CHECK (source_branch_generation > 0),
            source_operation_version_id BLOB NOT NULL CHECK (
                length(source_operation_version_id) = 32),
            base_root_id BLOB NOT NULL CHECK (length(base_root_id) = 32),
            source_root_id BLOB NOT NULL CHECK (length(source_root_id) = 32),
            destination_root_id BLOB NOT NULL CHECK (length(destination_root_id) = 32),
            result_root_id BLOB NOT NULL CHECK (length(result_root_id) = 32),
            source_delta_id BLOB NOT NULL CHECK (length(source_delta_id) = 32),
            applied_delta_id BLOB NOT NULL CHECK (length(applied_delta_id) = 32),
            FOREIGN KEY(source_branch_id, source_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(source_delta_id) REFERENCES layerfs_deltas(delta_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(applied_delta_id) REFERENCES layerfs_deltas(delta_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_branch_transitions",
        "CREATE TABLE layerfs_branch_transitions (
            transition_id BLOB PRIMARY KEY CHECK (length(transition_id) = 32),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            before_generation INTEGER NOT NULL CHECK (before_generation >= 0),
            after_generation INTEGER NOT NULL CHECK (after_generation = before_generation + 1),
            before_operation_version_id BLOB CHECK (
                before_operation_version_id IS NULL OR length(before_operation_version_id) = 32),
            after_operation_version_id BLOB CHECK (
                after_operation_version_id IS NULL OR length(after_operation_version_id) = 32),
            action_kind TEXT NOT NULL CHECK (action_kind IN
                ('operation_commit', 'child_branch_merge', 'branch_rollback')),
            source_record_id BLOB NOT NULL CHECK (length(source_record_id) = 32),
            request_id BLOB NOT NULL UNIQUE CHECK (length(request_id) = 32),
            UNIQUE(branch_id, after_generation),
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
];
