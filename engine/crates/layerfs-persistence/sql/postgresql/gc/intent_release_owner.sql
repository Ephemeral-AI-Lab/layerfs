-- The owner's release (#768): one autocommit statement deletes its live row
-- only when the Store is still at the session's generation $2 (0 before its
-- open set one); a retained `done` row is never deleted here. It returns the
-- Store's generation and the rows deleted, so a stale session is refused
-- with nothing deleted. Reading `history_meta` holds its AccessShareLock for
-- the statement, as the fence of every transaction does. When the statement
-- ran at that generation it also records the release attempt's token $3 in
-- the session setting `layerfs.released` (kept only if the statement
-- completes), which the unlock sent right after it (`mount/unlock.sql`)
-- reads: the keys are given back exactly when this delete answered success,
-- whatever happens to the generation afterwards.
WITH released AS (
    DELETE FROM {s}.gc_intent
     WHERE op_id = $1 AND state <> 'done'
       AND ($2::bigint = 0
            OR (SELECT catalog_incarnation FROM {s}.history_meta WHERE id = 1) = $2::bigint)
    RETURNING op_id
)
SELECT h.catalog_incarnation::bigint AS generation,
       (SELECT count(*) FROM released)::bigint AS released,
       set_config('layerfs.released',
                  CASE WHEN $2::bigint = 0 OR h.catalog_incarnation = $2::bigint
                       THEN $3::text ELSE '' END,
                  false) AS token
  FROM {s}.history_meta h
 WHERE h.id = 1;
