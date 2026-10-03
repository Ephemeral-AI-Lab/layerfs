-- C5 schema 1, independent of C2. No cross-component foreign keys.
BEGIN;
CREATE SCHEMA IF NOT EXISTS ${schema};
CREATE TABLE ${schema}.history_meta (
    id BIGINT PRIMARY KEY CHECK (id = 1),
    catalog_id BYTEA NOT NULL CHECK (octet_length(catalog_id) = 32),
    catalog_incarnation BIGINT NOT NULL
        CHECK (catalog_incarnation BETWEEN 1 AND 9223372036854775807),
    binding_key BYTEA NOT NULL CHECK (octet_length(binding_key) BETWEEN 1 AND 128),
    identity_format BIGINT NOT NULL CHECK (identity_format = 1),
    schema_version BIGINT NOT NULL CHECK (schema_version = 1),
    schema_source TEXT NOT NULL,
    schema_definition TEXT NOT NULL,
    next_stage_token BIGINT NOT NULL
        CHECK (next_stage_token BETWEEN 1 AND 9223372036854775807)
);

CREATE TABLE ${schema}.layer_stack (
    layer_stack_id BYTEA PRIMARY KEY
        CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
    name TEXT COLLATE "C" NOT NULL
        CHECK (octet_length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name ~ '^[a-z0-9][a-z0-9._-]*[a-z0-9]$' OR name ~ '^[a-z0-9]$'),
    scope_id BYTEA NOT NULL CHECK (octet_length(scope_id) = 32),
    profile_id BYTEA NOT NULL CHECK (octet_length(profile_id) = 32),
    head_layer_id BYTEA NOT NULL
        CHECK (octet_length(head_layer_id) = 33 AND get_byte(head_layer_id, 0) = 50)
);

CREATE TABLE ${schema}.layer (
    layer_id BYTEA PRIMARY KEY
        CHECK (octet_length(layer_id) = 33 AND get_byte(layer_id, 0) = 50),
    layer_stack_id BYTEA NOT NULL
        CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
    parent_layer_id BYTEA
        CHECK (
            parent_layer_id IS NULL
            OR (octet_length(parent_layer_id) = 33 AND get_byte(parent_layer_id, 0) = 50)
        ),
    root_id BYTEA NOT NULL CHECK (octet_length(root_id) = 32),
    source_branch_id BYTEA
        CHECK (
            source_branch_id IS NULL
            OR (octet_length(source_branch_id) = 17 AND get_byte(source_branch_id, 0) = 17)
        ),
    source_commit_id BYTEA
        CHECK (
            source_commit_id IS NULL
            OR (octet_length(source_commit_id) = 33 AND get_byte(source_commit_id, 0) = 18)
        ),
    CHECK (
        (parent_layer_id IS NULL AND source_branch_id IS NULL AND source_commit_id IS NULL)
        OR
        (parent_layer_id IS NOT NULL AND source_branch_id IS NOT NULL AND source_commit_id IS NOT NULL)
    )
);

CREATE TABLE ${schema}.commit (
    commit_id BYTEA PRIMARY KEY
        CHECK (octet_length(commit_id) = 33 AND get_byte(commit_id, 0) = 18),
    layer_stack_id BYTEA NOT NULL
        CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
    root_id BYTEA NOT NULL CHECK (octet_length(root_id) = 32),
    parent_commit_id BYTEA
        CHECK (
            parent_commit_id IS NULL
            OR (octet_length(parent_commit_id) = 33 AND get_byte(parent_commit_id, 0) = 18)
        ),
    base_layer_id BYTEA NOT NULL
        CHECK (octet_length(base_layer_id) = 33 AND get_byte(base_layer_id, 0) = 50)
);

CREATE TABLE ${schema}.branch (
    branch_id BYTEA PRIMARY KEY
        CHECK (octet_length(branch_id) = 17 AND get_byte(branch_id, 0) = 17),
    layer_stack_id BYTEA NOT NULL
        CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
    name TEXT COLLATE "C" NOT NULL
        CHECK (octet_length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name ~ '^[a-z0-9][a-z0-9._-]*[a-z0-9]$' OR name ~ '^[a-z0-9]$'),
    base_layer_id BYTEA NOT NULL
        CHECK (octet_length(base_layer_id) = 33 AND get_byte(base_layer_id, 0) = 50),
    head_commit_id BYTEA
        CHECK (
            head_commit_id IS NULL
            OR (octet_length(head_commit_id) = 33 AND get_byte(head_commit_id, 0) = 18)
        )
);

