-- family: branch
-- name: list
-- parameters: ?1 optional LayerStackId, ?2 exclusive BranchId cursor, ?3 limit
-- results: branch fields in global or scoped keyset order
SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id
FROM branches
WHERE ?1 IS NULL AND branch_id > ?2
UNION ALL
SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id
FROM branches
WHERE ?1 IS NOT NULL AND layer_stack_id = ?1 AND branch_id > ?2
ORDER BY branch_id
LIMIT ?3;
