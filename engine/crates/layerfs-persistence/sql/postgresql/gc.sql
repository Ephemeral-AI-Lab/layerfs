
CREATE TABLE branch_retired (
    branch_id BYTEA PRIMARY KEY CHECK (length(branch_id) = 17),
    removed_ms BIGINT NOT NULL CHECK (removed_ms >= 0),
    FOREIGN KEY (branch_id) REFERENCES branch(branch_id) ON DELETE CASCADE
);
CREATE TABLE layer_stack_retired (
    layer_stack_id BYTEA PRIMARY KEY CHECK (length(layer_stack_id) = 17),
    deleted_ms BIGINT NOT NULL CHECK (deleted_ms >= 0),
    FOREIGN KEY (layer_stack_id) REFERENCES layer_stack(layer_stack_id) ON DELETE CASCADE
);
CREATE TABLE path_removed (
    layer_stack_id BYTEA NOT NULL CHECK (length(layer_stack_id) = 17),
    path_key BYTEA NOT NULL CHECK (length(path_key) = 32),
    path BYTEA NOT NULL CHECK (length(path) BETWEEN 1 AND 4096),
    frontier_serial BIGINT NOT NULL CHECK (frontier_serial >= 0),
    removed_ms BIGINT NOT NULL CHECK (removed_ms >= 0),
    PRIMARY KEY (layer_stack_id, path_key),
    FOREIGN KEY (layer_stack_id) REFERENCES layer_stack(layer_stack_id) ON DELETE CASCADE
);
CREATE TABLE project_epoch (
    layer_stack_id BYTEA PRIMARY KEY CHECK (length(layer_stack_id) = 17),
    epoch BIGINT NOT NULL DEFAULT 0 CHECK (epoch >= 0),
    FOREIGN KEY (layer_stack_id) REFERENCES layer_stack(layer_stack_id) ON DELETE CASCADE
);
CREATE TABLE gc_intent (
    op_id BYTEA PRIMARY KEY CHECK (length(op_id) = 16),
    kind TEXT NOT NULL CHECK (kind IN ('mount','init','merge','removal','compaction')),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('store','project','branch')),
    scope_id BYTEA,
    workspace TEXT,
    sandbox TEXT,
    machine TEXT,
    project TEXT,
    branch TEXT,
    owner_runtime TEXT,
    owner_boot TEXT,
    state TEXT NOT NULL CHECK (state IN ('live','uncertain','done')),
    uncertain BOOLEAN NOT NULL,
    phase TEXT,
    generation BIGINT,
    artifact TEXT,
    high_water BIGINT,
    evidence TEXT,
    updated_ms BIGINT NOT NULL,
    UNIQUE (scope_kind, scope_id, owner_boot)
);
CREATE TABLE gc_op (
    id BIGINT PRIMARY KEY CHECK (id = 1),
    generation BIGINT NOT NULL,
    phase TEXT NOT NULL,
    scratch_schema TEXT,
    mark_serial BIGINT,
    mark_pack_id BIGINT,
    mark_digest BYTEA,
    mark_totals BYTEA,
    mark_stamp BIGINT,
    started_ms BIGINT NOT NULL,
    dead_canonical BIGINT NOT NULL DEFAULT 0,
    dead_stored BIGINT NOT NULL DEFAULT 0,
    dead_metadata BIGINT NOT NULL DEFAULT 0,
    dead_logical BIGINT NOT NULL DEFAULT 0,
    totals_applied BOOLEAN NOT NULL DEFAULT FALSE,
    artifact TEXT,
    pid BIGINT,
    backend_start TEXT,
    incarnation BIGINT
);
CREATE TABLE gc_dead (
    kind TEXT NOT NULL,
    id BYTEA NOT NULL,
    PRIMARY KEY (kind, id)
);
