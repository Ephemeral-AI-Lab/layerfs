-- The owner release's unlock (#768 step C), sent in one message after the
-- release's DELETE (`gc_intent_release_owner`), each request in its own
-- Sync: $1 the op's session keys, $2 whether each was taken shared, $3 the
-- release attempt's token. The keys are given back only when that DELETE
-- completed at the session's generation, which it records by setting
-- `layerfs.released` to the token: exactly when the statement-by-statement
-- release answered success and sent its unlock, whether the delete removed
-- the row or found it already gone (another session, a reconcile, an
-- operator's retained `done` row), and whatever the generation does after
-- it. A stale or refused DELETE records nothing, so its keys stay held. One
-- row per key given back, with the backend that ran it (spec §5.2 item 34);
-- no row when nothing was given back.
WITH g AS MATERIALIZED (
    SELECT current_setting('layerfs.released', true) IS NOT DISTINCT FROM $3::text AS go
)
SELECT CASE WHEN k.shared THEN pg_advisory_unlock_shared(k.key)
            ELSE pg_advisory_unlock(k.key) END AS released,
       pg_backend_pid()::bigint AS pid
  FROM g CROSS JOIN unnest($1::bigint[], $2::boolean[]) AS k(key, shared)
 WHERE g.go;
