-- family: branch
-- name: get
-- parameters: ?1 BranchId
-- results: branch_id, layer_stack_id, name, base_layer_id, head_commit_id
SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id
FROM branches
WHERE branch_id = ?1;
