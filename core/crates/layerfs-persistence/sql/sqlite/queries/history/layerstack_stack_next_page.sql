SELECT layer_stack_id, name, scope_id, profile_id, head_layer_id FROM layer_stack WHERE name > $1 ORDER BY name LIMIT $2;
