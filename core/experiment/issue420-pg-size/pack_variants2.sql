\set ON_ERROR_STOP 1
\timing on
-- B: pack header row + body cut into 8,120-byte slices stored PLAIN (one full slice fills a page)
CREATE TABLE pk_b_head (pack_id bigint NOT NULL, length int4 NOT NULL, domain int2 NOT NULL, digest bytea NOT NULL);
INSERT INTO pk_b_head SELECT pack_id, length, domain, digest FROM pack ORDER BY pack_id;
ALTER TABLE pk_b_head ADD PRIMARY KEY (pack_id);
CREATE TABLE pk_b_slice (pack_id bigint NOT NULL, seq int2 NOT NULL, data bytea NOT NULL);
ALTER TABLE pk_b_slice ALTER COLUMN data SET STORAGE PLAIN;
INSERT INTO pk_b_slice SELECT p.pack_id, s, substr(p.body, s::int * 8120 + 1, 8120)
FROM pack p, generate_series(0, (p.length::int - 1) / 8120) s ORDER BY p.pack_id, s;
ALTER TABLE pk_b_slice ADD PRIMARY KEY (pack_id, seq);
VACUUM (ANALYZE) pk_b_head, pk_b_slice;
