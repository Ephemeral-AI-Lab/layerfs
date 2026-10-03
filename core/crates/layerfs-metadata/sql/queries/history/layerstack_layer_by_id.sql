SELECT layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id FROM ${schema}.layer WHERE layer_id = $1;
