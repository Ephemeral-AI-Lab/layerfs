INSERT INTO layer_stack (layer_stack_id, name, scope_id, profile_id, head_layer_id) VALUES ($1, $2, $3, $4, $5) RETURNING 1;
