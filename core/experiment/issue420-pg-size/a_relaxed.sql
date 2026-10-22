\set ON_ERROR_STOP 1
-- object_location: A3 widths (pack_id int4) + hash exclusion; built from the pristine rows (seq st_ol rows excluded)
CREATE TABLE ol_a4 (pack_id int4 NOT NULL, canonical_length int4 NOT NULL, role int2 NOT NULL,
  group_number int2 NOT NULL, record_number int2 NOT NULL, object_id bytea NOT NULL);
INSERT INTO ol_a4 SELECT pack_id, canonical_length, role, group_number, record_number, object_id FROM ol_a3 ORDER BY object_id;
ALTER TABLE ol_a4 ADD CONSTRAINT ol_a4_unique EXCLUDE USING hash (object_id WITH =);
-- binding_hint: int4 serials, lookup btree on child, no uniqueness
CREATE TABLE bh_a4 (child int4 NOT NULL, parent int4 NOT NULL, name bytea NOT NULL);
INSERT INTO bh_a4 SELECT child, parent, name FROM binding_hint WHERE child < 17000000 ORDER BY child, parent, name;
CREATE INDEX bh_a4_child ON bh_a4 (child);
-- file_digest: fixed-width column first
CREATE TABLE fd_a1 (length bigint NOT NULL, digest bytea NOT NULL, root bytea NOT NULL);
INSERT INTO fd_a1 SELECT length, digest, root FROM file_digest ORDER BY digest, length;
ALTER TABLE fd_a1 ADD PRIMARY KEY (digest, length);
VACUUM (ANALYZE) ol_a4, bh_a4, fd_a1;
