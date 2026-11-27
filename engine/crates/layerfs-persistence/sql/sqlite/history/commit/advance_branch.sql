UPDATE branch SET head_commit_id = $1 WHERE branch_id = $2 AND base_layer_id = $3 AND (head_commit_id = $4 OR (head_commit_id IS NULL AND $4 IS NULL)) RETURNING 1;
