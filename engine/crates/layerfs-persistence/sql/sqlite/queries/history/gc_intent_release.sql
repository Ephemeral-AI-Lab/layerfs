-- The retained `done` row is the operator audit and is never removed by a
-- release. A late owner release therefore matches no live row, exactly as
-- on PostgreSQL, whose release reads the intent through the done-filtering
-- `gc_intent_get` first.
DELETE FROM gc_intent WHERE op_id = $1 AND state <> 'done' RETURNING op_id;
