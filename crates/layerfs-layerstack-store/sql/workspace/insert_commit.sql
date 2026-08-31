-- family: workspace
-- name: insert_commit
-- parameters: ?1 CommitId, ?2 root ObjectId, ?3 parent CommitId, ?4 base LayerId
-- affected rows: one inserted or zero if immutable Commit already exists
INSERT INTO commits(commit_id, root_id, parent_commit_id, base_layer_id)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(commit_id) DO NOTHING;
