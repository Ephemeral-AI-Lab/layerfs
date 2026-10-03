UPDATE ${schema}.branch SET head_commit_id = $1 WHERE branch_id = $2 AND base_layer_id = $3 AND head_commit_id IS NOT DISTINCT FROM $4 RETURNING 1;
