UPDATE store_totals
   SET logical = logical
     - (SELECT coalesce(sum(l.logical), 0) FROM layer l
         WHERE l.layer_stack_id = $1
           AND NOT EXISTS (SELECT 1 FROM kept_node k
                            WHERE k.kind = 'layer' AND k.node_id = l.layer_id))
     - (SELECT coalesce(sum(c.logical), 0) FROM "commit" c
         WHERE c.layer_stack_id = $1
           AND NOT EXISTS (SELECT 1 FROM kept_node k
                            WHERE k.kind = 'commit' AND k.node_id = c.commit_id))
 WHERE id = 1;
