SELECT nextval('seq_iv_a2') AS c \gset
INSERT INTO iv_a2 (ordinal, value) SELECT ordinal, value FROM st_iv WHERE c = :c;
