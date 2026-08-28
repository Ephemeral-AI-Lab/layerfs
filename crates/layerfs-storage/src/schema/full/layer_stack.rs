//! Full Layer and LayerStack accepted-history DDL.

pub(super) const SCHEMAS: [(&str, &str); 3] = [
    (
        "layerfs_layer_stacks",
        "CREATE TABLE layerfs_layer_stacks (
            layer_stack_id BLOB PRIMARY KEY CHECK (length(layer_stack_id) = 32),
            name TEXT NOT NULL UNIQUE CHECK (length(name) BETWEEN 1 AND 255),
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
        "CREATE TABLE layerfs_layers (
            layer_id BLOB PRIMARY KEY CHECK (length(layer_id) = 32),
            layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 32),
            parent_layer_id BLOB CHECK (
                parent_layer_id IS NULL OR length(parent_layer_id) = 32),
            result_root_id BLOB NOT NULL CHECK (length(result_root_id) = 32),
            creation_kind TEXT NOT NULL CHECK (creation_kind IN ('genesis', 'candidate')),
            source_branch_id BLOB CHECK (
                source_branch_id IS NULL OR length(source_branch_id) = 32),
            source_branch_depth INTEGER CHECK (
                source_branch_depth IS NULL OR source_branch_depth >= 0),
            source_branch_generation INTEGER CHECK (
                source_branch_generation IS NULL OR source_branch_generation > 0),
            source_operation_version_id BLOB CHECK (
                source_operation_version_id IS NULL OR length(source_operation_version_id) = 32),
            source_branch_delta_id BLOB CHECK (
                source_branch_delta_id IS NULL OR length(source_branch_delta_id) = 32),
            transition_delta_id BLOB CHECK (
                transition_delta_id IS NULL OR length(transition_delta_id) = 32),
            parent_root_id BLOB CHECK (
                parent_root_id IS NULL OR length(parent_root_id) = 32),
            state TEXT NOT NULL CHECK (state IN ('candidate', 'accepted', 'dropped')),
            prepared_request_id BLOB UNIQUE CHECK (
                prepared_request_id IS NULL OR length(prepared_request_id) = 32),
            accepted_generation INTEGER CHECK (
                accepted_generation IS NULL OR accepted_generation >= 0),
            UNIQUE(layer_stack_id, layer_id),
            UNIQUE(layer_stack_id, accepted_generation),
            CHECK ((creation_kind = 'genesis'
                       AND parent_layer_id IS NULL AND source_branch_id IS NULL
                       AND source_branch_depth IS NULL AND source_branch_generation IS NULL
                       AND source_operation_version_id IS NULL
                       AND source_branch_delta_id IS NULL AND transition_delta_id IS NULL
                       AND parent_root_id IS NULL AND prepared_request_id IS NULL
                       AND state = 'accepted' AND accepted_generation IS NOT NULL)
                   OR (creation_kind = 'candidate'
                       AND parent_layer_id IS NOT NULL AND source_branch_id IS NOT NULL
                       AND source_branch_depth IS NOT NULL
                       AND source_branch_generation IS NOT NULL
                       AND source_operation_version_id IS NOT NULL
                       AND source_branch_delta_id IS NOT NULL
                       AND transition_delta_id IS NOT NULL AND parent_root_id IS NOT NULL
                       AND prepared_request_id IS NOT NULL
                       AND ((state = 'accepted' AND accepted_generation IS NOT NULL)
                         OR (state IN ('candidate', 'dropped')
                             AND accepted_generation IS NULL)))),
            FOREIGN KEY(layer_stack_id) REFERENCES layerfs_layer_stacks(layer_stack_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(layer_stack_id, parent_layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(result_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(parent_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(source_branch_id, source_operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(source_branch_delta_id)
                REFERENCES layerfs_branch_deltas(branch_delta_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(transition_delta_id) REFERENCES layerfs_deltas(delta_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_layer_stack_transitions",
        "CREATE TABLE layerfs_layer_stack_transitions (
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
            UNIQUE(layer_stack_id, after_generation),
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
];
