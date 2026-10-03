INSERT INTO layer (layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id) VALUES ($1, $2, $3, $4, $5, $6) RETURNING 1;
