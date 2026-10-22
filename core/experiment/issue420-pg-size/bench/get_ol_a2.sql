\set i random(1, 10000)
SELECT role, canonical_length, pack_id, group_number, record_number FROM ol_a2 WHERE object_id = (SELECT object_id FROM ol_keys WHERE i = :i);
