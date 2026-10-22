SELECT nextval('seq_pk_b') AS c \gset
BEGIN;
INSERT INTO pk_b_head (pack_id, length, domain, digest) SELECT pack_id, length, domain, digest FROM st_pk WHERE c = :c;
INSERT INTO pk_b_slice (pack_id, seq, data) SELECT pack_id, seq, data FROM st_pk_slice WHERE c = :c;
COMMIT;
