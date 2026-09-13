-- Optional runtime metadata extension; canonical Store format versions are unchanged.
CREATE TABLE workspace_publications (
    workspace_id BLOB PRIMARY KEY CHECK (length(workspace_id) = 16),
    attempt_key BLOB NOT NULL CHECK (length(attempt_key) = 32),
    branch_id BLOB NOT NULL CHECK (length(branch_id) = 17) REFERENCES branches(branch_id),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id),
    expected_root BLOB NOT NULL CHECK (length(expected_root) = 32) REFERENCES objects(object_id),
    expected_head BLOB CHECK (expected_head IS NULL OR length(expected_head) = 33) REFERENCES commits(commit_id),
    expected_base BLOB NOT NULL CHECK (length(expected_base) = 33) REFERENCES layers(layer_id),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32) REFERENCES objects(object_id),
    base_after BLOB NOT NULL CHECK (length(base_after) = 33) REFERENCES layers(layer_id),
    covered_sequence BLOB NOT NULL CHECK (length(covered_sequence) = 8),
    outcome_kind INTEGER NOT NULL CHECK (outcome_kind IN (1, 2)),
    head_after BLOB CHECK (head_after IS NULL OR length(head_after) = 33) REFERENCES commits(commit_id),
    published_ns BLOB NOT NULL CHECK (length(published_ns) = 8)
) STRICT, WITHOUT ROWID;
