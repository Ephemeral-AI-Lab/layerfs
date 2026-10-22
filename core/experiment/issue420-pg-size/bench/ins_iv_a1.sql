SELECT nextval('seq_iv_a1') AS c \gset
INSERT INTO iv_a1 (ordinal, value) SELECT ordinal, value FROM st_iv WHERE c = :c;
