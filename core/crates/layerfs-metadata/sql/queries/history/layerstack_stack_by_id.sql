SELECT layer_stack_id, name, scope_id, profile_id, head_layer_id FROM ${schema}.layer_stack WHERE layer_stack_id = $1;
