-- family: layerstack
-- name: history_page
-- parameters: ?1 starting LayerId, ?2 maximum records
-- results: layer fields newest to oldest
WITH RECURSIVE history(layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id, depth) AS (
    SELECT layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id, 1
    FROM layers WHERE layer_id = ?1
    UNION ALL
    SELECT l.layer_id, l.layer_stack_id, l.parent_layer_id, l.root_id, l.source_branch_id, l.source_commit_id, h.depth + 1
    FROM layers AS l JOIN history AS h ON l.layer_id = h.parent_layer_id
    WHERE h.depth < ?2
)
SELECT layer_id, layer_stack_id, parent_layer_id, root_id, source_branch_id, source_commit_id
FROM history
ORDER BY depth;
