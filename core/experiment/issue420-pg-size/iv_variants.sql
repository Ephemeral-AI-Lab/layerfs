-- inode_value layouts built from the loaded base table (same rows).
\set ON_ERROR_STOP 1
\timing on
-- A3: same types, PK btree fillfactor 100
CREATE TABLE iv_a3 (ordinal bigint NOT NULL, value bytea NOT NULL);
INSERT INTO iv_a3 SELECT ordinal, value FROM inode_value ORDER BY ordinal;
ALTER TABLE iv_a3 ADD PRIMARY KEY (ordinal) WITH (fillfactor = 100);
-- A1: ordinal int4 (u32 mapped by -2^31 bias in an adapter; values here fit as is), PK ff100
CREATE TABLE iv_a1 (ordinal int4 NOT NULL, value bytea NOT NULL);
INSERT INTO iv_a1 SELECT ordinal, value FROM inode_value ORDER BY ordinal;
ALTER TABLE iv_a1 ADD PRIMARY KEY (ordinal) WITH (fillfactor = 100);
-- A2: int4 + BRIN, no uniqueness (duplicate ordinal no longer refused)
CREATE TABLE iv_a2 (ordinal int4 NOT NULL, value bytea NOT NULL);
INSERT INTO iv_a2 SELECT ordinal, value FROM inode_value ORDER BY ordinal;
CREATE INDEX iv_a2_brin ON iv_a2 USING brin (ordinal) WITH (pages_per_range = 4);
-- B18: 18 fixed 73-byte slots per row (absent slot zero), presence bitmap; 6 rows/page
CREATE TABLE iv_b18 (bucket int4 NOT NULL, present int4 NOT NULL, vals bytea NOT NULL);
ALTER TABLE iv_b18 ALTER COLUMN vals SET STORAGE PLAIN;
INSERT INTO iv_b18
SELECT b, bit_or(CASE WHEN v.value IS NULL THEN 0 ELSE 1 << s END),
       string_agg(coalesce(v.value, decode(repeat('00', 73), 'hex')), ''::bytea ORDER BY s)
FROM (SELECT DISTINCT ordinal / 18 AS b FROM inode_value) bk
CROSS JOIN generate_series(0, 17) s
LEFT JOIN inode_value v ON v.ordinal = bk.b * 18 + s
GROUP BY b ORDER BY b;
ALTER TABLE iv_b18 ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
-- B16p: 16 ordinals per row, only present values packed in ordinal order, uncompressed
CREATE TABLE iv_b16p (bucket int4 NOT NULL, present int4 NOT NULL, vals bytea NOT NULL);
ALTER TABLE iv_b16p ALTER COLUMN vals SET STORAGE PLAIN;
INSERT INTO iv_b16p
SELECT ordinal / 16, bit_or(1 << (ordinal % 16)::int), string_agg(value, ''::bytea ORDER BY ordinal)
FROM inode_value GROUP BY ordinal / 16 ORDER BY 1;
ALTER TABLE iv_b16p ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
-- B64z: 64 ordinals per row, present values packed, lz4 inline (STORAGE MAIN)
CREATE TABLE iv_b64z (bucket int4 NOT NULL, present int8 NOT NULL, vals bytea COMPRESSION lz4 NOT NULL)
  WITH (toast_tuple_target = 128);
ALTER TABLE iv_b64z ALTER COLUMN vals SET STORAGE MAIN;
INSERT INTO iv_b64z
SELECT ordinal / 64, bit_or(1::int8 << (ordinal % 64)::int), string_agg(value, ''::bytea ORDER BY ordinal)
FROM inode_value GROUP BY ordinal / 64 ORDER BY 1;
ALTER TABLE iv_b64z ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
-- B16z: 16 per row, packed, pglz attempt (lz4) to see if small groups compress
CREATE TABLE iv_b16z (bucket int4 NOT NULL, present int4 NOT NULL, vals bytea COMPRESSION lz4 NOT NULL)
  WITH (toast_tuple_target = 128);
ALTER TABLE iv_b16z ALTER COLUMN vals SET STORAGE MAIN;
INSERT INTO iv_b16z SELECT * FROM iv_b16p ORDER BY bucket;
ALTER TABLE iv_b16z ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
VACUUM (ANALYZE) iv_a3, iv_a1, iv_a2, iv_b18, iv_b16p, iv_b64z, iv_b16z;
