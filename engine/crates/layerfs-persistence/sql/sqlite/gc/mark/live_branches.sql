SELECT b.branch_id,b.head_commit_id,b.base_layer_id FROM branch b
 WHERE NOT EXISTS (SELECT 1 FROM branch_retired r WHERE r.branch_id=b.branch_id)
   AND NOT EXISTS (SELECT 1 FROM layer_stack_retired s WHERE s.layer_stack_id=b.layer_stack_id)
   AND b.branch_id>$1
 ORDER BY b.branch_id LIMIT $2
