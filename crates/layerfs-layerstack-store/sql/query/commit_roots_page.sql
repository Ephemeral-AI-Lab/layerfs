-- family: query
-- name: commit_roots_page
-- parameters: ?1 exclusive CommitId cursor, ?2 limit
-- results: commit_id, root_id in keyset order
SELECT commit_id, root_id
FROM commits
WHERE commit_id > ?1
ORDER BY commit_id
LIMIT ?2;
