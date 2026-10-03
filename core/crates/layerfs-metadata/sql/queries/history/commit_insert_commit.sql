INSERT INTO ${schema}.commit (commit_id, layer_stack_id, root_id, parent_commit_id, base_layer_id) VALUES ($1, $2, $3, $4, $5) RETURNING 1;
