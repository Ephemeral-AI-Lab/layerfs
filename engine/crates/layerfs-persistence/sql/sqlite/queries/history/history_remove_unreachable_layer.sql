DELETE FROM layer
 WHERE layer_stack_id = $1
   AND NOT EXISTS (SELECT 1 FROM kept_node k
                    WHERE k.kind = 'layer' AND k.node_id = layer.layer_id);
