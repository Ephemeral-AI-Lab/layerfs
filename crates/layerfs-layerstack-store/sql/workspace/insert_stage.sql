-- family: workspace
-- name: insert_stage
-- parameters: ?1 WorkspaceId, ?2 BranchId, ?3 root ObjectId
-- affected rows: one inserted or zero if this Workspace already has a stage
INSERT INTO workspace_stages(workspace_id, branch_id, root_id)
VALUES (?1, ?2, ?3)
ON CONFLICT(workspace_id) DO NOTHING;
