SELECT l.layer_id
  FROM layer l
 WHERE l.layer_stack_id = $1
   AND NOT EXISTS (SELECT 1 FROM layer_stack_retired r
                    WHERE r.layer_stack_id = l.layer_stack_id)
 ORDER BY l.layer_id;
