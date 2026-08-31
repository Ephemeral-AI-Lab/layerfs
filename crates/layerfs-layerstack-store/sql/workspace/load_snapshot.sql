-- family: workspace
-- name: load_snapshot
-- parameters: ?1 BranchId
-- results: Branch fields, LayerStack fields, effective root ObjectId
SELECT b.branch_id, b.layer_stack_id, b.name, b.base_layer_id, b.head_commit_id,
       s.layer_stack_id, s.name, s.head_layer_id,
       COALESCE(c.root_id, l.root_id)
FROM branches AS b
JOIN layers AS l ON l.layer_id = b.base_layer_id
JOIN layer_stacks AS s ON s.layer_stack_id = b.layer_stack_id
LEFT JOIN commits AS c ON c.commit_id = b.head_commit_id
WHERE b.branch_id = ?1;
