CREATE TABLE workspace_stages (
    workspace_id BLOB PRIMARY KEY CHECK (length(workspace_id) = 16),
    branch_id BLOB NOT NULL CHECK (length(branch_id) = 17)
        REFERENCES branches(branch_id),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32)
        REFERENCES objects(object_id)
) STRICT, WITHOUT ROWID;

PRAGMA user_version = 5;
