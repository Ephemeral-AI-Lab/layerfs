SELECT nextval('seq_ol_a2') AS c \gset
INSERT INTO ol_a2 (object_id, role, canonical_length, pack_id, group_number, record_number)
SELECT object_id, role, canonical_length, pack_id, group_number, record_number FROM st_ol WHERE c = :c
ON CONFLICT DO NOTHING RETURNING object_id;
