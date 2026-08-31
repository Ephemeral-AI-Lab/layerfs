-- family: branch
-- name: insert
-- parameters: ?1 BranchId, ?2 LayerStackId, ?3 name, ?4 base LayerId, ?5 head CommitId
-- affected rows: one
INSERT INTO branches(branch_id, layer_stack_id, name, base_layer_id, head_commit_id)
VALUES (?1, ?2, ?3, ?4, ?5);
