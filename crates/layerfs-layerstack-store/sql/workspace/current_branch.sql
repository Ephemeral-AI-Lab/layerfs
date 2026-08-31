-- family: workspace
-- name: current_branch
-- parameters: ?1 BranchId
-- results: current head CommitId, current base LayerId
SELECT head_commit_id, base_layer_id
FROM branches
WHERE branch_id = ?1;
