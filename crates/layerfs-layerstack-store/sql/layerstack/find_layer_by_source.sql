-- family: layerstack
-- name: find_layer_by_source
-- parameters: ?1 BranchId, ?2 CommitId
-- results: layer fields
SELECT layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id
FROM layers
WHERE source_branch_id = ?1 AND source_commit_id = ?2;
