-- A mount's session keys, all or none, inside its admission transaction
-- (#768): $1 the keys, $2 whether each is taken shared, $3 the backend this
-- session's keys are held on (0 before its first lock), $4 the attempt token
-- of the transaction. Nothing is tried unless the token is this
-- transaction's and the backend is the session's (spec §5.2 item 34). Every
-- key is tried; if any is refused, the granted ones are given back here,
-- before the statement fails, since session locks outlive the transaction.
-- It answers the backend, or fails: 22003 (smallint overflow) for another
-- backend, having tried no key; 22012 (division by zero) for a refused key
-- or a foreign transaction, holding no key.
WITH a AS MATERIALIZED (
    SELECT current_setting('layerfs.attempt', true) IS NOT DISTINCT FROM $4::text AS attempt,
           ($3::bigint = 0 OR pg_backend_pid() = $3::bigint) AS same
), t AS MATERIALIZED (
    SELECT k.key, k.shared,
           CASE WHEN a.attempt AND a.same THEN
                CASE WHEN k.shared THEN pg_try_advisory_lock_shared(k.key)
                     ELSE pg_try_advisory_lock(k.key) END
           END AS granted
      FROM a CROSS JOIN unnest($1::bigint[], $2::boolean[]) AS k(key, shared)
), r AS MATERIALIZED (
    SELECT CASE WHEN t.shared THEN pg_advisory_unlock_shared(t.key)
                ELSE pg_advisory_unlock(t.key) END AS released
      FROM t
     WHERE t.granted AND EXISTS (SELECT 1 FROM t u WHERE u.granted IS NOT TRUE)
)
SELECT CASE
           WHEN NOT a.same THEN (32768 + n.keys)::smallint::bigint
           WHEN n.refused > 0 THEN (1 / (n.refused - n.refused + 0 * (SELECT count(*) FROM r)))::bigint
           ELSE pg_backend_pid()::bigint
       END AS pid
  FROM a, (SELECT count(*) AS keys, count(*) FILTER (WHERE granted IS NOT TRUE) AS refused FROM t) n;
