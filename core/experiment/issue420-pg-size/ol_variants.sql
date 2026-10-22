\set ON_ERROR_STOP 1
\timing on
-- A1: fixed-width columns first, widths from the Store's CHECKs (role 1..13, length <= 2^24, group <= 255, record <= 8190), pack_id kept int8
CREATE TABLE ol_a1 (pack_id bigint NOT NULL, canonical_length int4 NOT NULL, role int2 NOT NULL,
  group_number int2 NOT NULL, record_number int2 NOT NULL, object_id bytea NOT NULL);
INSERT INTO ol_a1 SELECT pack_id, canonical_length, role, group_number, record_number, object_id FROM object_location ORDER BY object_id;
ALTER TABLE ol_a1 ADD PRIMARY KEY (object_id);
-- A3: as A1 with pack_id int4 (max today 303 M; schema allows i64)
CREATE TABLE ol_a3 (pack_id int4 NOT NULL, canonical_length int4 NOT NULL, role int2 NOT NULL,
  group_number int2 NOT NULL, record_number int2 NOT NULL, object_id bytea NOT NULL);
INSERT INTO ol_a3 SELECT pack_id, canonical_length, role, group_number, record_number, object_id FROM object_location ORDER BY object_id;
ALTER TABLE ol_a3 ADD PRIMARY KEY (object_id);
-- A2: as A1, uniqueness by hash exclusion (no btree; FK to object_id impossible)
CREATE TABLE ol_a2 (LIKE ol_a1);
INSERT INTO ol_a2 SELECT * FROM ol_a1;
ALTER TABLE ol_a2 ADD CONSTRAINT ol_a2_unique EXCLUDE USING hash (object_id WITH =);
-- B1: rows bucketed by the first 16 bits of object_id; entry = id[2:32] role u8 length u32 pack u64 group u8 record u16 (46 B)
CREATE FUNCTION ol_entry(bytea, bigint, bigint, bigint, bigint, bigint) RETURNS bytea IMMUTABLE STRICT LANGUAGE sql AS $$
  SELECT substr($1, 3) || decode(lpad(to_hex($2), 2, '0'), 'hex') || int4send($3::int4) || int8send($4)
      || decode(lpad(to_hex($5), 2, '0'), 'hex') || substr(int4send($6::int4), 3) $$;
CREATE TABLE ol_b16 (bucket int4 NOT NULL, entries bytea NOT NULL);
ALTER TABLE ol_b16 ALTER COLUMN entries SET STORAGE MAIN;
INSERT INTO ol_b16 SELECT get_byte(object_id, 0) * 256 + get_byte(object_id, 1),
  string_agg(ol_entry(object_id, role, canonical_length, pack_id, group_number, record_number), ''::bytea ORDER BY object_id)
FROM object_location GROUP BY 1 ORDER BY 1;
ALTER TABLE ol_b16 ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
-- B2: 18-bit buckets (about 5 entries a row)
CREATE TABLE ol_b18 (bucket int4 NOT NULL, entries bytea NOT NULL);
ALTER TABLE ol_b18 ALTER COLUMN entries SET STORAGE MAIN;
INSERT INTO ol_b18 SELECT (get_byte(object_id, 0) * 65536 + get_byte(object_id, 1) * 256 + get_byte(object_id, 2)) >> 6,
  string_agg(ol_entry(object_id, role, canonical_length, pack_id, group_number, record_number), ''::bytea ORDER BY object_id)
FROM object_location GROUP BY 1 ORDER BY 1;
ALTER TABLE ol_b18 ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
VACUUM (ANALYZE) ol_a1, ol_a2, ol_a3, ol_b16, ol_b18;
