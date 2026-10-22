-- Layouts for the whole-publish diagnostic (same definitions as the size study).
\set ON_ERROR_STOP 1
\timing on
CREATE EXTENSION pg_prewarm; CREATE EXTENSION pgstattuple;
ALTER TABLE object_location DROP CONSTRAINT IF EXISTS object_location_pack_id_fkey;
CREATE FUNCTION b1(int) RETURNS bytea IMMUTABLE STRICT LANGUAGE sql AS $$ SELECT decode(lpad(to_hex($1), 2, '0'), 'hex') $$;
CREATE FUNCTION ol_entry(bytea, bigint, bigint, bigint, bigint, bigint) RETURNS bytea IMMUTABLE STRICT LANGUAGE sql AS $$
  SELECT substr($1, 3) || decode(lpad(to_hex($2), 2, '0'), 'hex') || int4send($3::int4) || int8send($4)
      || decode(lpad(to_hex($5), 2, '0'), 'hex') || substr(int4send($6::int4), 3) $$;
CREATE TABLE ol_a1 (pack_id bigint NOT NULL, canonical_length int4 NOT NULL, role int2 NOT NULL,
  group_number int2 NOT NULL, record_number int2 NOT NULL, object_id bytea NOT NULL);
INSERT INTO ol_a1 SELECT pack_id, canonical_length, role, group_number, record_number, object_id FROM object_location ORDER BY object_id;
ALTER TABLE ol_a1 ADD PRIMARY KEY (object_id);
CREATE TABLE ol_a2 (LIKE ol_a1);
INSERT INTO ol_a2 SELECT * FROM ol_a1 ORDER BY object_id;
ALTER TABLE ol_a2 ADD CONSTRAINT ol_a2_unique EXCLUDE USING hash (object_id WITH =);
CREATE TABLE ol_b16 (bucket int4 NOT NULL, entries bytea NOT NULL);
ALTER TABLE ol_b16 ALTER COLUMN entries SET STORAGE MAIN;
INSERT INTO ol_b16 SELECT get_byte(object_id, 0) * 256 + get_byte(object_id, 1),
  string_agg(ol_entry(object_id, role, canonical_length, pack_id, group_number, record_number), ''::bytea ORDER BY object_id)
FROM object_location GROUP BY 1 ORDER BY 1;
ALTER TABLE ol_b16 ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
CREATE TABLE iv_a1 (ordinal int4 NOT NULL, value bytea NOT NULL);
INSERT INTO iv_a1 SELECT ordinal, value FROM inode_value ORDER BY ordinal;
ALTER TABLE iv_a1 ADD PRIMARY KEY (ordinal) WITH (fillfactor = 100);
CREATE TABLE iv_b32z (bucket int4 NOT NULL, present int8 NOT NULL, vals bytea COMPRESSION lz4 NOT NULL);
ALTER TABLE iv_b32z ALTER COLUMN vals SET STORAGE MAIN;
INSERT INTO iv_b32z SELECT ordinal / 32, bit_or(1::int8 << (ordinal % 32)::int), string_agg(value, ''::bytea ORDER BY ordinal)
FROM inode_value GROUP BY ordinal / 32 ORDER BY 1;
ALTER TABLE iv_b32z ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
CREATE TABLE bh_b3 (bucket bigint NOT NULL, entries bytea COMPRESSION lz4 NOT NULL);
ALTER TABLE bh_b3 ALTER COLUMN entries SET STORAGE MAIN;
INSERT INTO bh_b3 SELECT child / 128, string_agg(b1((child % 128)::int) || int8send(parent) || b1(length(name)) || name, ''::bytea ORDER BY child, parent, name)
FROM binding_hint GROUP BY child / 128 ORDER BY 1;
ALTER TABLE bh_b3 ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
CREATE TABLE pk_a4 (pack_id bigint NOT NULL, length int4 NOT NULL, domain int2 NOT NULL, digest bytea NOT NULL, body bytea COMPRESSION lz4 NOT NULL)
  WITH (toast_tuple_target = 8160);
INSERT INTO pk_a4 SELECT pack_id, length, domain, digest, body FROM pack ORDER BY pack_id;
ALTER TABLE pk_a4 ADD PRIMARY KEY (pack_id);
CREATE TABLE head (id int PRIMARY KEY, v bigint NOT NULL); INSERT INTO head VALUES (1, 0);
VACUUM (ANALYZE);
