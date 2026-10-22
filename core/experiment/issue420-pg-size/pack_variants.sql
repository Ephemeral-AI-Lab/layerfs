\set ON_ERROR_STOP 1
\timing on
-- A1: fixed-width first (domain 0/1, length <= 16781312), body EXTERNAL, bodies up to ~8 KB kept inline
CREATE TABLE pk_a1 (pack_id bigint NOT NULL, length int4 NOT NULL, domain int2 NOT NULL, digest bytea NOT NULL, body bytea NOT NULL)
  WITH (toast_tuple_target = 8160);
ALTER TABLE pk_a1 ALTER COLUMN body SET STORAGE EXTERNAL;
INSERT INTO pk_a1 SELECT pack_id, length, domain, digest, body FROM pack ORDER BY pack_id;
ALTER TABLE pk_a1 ADD PRIMARY KEY (pack_id);
VACUUM (ANALYZE) pk_a1;
