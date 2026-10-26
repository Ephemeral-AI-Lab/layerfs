CREATE TABLE merge_operation (
    operation INTEGER PRIMARY KEY,
    token BLOB NOT NULL CHECK (length(token) = 16),
    touched INTEGER NOT NULL CHECK (touched >= 0)
) STRICT;

CREATE TABLE merge_record (
    operation INTEGER NOT NULL,
    kind INTEGER NOT NULL CHECK (kind >= 0),
    key BLOB NOT NULL CHECK (length(key) = 32),
    value BLOB NOT NULL,
    PRIMARY KEY (operation, kind, key)
) STRICT, WITHOUT ROWID;

