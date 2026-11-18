INSERT INTO kept_node(layer_stack_id,kind,node_id,serial)
SELECT s.layer_stack_id,'stack',s.layer_stack_id,0 FROM layer_stack s
 WHERE NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=s.layer_stack_id)
UNION ALL
SELECT l.layer_stack_id,'layer',l.layer_id,l.history_serial FROM layer l
 WHERE NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=l.layer_stack_id)
UNION ALL
SELECT b.layer_stack_id,'branch',b.branch_id,0 FROM branch b
 WHERE NOT EXISTS (SELECT 1 FROM branch_retired r WHERE r.branch_id=b.branch_id)
   AND NOT EXISTS (SELECT 1 FROM layer_stack_retired r WHERE r.layer_stack_id=b.layer_stack_id)
