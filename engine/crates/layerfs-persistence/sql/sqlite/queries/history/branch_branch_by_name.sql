SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id, forked_from_id FROM branch WHERE layer_stack_id = $1 AND name = $2;
