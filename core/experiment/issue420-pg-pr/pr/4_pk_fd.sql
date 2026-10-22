-- pack -> pk_a4 (reordered, toast_tuple_target 8160, lz4 body); file_digest -> fd_a1 (reordered).
\set ON_ERROR_STOP 1
\timing on
CREATE TABLE pk_a4 (pack_id bigint NOT NULL, length int4 NOT NULL, domain int2 NOT NULL, digest bytea NOT NULL, body bytea COMPRESSION lz4 NOT NULL)
  WITH (toast_tuple_target = 8160);
INSERT INTO pk_a4 SELECT pack_id, length, domain, digest, body FROM pack ORDER BY pack_id;
ALTER TABLE pk_a4 ADD PRIMARY KEY (pack_id);
CREATE TABLE fd_a1 (length bigint NOT NULL, digest bytea NOT NULL, root bytea NOT NULL);
INSERT INTO fd_a1 SELECT length, digest, root FROM file_digest ORDER BY digest, length;
ALTER TABLE fd_a1 ADD PRIMARY KEY (digest, length);
