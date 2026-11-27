INSERT INTO layer_stack_retired (layer_stack_id, deleted_ms) VALUES ($1, $2) ON CONFLICT (layer_stack_id) DO NOTHING RETURNING 1;
