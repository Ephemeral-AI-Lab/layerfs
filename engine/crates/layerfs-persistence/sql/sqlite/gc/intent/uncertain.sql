-- A retained `done` audit row is never turned back into a live intent.
UPDATE gc_intent SET state = 'uncertain', uncertain = TRUE, updated_ms = $1 WHERE op_id = $2 AND state <> 'done' RETURNING op_id;
