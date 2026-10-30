DELETE FROM reach WHERE key = $1 OR key IN (SELECT l.run FROM commit_lineage l JOIN "commit" c ON c.commit_id = l.commit_id WHERE c.layer_stack_id = $1);
