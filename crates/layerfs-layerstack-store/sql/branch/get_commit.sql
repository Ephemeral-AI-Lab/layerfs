-- family: branch
-- name: get_commit
-- parameters: ?1 CommitId
-- results: commit_id, root_id, parent_commit_id, base_layer_id
SELECT commit_id, root_id, parent_commit_id, base_layer_id
FROM commits
WHERE commit_id = ?1;
