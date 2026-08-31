-- family: query
-- name: branch_roots_page
-- parameters: ?1 exclusive BranchId cursor, ?2 limit
-- results: branch_id, effective root_id in keyset order
SELECT b.branch_id, COALESCE(c.root_id, l.root_id)
FROM branches AS b
JOIN layers AS l ON l.layer_id = b.base_layer_id
LEFT JOIN commits AS c ON c.commit_id = b.head_commit_id
WHERE b.branch_id > ?1
ORDER BY b.branch_id
LIMIT ?2;
