SELECT k.layer_stack_id,k.kind,k.node_id,k.serial,COALESCE(c.root_id,l.root_id)
  FROM kept_node k
  LEFT JOIN "commit" c ON k.kind='commit' AND c.commit_id=k.node_id
  LEFT JOIN layer l ON k.kind='layer' AND l.layer_id=k.node_id
 WHERE (k.layer_stack_id,k.kind,k.node_id)>($1,$2,$3)
 ORDER BY k.layer_stack_id,k.kind,k.node_id
 LIMIT $4
