SELECT nextval('seq_bh_b1') AS c \gset
BEGIN;
INSERT INTO bh_name (name) SELECT DISTINCT name FROM st_bh WHERE c = :c ON CONFLICT (name) DO NOTHING;
INSERT INTO bh_b1 (child, parent, name_id) SELECT s.child, s.parent, n.name_id FROM st_bh s JOIN bh_name n USING (name) WHERE s.c = :c ON CONFLICT DO NOTHING;
COMMIT;
