-- family: layerstack
-- name: insert_layer
-- parameters: ?1 LayerId, ?2 LayerStackId, ?3 parent LayerId, ?4 root ObjectId, ?5 source BranchId, ?6 source CommitId
-- affected rows: one
INSERT INTO layers(layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id)
VALUES (?1, ?2, ?3, ?4, ?5, ?6);
