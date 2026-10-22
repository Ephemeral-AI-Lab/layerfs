-- Untimed staging: 60 Commits' new rows per family, prebuilt in every layout's
-- client-side form so the timed statements do only storage work.
\set ON_ERROR_STOP 1
-- the port's FKs to these tables are not carried by the variants; drop them on this copy so arms compare like for like
ALTER TABLE object_location DROP CONSTRAINT IF EXISTS object_location_pack_id_fkey;
ALTER TABLE content_signature DROP CONSTRAINT IF EXISTS content_signature_object_id_fkey;
CREATE TABLE st_ol AS
SELECT c, g, sha256(int8send(c * 1000 + g)) AS object_id, 7::bigint AS role, (100 + g)::bigint AS canonical_length,
       (SELECT pack_id FROM pk_keys WHERE i = 1) AS pack_id, 0::bigint AS group_number, g::bigint AS record_number
FROM generate_series(1, 60) c, generate_series(0, 325) g;
CREATE INDEX ON st_ol (c);
CREATE TABLE st_ol_b16 AS
SELECT c, get_byte(object_id, 0) * 256 + get_byte(object_id, 1) AS bucket,
       string_agg(ol_entry(object_id, role, canonical_length, pack_id, group_number, record_number), ''::bytea ORDER BY object_id) AS entries
FROM st_ol GROUP BY 1, 2;
CREATE INDEX ON st_ol_b16 (c);
-- inode values: 323 consecutive fresh ordinals a Commit, continuing across Commits
CREATE TABLE st_iv AS SELECT c, 160000000 + (c - 1) * 323 + s.g AS ordinal, s.value FROM generate_series(1, 60) c, iv_src s;
CREATE INDEX ON st_iv (c);
CREATE TABLE st_iv_b18 AS
SELECT bk.c, b AS bucket, bit_or(CASE WHEN v.value IS NULL THEN 0 ELSE 1 << sl END) AS present,
       string_agg(coalesce(v.value, decode(repeat('00', 73), 'hex')), ''::bytea ORDER BY sl) AS vals
FROM (SELECT DISTINCT c, ordinal / 18 AS b FROM st_iv) bk CROSS JOIN generate_series(0, 17) sl
LEFT JOIN st_iv v ON v.c = bk.c AND v.ordinal = bk.b * 18 + sl GROUP BY 1, 2;
CREATE INDEX ON st_iv_b18 (c);
CREATE TABLE st_iv_b32 AS
SELECT c, ordinal / 32 AS bucket, bit_or(1::int8 << (ordinal % 32)::int) AS present, string_agg(value, ''::bytea ORDER BY ordinal) AS vals
FROM st_iv GROUP BY 1, 2;
CREATE INDEX ON st_iv_b32 (c);
-- binding hints: 322 fresh ascending children a Commit with real (parent, name)
CREATE TABLE st_bh AS SELECT c, 17000000 + (c - 1) * 322 + s.g AS child, s.parent, s.name FROM generate_series(1, 60) c, bh_src s;
CREATE INDEX ON st_bh (c);
CREATE TABLE st_bh_b2 AS SELECT c, child / 16 AS bucket,
  string_agg(b1((child % 16)::int) || int8send(parent) || b1(length(name)) || name, ''::bytea ORDER BY child, parent, name) AS entries
FROM st_bh GROUP BY 1, 2;
CREATE INDEX ON st_bh_b2 (c);
CREATE TABLE st_bh_b3 AS SELECT c, child / 128 AS bucket,
  string_agg(b1((child % 128)::int) || int8send(parent) || b1(length(name)) || name, ''::bytea ORDER BY child, parent, name) AS entries
FROM st_bh GROUP BY 1, 2;
CREATE INDEX ON st_bh_b3 (c);
-- packs: three a Commit
CREATE TABLE st_pk AS SELECT c, 500000000 + c * 3 + s.g AS pack_id, s.domain, sha256(s.body) AS digest, s.length, s.body
FROM generate_series(1, 60) c, pk_src s;
ALTER TABLE st_pk ALTER COLUMN body SET STORAGE EXTERNAL;
CREATE INDEX ON st_pk (c);
CREATE TABLE st_pk_slice AS SELECT c, pack_id, sl::int2 AS seq, substr(body, sl * 8120 + 1, 8120) AS data
FROM st_pk, generate_series(0, (length::int - 1) / 8120) sl;
CREATE INDEX ON st_pk_slice (c);
-- per-arm Commit counters
DO $$ DECLARE a text; BEGIN
  FOREACH a IN ARRAY ARRAY['ol0','ol_a1','ol_a2','ol_b16','iv0','iv_a1','iv_a2','iv_b18','iv_b32z','bh0','bh_a1','bh_a3','bh_b1','bh_b2','bh_b3','pk0','pk_a1','pk_b'] LOOP
    EXECUTE format('CREATE SEQUENCE seq_%s', a);
  END LOOP; END $$;
-- B18 merge of a partial bucket: new slot where the new bitmap has it, else the old slot
CREATE FUNCTION iv18_merge(old bytea, new bytea, newmask int) RETURNS bytea IMMUTABLE LANGUAGE sql AS $$
  SELECT string_agg(CASE WHEN newmask & (1 << s) <> 0 THEN substr(new, s * 73 + 1, 73) ELSE substr(old, s * 73 + 1, 73) END, ''::bytea ORDER BY s)
  FROM generate_series(0, 17) s $$;
ANALYZE;
SELECT 'st_ol', count(*) FROM st_ol UNION ALL SELECT 'st_ol_b16', count(*) FROM st_ol_b16 UNION ALL SELECT 'st_iv_b18', count(*) FROM st_iv_b18
UNION ALL SELECT 'st_iv_b32', count(*) FROM st_iv_b32 UNION ALL SELECT 'st_bh_b2', count(*) FROM st_bh_b2 UNION ALL SELECT 'st_bh_b3', count(*) FROM st_bh_b3
UNION ALL SELECT 'st_pk_slice', count(*) FROM st_pk_slice;
