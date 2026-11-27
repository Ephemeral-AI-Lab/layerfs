CREATE TABLE file_digest (
    digest BLOB NOT NULL CHECK (length(digest) = 32),
    length INTEGER NOT NULL CHECK (length >= 0),
    root BLOB NOT NULL CHECK (length(root) = 32),
    PRIMARY KEY (digest, length)
) STRICT, WITHOUT ROWID;
