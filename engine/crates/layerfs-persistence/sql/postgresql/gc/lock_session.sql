-- Takes all of one GC intent's session advisory keys in one statement
-- (spec §5.2): $1 the keys, $2 whether each is taken shared, $3 the
-- generation the session opened at (0 before its open set one). No key is
-- tried when the Store's generation differs, so a stale session takes
-- nothing. Otherwise every key is tried, even after one fails, so the caller
-- unlocks exactly the granted ones (session locks survive any rollback).
-- One row per key: its position, whether it was granted (NULL: not tried),
-- the backend that took it (§5.2 item 34) and the Store's generation.
SELECT k.ord::bigint AS ord,
       CASE WHEN $3::bigint = 0 OR f.generation = $3::bigint THEN
            CASE WHEN k.shared THEN pg_try_advisory_lock_shared(k.key)
                 ELSE pg_try_advisory_lock(k.key) END
       END AS granted,
       pg_backend_pid()::bigint AS pid,
       f.generation
  FROM (SELECT catalog_incarnation::bigint AS generation
          FROM {s}.history_meta WHERE id = 1) f
 CROSS JOIN unnest($1::bigint[], $2::boolean[]) WITH ORDINALITY AS k(key, shared, ord);
