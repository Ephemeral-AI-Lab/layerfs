/* Private LFCS v3: immutable exclusive site membership and monotone base facts. */
PRAGMA page_size = 4096;
PRAGMA application_id = 1279673171;
PRAGMA user_version = 3;

CREATE TABLE session_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    header BLOB NOT NULL CHECK (length(header) = 200),
    scope BLOB CHECK (scope IS NULL OR length(scope) = 81),
    sealed INTEGER NOT NULL CHECK (sealed IN (0, 1)),
    records INTEGER NOT NULL CHECK (records BETWEEN 0 AND 65536),
    record_bytes INTEGER NOT NULL CHECK (record_bytes = records * 63),
    digest BLOB CHECK (digest IS NULL OR length(digest) = 32),
    declared_roots INTEGER NOT NULL CHECK (declared_roots BETWEEN 0 AND 65536),
    declared_sites INTEGER NOT NULL CHECK (declared_sites BETWEEN 0 AND 65536),
    CHECK (records <= declared_roots),
    CHECK ((sealed = 0 AND digest IS NULL) OR (sealed = 1 AND digest IS NOT NULL))
) STRICT, WITHOUT ROWID;

CREATE TABLE site_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    scope BLOB NOT NULL CHECK (length(scope) = 89),
    stage INTEGER NOT NULL CHECK (stage IN (0, 1, 2, 3, 4)),
    records INTEGER NOT NULL CHECK (records BETWEEN 0 AND 65536),
    remaining INTEGER NOT NULL CHECK (remaining BETWEEN 0 AND records),
    birth_digest BLOB CHECK (birth_digest IS NULL OR length(birth_digest) = 32),
    birth_max BLOB CHECK (birth_max IS NULL OR length(birth_max) = 25),
    final_digest BLOB CHECK (final_digest IS NULL OR length(final_digest) = 32),
    final_max BLOB CHECK (final_max IS NULL OR length(final_max) = 25),
    after_key BLOB CHECK (after_key IS NULL OR length(after_key) = 25),
    CHECK ((stage = 0 AND birth_digest IS NULL AND birth_max IS NULL AND remaining = 0)
        OR (stage > 0 AND birth_digest IS NOT NULL)),
    CHECK (stage < 2 OR final_digest IS NOT NULL),
    CHECK (stage != 4 OR remaining = 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE binding_sites (
    key BLOB PRIMARY KEY CHECK (length(key) = 25),
    flags INTEGER NOT NULL CHECK (flags IN (0, 1, 3, 7)),
    point BLOB NOT NULL CHECK (length(point) = 28),
    parent INTEGER NOT NULL CHECK (parent BETWEEN 1 AND 9223372036854775807),
    binding_ordinal INTEGER NOT NULL CHECK (binding_ordinal BETWEEN 0 AND 4294967295)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX site_birth_order ON binding_sites(parent, binding_ordinal);
CREATE INDEX site_existing_parent ON binding_sites(parent, binding_ordinal)
    WHERE (flags&1)=1;

CREATE TABLE directory_roots (
    key BLOB PRIMARY KEY CHECK (length(key) = 25),
    ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal BETWEEN 1 AND 65536),
    root BLOB NOT NULL CHECK (length(root) = 32)
) STRICT, WITHOUT ROWID;
