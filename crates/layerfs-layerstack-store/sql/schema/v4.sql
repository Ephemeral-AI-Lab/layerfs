PRAGMA application_id = 1279677260;
PRAGMA user_version = 4;

CREATE TABLE objects (
    object_id BLOB PRIMARY KEY
        CHECK (length(object_id) = 32),
    bytes BLOB NOT NULL
) STRICT;

CREATE TABLE commits (
    commit_id BLOB PRIMARY KEY
        CHECK (length(commit_id) = 33),
    root_id BLOB NOT NULL
        CHECK (length(root_id) = 32)
        REFERENCES objects(object_id),
    parent_commit_id BLOB
        CHECK (
            parent_commit_id IS NULL
            OR length(parent_commit_id) = 33
        )
        REFERENCES commits(commit_id),
    base_layer_id BLOB NOT NULL
        CHECK (length(base_layer_id) = 33)
        REFERENCES layers(layer_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE branches (
    branch_id BLOB PRIMARY KEY
        CHECK (length(branch_id) = 17),
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17)
        REFERENCES layer_stacks(layer_stack_id)
        DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    base_layer_id BLOB NOT NULL
        CHECK (length(base_layer_id) = 33),
    head_commit_id BLOB
        CHECK (
            head_commit_id IS NULL
            OR length(head_commit_id) = 33
        )
        REFERENCES commits(commit_id),
    FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES layers(layer_stack_id, layer_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE layer_stacks (
    layer_stack_id BLOB PRIMARY KEY
        CHECK (length(layer_stack_id) = 17),
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    head_layer_id BLOB NOT NULL
        CHECK (length(head_layer_id) = 33),
    FOREIGN KEY (layer_stack_id, head_layer_id)
        REFERENCES layers(layer_stack_id, layer_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE layers (
    layer_id BLOB PRIMARY KEY
        CHECK (length(layer_id) = 33),
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17)
        REFERENCES layer_stacks(layer_stack_id)
        DEFERRABLE INITIALLY DEFERRED,
    parent_layer_id BLOB
        CHECK (
            parent_layer_id IS NULL
            OR length(parent_layer_id) = 33
        ),
    root_id BLOB NOT NULL
        CHECK (length(root_id) = 32)
        REFERENCES objects(object_id),
    source_branch_id BLOB
        CHECK (
            source_branch_id IS NULL
            OR length(source_branch_id) = 17
        ),
    source_commit_id BLOB
        CHECK (
            source_commit_id IS NULL
            OR length(source_commit_id) = 33
        )
        REFERENCES commits(commit_id)
        DEFERRABLE INITIALLY DEFERRED,
    CHECK (
        (
            parent_layer_id IS NULL
            AND source_branch_id IS NULL
            AND source_commit_id IS NULL
        )
        OR
        (
            parent_layer_id IS NOT NULL
            AND source_branch_id IS NOT NULL
            AND source_commit_id IS NOT NULL
        )
    ),
    FOREIGN KEY (layer_stack_id, parent_layer_id)
        REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, source_branch_id)
        REFERENCES branches(layer_stack_id, branch_id)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX layer_stack_names
    ON layer_stacks(name);

CREATE UNIQUE INDEX layer_identity
    ON layers(layer_stack_id, layer_id);

CREATE UNIQUE INDEX layers_genesis
    ON layers(layer_stack_id)
    WHERE parent_layer_id IS NULL;

CREATE UNIQUE INDEX layers_child
    ON layers(layer_stack_id, parent_layer_id)
    WHERE parent_layer_id IS NOT NULL;

CREATE UNIQUE INDEX layers_source
    ON layers(source_branch_id, source_commit_id)
    WHERE source_branch_id IS NOT NULL;

CREATE UNIQUE INDEX branch_identity
    ON branches(layer_stack_id, branch_id);

CREATE UNIQUE INDEX branch_names
    ON branches(layer_stack_id, name);
