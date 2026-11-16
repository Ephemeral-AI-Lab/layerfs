CREATE TABLE branch_retired (
    branch_id BLOB PRIMARY KEY
        CHECK (length(branch_id) = 17 AND substr(branch_id, 1, 1) = X'11'),
    removed_ms INTEGER NOT NULL CHECK (removed_ms >= 0),
    FOREIGN KEY (branch_id) REFERENCES branch(branch_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE layer_stack_retired (
    layer_stack_id BLOB PRIMARY KEY
        CHECK (length(layer_stack_id) = 17 AND substr(layer_stack_id, 1, 1) = X'31'),
    deleted_ms INTEGER NOT NULL CHECK (deleted_ms >= 0),
    FOREIGN KEY (layer_stack_id) REFERENCES layer_stack(layer_stack_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE path_removed (
    layer_stack_id BLOB NOT NULL
        CHECK (length(layer_stack_id) = 17 AND substr(layer_stack_id, 1, 1) = X'31'),
    path_key BLOB NOT NULL CHECK (length(path_key) = 32),
    path BLOB NOT NULL CHECK (length(path) BETWEEN 1 AND 4096),
    frontier_serial INTEGER NOT NULL CHECK (frontier_serial >= 0),
    removed_ms INTEGER NOT NULL CHECK (removed_ms >= 0),
    PRIMARY KEY (layer_stack_id, path_key),
    FOREIGN KEY (layer_stack_id) REFERENCES layer_stack(layer_stack_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE project_epoch (
    layer_stack_id BLOB PRIMARY KEY
        CHECK (length(layer_stack_id) = 17 AND substr(layer_stack_id, 1, 1) = X'31'),
    epoch INTEGER NOT NULL DEFAULT 0 CHECK (epoch >= 0),
    FOREIGN KEY (layer_stack_id) REFERENCES layer_stack(layer_stack_id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;

CREATE TABLE gc_intent (
    op_id BLOB PRIMARY KEY CHECK (length(op_id) = 16),
    kind TEXT NOT NULL CHECK (kind IN ('mount','init','merge','removal','compaction')),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('store','project','branch')),
    scope_id BLOB,
    workspace TEXT,
    sandbox TEXT,
    machine TEXT,
    project TEXT,
    branch TEXT,
    owner_runtime TEXT,
    owner_boot TEXT,
    state TEXT NOT NULL CHECK (state IN ('live','uncertain','done')),
    uncertain INTEGER NOT NULL CHECK (uncertain IN (0,1)),
    phase TEXT,
    generation INTEGER,
    artifact TEXT,
    high_water INTEGER,
    evidence TEXT,
    updated_ms INTEGER NOT NULL,
    UNIQUE (scope_kind, scope_id, owner_boot)
) STRICT;

CREATE TABLE gc_op (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    generation INTEGER NOT NULL,
    phase TEXT NOT NULL,
    scratch_schema TEXT,
    mark_serial INTEGER,
    mark_pack_id INTEGER,
    mark_digest BLOB,
    mark_totals BLOB,
    mark_stamp INTEGER,
    started_ms INTEGER NOT NULL,
    dead_canonical INTEGER NOT NULL DEFAULT 0,
    dead_stored INTEGER NOT NULL DEFAULT 0,
    dead_metadata INTEGER NOT NULL DEFAULT 0,
    dead_logical INTEGER NOT NULL DEFAULT 0,
    totals_applied INTEGER NOT NULL DEFAULT 0,
    artifact TEXT,
    pid INTEGER,
    backend_start TEXT,
    incarnation INTEGER
) STRICT;

CREATE TABLE gc_dead (
    kind TEXT NOT NULL,
    id BLOB NOT NULL,
    PRIMARY KEY (kind, id)
) STRICT, WITHOUT ROWID;
