SELECT nextval('seq_ol_b16') AS c \gset
BEGIN;
SELECT b.bucket, b.entries FROM ol_b16 b WHERE b.bucket IN (SELECT bucket FROM st_ol_b16 WHERE c = :c) FOR UPDATE;
INSERT INTO ol_b16 (bucket, entries) SELECT bucket, entries FROM st_ol_b16 WHERE c = :c
ON CONFLICT (bucket) DO UPDATE SET entries = ol_b16.entries || excluded.entries;
COMMIT;
