\set ON_ERROR_STOP 1
\timing on
-- Packed buckets of k ordinals with lz4 (compression only fires above the 2032-byte TOAST threshold)
CREATE TABLE iv_b32z (bucket int4 NOT NULL, present int8 NOT NULL, vals bytea COMPRESSION lz4 NOT NULL);
ALTER TABLE iv_b32z ALTER COLUMN vals SET STORAGE MAIN;
INSERT INTO iv_b32z SELECT ordinal / 32, bit_or(1::int8 << (ordinal % 32)::int), string_agg(value, ''::bytea ORDER BY ordinal)
FROM inode_value GROUP BY ordinal / 32 ORDER BY 1;
ALTER TABLE iv_b32z ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
CREATE TABLE iv_b64p (bucket int4 NOT NULL, present int8 NOT NULL, vals bytea COMPRESSION pglz NOT NULL);
ALTER TABLE iv_b64p ALTER COLUMN vals SET STORAGE MAIN;
INSERT INTO iv_b64p SELECT bucket, present, vals FROM iv_b64z ORDER BY bucket;
ALTER TABLE iv_b64p ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
CREATE TABLE iv_b128z (bucket int4 NOT NULL, present bytea NOT NULL, vals bytea COMPRESSION lz4 NOT NULL);
ALTER TABLE iv_b128z ALTER COLUMN vals SET STORAGE MAIN;
INSERT INTO iv_b128z SELECT ordinal / 128, int8send(bit_or(CASE WHEN ordinal % 128 < 64 THEN 1::int8 << (ordinal % 64)::int ELSE 0 END)) || int8send(bit_or(CASE WHEN ordinal % 128 >= 64 THEN 1::int8 << (ordinal % 64)::int ELSE 0 END)), string_agg(value, ''::bytea ORDER BY ordinal)
FROM inode_value GROUP BY ordinal / 128 ORDER BY 1;
ALTER TABLE iv_b128z ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
VACUUM (ANALYZE) iv_b32z, iv_b64p, iv_b128z;
