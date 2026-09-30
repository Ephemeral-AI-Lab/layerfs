/* Private LFCS v2: one fixed claims phase, known retirement, then roots. */
PRAGMA page_size = 4096;
PRAGMA application_id = 1279673171;
PRAGMA user_version = 2;

CREATE TABLE session_owner (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    header BLOB NOT NULL CHECK (length(header) = 192),
    scope BLOB CHECK (scope IS NULL OR length(scope) = 81),
    sealed INTEGER NOT NULL CHECK (sealed IN (0, 1)),
    records INTEGER NOT NULL CHECK (records BETWEEN 0 AND 65536),
    record_bytes INTEGER NOT NULL CHECK (record_bytes = records * 63),
    digest BLOB CHECK (digest IS NULL OR length(digest) = 32),
    declared_roots INTEGER NOT NULL CHECK (declared_roots BETWEEN 0 AND 65536),
    declared_claims INTEGER NOT NULL CHECK (declared_claims BETWEEN 0 AND 65536),
    claim_scope BLOB NOT NULL CHECK (length(claim_scope) = 81),
    claim_state INTEGER NOT NULL CHECK (claim_state IN (0, 1, 2, 3)),
    claim_records INTEGER NOT NULL CHECK (claim_records BETWEEN 0 AND declared_claims),
    claim_remaining INTEGER NOT NULL CHECK (claim_remaining BETWEEN 0 AND claim_records),
    claim_digest BLOB CHECK (claim_digest IS NULL OR length(claim_digest) = 32),
    claim_max BLOB CHECK (claim_max IS NULL OR length(claim_max) = 25),
    claim_after BLOB CHECK (claim_after IS NULL OR length(claim_after) = 25),
    CHECK (records <= declared_roots),
    CHECK ((sealed = 0 AND digest IS NULL) OR (sealed = 1 AND digest IS NOT NULL)),
    CHECK ((claim_state = 0 AND claim_digest IS NULL AND claim_remaining = 0
            AND claim_max IS NULL AND claim_after IS NULL)
        OR (claim_state IN (1, 2, 3) AND claim_digest IS NOT NULL)),
    CHECK (claim_state != 3 OR claim_remaining = 0)
) STRICT, WITHOUT ROWID;

CREATE TABLE exclusive_claims (
    key BLOB PRIMARY KEY CHECK (length(key) = 25),
    class BLOB NOT NULL CHECK (class = x'01')
) STRICT, WITHOUT ROWID;

CREATE TABLE directory_roots (
    key BLOB PRIMARY KEY CHECK (length(key) = 25),
    ordinal INTEGER NOT NULL UNIQUE CHECK (ordinal BETWEEN 1 AND 65536),
    root BLOB NOT NULL CHECK (length(root) = 32)
) STRICT, WITHOUT ROWID;
