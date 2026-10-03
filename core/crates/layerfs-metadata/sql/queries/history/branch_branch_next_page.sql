SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id FROM ${schema}.branch WHERE layer_stack_id = $1 AND name > $2 ORDER BY name LIMIT $3;
