//! Private prepared LayerStack-finalization candidate DDL.

pub(super) const SCHEMAS: [(&str, &str); 1] = [(
    "layerfs_working_layer_candidates",
    "CREATE TABLE layerfs_working_layer_candidates (
        candidate_id BLOB PRIMARY KEY CHECK (length(candidate_id) = 32),
        layer_id BLOB NOT NULL UNIQUE CHECK (length(layer_id) = 32),
        origin_base_binding_id BLOB NOT NULL CHECK (length(origin_base_binding_id) = 32),
        source_branch_id BLOB NOT NULL CHECK (length(source_branch_id) = 32),
        source_operation_version_id BLOB NOT NULL CHECK (
            length(source_operation_version_id) = 32),
        expected_layer_stack_id BLOB NOT NULL CHECK (length(expected_layer_stack_id) = 32),
        expected_generation INTEGER NOT NULL CHECK (expected_generation >= 0),
        expected_layer_id BLOB NOT NULL CHECK (length(expected_layer_id) = 32),
        expected_root_id BLOB NOT NULL CHECK (length(expected_root_id) = 32),
        result_root_id BLOB NOT NULL CHECK (length(result_root_id) = 32),
        transition_delta_id BLOB NOT NULL CHECK (length(transition_delta_id) = 32),
        request_id BLOB NOT NULL UNIQUE CHECK (length(request_id) = 32),
        state TEXT NOT NULL CHECK (state IN
            ('prepared', 'durably_accepted', 'conflicted', 'dropped', 'indeterminate')),
        actual_generation INTEGER CHECK (
            actual_generation IS NULL OR actual_generation >= 0),
        actual_layer_id BLOB CHECK (
            actual_layer_id IS NULL OR length(actual_layer_id) = 32),
        actual_root_id BLOB CHECK (
            actual_root_id IS NULL OR length(actual_root_id) = 32),
        CHECK ((state = 'conflicted' AND actual_generation IS NOT NULL
                   AND actual_layer_id IS NOT NULL AND actual_root_id IS NOT NULL)
               OR (state != 'conflicted' AND actual_generation IS NULL
                   AND actual_layer_id IS NULL AND actual_root_id IS NULL)),
        FOREIGN KEY(origin_base_binding_id)
            REFERENCES layerfs_working_base_bindings(binding_id)
            DEFERRABLE INITIALLY DEFERRED,
        FOREIGN KEY(source_branch_id, source_operation_version_id)
            REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
            DEFERRABLE INITIALLY DEFERRED,
        FOREIGN KEY(transition_delta_id) REFERENCES layerfs_deltas(delta_id)
            DEFERRABLE INITIALLY DEFERRED
    )",
)];