CREATE TABLE ${schema}.workspace_stage (
    workspace_id BYTEA PRIMARY KEY CHECK (octet_length(workspace_id) = 32),
    stage_token BIGINT NOT NULL
        CHECK (stage_token BETWEEN 1 AND 9223372036854775807),
    layer_stack_id BYTEA NOT NULL
        CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
    branch_id BYTEA NOT NULL
        CHECK (octet_length(branch_id) = 17 AND get_byte(branch_id, 0) = 17),
    expected_head_commit_id BYTEA
        CHECK (
            expected_head_commit_id IS NULL
            OR (octet_length(expected_head_commit_id) = 33 AND get_byte(expected_head_commit_id, 0) = 18)
        ),
    expected_base_layer_id BYTEA NOT NULL
        CHECK (octet_length(expected_base_layer_id) = 33 AND get_byte(expected_base_layer_id, 0) = 50),
    expected_root_id BYTEA NOT NULL CHECK (octet_length(expected_root_id) = 32),
    construction_base_root_id BYTEA NOT NULL CHECK (octet_length(construction_base_root_id) = 32),
    intended_commit_base_layer_id BYTEA NOT NULL
        CHECK (octet_length(intended_commit_base_layer_id) = 33 AND get_byte(intended_commit_base_layer_id, 0) = 50),
    candidate_root_id BYTEA NOT NULL CHECK (octet_length(candidate_root_id) = 32),
    profile_id BYTEA NOT NULL CHECK (octet_length(profile_id) = 32),
    scope_id BYTEA NOT NULL CHECK (octet_length(scope_id) = 32),
    input_generation BIGINT NOT NULL CHECK (input_generation >= 0)
);

CREATE TABLE ${schema}.scope_allocator (
    scope_id BYTEA PRIMARY KEY CHECK (octet_length(scope_id) = 32),
    highwater BIGINT NOT NULL CHECK (highwater BETWEEN 0 AND 9223372036854775807),
    authority_id BYTEA NOT NULL CHECK (octet_length(authority_id) = 32)
);

CREATE UNIQUE INDEX layer_stack_names ON ${schema}.layer_stack(name);
CREATE UNIQUE INDEX layer_identity ON ${schema}.layer(layer_stack_id, layer_id);
CREATE UNIQUE INDEX layers_genesis ON ${schema}.layer(layer_stack_id) WHERE parent_layer_id IS NULL;
CREATE UNIQUE INDEX layers_child ON ${schema}.layer(layer_stack_id, parent_layer_id)
    WHERE parent_layer_id IS NOT NULL;
CREATE UNIQUE INDEX layers_source ON ${schema}.layer(source_branch_id, source_commit_id)
    WHERE source_branch_id IS NOT NULL;
CREATE UNIQUE INDEX commit_identity ON ${schema}.commit(layer_stack_id, commit_id);
CREATE UNIQUE INDEX branch_identity ON ${schema}.branch(layer_stack_id, branch_id);
CREATE UNIQUE INDEX branch_names ON ${schema}.branch(layer_stack_id, name);
CREATE UNIQUE INDEX stage_tokens ON ${schema}.workspace_stage(stage_token);
CREATE INDEX stage_branches ON ${schema}.workspace_stage(layer_stack_id, branch_id, stage_token);

ALTER TABLE ${schema}.layer_stack ADD FOREIGN KEY (layer_stack_id, head_layer_id)
        REFERENCES ${schema}.layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.layer ADD FOREIGN KEY (layer_stack_id)
        REFERENCES ${schema}.layer_stack(layer_stack_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.layer ADD FOREIGN KEY (layer_stack_id, parent_layer_id)
        REFERENCES ${schema}.layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.layer ADD FOREIGN KEY (layer_stack_id, source_branch_id)
        REFERENCES ${schema}.branch(layer_stack_id, branch_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.layer ADD FOREIGN KEY (layer_stack_id, source_commit_id)
        REFERENCES ${schema}.commit(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.commit ADD FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES ${schema}.layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.commit ADD FOREIGN KEY (layer_stack_id, parent_commit_id)
        REFERENCES ${schema}.commit(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.branch ADD FOREIGN KEY (layer_stack_id)
        REFERENCES ${schema}.layer_stack(layer_stack_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.branch ADD FOREIGN KEY (layer_stack_id, base_layer_id)
        REFERENCES ${schema}.layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.branch ADD FOREIGN KEY (layer_stack_id, head_commit_id)
        REFERENCES ${schema}.commit(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.workspace_stage ADD FOREIGN KEY (layer_stack_id, branch_id)
        REFERENCES ${schema}.branch(layer_stack_id, branch_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.workspace_stage ADD FOREIGN KEY (layer_stack_id, expected_base_layer_id)
        REFERENCES ${schema}.layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.workspace_stage ADD FOREIGN KEY (layer_stack_id, intended_commit_base_layer_id)
        REFERENCES ${schema}.layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE ${schema}.workspace_stage ADD FOREIGN KEY (layer_stack_id, expected_head_commit_id)
        REFERENCES ${schema}.commit(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
COMMIT;
