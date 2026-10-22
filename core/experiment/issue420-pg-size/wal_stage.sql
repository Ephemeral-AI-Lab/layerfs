\set ON_ERROR_STOP 1
INSERT INTO st_ol SELECT c, g, sha256(int8send(c * 1000 + g)), 7, 100 + g, (SELECT pack_id FROM pk_keys WHERE i = 1), 0, g
FROM generate_series(2061, 2200) c, generate_series(0, 325) g;
INSERT INTO st_ol_b16 SELECT c, get_byte(object_id, 0) * 256 + get_byte(object_id, 1),
  string_agg(ol_entry(object_id, role, canonical_length, pack_id, group_number, record_number), ''::bytea ORDER BY object_id)
FROM st_ol WHERE c > 2060 GROUP BY 1, 2;
ANALYZE st_ol, st_ol_b16;
INSERT INTO st_iv SELECT c, 160000000 + (c - 1) * 323 + s.g, s.value FROM generate_series(361, 500) c, iv_src s;
INSERT INTO st_iv_b32 SELECT c, ordinal / 32, bit_or(1::int8 << (ordinal % 32)::int), string_agg(value, ''::bytea ORDER BY ordinal)
FROM st_iv WHERE c > 360 GROUP BY 1, 2;
INSERT INTO st_iv_b18 SELECT bk.c, b, bit_or(CASE WHEN v.value IS NULL THEN 0 ELSE 1 << sl END),
       string_agg(coalesce(v.value, decode(repeat('00', 73), 'hex')), ''::bytea ORDER BY sl)
FROM (SELECT DISTINCT c, ordinal / 18 AS b FROM st_iv WHERE c > 360) bk CROSS JOIN generate_series(0, 17) sl
LEFT JOIN st_iv v ON v.c = bk.c AND v.ordinal = bk.b * 18 + sl GROUP BY 1, 2;
INSERT INTO st_bh SELECT c, 17000000 + (c - 1) * 322 + s.g, s.parent, s.name FROM generate_series(361, 500) c, bh_src s;
INSERT INTO st_bh_b2 SELECT c, child / 16, string_agg(b1((child % 16)::int) || int8send(parent) || b1(length(name)) || name, ''::bytea ORDER BY child, parent, name)
FROM st_bh WHERE c > 360 GROUP BY 1, 2;
INSERT INTO st_pk SELECT c, 500000000 + c * 3 + s.g, s.domain, sha256(s.body), s.length, s.body FROM generate_series(61, 200) c, pk_src s;
INSERT INTO st_pk_slice SELECT c, pack_id, sl::int2, substr(body, sl * 8120 + 1, 8120) FROM st_pk, generate_series(0, (length::int - 1) / 8120) sl WHERE c > 60;
ANALYZE;
