SELECT s.layer_stack_id,s.head_layer_id FROM layer_stack s
 WHERE NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=s.layer_stack_id)
   AND s.layer_stack_id>$1
 ORDER BY s.layer_stack_id LIMIT $2
