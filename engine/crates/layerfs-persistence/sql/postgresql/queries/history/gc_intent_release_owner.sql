-- The owner's release (#768): one autocommit statement deletes its live row
-- only when the Store is still at the session's generation $2 (0 before its
-- open set one); a retained `done` row is never deleted here. It returns the
-- Store's generation and the rows deleted, so a stale session is refused
-- with nothing deleted. Reading `history_meta` holds its AccessShareLock for
-- the statement, as the fence of every transaction does.
WITH released AS (
    DELETE FROM {s}.gc_intent
     WHERE op_id = $1 AND state <> 'done'
       AND ($2::bigint = 0
            OR (SELECT catalog_incarnation FROM {s}.history_meta WHERE id = 1) = $2::bigint)
    RETURNING op_id
)
SELECT h.catalog_incarnation::bigint AS generation,
       (SELECT count(*) FROM released)::bigint AS released
  FROM {s}.history_meta h
 WHERE h.id = 1;
