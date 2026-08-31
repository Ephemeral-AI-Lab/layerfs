-- family: layerstack
-- name: current_head
-- parameters: ?1 LayerStackId
-- results: head LayerId
SELECT head_layer_id FROM layer_stacks WHERE layer_stack_id = ?1;
