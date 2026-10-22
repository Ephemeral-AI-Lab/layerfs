-- Untimed staging of 1,200 Commits' rows in every layout's client-built form.
\set ON_ERROR_STOP 1
SELECT setseed(0.420);
CREATE TABLE iv_src AS SELECT row_number() OVER (ORDER BY ordinal)::int - 1 AS g, value FROM (SELECT ordinal, value FROM inode_value ORDER BY ordinal OFFSET 3000000 LIMIT 323) s;
CREATE TABLE bh_src AS SELECT row_number() OVER ()::int - 1 AS g, parent, name FROM (SELECT parent, name FROM binding_hint ORDER BY random() LIMIT 322) s;
CREATE TABLE pk_src AS SELECT row_number() OVER ()::int - 1 AS g, domain, length, body FROM (
  (SELECT domain, length, body FROM pack WHERE length BETWEEN 300 AND 1500 ORDER BY pack_id DESC LIMIT 2)
  UNION ALL (SELECT domain, length, body FROM pack WHERE length BETWEEN 6000 AND 12000 ORDER BY pack_id DESC LIMIT 1)) s;
CREATE TABLE st_ol AS SELECT c, sha256(int8send(c * 1000 + g)) AS object_id, 7::bigint AS role, (100 + g)::bigint AS canonical_length,
  (SELECT min(pack_id) FROM pack) AS pack_id, 0::bigint AS group_number, g::bigint AS record_number
FROM generate_series(1, 1200) c, generate_series(0, 325) g;
CREATE INDEX ON st_ol (c);
CREATE TABLE st_ol_b16 AS SELECT c, get_byte(object_id, 0) * 256 + get_byte(object_id, 1) AS bucket,
  string_agg(ol_entry(object_id, role, canonical_length, pack_id, group_number, record_number), ''::bytea ORDER BY object_id) AS entries
FROM st_ol GROUP BY 1, 2;
CREATE INDEX ON st_ol_b16 (c);
CREATE TABLE st_iv AS SELECT c, 160000000 + (c - 1) * 323 + s.g AS ordinal, s.value FROM generate_series(1, 1200) c, iv_src s;
CREATE INDEX ON st_iv (c);
CREATE TABLE st_iv_b32 AS SELECT c, ordinal / 32 AS bucket, bit_or(1::int8 << (ordinal % 32)::int) AS present, string_agg(value, ''::bytea ORDER BY ordinal) AS vals
FROM st_iv GROUP BY 1, 2;
CREATE INDEX ON st_iv_b32 (c);
CREATE TABLE st_bh AS SELECT c, 17000000 + (c - 1) * 322 + s.g AS child, s.parent, s.name FROM generate_series(1, 1200) c, bh_src s;
CREATE INDEX ON st_bh (c);
CREATE TABLE st_bh_b3 AS SELECT c, child / 128 AS bucket,
  string_agg(b1((child % 128)::int) || int8send(parent) || b1(length(name)) || name, ''::bytea ORDER BY child, parent, name) AS entries
FROM st_bh GROUP BY 1, 2;
CREATE INDEX ON st_bh_b3 (c);
CREATE TABLE st_fd AS SELECT c, sha256(int8send(c * 1000 + g + 500)) AS digest, (1000 + g)::bigint AS length, sha256(int8send(c * 1000 + g + 900)) AS root
FROM generate_series(1, 1200) c, generate_series(0, 120) g;
CREATE INDEX ON st_fd (c);
CREATE TABLE st_pk AS SELECT c, 500000000 + c * 3 + s.g AS pack_id, s.domain, sha256(s.body) AS digest, s.length, s.body
FROM generate_series(1, 1200) c, pk_src s;
ALTER TABLE st_pk ALTER COLUMN body SET STORAGE EXTERNAL;
CREATE INDEX ON st_pk (c);
CREATE SEQUENCE pub_p0 START 1; CREATE SEQUENCE pub_pa START 301; CREATE SEQUENCE pub_pr START 601; CREATE SEQUENCE pub_pb START 901;
ANALYZE;
SELECT (SELECT count(*) FROM iv_src) iv, (SELECT count(*) FROM bh_src) bh, (SELECT count(*) FROM st_ol) ol, (SELECT count(*) FROM st_iv_b32) ivb, (SELECT count(*) FROM st_bh_b3) bhb;
