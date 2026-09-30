/* Private LFCS v1 construction state. No Store schema or payload CAS. */
PRAGMA page_size = 4096;
PRAGMA application_id = 1279673171;
PRAGMA user_version = 1;

CREATE TABLE session_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    header BLOB NOT NULL CHECK (length(header) = 192),
    scope BLOB CHECK (scope IS NULL OR length(scope) = 81),
    sealed INTEGER NOT NULL CHECK (sealed IN (0, 1)),
    records INTEGER NOT NULL CHECK (records BETWEEN 0 AND 65536),
    record_bytes INTEGER NOT NULL CHECK (record_bytes = records * 63),
    digest BLOB CHECK (digest IS NULL OR length(digest) = 32),
    CHECK ((sealed = 0 AND digest IS NULL) OR (sealed = 1 AND digest IS NOT NULL))
) STRICT, WITHOUT ROWID;

-- The complete25-byte typed key is the primary-key range authority. Ordinals
-- establish EOF by exact sealed counts; pages never count a repeated prefix.
CREATE TABLE directory_roots (
    key BLOB PRIMARY KEY CHECK (length(key) = 25),
    ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal BETWEEN 1 AND 65536),
    root BLOB NOT NULL CHECK (length(root) = 32)
) STRICT, WITHOUT ROWID;
