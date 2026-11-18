UPDATE branch SET base_layer_id = $1, head_commit_id = $2 WHERE branch_id = $3 RETURNING 1;
