\set ON_ERROR_STOP 1
\timing on
-- A4: as A1 but body EXTENDED with lz4 (compression only fires for tuples above 2032 bytes); bodies of the pristine 152,843 packs only
CREATE TABLE pk_a4 (pack_id bigint NOT NULL, length int4 NOT NULL, domain int2 NOT NULL, digest bytea NOT NULL, body bytea COMPRESSION lz4 NOT NULL)
  WITH (toast_tuple_target = 8160);
INSERT INTO pk_a4 SELECT pack_id, length, domain, digest, body FROM pack WHERE pack_id < 500000000 ORDER BY pack_id;
ALTER TABLE pk_a4 ADD PRIMARY KEY (pack_id);
VACUUM (ANALYZE) pk_a4;
SELECT count(*) FILTER (WHERE pg_column_compression(body) IS NOT NULL) compressed, sum(octet_length(body)) raw, sum(pg_column_size(body)) stored FROM pk_a4;
