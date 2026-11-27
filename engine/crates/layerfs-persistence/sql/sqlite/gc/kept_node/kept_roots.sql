SELECT node_id
  FROM kept_node
 WHERE layer_stack_id = $1
   AND kind = 'layer'
 ORDER BY node_id;
