-- family: layerstack
-- name: get
-- parameters: ?1 LayerStackId
-- results: layer_stack_id, name, head_layer_id
SELECT layer_stack_id, name, head_layer_id
FROM layer_stacks
WHERE layer_stack_id = ?1;
