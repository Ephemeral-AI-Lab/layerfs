-- family: layerstack
-- name: list_layers
-- parameters: ?1 optional LayerStackId, ?2 exclusive LayerId cursor, ?3 limit
-- results: layer fields in global or scoped keyset order
SELECT layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id
FROM layers
WHERE ?1 IS NULL AND layer_id > ?2
UNION ALL
SELECT layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id
FROM layers
WHERE ?1 IS NOT NULL AND layer_stack_id = ?1 AND layer_id > ?2
ORDER BY layer_id
LIMIT ?3;
