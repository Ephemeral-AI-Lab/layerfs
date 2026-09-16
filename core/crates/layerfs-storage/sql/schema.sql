-- Candidate C2 schema: four tables, twenty declared columns.
--
-- This identifier is deliberately not the reference schema 10 application id and
-- user_version: the candidate stores a role column, a nullable direct base and a
-- persisted storage policy, so a file that carries this header must not be
-- mistaken for a schema-10 Store. Version 3 widens the persisted policy CHECK
-- ranges to the supported configurable profile; a version-2 Store is rejected
-- rather than migrated.
PRAGMA application_id = 1279677261;
PRAGMA user_version = 3;

CREATE TABLE store_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    format_profile INTEGER NOT NULL CHECK (format_profile = 1),
    -- Supported cutoffs are powers of two from 128 KiB to 1 MiB; the range is a
    -- necessary condition and the exact power-of-two check runs in the policy.
    small_file_threshold_bytes INTEGER NOT NULL
        CHECK (small_file_threshold_bytes BETWEEN 131072 AND 1048576),
    whole_file_delta_max_depth INTEGER NOT NULL
        CHECK (whole_file_delta_max_depth BETWEEN 0 AND 50),
    chunk_delta_max_depth INTEGER NOT NULL
        CHECK (chunk_delta_max_depth BETWEEN 0 AND 50),
    -- Publication watermark: the highest pack id belonging to a COMPLETED save.
    -- It advances only inside a save's final transaction, so a pack is visible to
    -- an ordinary reader exactly when the save that created it was acknowledged.
    retained_pack_ceiling INTEGER NOT NULL
        CHECK (retained_pack_ceiling >= 0)
) STRICT;

CREATE TABLE object_packs (
    pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),
    data BLOB NOT NULL CHECK (length(data) >= 32)
) STRICT;

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
    object_id BLOB NOT NULL PRIMARY KEY CHECK (length(object_id) = 32),
    object_role INTEGER NOT NULL CHECK (object_role BETWEEN 1 AND 6),
    canonical_length INTEGER NOT NULL
        CHECK (canonical_length > 0 AND canonical_length <= 16777216),
    base_object_id BLOB
        CHECK (
            base_object_id IS NULL
            OR (length(base_object_id) = 32 AND base_object_id <> object_id)
        )
        REFERENCES objects(object_id) ON DELETE NO ACTION,
    pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id),
    group_number INTEGER NOT NULL CHECK (group_number >= 0 AND group_number < 256),
    record_number INTEGER NOT NULL CHECK (record_number >= 0 AND record_number < 8191)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX objects_locations
    ON objects(pack_id, group_number, record_number);

CREATE INDEX objects_bases
    ON objects(base_object_id) WHERE base_object_id IS NOT NULL;
