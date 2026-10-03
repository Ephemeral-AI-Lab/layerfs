INSERT INTO ${schema}.branch (branch_id, layer_stack_id, name, base_layer_id, head_commit_id) VALUES ($1, $2, $3, $4, $5) RETURNING 1;
