DELETE FROM absorbed WHERE run IN (SELECT l.run FROM commit_lineage l JOIN "commit" c ON c.commit_id = l.commit_id WHERE c.layer_stack_id = $1);
