CREATE TABLE store_lane (
 id bigint PRIMARY KEY CHECK (id = 1)
);
INSERT INTO store_lane (id) VALUES (1);
CREATE TABLE store_policy (
 id bigint PRIMARY KEY CHECK (id = 1),
 schema_version bigint NOT NULL CHECK (schema_version = 1),
 format_profile bigint NOT NULL CHECK (format_profile = 1),
 small_file_threshold_bytes bigint NOT NULL CHECK (small_file_threshold_bytes BETWEEN 131072 AND 1048576),
 whole_file_delta_max_depth bigint NOT NULL CHECK (whole_file_delta_max_depth BETWEEN 0 AND 50),
 chunk_delta_max_depth bigint NOT NULL CHECK (chunk_delta_max_depth BETWEEN 0 AND 50),
 metadata_delta_max_depth bigint NOT NULL CHECK (metadata_delta_max_depth BETWEEN 0 AND 50),
 next_pack_id bigint NOT NULL DEFAULT 1 CHECK (next_pack_id > 0),
 next_ordinal bigint NOT NULL DEFAULT 1 CHECK (next_ordinal BETWEEN 1 AND 4294967296)
);
CREATE TABLE pack (
 pack_id bigint PRIMARY KEY CHECK (pack_id > 0),
 domain bigint NOT NULL CHECK (domain IN (0, 1)),
 digest bytea NOT NULL CHECK (octet_length(digest) = 32),
 length bigint NOT NULL CHECK (length BETWEEN 32 AND 16781312),
 body bytea NOT NULL CHECK (octet_length(body) = length),
 CHECK (domain = 1 OR length <= 262144)
);
ALTER TABLE pack ALTER COLUMN body SET STORAGE EXTERNAL;
CREATE FUNCTION pack_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN RAISE EXCEPTION 'published pack immutable'; END $$;
CREATE TRIGGER pack_immutable BEFORE UPDATE OR DELETE ON pack
 FOR EACH ROW EXECUTE FUNCTION pack_immutable();
