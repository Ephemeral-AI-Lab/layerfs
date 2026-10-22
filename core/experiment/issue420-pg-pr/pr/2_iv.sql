-- inode_value -> iv_b32z: 32 ordinals per row, packed 73-byte values, lz4.
\set ON_ERROR_STOP 1
\timing on
CREATE TABLE iv_b32z (bucket int4 NOT NULL, present int8 NOT NULL, vals bytea COMPRESSION lz4 NOT NULL);
ALTER TABLE iv_b32z ALTER COLUMN vals SET STORAGE MAIN;
INSERT INTO iv_b32z SELECT ordinal / 32, bit_or(1::int8 << (ordinal % 32)::int), string_agg(value, ''::bytea ORDER BY ordinal)
FROM inode_value GROUP BY ordinal / 32 ORDER BY 1;
ALTER TABLE iv_b32z ADD PRIMARY KEY (bucket) WITH (fillfactor = 100);
