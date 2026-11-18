SELECT c.root_id FROM kept_node k
  JOIN "commit" c ON c.commit_id=k.node_id
 WHERE k.kind='commit'
UNION
SELECT l.root_id FROM kept_node k
  JOIN layer l ON l.layer_id=k.node_id
 WHERE k.kind='layer'
