SELECT nextval('seq_pk0') AS c \gset
INSERT INTO pack (pack_id, domain, digest, length, body) SELECT pack_id, domain, digest, length, body FROM st_pk WHERE c = :c;
