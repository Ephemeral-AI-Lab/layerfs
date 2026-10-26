CREATE TABLE pack (
 pack_id INTEGER PRIMARY KEY CHECK(pack_id>0),
 domain INTEGER NOT NULL CHECK(domain IN(0,1)),
 digest BLOB NOT NULL CHECK(length(digest)=32),
 length INTEGER NOT NULL CHECK(length BETWEEN 32 AND 16781312),
 control BLOB NOT NULL CHECK(length(control) BETWEEN 24 AND 4120),
 CHECK(domain=1 OR length<=262144)
) STRICT;
CREATE TABLE pack_unit (
 unit_id INTEGER PRIMARY KEY CHECK(unit_id>0),
 pack_id INTEGER NOT NULL REFERENCES pack(pack_id),
 group_number INTEGER NOT NULL CHECK(group_number BETWEEN 0 AND 255),
 offset INTEGER NOT NULL CHECK(offset>=24),
 length INTEGER NOT NULL CHECK(length BETWEEN 1 AND 16781312),
 body BLOB NOT NULL CHECK(length(body)=length),
 UNIQUE(pack_id,group_number)
) STRICT;
CREATE TRIGGER pack_immutable_update BEFORE UPDATE ON pack BEGIN
 SELECT RAISE(ABORT,'published pack immutable'); END;
CREATE TRIGGER pack_immutable_delete BEFORE DELETE ON pack BEGIN
 SELECT RAISE(ABORT,'published pack immutable'); END;
CREATE TRIGGER pack_unit_immutable_update BEFORE UPDATE ON pack_unit BEGIN
 SELECT RAISE(ABORT,'published pack unit immutable'); END;
CREATE TRIGGER pack_unit_immutable_delete BEFORE DELETE ON pack_unit BEGIN
 SELECT RAISE(ABORT,'published pack unit immutable'); END;
