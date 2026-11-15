UPDATE gc_intent SET state = 'uncertain', uncertain = TRUE, updated_ms = $1 WHERE op_id = $2 RETURNING op_id;
