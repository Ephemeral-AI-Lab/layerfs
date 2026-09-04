-- family: workspace
-- name: get_stage
-- parameters: ?1 WorkspaceId
-- results: WorkspaceId, BranchId, root ObjectId
SELECT workspace_id, branch_id, root_id
FROM workspace_stages
WHERE workspace_id = ?1;
