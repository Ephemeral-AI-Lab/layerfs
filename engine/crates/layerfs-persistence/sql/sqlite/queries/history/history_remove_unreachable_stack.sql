DELETE FROM layer_stack
 WHERE layer_stack_id = $1
   AND NOT EXISTS (SELECT 1 FROM kept_node k
                    WHERE k.kind = 'stack' AND k.node_id = layer_stack.layer_stack_id);
