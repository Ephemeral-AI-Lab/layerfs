\set ON_ERROR_STOP 1
INSERT INTO st_ol SELECT c, g, sha256(int8send(c * 1000 + g)), 7, 100 + g, (SELECT pack_id FROM pk_keys WHERE i = 1), 0, g
FROM generate_series(1061, 2060) c, generate_series(0, 325) g;
INSERT INTO st_ol_b16 SELECT c, get_byte(object_id, 0) * 256 + get_byte(object_id, 1),
  string_agg(ol_entry(object_id, role, canonical_length, pack_id, group_number, record_number), ''::bytea ORDER BY object_id)
FROM st_ol WHERE c > 1060 GROUP BY 1, 2;
ANALYZE st_ol, st_ol_b16;
