-- Candidate schema 8: save-owned physical data, atomic per-save publication and
-- the configured per-Store writer budget.
-- Canonical profile 1 and pack/codec formats are unchanged. Older schemas are
-- rejected, never migrated. Duplicate locators and bytes are permitted.
PRAGMA application_id = 1279677261;
PRAGMA user_version = 8;

CREATE TABLE store_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    format_profile INTEGER NOT NULL CHECK (format_profile = 1),
    small_file_threshold_bytes INTEGER NOT NULL
        CHECK (small_file_threshold_bytes BETWEEN 131072 AND 1048576),
    whole_file_delta_max_depth INTEGER NOT NULL
        CHECK (whole_file_delta_max_depth BETWEEN 0 AND 50),
    chunk_delta_max_depth INTEGER NOT NULL
        CHECK (chunk_delta_max_depth BETWEEN 0 AND 50),
    metadata_delta_max_depth INTEGER NOT NULL
        CHECK (metadata_delta_max_depth BETWEEN 0 AND 50),
    -- Authoritative writer budget of this Store (#216). One number, shared by
    -- every sandbox and process that opens the file; it is the admission limit,
    -- while `saves.active_slot` below spans the wider supported slot space.
    max_concurrent_writes INTEGER NOT NULL CHECK (max_concurrent_writes BETWEEN 1 AND 64),
    publication_sequence INTEGER NOT NULL DEFAULT 0 CHECK (publication_sequence >= 0),
    retained_pack_ceiling INTEGER NOT NULL DEFAULT 0 CHECK (retained_pack_ceiling >= 0),
    next_pack_id INTEGER NOT NULL DEFAULT 1 CHECK (next_pack_id > 0),
    next_ordinal INTEGER NOT NULL DEFAULT 1 CHECK (next_ordinal BETWEEN 1 AND 4294967296),
    metadata_window_start INTEGER NOT NULL DEFAULT 1 CHECK (metadata_window_start > 0),
    metadata_window_values INTEGER NOT NULL DEFAULT 0 CHECK (metadata_window_values BETWEEN 0 AND 131072)
) STRICT;

CREATE TABLE saves (
    save_id INTEGER PRIMARY KEY AUTOINCREMENT,
    -- 64 is the supported slot space (MAX_CONCURRENT_WRITES_LIMIT), not the
    -- current budget: rows written under a higher setting stay valid after it is
    -- lowered. Admission is bounded by store_policy.max_concurrent_writes.
    active_slot INTEGER UNIQUE CHECK (active_slot BETWEEN 1 AND 64),
    publication INTEGER UNIQUE CHECK (publication > 0),
    pack_ceiling INTEGER NOT NULL DEFAULT 0 CHECK (pack_ceiling >= 0),
    CHECK ((active_slot IS NULL) != (publication IS NULL))
) STRICT;

CREATE TABLE object_packs (
    pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),
    save_id INTEGER NOT NULL REFERENCES saves(save_id),
    data BLOB NOT NULL CHECK (length(data) BETWEEN 32 AND 16781312)
) STRICT;
CREATE INDEX packs_save ON object_packs(save_id, pack_id);

CREATE TABLE metadata_value_groups (
    first_ordinal INTEGER PRIMARY KEY CHECK (first_ordinal BETWEEN 1 AND 4294967295),
    count INTEGER NOT NULL CHECK (count BETWEEN 1 AND 165),
    pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id) ON DELETE CASCADE,
    group_number INTEGER NOT NULL CHECK (group_number BETWEEN 0 AND 255),
    digest BLOB NOT NULL CHECK (length(digest) = 32),
    CHECK (first_ordinal + count <= 4294967296),
    UNIQUE (pack_id, group_number)
) STRICT;

CREATE TABLE objects (
    object_id BLOB NOT NULL CHECK (length(object_id) = 32),
    save_id INTEGER NOT NULL REFERENCES saves(save_id),
    object_role INTEGER NOT NULL CHECK (object_role BETWEEN 1 AND 13),
    canonical_length INTEGER NOT NULL
        CHECK (canonical_length > 0 AND canonical_length <= 16777216),
    pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id),
    group_number INTEGER NOT NULL CHECK (group_number >= 0 AND group_number < 256),
    record_number INTEGER NOT NULL CHECK (record_number >= 0 AND record_number < 8191),
    PRIMARY KEY (object_id, save_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX objects_save ON objects(save_id, object_id);

-- Disposable hints remain a bounded ring. An overwritten hint may reduce later
-- compression opportunities; it can never make private content eligible.
CREATE TABLE content_signatures (
    slot INTEGER PRIMARY KEY CHECK (slot BETWEEN 0 AND 8191),
    stamp INTEGER NOT NULL CHECK (stamp > 0),
    object_id BLOB NOT NULL CHECK (length(object_id) = 32),
    signature BLOB NOT NULL CHECK (length(signature) = 32),
    save_id INTEGER NOT NULL REFERENCES saves(save_id)
) STRICT;
CREATE INDEX signatures_save ON content_signatures(save_id, slot);
