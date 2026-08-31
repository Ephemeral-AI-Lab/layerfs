-- family: layerstack
-- name: get_layer
-- parameters: ?1 LayerId
-- results: layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id
SELECT layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id
FROM layers
WHERE layer_id = ?1;
