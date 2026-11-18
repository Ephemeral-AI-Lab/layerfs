DELETE FROM gc_intent WHERE op_id = $1 RETURNING op_id;
