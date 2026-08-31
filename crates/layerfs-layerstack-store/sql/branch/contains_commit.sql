-- family: branch
-- name: contains_commit
-- parameters: ?1 BranchId, ?2 selected CommitId, ?3 traversal ceiling
-- results: one row iff selected Commit is visible in Branch ancestry
WITH RECURSIVE history(commit_id, parent_commit_id, depth) AS (
    SELECT c.commit_id, c.parent_commit_id, 1
    FROM branches AS b JOIN commits AS c ON c.commit_id = b.head_commit_id
    WHERE b.branch_id = ?1
    UNION ALL
    SELECT c.commit_id, c.parent_commit_id, h.depth + 1
    FROM commits AS c JOIN history AS h ON c.commit_id = h.parent_commit_id
    WHERE h.depth < ?3
)
SELECT 1 FROM history WHERE commit_id = ?2 LIMIT 1;
