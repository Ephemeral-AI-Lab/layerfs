CREATE TABLE store (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    store_id BLOB NOT NULL UNIQUE CHECK (length(store_id) = 32),
    parent_store_id BLOB NOT NULL CHECK (length(parent_store_id) = 32)
) STRICT;
CREATE TABLE objects (
    object_id BLOB PRIMARY KEY CHECK (length(object_id) = 32),
    bytes BLOB NOT NULL
) STRICT;
CREATE TABLE commits (
    commit_id BLOB PRIMARY KEY CHECK (length(commit_id) = 33),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32),
    parent_commit_id BLOB CHECK (parent_commit_id IS NULL OR length(parent_commit_id) = 33),
    base_layer_id BLOB NOT NULL CHECK (length(base_layer_id) = 33) REFERENCES layers(layer_id),
    FOREIGN KEY (parent_commit_id) REFERENCES commits(commit_id)
) STRICT;
CREATE TABLE branches (
    branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 17),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    base_layer_id BLOB CHECK (base_layer_id IS NULL OR length(base_layer_id) = 33),
    head_commit_id BLOB CHECK (head_commit_id IS NULL OR length(head_commit_id) = 33) REFERENCES commits(commit_id),
    forked_from_layer_id BLOB CHECK (forked_from_layer_id IS NULL OR length(forked_from_layer_id) = 33),
    forked_from_branch_id BLOB CHECK (forked_from_branch_id IS NULL OR length(forked_from_branch_id) = 17),
    forked_from_commit_id BLOB CHECK (forked_from_commit_id IS NULL OR length(forked_from_commit_id) = 33) REFERENCES commits(commit_id),
    CHECK ((forked_from_layer_id IS NOT NULL AND forked_from_branch_id IS NULL AND forked_from_commit_id IS NULL)
        OR (forked_from_layer_id IS NULL AND forked_from_branch_id IS NOT NULL AND forked_from_commit_id IS NOT NULL)),
    CHECK (base_layer_id IS NOT NULL OR head_commit_id IS NULL),
    FOREIGN KEY (layer_stack_id, base_layer_id) REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, forked_from_layer_id) REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, forked_from_branch_id) REFERENCES branches(layer_stack_id, branch_id)
) STRICT;
CREATE TABLE branch_scopes (
    branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 17) REFERENCES branches(branch_id),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('local', 'remote')),
    through_commit_id BLOB CHECK (through_commit_id IS NULL OR length(through_commit_id) = 33),
    serving_mode TEXT CHECK (serving_mode IS NULL OR serving_mode IN ('reference', 'replica')),
    CHECK ((scope_kind = 'local' AND through_commit_id IS NULL AND serving_mode IS NULL)
        OR (scope_kind = 'remote' AND through_commit_id IS NOT NULL AND serving_mode IS NOT NULL)),
    FOREIGN KEY (branch_id, through_commit_id) REFERENCES branches(branch_id, head_commit_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE layer_stacks (
    layer_stack_id BLOB PRIMARY KEY CHECK (length(layer_stack_id) = 17),
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]')
) STRICT;
CREATE TABLE layer_stack_scopes (
    layer_stack_id BLOB PRIMARY KEY CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id),
    through_layer_id BLOB NOT NULL CHECK (length(through_layer_id) = 33),
    serving_mode TEXT NOT NULL CHECK (serving_mode IN ('reference', 'replica')),
    FOREIGN KEY (layer_stack_id, through_layer_id) REFERENCES layers(layer_stack_id, layer_id)
) STRICT;
CREATE TABLE layers (
    layer_id BLOB PRIMARY KEY CHECK (length(layer_id) = 33),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    parent_layer_id BLOB CHECK (parent_layer_id IS NULL OR length(parent_layer_id) = 33),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32),
    source_branch_id BLOB CHECK (source_branch_id IS NULL OR length(source_branch_id) = 17),
    source_commit_id BLOB CHECK (source_commit_id IS NULL OR length(source_commit_id) = 33),
    CHECK ((parent_layer_id IS NULL AND source_branch_id IS NULL AND source_commit_id IS NULL)
        OR (parent_layer_id IS NOT NULL AND source_branch_id IS NOT NULL AND source_commit_id IS NOT NULL)),
    FOREIGN KEY (layer_stack_id, parent_layer_id) REFERENCES layers(layer_stack_id, layer_id)
) STRICT;
CREATE TABLE complete_roots (
    root_id BLOB PRIMARY KEY CHECK (length(root_id) = 32) REFERENCES objects(object_id)
) STRICT;
CREATE UNIQUE INDEX layer_stack_names ON layer_stacks(name);
CREATE UNIQUE INDEX layer_identity ON layers(layer_stack_id, layer_id);
CREATE UNIQUE INDEX layers_genesis ON layers(layer_stack_id) WHERE parent_layer_id IS NULL;
CREATE UNIQUE INDEX layers_child ON layers(layer_stack_id, parent_layer_id) WHERE parent_layer_id IS NOT NULL;
CREATE UNIQUE INDEX layers_source ON layers(source_branch_id, source_commit_id) WHERE source_branch_id IS NOT NULL;
CREATE INDEX layers_parent ON layers(parent_layer_id);
CREATE INDEX commits_parent ON commits(parent_commit_id);
CREATE UNIQUE INDEX branch_identity ON branches(layer_stack_id, branch_id);
CREATE UNIQUE INDEX branch_names ON branches(layer_stack_id, name);
CREATE UNIQUE INDEX branch_pointer ON branches(branch_id, head_commit_id);
CREATE INDEX branches_head ON branches(head_commit_id);
CREATE INDEX branches_fork ON branches(forked_from_branch_id, forked_from_commit_id);
