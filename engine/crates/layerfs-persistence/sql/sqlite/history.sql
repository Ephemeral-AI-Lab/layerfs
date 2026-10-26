CREATE TABLE history_meta (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    catalog_id BLOB NOT NULL CHECK (length(catalog_id) = 32),
    catalog_incarnation INTEGER NOT NULL
        CHECK (catalog_incarnation BETWEEN 1 AND 9223372036854775807),
    binding_key BLOB NOT NULL CHECK (length(binding_key) BETWEEN 1 AND 128),
    identity_format INTEGER NOT NULL CHECK (identity_format = 3),
    schema_version INTEGER NOT NULL CHECK(schema_version=1),
    schema_source TEXT NOT NULL,
    schema_definition TEXT NOT NULL
) STRICT;

CREATE TABLE layer_stack (
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
        REFERENCES layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE layer (
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
    logical INTEGER NOT NULL DEFAULT 0 CHECK(logical>=0),
    overwrote BLOB NOT NULL,
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
        REFERENCES layer_stack(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, parent_layer_id)
        REFERENCES layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, source_branch_id)
        REFERENCES branch(layer_stack_id, branch_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, source_commit_id)
        REFERENCES "commit"(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE "commit" (
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
    second_parent_id BLOB
        CHECK (
            second_parent_id IS NULL
            OR (length(second_parent_id) = 33 AND substr(second_parent_id, 1, 1) IN (X'12', X'32'))
        ),
    branch_id BLOB
        CHECK (
            branch_id IS NULL
            OR (length(branch_id) = 17 AND substr(branch_id, 1, 1) = X'11')
        ),
    via INTEGER NOT NULL CHECK (via IN (1, 2, 3)),
    message TEXT CHECK (message IS NULL OR length(CAST(message AS BLOB)) <= 4096),
    merged_from_id BLOB
        CHECK (
            merged_from_id IS NULL
            OR (length(merged_from_id) = 33 AND substr(merged_from_id, 1, 1) IN (X'12',X'32'))
        ),
    created_ms INTEGER NOT NULL,
    logical INTEGER NOT NULL DEFAULT 0 CHECK(logical>=0),
    new_canonical INTEGER NOT NULL DEFAULT 0 CHECK(new_canonical>=0),
    new_stored INTEGER NOT NULL DEFAULT 0 CHECK(new_stored>=0),
    new_metadata INTEGER NOT NULL DEFAULT 0 CHECK(new_metadata>=0),
    lww BLOB NOT NULL,
    displaced_id BLOB CHECK(displaced_id IS NULL OR length(displaced_id)=33),
    FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, parent_commit_id)
        REFERENCES "commit"(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE branch (
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
    forked_from_id BLOB CHECK(forked_from_id IS NULL OR length(forked_from_id)=33),
    head_commit_id BLOB
        CHECK (
            head_commit_id IS NULL
            OR (length(head_commit_id) = 33 AND substr(head_commit_id, 1, 1) = X'12')
        ),
    FOREIGN KEY (layer_stack_id)
        REFERENCES layer_stack(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (layer_stack_id, head_commit_id)
        REFERENCES "commit"(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED
) STRICT, WITHOUT ROWID;

CREATE TABLE scope_allocator (
    scope_id BLOB PRIMARY KEY CHECK (length(scope_id) = 32),
    highwater INTEGER NOT NULL CHECK (highwater BETWEEN 0 AND 9223372036854775807),
    authority_id BLOB NOT NULL CHECK (length(authority_id) = 32)
) STRICT, WITHOUT ROWID;

CREATE TABLE binding_hint (
    child INTEGER NOT NULL CHECK (child != 0),
    parent INTEGER NOT NULL CHECK (parent != 0),
    name BLOB NOT NULL CHECK (length(name) BETWEEN 1 AND 255),
    PRIMARY KEY (child, parent, name)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX layer_stack_names ON layer_stack(name);
CREATE UNIQUE INDEX layer_identity ON layer(layer_stack_id, layer_id);
CREATE UNIQUE INDEX layers_genesis ON layer(layer_stack_id) WHERE parent_layer_id IS NULL;
CREATE UNIQUE INDEX layers_child ON layer(layer_stack_id, parent_layer_id)
    WHERE parent_layer_id IS NOT NULL;
CREATE UNIQUE INDEX layers_source ON layer(source_branch_id, source_commit_id)
    WHERE source_branch_id IS NOT NULL;
CREATE UNIQUE INDEX commit_identity ON "commit"(layer_stack_id, commit_id);
CREATE UNIQUE INDEX branch_identity ON branch(layer_stack_id, branch_id);
CREATE UNIQUE INDEX branch_names ON branch(layer_stack_id, name);

CREATE TRIGGER commit_logical AFTER INSERT ON "commit" BEGIN
 UPDATE store_totals SET logical=logical+new.logical WHERE id=1;
END;
CREATE TRIGGER layer_logical AFTER INSERT ON layer BEGIN
 UPDATE store_totals SET logical=logical+new.logical WHERE id=1;
END;
