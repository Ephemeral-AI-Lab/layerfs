-- The owner's own release deletes its live row; a retained `done` audit row
-- is never deleted here (`gc_intent_done_cap` bounds a Branch's done rows).
DELETE FROM gc_intent WHERE op_id = $1 AND state <> 'done' RETURNING op_id;
