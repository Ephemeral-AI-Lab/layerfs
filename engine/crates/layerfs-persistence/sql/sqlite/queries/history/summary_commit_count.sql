SELECT COUNT(*) FROM "commit" INDEXED BY commit_identity WHERE layer_stack_id = $1;
