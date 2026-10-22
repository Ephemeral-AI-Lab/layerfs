SELECT nextval('seq_pk_a1') AS c \gset
INSERT INTO pk_a1 (pack_id, length, domain, digest, body) SELECT pack_id, length, domain, digest, body FROM st_pk WHERE c = :c;
