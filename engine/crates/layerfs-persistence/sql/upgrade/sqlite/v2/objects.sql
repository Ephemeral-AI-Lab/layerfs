CREATE TABLE pack (
 pack_id INTEGER PRIMARY KEY CHECK(pack_id>0),
 domain INTEGER NOT NULL CHECK(domain IN(0,1)),
 digest BLOB NOT NULL CHECK(length(digest)=32),
 length INTEGER NOT NULL CHECK(length BETWEEN 32 AND 16781312),
 body BLOB NOT NULL CHECK(length(body)=length),
 CHECK(domain=1 OR length<=262144)
) STRICT;
CREATE TRIGGER pack_immutable_update BEFORE UPDATE ON pack BEGIN
 SELECT RAISE(ABORT,'published pack immutable'); END;
CREATE TRIGGER pack_immutable_delete BEFORE DELETE ON pack BEGIN
 SELECT RAISE(ABORT,'published pack immutable'); END;
