-- family: layerstack
-- name: advance_head
-- parameters: ?1 LayerStackId, ?2 new LayerId, ?3 expected LayerId
-- affected rows: one CAS winner, zero if head moved
UPDATE layer_stacks
SET head_layer_id = ?2
WHERE layer_stack_id = ?1 AND head_layer_id = ?3;
