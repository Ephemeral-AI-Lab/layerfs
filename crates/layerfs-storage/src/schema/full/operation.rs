//! Full accepted/direct Operation and folded OperationVersion DDL.

pub(super) const SCHEMAS: [(&str, &str); 2] = [
    (
        "layerfs_operations",
        "CREATE TABLE layerfs_operations (
            operation_id BLOB PRIMARY KEY CHECK (length(operation_id) = 32),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            expected_branch_generation INTEGER NOT NULL CHECK (expected_branch_generation >= 0),
            base_kind TEXT NOT NULL CHECK (base_kind IN ('layer', 'operation_version')),
            base_layer_stack_id BLOB CHECK (
                base_layer_stack_id IS NULL OR length(base_layer_stack_id) = 32),
            base_layer_id BLOB CHECK (base_layer_id IS NULL OR length(base_layer_id) = 32),
            base_operation_version_id BLOB CHECK (
                base_operation_version_id IS NULL OR length(base_operation_version_id) = 32),
            base_root_id BLOB NOT NULL CHECK (length(base_root_id) = 32),
            candidate_root_id BLOB CHECK (
                candidate_root_id IS NULL OR length(candidate_root_id) = 32),
            result_operation_version_id BLOB CHECK (
                result_operation_version_id IS NULL OR length(result_operation_version_id) = 32),
            state TEXT NOT NULL CHECK (state IN
                ('running', 'candidate', 'working_recorded', 'durably_accepted',
                 'conflicted', 'discarded', 'failed', 'preserved', 'indeterminate')),
            reconciliation_class TEXT CHECK (reconciliation_class IS NULL OR
                reconciliation_class IN ('exact', 'ordered_replay')),
            UNIQUE(branch_id, sequence),
            UNIQUE(branch_id, operation_id),
            UNIQUE(operation_id, result_operation_version_id),
            CHECK ((base_kind = 'layer' AND base_layer_stack_id IS NOT NULL
                       AND base_layer_id IS NOT NULL AND base_operation_version_id IS NULL)
                   OR (base_kind = 'operation_version' AND base_layer_stack_id IS NULL
                       AND base_layer_id IS NULL AND base_operation_version_id IS NOT NULL)),
            FOREIGN KEY(branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(base_layer_stack_id, base_layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(base_operation_version_id)
                REFERENCES layerfs_operation_versions(operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(base_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(candidate_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, result_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_operation_versions",
        "CREATE TABLE layerfs_operation_versions (
            operation_version_id BLOB PRIMARY KEY CHECK (length(operation_version_id) = 32),
            branch_id BLOB NOT NULL CHECK (length(branch_id) = 32),
            sequence INTEGER NOT NULL CHECK (sequence >= 0),
            parent_operation_version_id BLOB CHECK (
                parent_operation_version_id IS NULL OR length(parent_operation_version_id) = 32),
            created_by_kind TEXT NOT NULL CHECK (created_by_kind IN ('operation', 'child_merge')),
            operation_id BLOB CHECK (operation_id IS NULL OR length(operation_id) = 32),
            child_branch_id BLOB CHECK (
                child_branch_id IS NULL OR length(child_branch_id) = 32),
            branch_delta_id BLOB CHECK (
                branch_delta_id IS NULL OR length(branch_delta_id) = 32),
            transition_delta_id BLOB NOT NULL CHECK (length(transition_delta_id) = 32),
            base_root_id BLOB NOT NULL CHECK (length(base_root_id) = 32),
            result_root_id BLOB NOT NULL CHECK (length(result_root_id) = 32),
            UNIQUE(branch_id, sequence),
            UNIQUE(branch_id, operation_version_id),
            UNIQUE(operation_id, operation_version_id),
            CHECK ((created_by_kind = 'operation' AND operation_id IS NOT NULL
                       AND child_branch_id IS NULL AND branch_delta_id IS NULL)
                   OR (created_by_kind = 'child_merge' AND operation_id IS NULL
                       AND child_branch_id IS NOT NULL AND branch_delta_id IS NOT NULL)),
            FOREIGN KEY(branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, parent_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, operation_id)
                REFERENCES layerfs_operations(branch_id, operation_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(child_branch_id) REFERENCES layerfs_branches(branch_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_delta_id) REFERENCES layerfs_branch_deltas(branch_delta_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(transition_delta_id) REFERENCES layerfs_deltas(delta_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(base_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(result_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
];
