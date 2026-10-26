SELECT l.run, l.depth, l.newest, c.layer_stack_id, c.parent_commit_id FROM commit_lineage l JOIN "commit" c ON c.commit_id = l.commit_id WHERE l.commit_id = $1;
