\set i random(1, 10000)
SELECT entries FROM ol_b16 WHERE bucket = (SELECT get_byte(object_id, 0) * 256 + get_byte(object_id, 1) FROM ol_keys WHERE i = :i);
