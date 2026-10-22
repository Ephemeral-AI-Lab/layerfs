SELECT nextval('seq_bh_a1') AS c \gset
INSERT INTO bh_a1 (child, parent, name) SELECT child, parent, name FROM st_bh WHERE c = :c ON CONFLICT DO NOTHING;
