CREATE TABLE store_policy (
 id INTEGER PRIMARY KEY CHECK(id=1), schema_version INTEGER NOT NULL CHECK(schema_version=1),
 format_profile INTEGER NOT NULL CHECK(format_profile=1),
 small_file_threshold_bytes INTEGER NOT NULL CHECK(small_file_threshold_bytes BETWEEN 131072 AND 1048576),
 whole_file_delta_max_depth INTEGER NOT NULL CHECK(whole_file_delta_max_depth BETWEEN 0 AND 50),
 chunk_delta_max_depth INTEGER NOT NULL CHECK(chunk_delta_max_depth BETWEEN 0 AND 50),
 metadata_delta_max_depth INTEGER NOT NULL CHECK(metadata_delta_max_depth BETWEEN 0 AND 50),
 next_pack_id INTEGER NOT NULL DEFAULT 1 CHECK(next_pack_id>0),
 next_ordinal INTEGER NOT NULL DEFAULT 1 CHECK(next_ordinal BETWEEN 1 AND 4294967296),
 metadata_window_start INTEGER NOT NULL DEFAULT 1 CHECK(metadata_window_start BETWEEN 1 AND 4294967295),
 metadata_window_values INTEGER NOT NULL DEFAULT 0 CHECK(metadata_window_values BETWEEN 0 AND 131072)
) STRICT;
CREATE TABLE object_location (
 object_id BLOB PRIMARY KEY CHECK(length(object_id)=32),
 role INTEGER NOT NULL CHECK(role BETWEEN 1 AND 13),
 canonical_length INTEGER NOT NULL CHECK(canonical_length BETWEEN 1 AND 16777216),
 pack_id INTEGER NOT NULL REFERENCES pack(pack_id),
 group_number INTEGER NOT NULL CHECK(group_number BETWEEN 0 AND 255),
 record_number INTEGER NOT NULL CHECK(record_number BETWEEN 0 AND 8190)
) STRICT, WITHOUT ROWID;
CREATE TABLE metadata_value_group (
 first_ordinal INTEGER PRIMARY KEY CHECK(first_ordinal BETWEEN 1 AND 4294967295),
 count INTEGER NOT NULL CHECK(count BETWEEN 1 AND 165),
 pack_id INTEGER NOT NULL REFERENCES pack(pack_id),
 group_number INTEGER NOT NULL CHECK(group_number BETWEEN 0 AND 255),
 digest BLOB NOT NULL CHECK(length(digest)=32),
 CHECK(first_ordinal+count<=4294967296), UNIQUE(pack_id,group_number)
) STRICT;
CREATE TABLE content_signature (
 slot INTEGER PRIMARY KEY CHECK(slot BETWEEN 0 AND 8191),
 stamp INTEGER NOT NULL CHECK(stamp>0 AND (stamp-1)%8192=slot),
 object_id BLOB NOT NULL REFERENCES object_location(object_id),
 signature BLOB NOT NULL CHECK(length(signature)=32)
) STRICT;
