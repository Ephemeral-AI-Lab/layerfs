INSERT INTO commit_lineage (commit_id, run, depth, newest, merged) VALUES ($1, $2, $3, $4, $5) RETURNING 1;
