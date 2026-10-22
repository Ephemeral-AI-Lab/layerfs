SELECT nextval('seq_bh0') AS c \gset
INSERT INTO binding_hint (child, parent, name) SELECT child, parent, name FROM st_bh WHERE c = :c ON CONFLICT DO NOTHING;
