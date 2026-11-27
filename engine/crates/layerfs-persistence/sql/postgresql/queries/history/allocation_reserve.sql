-- One inode reservation in one statement (#818 W4): the PostgreSQL form of
-- `allocation::reserve_inodes`, sent in one message after the write
-- transaction's lane row. $1 the scope, $2 this catalog (the scope's
-- authority), $3 the count (at least 1; a count past 2^63-1 is bound as
-- 2^63-1, which no range fits), $4 the attempt token the transaction set.
-- A new scope's row starts at the count; an existing one advances by the
-- count only when this catalog owns it and the range ends below 2^63-1.
-- Either way the answer is the high-water before the range (`reserved`).
-- Otherwise nothing is written and the scope's row answers as it stands
-- (`highwater`, `authority`), so the caller refuses in the shared order
-- (the authority, then the capacity); no row at all is a new scope whose
-- range does not fit.
WITH r AS (
  INSERT INTO {s}.scope_allocator AS a (scope_id, highwater, authority_id)
  SELECT $1::bytea, $3::bigint, $2::bytea
   WHERE $3::bigint < 9223372036854775807
     AND 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM $4::text)::int=1
  ON CONFLICT (scope_id) DO UPDATE SET highwater = a.highwater + $3::bigint
   WHERE a.authority_id = $2::bytea AND a.highwater < 9223372036854775807 - $3::bigint
  RETURNING a.highwater - $3::bigint AS reserved)
SELECT r.reserved, NULL::bigint AS highwater, NULL::bytea AS authority FROM r
UNION ALL
SELECT NULL::bigint, a.highwater, a.authority_id FROM {s}.scope_allocator a
 WHERE a.scope_id = $1::bytea AND NOT EXISTS (SELECT 1 FROM r)
