SELECT nextval('pub_pr') AS c \gset
BEGIN;
SELECT pg_advisory_xact_lock(4200);
SELECT v FROM head WHERE id = 1;
INSERT INTO ol_a2 (object_id, role, canonical_length, pack_id, group_number, record_number) SELECT object_id, role, canonical_length, pack_id, group_number, record_number FROM st_ol WHERE c = :c ON CONFLICT DO NOTHING RETURNING object_id;
INSERT INTO iv_b32z (bucket, present, vals) SELECT bucket, present, vals FROM st_iv_b32 WHERE c = :c ON CONFLICT (bucket) DO UPDATE SET present = iv_b32z.present | excluded.present, vals = iv_b32z.vals || excluded.vals WHERE iv_b32z.present & excluded.present = 0;
INSERT INTO bh_b3 (bucket, entries) SELECT bucket, entries FROM st_bh_b3 WHERE c = :c ON CONFLICT (bucket) DO UPDATE SET entries = bh_b3.entries || excluded.entries WHERE position(excluded.entries IN bh_b3.entries) = 0;
INSERT INTO file_digest (digest, length, root) SELECT digest, length, root FROM st_fd WHERE c = :c ON CONFLICT DO NOTHING;
INSERT INTO pk_a4 (pack_id, length, domain, digest, body) SELECT pack_id, length, domain, digest, body FROM st_pk WHERE c = :c;
UPDATE head SET v = v + 1 WHERE id = 1;
COMMIT;
