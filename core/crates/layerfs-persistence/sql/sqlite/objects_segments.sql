CREATE TABLE body_directory (
 id INTEGER PRIMARY KEY CHECK(id=1),
 device INTEGER NOT NULL CHECK(device>=0), inode INTEGER NOT NULL CHECK(inode>0)
) STRICT;
CREATE TRIGGER body_directory_immutable_update BEFORE UPDATE ON body_directory BEGIN
 SELECT RAISE(ABORT,'payload directory immutable'); END;
CREATE TRIGGER body_directory_immutable_delete BEFORE DELETE ON body_directory BEGIN
 SELECT RAISE(ABORT,'payload directory immutable'); END;
CREATE TABLE body_segment (
 segment_id INTEGER PRIMARY KEY CHECK(segment_id>0),
 device INTEGER NOT NULL CHECK(device>=0), inode INTEGER NOT NULL CHECK(inode>0),
 length INTEGER NOT NULL CHECK(length BETWEEN 65537 AND 16781312)
) STRICT;
CREATE TRIGGER body_segment_immutable_update BEFORE UPDATE ON body_segment BEGIN
 SELECT RAISE(ABORT,'published segment immutable'); END;
CREATE TRIGGER body_segment_immutable_delete BEFORE DELETE ON body_segment BEGIN
 SELECT RAISE(ABORT,'published segment immutable'); END;
CREATE TABLE pack (
 pack_id INTEGER PRIMARY KEY CHECK(pack_id>0),
 domain INTEGER NOT NULL CHECK(domain IN(0,1)),
 digest BLOB NOT NULL CHECK(length(digest)=32),
 length INTEGER NOT NULL CHECK(length BETWEEN 32 AND 16781312),
 body BLOB,
 segment_id INTEGER REFERENCES body_segment(segment_id), segment_offset INTEGER,
 CHECK(domain=1 OR length<=262144),
 CHECK((body IS NOT NULL AND length(body)=length AND segment_id IS NULL AND segment_offset IS NULL AND (domain=0 OR length<=65536))
    OR (body IS NULL AND domain=1 AND length>65536 AND segment_id IS NOT NULL AND segment_offset IS NOT NULL AND segment_offset>=0))
) STRICT;
CREATE TRIGGER pack_segment_bounds BEFORE INSERT ON pack WHEN NEW.segment_id IS NOT NULL BEGIN
 SELECT CASE WHEN NEW.segment_offset+NEW.length>(SELECT length FROM body_segment WHERE segment_id=NEW.segment_id)
 THEN RAISE(ABORT,'pack segment bounds') END; END;
CREATE TRIGGER pack_immutable_update BEFORE UPDATE ON pack BEGIN
 SELECT RAISE(ABORT,'published pack immutable'); END;
CREATE TRIGGER pack_immutable_delete BEFORE DELETE ON pack BEGIN
 SELECT RAISE(ABORT,'published pack immutable'); END;
