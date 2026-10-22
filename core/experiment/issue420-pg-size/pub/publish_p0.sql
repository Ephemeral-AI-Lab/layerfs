SELECT nextval('pub_p0') AS c \gset
BEGIN;
SELECT pg_advisory_xact_lock(4200);
SELECT v FROM head WHERE id = 1;
INSERT INTO object_location (object_id, role, canonical_length, pack_id, group_number, record_number) SELECT object_id, role, canonical_length, pack_id, group_number, record_number FROM st_ol WHERE c = :c ON CONFLICT (object_id) DO NOTHING RETURNING object_id;
INSERT INTO inode_value (ordinal, value) SELECT ordinal, value FROM st_iv WHERE c = :c;
INSERT INTO binding_hint (child, parent, name) SELECT child, parent, name FROM st_bh WHERE c = :c ON CONFLICT DO NOTHING;
INSERT INTO file_digest (digest, length, root) SELECT digest, length, root FROM st_fd WHERE c = :c ON CONFLICT DO NOTHING;
INSERT INTO pack (pack_id, domain, digest, length, body) SELECT pack_id, domain, digest, length, body FROM st_pk WHERE c = :c;
UPDATE head SET v = v + 1 WHERE id = 1;
COMMIT;
