DELETE FROM commit_lineage
 WHERE NOT EXISTS (SELECT 1 FROM "commit" c WHERE c.commit_id = commit_lineage.commit_id);
