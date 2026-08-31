-- family: layerstack
-- name: get_by_name
-- parameters: ?1 EntityName
-- results: layer_stack_id, name, head_layer_id
SELECT layer_stack_id, name, head_layer_id
FROM layer_stacks
WHERE name = ?1;
