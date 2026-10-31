SELECT commit_id, base_layer_id, created_ms FROM "commit" INDEXED BY commit_identity WHERE layer_stack_id = $1 AND commit_id = $2;
