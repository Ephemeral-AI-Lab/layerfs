UPDATE branch SET head_commit_id = $1 WHERE branch_id = $2 AND base_layer_id = $3 RETURNING 1;
