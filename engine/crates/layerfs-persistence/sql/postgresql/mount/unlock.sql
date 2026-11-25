-- The owner release's unlock (#768 step C), sent in one message after the
-- release's DELETE (`gc_intent_release_owner`), each request in its own
-- Sync: $1 the op's session keys, $2 whether each was taken shared, $3 the
-- op, $4 the generation the session opened at (0 before its open set one).
-- The keys are given back only when the Store is still at that generation
-- and no live row of the op remains: the DELETE before it ran, or the row
-- was already gone (another session, a reconcile, or an operator's retained
-- `done` row). A refused or stale DELETE keeps its row, so its keys stay
-- held. One row per key given back, with the backend that ran it (spec
-- §5.2 item 34); no row when nothing was given back.
WITH g AS MATERIALIZED (
    SELECT ($4::bigint = 0
            OR (SELECT catalog_incarnation FROM {s}.history_meta WHERE id = 1) = $4::bigint)
       AND NOT EXISTS (SELECT 1 FROM {s}.gc_intent
                        WHERE op_id = $3::bytea AND state <> 'done') AS go
)
SELECT CASE WHEN k.shared THEN pg_advisory_unlock_shared(k.key)
            ELSE pg_advisory_unlock(k.key) END AS released,
       pg_backend_pid()::bigint AS pid
  FROM g CROSS JOIN unnest($1::bigint[], $2::boolean[]) AS k(key, shared)
 WHERE g.go;
