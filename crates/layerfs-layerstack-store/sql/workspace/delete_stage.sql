-- family: workspace
-- name: delete_stage
-- parameters: ?1 WorkspaceId, ?2 BranchId, ?3 root ObjectId
-- affected rows: one exact stage retired or zero if absent/different
DELETE FROM workspace_stages
WHERE workspace_id = ?1
  AND branch_id = ?2
  AND root_id = ?3;
