-- family: layerstack
-- name: list
-- parameters: ?1 exclusive LayerStackId cursor, ?2 limit
-- results: layer_stack_id, name, head_layer_id in keyset order
SELECT layer_stack_id, name, head_layer_id
FROM layer_stacks
WHERE layer_stack_id > ?1
ORDER BY layer_stack_id
LIMIT ?2;
