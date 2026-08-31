-- family: branch
-- name: list_commits
-- parameters: ?1 exclusive CommitId cursor, ?2 limit
-- results: commit fields in keyset order
SELECT commit_id, root_id, parent_commit_id, base_layer_id
FROM commits
WHERE commit_id > ?1
ORDER BY commit_id
LIMIT ?2;
