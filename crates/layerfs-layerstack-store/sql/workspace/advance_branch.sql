-- family: workspace
-- name: advance_branch
-- parameters: ?1 BranchId, ?2 new CommitId, ?3 expected CommitId, ?4 new base LayerId, ?5 expected base LayerId
-- affected rows: one CAS winner, zero if head/base moved
UPDATE branches
SET head_commit_id = ?2, base_layer_id = ?4
WHERE branch_id = ?1
  AND head_commit_id IS ?3
  AND base_layer_id = ?5;
