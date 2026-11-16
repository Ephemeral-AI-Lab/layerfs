SELECT CASE
  WHEN NOT EXISTS (SELECT 1 FROM kept_node) THEN 0
  ELSE
    (SELECT COUNT(*) FROM "commit" c
      WHERE c.layer_stack_id = $1
        AND NOT EXISTS (SELECT 1 FROM kept_node k
                         WHERE k.kind = 'commit' AND k.node_id = c.commit_id))
  + (SELECT COUNT(*) FROM layer l
      WHERE l.layer_stack_id = $1
        AND NOT EXISTS (SELECT 1 FROM kept_node k
                         WHERE k.kind = 'layer' AND k.node_id = l.layer_id))
  + (SELECT COUNT(*) FROM branch b
      WHERE b.layer_stack_id = $1
        AND NOT EXISTS (SELECT 1 FROM kept_node k
                         WHERE k.kind = 'branch' AND k.node_id = b.branch_id))
  + (SELECT COUNT(*) FROM layer_stack s
      WHERE s.layer_stack_id = $1
        AND NOT EXISTS (SELECT 1 FROM kept_node k
                         WHERE k.kind = 'stack' AND k.node_id = s.layer_stack_id))
END;
