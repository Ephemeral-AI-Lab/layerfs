SELECT layer_id FROM ${schema}.layer WHERE source_branch_id = $1 AND source_commit_id = $2;
