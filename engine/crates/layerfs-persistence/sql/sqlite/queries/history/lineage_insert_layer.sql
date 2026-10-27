INSERT INTO layer_lineage (layer_id, layer_stack_id, height, overwrote_count) VALUES ($1, $2, $3, $4) RETURNING 1;
