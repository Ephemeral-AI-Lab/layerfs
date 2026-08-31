-- family: layerstack
-- name: load_add_snapshot
-- parameters: ?1 BranchId
-- results: Branch fields, Commit root/base, base root, LayerStack head, existing source LayerId
SELECT b.branch_id, b.layer_stack_id, b.name, b.base_layer_id, b.head_commit_id,
       c.root_id, c.base_layer_id, base.root_id, stack.head_layer_id, added.layer_id
FROM branches AS b
JOIN commits AS c ON c.commit_id = b.head_commit_id
JOIN layers AS base ON base.layer_id = b.base_layer_id
JOIN layer_stacks AS stack ON stack.layer_stack_id = b.layer_stack_id
LEFT JOIN layers AS added
  ON added.source_branch_id = b.branch_id AND added.source_commit_id = b.head_commit_id
WHERE b.branch_id = ?1;
