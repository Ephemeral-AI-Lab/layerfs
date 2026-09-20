-- C5 history catalog schema 1: immutable history metadata, exact stages and
-- scope-wide inode allocation. Seven application tables; no foreign key crosses
-- into the C2 content database. C2 schema 7 is unchanged and is never migrated,
-- repaired or promoted from here; incompatible catalogs fail at open.
PRAGMA application_id = 1279677256;
PRAGMA user_version = 1;

CREATE TABLE history_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    catalog_id BLOB NOT NULL CHECK (length(catalog_id) = 32),
    catalog_incarnation INTEGER NOT NULL
        CHECK (catalog_incarnation BETWEEN 1 AND 9223372036854775807),
    binding_key BLOB NOT NULL CHECK (length(binding_key) BETWEEN 1 AND 128),
    identity_format INTEGER NOT NULL CHECK (identity_format = 1),
    next_stage_token INTEGER NOT NULL
        CHECK (next_stage_token BETWEEN 1 AND 9223372036854775807)
) STRICT;

CREATE TABLE layer_stacks (
    layer_stack_id BLOB PRIMARY KEY
        CHECK (length(layer_stack_id) = 17 AND substr(layer_stack_id, 1, 1) = X'31'),
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    scope_id BLOB NOT NULL CHECK (length(scope_id) = 32),
    profile_id BLOB NOT NULL CHECK (length(profile_id) = 32),
    head_layer_id BLOB NOT NULL
        CHECK (length(head_layer_id) = 33 AND substr(head_layer_id, 1, 1) = X'32'),
    FOREIGN KEY (layer_stack_id, head_layer_id)
        REFERENCES layers(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE layers (
    layer_id BLOB PRIMARY KEY
        CHECK (length(layer_id) = 33 AND substr(layer_id, 1, 1) = X'32'),
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17 AND substr(layer_stack_id, 1, 1) = X'31'),
    parent_layer_id BLOB
        CHECK (
            parent_layer_id IS NULL
            OR (length(parent_layer_id) = 33 AND substr(parent_layer_id, 1, 1) = X'32')
        ),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32),
    source_branch_id BLOB
        CHECK (
            source_branch_id IS NULL
            OR (length(source_branch_id) = 17 AND substr(source_branch_id, 1, 1) = X'11')
        ),
    source_commit_id BLOB
        CHECK (
            source_commit_id IS NULL
            OR (length(source_commit_id) = 33 AND substr(source_commit_id, 1, 1) = X'12')
        ),
    CHECK (
        (parent_layer_id IS NULL AND source_branch_id IS NULL AND source_commit_id IS NULL)
        OR
        (parent_layer_id IS NOT NULL AND source_branch_id IS NOT NULL AND source_commit_id IS NOT NULL)
    ),
    FOREIGN KEY (layer_stack_id)
        REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, parent_layer_id)
        REFERENCES layers(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, source_branch_id)
        REFERENCES branches(layer_stack_id, branch_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, source_commit_id)
        REFERENCES commits(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE commits (
    commit_id BLOB PRIMARY KEY
        CHECK (length(commit_id) = 33 AND substr(commit_id, 1, 1) = X'12'),
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17 AND substr(layer_stack_id, 1, 1) = X'31'),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32),
    parent_commit_id BLOB
        CHECK (
            parent_commit_id IS NULL
            OR (length(parent_commit_id) = 33 AND substr(parent_commit_id, 1, 1) = X'12')
        ),
    base_layer_id BLOB NOT NULL
        CHECK (length(base_layer_id) = 33 AND substr(base_layer_id, 1, 1) = X'32'),
    FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES layers(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, parent_commit_id)
        REFERENCES commits(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE branches (
    branch_id BLOB PRIMARY KEY
        CHECK (length(branch_id) = 17 AND substr(branch_id, 1, 1) = X'11'),
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17 AND substr(layer_stack_id, 1, 1) = X'31'),
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    base_layer_id BLOB NOT NULL
        CHECK (length(base_layer_id) = 33 AND substr(base_layer_id, 1, 1) = X'32'),
    head_commit_id BLOB
        CHECK (
            head_commit_id IS NULL
            OR (length(head_commit_id) = 33 AND substr(head_commit_id, 1, 1) = X'12')
        ),
    FOREIGN KEY (layer_stack_id)
        REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES layers(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, head_commit_id)
        REFERENCES commits(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE workspace_stages (
    workspace_id BLOB PRIMARY KEY CHECK (length(workspace_id) = 32),
    stage_token INTEGER NOT NULL
        CHECK (stage_token BETWEEN 1 AND 9223372036854775807),
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17 AND substr(layer_stack_id, 1, 1) = X'31'),
    branch_id BLOB NOT NULL
        CHECK (length(branch_id) = 17 AND substr(branch_id, 1, 1) = X'11'),
    expected_head_commit_id BLOB
        CHECK (
            expected_head_commit_id IS NULL
            OR (length(expected_head_commit_id) = 33 AND substr(expected_head_commit_id, 1, 1) = X'12')
        ),
    expected_base_layer_id BLOB NOT NULL
        CHECK (length(expected_base_layer_id) = 33 AND substr(expected_base_layer_id, 1, 1) = X'32'),
    expected_root_id BLOB NOT NULL CHECK (length(expected_root_id) = 32),
    construction_base_root_id BLOB NOT NULL CHECK (length(construction_base_root_id) = 32),
    intended_commit_base_layer_id BLOB NOT NULL
        CHECK (length(intended_commit_base_layer_id) = 33 AND substr(intended_commit_base_layer_id, 1, 1) = X'32'),
    candidate_root_id BLOB NOT NULL CHECK (length(candidate_root_id) = 32),
    profile_id BLOB NOT NULL CHECK (length(profile_id) = 32),
    scope_id BLOB NOT NULL CHECK (length(scope_id) = 32),
    input_generation INTEGER NOT NULL CHECK (input_generation >= 0),
    FOREIGN KEY (layer_stack_id, branch_id)
        REFERENCES branches(layer_stack_id, branch_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, expected_base_layer_id)
        REFERENCES layers(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, intended_commit_base_layer_id)
        REFERENCES layers(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, expected_head_commit_id)
        REFERENCES commits(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE scope_allocator (
    scope_id BLOB PRIMARY KEY CHECK (length(scope_id) = 32),
    highwater INTEGER NOT NULL CHECK (highwater BETWEEN 0 AND 9223372036854775807),
    authority_id BLOB NOT NULL CHECK (length(authority_id) = 32)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX layer_stack_names ON layer_stacks(name);
CREATE UNIQUE INDEX layer_identity ON layers(layer_stack_id, layer_id);
CREATE UNIQUE INDEX layers_genesis ON layers(layer_stack_id) WHERE parent_layer_id IS NULL;
CREATE UNIQUE INDEX layers_child ON layers(layer_stack_id, parent_layer_id)
    WHERE parent_layer_id IS NOT NULL;
CREATE UNIQUE INDEX layers_source ON layers(source_branch_id, source_commit_id)
    WHERE source_branch_id IS NOT NULL;
CREATE UNIQUE INDEX commit_identity ON commits(layer_stack_id, commit_id);
CREATE UNIQUE INDEX branch_identity ON branches(layer_stack_id, branch_id);
CREATE UNIQUE INDEX branch_names ON branches(layer_stack_id, name);
CREATE UNIQUE INDEX stage_tokens ON workspace_stages(stage_token);
CREATE INDEX stage_branches ON workspace_stages(layer_stack_id, branch_id, stage_token);
