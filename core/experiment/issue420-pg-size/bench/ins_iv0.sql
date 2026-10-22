SELECT nextval('seq_iv0') AS c \gset
INSERT INTO inode_value (ordinal, value) SELECT ordinal, value FROM st_iv WHERE c = :c;
