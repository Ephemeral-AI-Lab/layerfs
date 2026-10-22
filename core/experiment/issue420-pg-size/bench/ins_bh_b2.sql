SELECT nextval('seq_bh_b2') AS c \gset
BEGIN;
SELECT b.bucket, b.entries FROM bh_b2 b WHERE b.bucket IN (SELECT bucket FROM st_bh_b2 WHERE c = :c) FOR UPDATE;
INSERT INTO bh_b2 (bucket, entries) SELECT bucket, entries FROM st_bh_b2 WHERE c = :c
ON CONFLICT (bucket) DO UPDATE SET entries = bh_b2.entries || excluded.entries;
COMMIT;
