-- family: branch
-- name: get_by_name
-- parameters: ?1 LayerStackId, ?2 EntityName
-- results: branch fields
SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id
FROM branches
WHERE layer_stack_id = ?1 AND name = ?2;
