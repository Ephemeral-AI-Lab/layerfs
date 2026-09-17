"""Independent old-schema Store builder: same file format identity (application_id
1279677261, user_version 4, four tables, twenty-one columns) but the PREVIOUS
narrow object_role ceiling. Uses only the Python standard library."""
import os, sqlite3, sys

path = sys.argv[1]
os.makedirs(os.path.dirname(path), exist_ok=True)
if os.path.exists(path):
    os.remove(path)
connection = sqlite3.connect(path)
connection.executescript("""
PRAGMA application_id = 1279677261;
PRAGMA user_version = 4;
CREATE TABLE store_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    format_profile INTEGER NOT NULL CHECK (format_profile = 1),
    small_file_threshold_bytes INTEGER NOT NULL CHECK (small_file_threshold_bytes BETWEEN 131072 AND 1048576),
    whole_file_delta_max_depth INTEGER NOT NULL CHECK (whole_file_delta_max_depth BETWEEN 0 AND 50),
    chunk_delta_max_depth INTEGER NOT NULL CHECK (chunk_delta_max_depth BETWEEN 0 AND 50),
    metadata_delta_max_depth INTEGER NOT NULL CHECK (metadata_delta_max_depth BETWEEN 0 AND 50),
    retained_pack_ceiling INTEGER NOT NULL CHECK (retained_pack_ceiling >= 0)
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
    canonical_length INTEGER NOT NULL CHECK (canonical_length > 0 AND canonical_length <= 16777216),
    base_object_id BLOB CHECK (base_object_id IS NULL OR (length(base_object_id) = 32 AND base_object_id <> object_id)) REFERENCES objects(object_id) ON DELETE NO ACTION,
    pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id),
    group_number INTEGER NOT NULL CHECK (group_number >= 0 AND group_number < 256),
    record_number INTEGER NOT NULL CHECK (record_number >= 0 AND record_number < 8191)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX objects_locations ON objects(pack_id, group_number, record_number);
CREATE INDEX objects_bases ON objects(base_object_id) WHERE base_object_id IS NOT NULL;
INSERT INTO store_policy VALUES (1, 1, 1048576, 50, 50, 50, 0);
""")
connection.commit()
row = connection.execute("select sql from sqlite_master where name='objects'").fetchone()[0]
print("OLD_SCHEMA role clause:", [line.strip() for line in row.splitlines() if "object_role" in line])
connection.execute("INSERT INTO object_packs VALUES (1, ?)", (b"x" * 32,))
connection.commit()
for role in (1, 6, 10, 13):
    try:
        connection.execute(
            "INSERT INTO objects VALUES (?,?,?,?,?,?,?)",
            (bytes([role]) * 32, role, 8, None, 1, 0, role),
        )
        connection.commit()
        print(f"OLD_SCHEMA insert role={role}: ACCEPTED")
    except Exception as error:
        print(f"OLD_SCHEMA insert role={role}: REFUSED ({error})")
connection.close()
