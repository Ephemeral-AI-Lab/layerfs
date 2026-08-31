-- family: branch
-- name: history_page
-- parameters: ?1 starting CommitId, ?2 maximum records
-- results: commit fields newest to oldest
WITH RECURSIVE history(commit_id, root_id, parent_commit_id, base_layer_id, depth) AS (
    SELECT commit_id, root_id, parent_commit_id, base_layer_id, 1
    FROM commits WHERE commit_id = ?1
    UNION ALL
    SELECT c.commit_id, c.root_id, c.parent_commit_id, c.base_layer_id, h.depth + 1
    FROM commits AS c JOIN history AS h ON c.commit_id = h.parent_commit_id
    WHERE h.depth < ?2
)
SELECT commit_id, root_id, parent_commit_id, base_layer_id
FROM history
ORDER BY depth;
