DELETE FROM commit_lineage WHERE commit_id IN (SELECT commit_id FROM "commit" WHERE layer_stack_id = $1);
