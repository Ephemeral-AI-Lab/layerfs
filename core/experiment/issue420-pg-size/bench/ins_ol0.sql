SELECT nextval('seq_ol0') AS c \gset
INSERT INTO object_location (object_id, role, canonical_length, pack_id, group_number, record_number)
SELECT object_id, role, canonical_length, pack_id, group_number, record_number FROM st_ol WHERE c = :c
ON CONFLICT (object_id) DO NOTHING RETURNING object_id;
