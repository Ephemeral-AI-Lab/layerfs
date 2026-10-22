SELECT nextval('seq_bh_b3') AS c \gset
BEGIN;
SELECT b.bucket, b.entries FROM bh_b3 b WHERE b.bucket IN (SELECT bucket FROM st_bh_b3 WHERE c = :c) FOR UPDATE;
INSERT INTO bh_b3 (bucket, entries) SELECT bucket, entries FROM st_bh_b3 WHERE c = :c
ON CONFLICT (bucket) DO UPDATE SET entries = bh_b3.entries || excluded.entries;
COMMIT;