CREATE TABLE object_location (
 object_id bytea PRIMARY KEY CHECK (octet_length(object_id) = 32),
 role bigint NOT NULL CHECK (role BETWEEN 1 AND 13),
 canonical_length bigint NOT NULL CHECK (canonical_length BETWEEN 1 AND 16777216),
 pack_id bigint NOT NULL REFERENCES pack (pack_id),
 group_number bigint NOT NULL CHECK (group_number BETWEEN 0 AND 255),
 record_number bigint NOT NULL CHECK (record_number BETWEEN 0 AND 8190)
);
CREATE TABLE inode_value (
 ordinal bigint PRIMARY KEY CHECK (ordinal BETWEEN 1 AND 4294967295),
 value bytea NOT NULL CHECK (octet_length(value) = 73)
);
CREATE TABLE content_signature (
 slot bigint PRIMARY KEY CHECK (slot BETWEEN 0 AND 8191),
 stamp bigint NOT NULL CHECK (stamp > 0 AND (stamp - 1) % 8192 = slot),
 object_id bytea NOT NULL REFERENCES object_location (object_id),
 signature bytea NOT NULL CHECK (octet_length(signature) = 32)
);
CREATE TABLE store_totals (
 id bigint PRIMARY KEY CHECK (id = 1),
 logical bigint NOT NULL DEFAULT 0 CHECK (logical >= 0),
 canonical bigint NOT NULL DEFAULT 0 CHECK (canonical >= 0),
 stored bigint NOT NULL DEFAULT 0 CHECK (stored >= 0),
 metadata bigint NOT NULL DEFAULT 0 CHECK (metadata >= 0),
 init_entries bigint NOT NULL DEFAULT 0 CHECK (init_entries >= 0),
 init_files bigint NOT NULL DEFAULT 0 CHECK (init_files >= 0),
 init_bytes bigint NOT NULL DEFAULT 0 CHECK (init_bytes >= 0),
 init_canonical bigint NOT NULL DEFAULT 0 CHECK (init_canonical >= 0),
 init_stored bigint NOT NULL DEFAULT 0 CHECK (init_stored >= 0),
 init_metadata bigint NOT NULL DEFAULT 0 CHECK (init_metadata >= 0),
 init_millis bigint NOT NULL DEFAULT 0 CHECK (init_millis >= 0)
);
INSERT INTO store_totals (id) VALUES (1);
CREATE TABLE history_meta (
 id bigint PRIMARY KEY CHECK (id = 1),
 catalog_id bytea NOT NULL CHECK (octet_length(catalog_id) = 32),
 catalog_incarnation bigint NOT NULL CHECK (catalog_incarnation BETWEEN 1 AND 9223372036854775807),
 binding_key bytea NOT NULL CHECK (octet_length(binding_key) BETWEEN 1 AND 128),
 identity_format bigint NOT NULL CHECK (identity_format = 3),
 schema_version bigint NOT NULL CHECK (schema_version = 1),
 schema_source text NOT NULL,
 schema_definition text NOT NULL
);
CREATE TABLE layer_stack (
 layer_stack_id bytea PRIMARY KEY
  CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
 name text COLLATE "C" NOT NULL
  CHECK (length(name) BETWEEN 1 AND 63 AND name ~ '^[a-z0-9]([a-z0-9._-]*[a-z0-9])?$'),
 scope_id bytea NOT NULL CHECK (octet_length(scope_id) = 32),
 profile_id bytea NOT NULL CHECK (octet_length(profile_id) = 32),
 head_layer_id bytea NOT NULL
  CHECK (octet_length(head_layer_id) = 33 AND get_byte(head_layer_id, 0) = 50)
);
CREATE TABLE layer (
 layer_id bytea PRIMARY KEY CHECK (octet_length(layer_id) = 33 AND get_byte(layer_id, 0) = 50),
 layer_stack_id bytea NOT NULL
  CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
 parent_layer_id bytea
  CHECK (parent_layer_id IS NULL OR (octet_length(parent_layer_id) = 33 AND get_byte(parent_layer_id, 0) = 50)),
 root_id bytea NOT NULL CHECK (octet_length(root_id) = 32),
 source_branch_id bytea
  CHECK (source_branch_id IS NULL OR (octet_length(source_branch_id) = 17 AND get_byte(source_branch_id, 0) = 17)),
 logical bigint NOT NULL DEFAULT 0 CHECK (logical >= 0),
 overwrote bytea NOT NULL,
 source_commit_id bytea
  CHECK (source_commit_id IS NULL OR (octet_length(source_commit_id) = 33 AND get_byte(source_commit_id, 0) = 18)),
 CHECK (
  (parent_layer_id IS NULL AND source_branch_id IS NULL AND source_commit_id IS NULL)
  OR (parent_layer_id IS NOT NULL AND source_branch_id IS NOT NULL AND source_commit_id IS NOT NULL)
 ),
 UNIQUE (layer_stack_id, layer_id)
);
CREATE TABLE "commit" (
 commit_id bytea PRIMARY KEY CHECK (octet_length(commit_id) = 33 AND get_byte(commit_id, 0) = 18),
 layer_stack_id bytea NOT NULL
  CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
 root_id bytea NOT NULL CHECK (octet_length(root_id) = 32),
 parent_commit_id bytea
  CHECK (parent_commit_id IS NULL OR (octet_length(parent_commit_id) = 33 AND get_byte(parent_commit_id, 0) = 18)),
 base_layer_id bytea NOT NULL
  CHECK (octet_length(base_layer_id) = 33 AND get_byte(base_layer_id, 0) = 50),
 second_parent_id bytea
  CHECK (second_parent_id IS NULL OR (octet_length(second_parent_id) = 33 AND get_byte(second_parent_id, 0) IN (18, 50))),
 branch_id bytea
  CHECK (branch_id IS NULL OR (octet_length(branch_id) = 17 AND get_byte(branch_id, 0) = 17)),
 via bigint NOT NULL CHECK (via IN (1, 2, 3)),
 message text CHECK (message IS NULL OR octet_length(message) <= 4096),
 merged_from_id bytea
  CHECK (merged_from_id IS NULL OR (octet_length(merged_from_id) = 33 AND get_byte(merged_from_id, 0) IN (18, 50))),
 created_ms bigint NOT NULL,
 logical bigint NOT NULL DEFAULT 0 CHECK (logical >= 0),
 new_canonical bigint NOT NULL DEFAULT 0 CHECK (new_canonical >= 0),
 new_stored bigint NOT NULL DEFAULT 0 CHECK (new_stored >= 0),
 new_metadata bigint NOT NULL DEFAULT 0 CHECK (new_metadata >= 0),
 lww bytea NOT NULL,
 displaced_id bytea CHECK (displaced_id IS NULL OR octet_length(displaced_id) = 33),
 UNIQUE (layer_stack_id, commit_id)
);
CREATE TABLE branch (
 branch_id bytea PRIMARY KEY CHECK (octet_length(branch_id) = 17 AND get_byte(branch_id, 0) = 17),
 layer_stack_id bytea NOT NULL
  CHECK (octet_length(layer_stack_id) = 17 AND get_byte(layer_stack_id, 0) = 49),
 name text COLLATE "C" NOT NULL
  CHECK (length(name) BETWEEN 1 AND 63 AND name ~ '^[a-z0-9]([a-z0-9._-]*[a-z0-9])?$'),
 base_layer_id bytea NOT NULL
  CHECK (octet_length(base_layer_id) = 33 AND get_byte(base_layer_id, 0) = 50),
 forked_from_id bytea CHECK (forked_from_id IS NULL OR octet_length(forked_from_id) = 33),
 head_commit_id bytea
  CHECK (head_commit_id IS NULL OR (octet_length(head_commit_id) = 33 AND get_byte(head_commit_id, 0) = 18)),
 UNIQUE (layer_stack_id, branch_id),
 UNIQUE (layer_stack_id, name)
);
ALTER TABLE layer_stack ADD FOREIGN KEY (layer_stack_id, head_layer_id)
 REFERENCES layer (layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE layer ADD FOREIGN KEY (layer_stack_id)
 REFERENCES layer_stack (layer_stack_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE layer ADD FOREIGN KEY (layer_stack_id, parent_layer_id)
 REFERENCES layer (layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE layer ADD FOREIGN KEY (layer_stack_id, source_branch_id)
 REFERENCES branch (layer_stack_id, branch_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE layer ADD FOREIGN KEY (layer_stack_id, source_commit_id)
 REFERENCES "commit" (layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE "commit" ADD FOREIGN KEY (layer_stack_id, base_layer_id)
 REFERENCES layer (layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE "commit" ADD FOREIGN KEY (layer_stack_id, parent_commit_id)
 REFERENCES "commit" (layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE branch ADD FOREIGN KEY (layer_stack_id)
 REFERENCES layer_stack (layer_stack_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE branch ADD FOREIGN KEY (layer_stack_id, base_layer_id)
 REFERENCES layer (layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE branch ADD FOREIGN KEY (layer_stack_id, head_commit_id)
 REFERENCES "commit" (layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
CREATE TABLE scope_allocator (
 scope_id bytea PRIMARY KEY CHECK (octet_length(scope_id) = 32),
 highwater bigint NOT NULL CHECK (highwater BETWEEN 0 AND 9223372036854775807),
 authority_id bytea NOT NULL CHECK (octet_length(authority_id) = 32)
);
CREATE TABLE binding_hint (
 child bigint NOT NULL CHECK (child != 0),
 parent bigint NOT NULL CHECK (parent != 0),
 name bytea NOT NULL CHECK (octet_length(name) BETWEEN 1 AND 255),
 PRIMARY KEY (child, parent, name)
);
CREATE UNIQUE INDEX layer_stack_names ON layer_stack (name);
CREATE UNIQUE INDEX layers_genesis ON layer (layer_stack_id) WHERE parent_layer_id IS NULL;
CREATE UNIQUE INDEX layers_child ON layer (layer_stack_id, parent_layer_id)
 WHERE parent_layer_id IS NOT NULL;
CREATE UNIQUE INDEX layers_source ON layer (source_branch_id, source_commit_id)
 WHERE source_branch_id IS NOT NULL;
CREATE FUNCTION add_logical() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN UPDATE store_totals SET logical = logical + NEW.logical WHERE id = 1; RETURN NULL; END $$;
CREATE TRIGGER commit_logical AFTER INSERT ON "commit" FOR EACH ROW EXECUTE FUNCTION add_logical();
CREATE TRIGGER layer_logical AFTER INSERT ON layer FOR EACH ROW EXECUTE FUNCTION add_logical();
CREATE TABLE layer_lineage (
 layer_id bytea PRIMARY KEY CHECK (octet_length(layer_id) = 33 AND get_byte(layer_id, 0) = 50),
 layer_stack_id bytea NOT NULL CHECK (octet_length(layer_stack_id) = 17),
 height bigint NOT NULL CHECK (height >= 0),
 overwrote_count bigint NOT NULL CHECK (overwrote_count >= 0)
);
CREATE TABLE commit_lineage (
 commit_id bytea PRIMARY KEY CHECK (octet_length(commit_id) = 33 AND get_byte(commit_id, 0) = 18),
 run bytea NOT NULL CHECK (octet_length(run) = 33),
 depth bigint NOT NULL CHECK (depth >= 1),
 newest bigint NOT NULL CHECK (newest >= 0),
 merged bytea CHECK (merged IS NULL OR octet_length(merged) = 33)
);
CREATE TABLE absorbed (
 run bytea NOT NULL CHECK (octet_length(run) = 33),
 depth bigint NOT NULL CHECK (depth >= 1),
 height bigint NOT NULL CHECK (height >= 0),
 PRIMARY KEY (run, depth)
);
CREATE TABLE reach (
 key bytea NOT NULL,
 x bigint NOT NULL CHECK (x >= 0),
 height bigint NOT NULL CHECK (height >= 0),
 PRIMARY KEY (key, x)
);
CREATE UNIQUE INDEX layer_heights ON layer_lineage (layer_stack_id, height);
CREATE UNIQUE INDEX commit_runs ON commit_lineage (run, depth);
CREATE INDEX run_merges ON commit_lineage (run, depth) WHERE merged IS NOT NULL;
CREATE INDEX absorbed_heights ON absorbed (run, height);
CREATE TABLE file_digest (
 digest bytea NOT NULL CHECK (octet_length(digest) = 32),
 length bigint NOT NULL CHECK (length >= 0),
 root bytea NOT NULL CHECK (octet_length(root) = 32),
 PRIMARY KEY (digest, length)
);
