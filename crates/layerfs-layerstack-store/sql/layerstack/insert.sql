-- family: layerstack
-- name: insert
-- parameters: ?1 LayerStackId, ?2 name, ?3 head LayerId
-- affected rows: one
INSERT INTO layer_stacks(layer_stack_id, name, head_layer_id)
VALUES (?1, ?2, ?3);
